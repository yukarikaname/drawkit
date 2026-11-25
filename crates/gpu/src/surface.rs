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
mod draw;
mod native;
mod present;
mod shaders;
mod state;
mod stroke;
mod thumbnail;
mod upload;
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
    pipeline: wgpu::RenderPipeline,
    texture: Option<wgpu::Texture>,
    texture_size: (u32, u32),
    hdr: bool,
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
    /// Tiles read back by the last stroke commit, handed to the shell one at a time.
    committed: Vec<crate::layers::CommittedTile>,
    linear_committed: Vec<crate::layers::LinearTile>,
    journal: crate::layers::TileJournal,
    committed_rects: Vec<(u32, u32, u32, u32)>,
    /// Layer rectangles whose GPU pixels have not reached the CPU cache yet.
    stale: Vec<(usize, (u32, u32, u32, u32))>,
    layers: crate::LayerStack,
    layer_specs: Vec<crate::LayerSpec>,
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
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("drawkit canvas surface"),
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
        let spaces = capabilities.color_spaces(wgpu::TextureFormat::Rgba16Float);
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
        surface.configure(&device, &config);
        let pipeline = pipeline(&device, format, hdr);
        let composite_pipeline = crate::composite::pipeline(&device);
        let composite_hdr_pipeline =
            crate::composite::pipeline_for(&device, wgpu::TextureFormat::Rgba16Float);
        let tile_pipeline = crate::layers::commit_pipeline(&device);
        let tile_hdr_pipeline =
            crate::layers::commit_pipeline_for(&device, wgpu::TextureFormat::Rgba16Float);
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
            surface,
            device,
            queue,
            config,
            pipeline,
            texture: None,
            texture_size: (0, 0),
            hdr,
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
            committed: Vec::new(),
            linear_committed: Vec::new(),
            journal: crate::layers::TileJournal::new(512 << 20),
            committed_rects: Vec::new(),
            stale: Vec::new(),
            layers: crate::LayerStack::new(0, 0),
            layer_specs: Vec::new(),
        })
    }

    /// Rotate the presented paper around its centre. The canvas texture and the paper rectangle
    /// stay axis-aligned; only the shader swizzles the sample position.
    pub fn set_rotation(&mut self, radians: f32) {
        self.rotation = radians;
    }

    pub fn set_layer_spec(&mut self, index: usize, spec: crate::LayerSpec) {
        if self.layer_specs.get(index) == Some(&spec) {
            return;
        }
        if self.layer_specs.len() <= index {
            self.layer_specs
                .resize(index + 1, crate::LayerSpec::new(index));
        }
        self.layer_specs[index] = spec;
        self.layers.mark_all_dirty();
    }

    /// Set the group a resident layer belongs to, or `None` for the root.
    pub fn set_layer_parent(&mut self, index: usize, parent: Option<usize>) {
        if self.layer_specs.get(index).map(|spec| spec.parent) == Some(parent) {
            return;
        }
        if self.layer_specs.len() <= index {
            self.layer_specs
                .resize(index + 1, crate::LayerSpec::new(index));
        }
        self.layer_specs[index].parent = parent;
        self.layers.mark_all_dirty();
    }

    pub fn set_layer_clip(&mut self, index: usize, clip: Option<usize>) {
        if self.layer_specs.get(index).map(|spec| spec.clip) == Some(clip) {
            return;
        }
        if self.layer_specs.len() <= index {
            self.layer_specs
                .resize(index + 1, crate::LayerSpec::new(index));
        }
        self.layer_specs[index].clip = clip;
        self.layers.mark_all_dirty();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
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
