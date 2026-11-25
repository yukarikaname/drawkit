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
mod native;
mod shaders;
mod state;

use shaders::dirty_region;
use shaders::pipeline;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    paper: [f32; 4],
    canvas: [f32; 2],
    /// Stops of display exposure for the extended-range path; zero for eight-bit.
    exposure: f32,
    padding: f32,
}

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
    composite_pipeline: wgpu::RenderPipeline,
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
        // other machine the same painting falls back to the eight-bit path rather than failing.
        let hdr = hdr
            && capabilities
                .formats
                .contains(&wgpu::TextureFormat::Rgba16Float);
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
        config.color_space = if hdr || color_space == 2 {
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
            composite_pipeline,
            layers: crate::LayerStack::new(0, 0),
            layer_specs: Vec::new(),
        })
    }

    /// True when the surface really took the extended-range format.
    pub fn upload_layer_rect(
        &mut self,
        index: usize,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> bool {
        self.layers.update_rect(
            &self.device,
            &self.queue,
            crate::layers::RectUpload {
                index,
                x,
                y,
                width,
                height,
                pixels,
            },
        )
    }

    pub fn set_layer_spec(&mut self, index: usize, spec: crate::LayerSpec) {
        if self.layer_specs.len() <= index {
            self.layer_specs
                .resize(index + 1, crate::LayerSpec::new(index));
        }
        self.layer_specs[index] = spec;
        self.layers.mark_all_dirty();
    }

    /// Composite the resident layers on the GPU and draw the result. No readback happens; the
    /// finished texture is copied into the painting texture and sampled by the paper shader.
    /// 1 means drawn, 2 means composited but the swapchain was not available (an occluded
    /// window), 0 means the layer path could not run and the shell should use its CPU pixels.
    pub fn present_layers(&mut self, paper: [f32; 4]) -> i32 {
        if self.hdr {
            return 0;
        }
        let (width, height) = self.layers.size();
        if width == 0 || height == 0 {
            return 0;
        }
        let composited = self.layers.composite_texture(
            &self.device,
            &self.queue,
            &self.composite_pipeline,
            &self.layer_specs,
        );
        self.prepare(width, height);
        let Some(texture) = self.texture.as_ref() else {
            return 0;
        };
        let view = texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        match &composited {
            Some(source) => {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: source,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            None => {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("canvas clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
        }
        self.queue.submit(Some(encoder.finish()));
        drop(composited);
        if self.draw(paper) { 1 } else { 2 }
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

    fn draw(&mut self, paper: [f32; 4]) -> bool {
        let Some(texture) = self.texture.as_ref() else {
            return false;
        };
        let (canvas_width, canvas_height) = self.texture_size;
        let view = texture.create_view(&Default::default());
        let params = Params {
            paper,
            canvas: [canvas_width as f32, canvas_height as f32],
            exposure: if self.hdr { self.exposure } else { 0.0 },
            padding: 0.0,
        };
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("canvas surface params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let layout = self.pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas surface"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.config);
                frame
            }
            status => {
                // An occluded window is a normal state (another Space, a hidden window), not a
                // surface failure; present_layers already reports it as composed-but-not-shown.
                if !matches!(status, wgpu::CurrentSurfaceTexture::Occluded) {
                    eprintln!("canvas surface: acquire failed: {status:?}");
                }
                if matches!(status, wgpu::CurrentSurfaceTexture::Outdated) {
                    self.surface.configure(&self.device, &self.config);
                }
                return false;
            }
        };
        let frame_view = frame.texture.create_view(&Default::default());
        // The surround is authored in encoded values; linear light needs the decoded number or
        // the extended-range frame shows a different grey than the standard one.
        let surround = if self.hdr {
            [0.012, 0.011, 0.010]
        } else {
            [0.106, 0.098, 0.090]
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas surface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: surround[0],
                            g: surround[1],
                            b: surround[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        true
    }
}
