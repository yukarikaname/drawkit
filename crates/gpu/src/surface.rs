//! A CAMetalLayer-backed wgpu surface for the painting.
//!
//! This is the display path, not an offscreen render: the painting texture is uploaded and drawn
//! straight into the layer's swapchain, so the shell no longer builds a CGImage and blits it. The
//! paper rectangle is a uniform, which lets the canvas zoom and pan without recreating the texture.
//!
//! An HDR surface is the same pipeline with two changes: the painting texture is `Rgba16Float`, and
//! the swapchain is `Rgba16Float` too, which the layer presents as extended range content. The
//! shell uploads premultiplied linear values instead of eight-bit ones.

use std::ffi::c_void;

use half::f16;
mod commit;
#[cfg(target_os = "linux")]
mod dmabuf;
mod draw;
mod export;
mod fill;
mod layers;
mod native;
pub(crate) mod present;
mod preview;
mod shaders;
mod state;
mod stroke;
mod thumbnail;
mod upload;
mod wet;
pub use stroke::StrokePreview;

use shaders::dirty_region;
use shaders::pipeline;
use wgpu::util::DeviceExt;

pub struct CanvasSurface {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// The present modes this swapchain accepts, so a shell can ask for a low-latency one.
    present_modes: Vec<wgpu::PresentMode>,
    present_mode: u32,
    pipeline: wgpu::RenderPipeline,
    texture: Option<wgpu::Texture>,
    texture_size: (u32, u32),
    /// The canvas rectangle `texture` holds: origin in canvas pixels, then its size. A live
    /// present stores only the visible rectangle, so the paper shader subtracts this origin
    /// before sampling.
    texture_region: (u32, u32, u32, u32),
    /// The full canvas size, which the paper rectangle maps its texture coordinates against.
    canvas_size: (u32, u32),
    /// One imported Linux dma-buf per buffer the shell keeps, so a frame does not re-import.
    #[cfg(target_os = "linux")]
    dmabuf_imports: std::collections::HashMap<u32, dmabuf::Imported>,
    hdr: bool,
    /// 0 sRGB, 1 Display P3, 2 Adobe RGB. An export is encoded back into this space.
    color_space: i32,
    painting_format: wgpu::TextureFormat,
    exposure: f32,
    rotation: f32,
    /// 0 linear Display P3, 1 linear extended sRGB, 2 BT.2100 PQ.
    color_mode: f32,
    stroke: stroke::StrokePreview,
    stroke_dummy: wgpu::Buffer,
    stroke_alpha_lock: bool,
    stroke_smudge: Option<[f32; 2]>,
    skip_texture: Option<wgpu::Texture>,
    thumbnail_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    composite_hdr_pipeline: wgpu::RenderPipeline,
    tile_pipeline: wgpu::RenderPipeline,
    tile_hdr_pipeline: wgpu::RenderPipeline,
    /// Blends a fill mask into a resident tile, in both ranges.
    fill_pipeline: wgpu::RenderPipeline,
    fill_hdr_pipeline: wgpu::RenderPipeline,
    /// Bakes a wet field's settled pigment into a resident tile, in both ranges.
    wet_pipeline: wgpu::RenderPipeline,
    wet_hdr_pipeline: wgpu::RenderPipeline,
    /// Tiles read back by the last stroke commit, handed to the shell one at a time.
    committed: Vec<crate::layers::CommittedTile>,
    /// Half-float tiles read back by the last stroke commit, for an extended-range document.
    linear_committed: Vec<crate::layers::LinearTile>,
    journal: crate::layers::TileJournal,
    committed_rects: Vec<(u32, u32, u32, u32)>,
    /// Layer rectangles whose GPU pixels have not reached the CPU cache yet.
    stale: Vec<(usize, (u32, u32, u32, u32))>,
    layers: crate::LayerStack,
    layer_specs: Vec<crate::LayerSpec>,
    /// Layers the canvas shows but an export leaves out.
    export_references: Vec<bool>,
}

impl CanvasSurface {
    /// # Safety
    /// `layer` must be a live `CAMetalLayer` that outlives the returned surface.
    ///
    /// `hdr` asks for the extended linear Display P3 path; `color_space` is 0 sRGB, 1 Display P3
    /// or 2 Adobe RGB and only selects how an eight-bit surface is tagged.
    pub unsafe fn new(
        layer: *mut c_void,
        width: u32,
        height: u32,
        hdr: bool,
        color_space: i32,
    ) -> Option<Self> {
        if layer.is_null() || width == 0 || height == 0 {
            return None;
        }
        // A CAMetalLayer surface only exists on Apple platforms; Windows and Linux call
        // `new_native` with the window handle their shell owns.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let surface: wgpu::Surface<'static> = unsafe {
                instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
            }
            .ok()?;
            Self::from_surface(instance, surface, width, height, hdr, color_space)
        }

        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            let _ = (layer, width, height, hdr, color_space);
            None
        }
    }

    fn from_surface(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        hdr: bool,
        color_space: i32,
    ) -> Option<Self> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .ok()?;
        // A 13" iPad drawable is wider than the downlevel 2048 limit, so take the adapter's own
        // maximum instead of forcing a window-sized surface onto a default device.
        // The Linux shell renders into a GBM buffer imported as a texture; that import needs the
        // device feature. Other platforms never ask for it.
        let dmabuf_features = {
            #[cfg(target_os = "linux")]
            {
                if adapter
                    .features()
                    .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF)
                {
                    wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF
                } else {
                    wgpu::Features::empty()
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                wgpu::Features::empty()
            }
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("drawkit canvas surface"),
            required_features: dmabuf_features,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()?;
        let capabilities = surface.get_capabilities(&adapter);
        // An extended-range drawable only exists when the window system advertises it; on any
        // other machine the painting falls back to the eight-bit path rather than failing.
        //
        // The order matters: extended Display P3 is the Apple scRGB-equivalent with Display P3
        // primaries, then Windows/X11 scRGB, then BT.2100 PQ. The shader converts the linear
        // Display P3 composite into whichever space the drawable actually takes.
        // Linux never draws into this window surface: the target is the shell's dma-buf, so the
        // format and the colour encoding are ours to choose and the window's capabilities say
        // nothing useful. HDR means a 10-bit buffer encoded as PQ.
        #[cfg(target_os = "linux")]
        let (hdr, color_mode, hdr_space, format) = if hdr {
            (
                true,
                2.0,
                wgpu::SurfaceColorSpace::Bt2100Pq,
                wgpu::TextureFormat::Rgb10a2Unorm,
            )
        } else {
            (
                false,
                0.0,
                wgpu::SurfaceColorSpace::Srgb,
                wgpu::TextureFormat::Bgra8Unorm,
            )
        };
        #[cfg(not(target_os = "linux"))]
        let spaces = capabilities.color_spaces(wgpu::TextureFormat::Rgba16Float);
        #[cfg(not(target_os = "linux"))]
        let (hdr, color_mode, hdr_space) = if hdr {
            if spaces.contains(wgpu::SurfaceColorSpaces::EXTENDED_DISPLAY_P3) {
                (true, 0.0, wgpu::SurfaceColorSpace::ExtendedDisplayP3)
            } else if spaces.contains(wgpu::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR) {
                (true, 1.0, wgpu::SurfaceColorSpace::ExtendedSrgbLinear)
            } else if spaces.contains(wgpu::SurfaceColorSpaces::BT2100_PQ) {
                (true, 2.0, wgpu::SurfaceColorSpace::Bt2100Pq)
            } else if capabilities
                .formats
                .contains(&wgpu::TextureFormat::Rgba16Float)
            {
                // Older wgpu surface reporting on Apple: the format exists and Auto resolves to
                // extended linear sRGB, but the layer takes the Display P3 tag.
                (true, 0.0, wgpu::SurfaceColorSpace::ExtendedDisplayP3)
            } else {
                (false, 0.0, wgpu::SurfaceColorSpace::Srgb)
            }
        } else {
            (false, 0.0, wgpu::SurfaceColorSpace::Srgb)
        };
        #[cfg(not(target_os = "linux"))]
        let format = if hdr {
            wgpu::TextureFormat::Rgba16Float
        } else {
            capabilities
                .formats
                .iter()
                .copied()
                .find(|format| !format.is_srgb())
                .or_else(|| capabilities.formats.first().copied())?
        };
        let painting_format = if hdr {
            wgpu::TextureFormat::Rgba16Float
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let mut config = surface.get_default_config(&adapter, width, height)?;
        config.format = format;
        config.color_space = if hdr {
            hdr_space
        } else if color_space == 2 {
            wgpu::SurfaceColorSpace::ExtendedDisplayP3
        } else if color_space == 1 {
            wgpu::SurfaceColorSpace::DisplayP3
        } else {
            wgpu::SurfaceColorSpace::Srgb
        };
        if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Opaque)
        {
            config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        }
        // Linux never presents here: the canvas is a GBM buffer handed to GTK. Configuring would
        // put a second explicit-sync surface on GTK's wl_surface and the compositor refuses it.
        #[cfg(not(target_os = "linux"))]
        surface.configure(&device, &config);
        let pipeline = pipeline(&device, format, hdr);
        let composite_pipeline = crate::composite::pipeline(&device);
        let composite_hdr_pipeline =
            crate::composite::pipeline_for(&device, wgpu::TextureFormat::Rgba16Float);
        let tile_pipeline = crate::layers::commit_pipeline(&device);
        let tile_hdr_pipeline =
            crate::layers::commit_pipeline_for(&device, wgpu::TextureFormat::Rgba16Float);
        let fill_pipeline =
            crate::LayerStack::fill_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let fill_hdr_pipeline =
            crate::LayerStack::fill_pipeline(&device, wgpu::TextureFormat::Rgba16Float);
        let wet_pipeline =
            crate::LayerStack::wet_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let wet_hdr_pipeline =
            crate::LayerStack::wet_pipeline(&device, wgpu::TextureFormat::Rgba16Float);
        let stroke = stroke::StrokePreview::new(&device, &queue);
        let thumbnail_pipeline = thumbnail::pipeline(&device);
        let stroke_dummy = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("empty stroke coverage"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Some(Self {
            _instance: instance,
            present_modes: capabilities.present_modes.clone(),
            present_mode: 0,
            surface,
            device,
            queue,
            config,
            pipeline,
            texture: None,
            texture_size: (0, 0),
            texture_region: (0, 0, 0, 0),
            canvas_size: (0, 0),
            hdr,
            color_space,
            painting_format,
            exposure: 0.0,
            rotation: 0.0,
            color_mode,
            stroke,
            stroke_dummy,
            stroke_alpha_lock: false,
            stroke_smudge: None,
            skip_texture: None,
            thumbnail_pipeline,
            composite_pipeline,
            composite_hdr_pipeline,
            tile_pipeline,
            tile_hdr_pipeline,
            fill_pipeline,
            fill_hdr_pipeline,
            wet_pipeline,
            wet_hdr_pipeline,
            committed: Vec::new(),
            linear_committed: Vec::new(),
            journal: crate::layers::TileJournal::new(512 << 20),
            committed_rects: Vec::new(),
            stale: Vec::new(),
            layers: crate::LayerStack::without_backups(
                0,
                0,
                if hdr {
                    crate::FrameFormat::Rgba16
                } else {
                    crate::FrameFormat::Rgba8
                },
            ),
            layer_specs: Vec::new(),
            export_references: Vec::new(),
            #[cfg(target_os = "linux")]
            dmabuf_imports: std::collections::HashMap::new(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        #[cfg(not(target_os = "linux"))]
        self.surface.configure(&self.device, &self.config);
    }

    /// Draw and present an eight-bit painting. `paper` is x, y, width, height in drawable pixels.
    pub fn present(
        &mut self,
        pixels: &[u8],
        canvas_width: u32,
        canvas_height: u32,
        paper: [f32; 4],
        dirty: Option<(u32, u32, u32, u32)>,
    ) -> bool {
        let expected = canvas_width as usize * canvas_height as usize * 4;
        if self.hdr || canvas_width == 0 || canvas_height == 0 || pixels.len() < expected {
            return false;
        }
        // A CPU upload holds the whole canvas; a skip texture from the last layer present would
        // have the region's size, which this shader map no longer describes.
        self.skip_texture = None;
        let recreated = self.prepare(canvas_width, canvas_height);
        let Some(texture) = self.texture.as_ref() else {
            return false;
        };
        if let Some((x, y, width, height)) =
            dirty_region(recreated, dirty, canvas_width, canvas_height)
        {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels[..expected],
                wgpu::TexelCopyBufferLayout {
                    offset: ((y * canvas_width + x) * 4) as u64,
                    bytes_per_row: Some(canvas_width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.draw(paper)
    }

    /// The same, for premultiplied linear `f32` values in extended linear Display P3.
    pub fn present_linear(
        &mut self,
        pixels: &[f32],
        canvas_width: u32,
        canvas_height: u32,
        paper: [f32; 4],
        dirty: Option<(u32, u32, u32, u32)>,
    ) -> bool {
        let expected = canvas_width as usize * canvas_height as usize * 4;
        if !self.hdr || canvas_width == 0 || canvas_height == 0 || pixels.len() < expected {
            return false;
        }
        self.skip_texture = None;
        let recreated = self.prepare(canvas_width, canvas_height);
        let Some(texture) = self.texture.as_ref() else {
            return false;
        };
        if let Some((x, y, width, height)) =
            dirty_region(recreated, dirty, canvas_width, canvas_height)
        {
            let mut half = Vec::with_capacity(width as usize * height as usize * 8);
            for row in 0..height as usize {
                let start = ((y as usize + row) * canvas_width as usize + x as usize) * 4;
                for channel in &pixels[start..start + width as usize * 4] {
                    half.extend_from_slice(&f16::from_f32(*channel).to_bits().to_le_bytes());
                }
            }
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &half,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 8),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.draw(paper)
    }

    /// Allocate the painting texture for a new canvas size. `true` means every pixel must upload.
    fn prepare(&mut self, canvas_width: u32, canvas_height: u32) -> bool {
        self.canvas_size = (canvas_width, canvas_height);
        self.texture_region = (0, 0, canvas_width, canvas_height);
        self.allocate(canvas_width, canvas_height)
    }

    /// Point the paper shader at `region` of a `canvas`-sized document without allocating: a
    /// composited present samples the stack's own texture.
    fn set_region(&mut self, canvas: (u32, u32), region: (u32, u32, u32, u32)) {
        self.canvas_size = canvas;
        self.texture_region = region;
    }

    /// Allocate a painting texture that holds only `region` of a `canvas`-sized document. The
    /// live present composites the visible rectangle, so the texture neither grows with the
    /// canvas nor keeps the last pan's pixels.
    fn prepare_region(&mut self, canvas: (u32, u32), region: (u32, u32, u32, u32)) -> bool {
        self.set_region(canvas, region);
        self.allocate(region.2, region.3)
    }

    fn allocate(&mut self, canvas_width: u32, canvas_height: u32) -> bool {
        let recreated =
            self.texture_size != (canvas_width, canvas_height) || self.texture.is_none();
        if recreated {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("painting"),
                size: wgpu::Extent3d {
                    width: canvas_width,
                    height: canvas_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.painting_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.texture = Some(texture);
            self.texture_size = (canvas_width, canvas_height);
        }
        recreated
    }
}
