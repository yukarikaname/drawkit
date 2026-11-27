//! Importing a GBM buffer as the canvas texture: the Linux zero-copy present.
//!
//! The shell owns the buffer (`app-linux/src/dmabuf.rs`): it allocates a `gbm_bo`, renders the
//! resident stack into it through `present_dmabuf`, and gives the same buffer to GTK as a
//! `GdkDmabufTexture`. Nothing is read back and no window surface is involved.
//!
//! The import is `wgpu-hal`'s Vulkan `texture_from_dmabuf_fd`, so it works on the device the shell
//! already has, without patching wgpu. It needs `VK_EXT_external_memory_dma_buf` and
//! `VK_EXT_image_drm_format_modifier` on the adapter; `surface.rs` enables that device feature when
//! the adapter has it. `docs/linux-wayland.md` records the route and the fallbacks it replaces.

use std::os::fd::OwnedFd;

use super::*;

/// The frame was drawn into the imported buffer.
pub const DMABUF_DRAWN: i32 = 1;
/// The resident stack is empty, so there was nothing to composite.
pub const DMABUF_NO_LAYERS: i32 = 0;
/// The fd or the buffer geometry is not a single-plane `Bgra8Unorm` canvas.
pub const DMABUF_BAD_ARGUMENT: i32 = -1;
/// The wgpu device is not Vulkan (for example the GL backend picked the adapter).
pub const DMABUF_NOT_VULKAN: i32 = -2;
/// The device was created without `VULKAN_EXTERNAL_MEMORY_DMA_BUF`.
pub const DMABUF_NO_FEATURE: i32 = -3;
/// The driver refused the fd, the stride or the DRM modifier.
pub const DMABUF_IMPORT_FAILED: i32 = -4;
/// The import worked but the paper pass could not be encoded.
pub const DMABUF_DRAW_FAILED: i32 = -5;
/// The buffer was never imported, so there is nothing to render into.
pub const DMABUF_NOT_IMPORTED: i32 = -6;

/// One imported buffer: the wgpu texture over the shell's GBM memory, and the geometry it was
/// imported with.
pub struct Imported {
    pub width: u32,
    pub height: u32,
    pub texture: wgpu::Texture,
}

impl CanvasSurface {
    /// Import `fd` as buffer `index` and remember the texture, so the frames that follow do not
    /// pay for a Vulkan image and memory import again. The fd is owned by this call (Vulkan takes
    /// it, and closes it on failure). The buffer must be `DRM_FORMAT_XRGB8888`.
    #[allow(clippy::too_many_arguments)]
    pub fn import_dmabuf(
        &mut self,
        index: u32,
        fd: OwnedFd,
        width: u32,
        height: u32,
        stride: u32,
        offset: u64,
        modifier: u64,
    ) -> i32 {
        if width == 0 || height == 0 || stride < width * 4 || self.hdr {
            return DMABUF_BAD_ARGUMENT;
        }
        let (canvas_width, canvas_height) = self.layers.size();
        if canvas_width == 0 || canvas_height == 0 {
            return DMABUF_NO_LAYERS;
        }
        if !self
            .device
            .features()
            .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF)
        {
            eprintln!("dmabuf: VK_EXT_external_memory_dma_buf was not enabled on the device");
            return DMABUF_NO_FEATURE;
        }
        let Some(hal_device) = (unsafe { self.device.as_hal::<wgpu::hal::api::Vulkan>() }) else {
            eprintln!("dmabuf: the device is not the Vulkan backend");
            return DMABUF_NOT_VULKAN;
        };
        let hal_desc = wgpu::hal::TextureDescriptor {
            label: Some("dmabuf canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUses::COLOR_TARGET,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: Vec::new(),
        };
        // SAFETY: the fd is a live single-plane dmabuf with this exact geometry, and the device
        // enabled the feature above. Vulkan owns the fd from here on.
        let hal_texture = match unsafe {
            hal_device.texture_from_dmabuf_fd(fd, &hal_desc, modifier, u64::from(stride), offset)
        } {
            Ok(texture) => texture,
            Err(error) => {
                eprintln!("dmabuf: import failed: {error:?}");
                return DMABUF_IMPORT_FAILED;
            }
        };
        drop(hal_device);
        let desc = wgpu::TextureDescriptor {
            label: Some("dmabuf canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };
        // SAFETY: the hal texture was created by this device with the descriptor above.
        let texture = unsafe {
            self.device
                .create_texture_from_hal::<wgpu::hal::api::Vulkan>(
                    hal_texture,
                    &desc,
                    wgpu::TextureUses::UNINITIALIZED,
                )
        };
        self.dmabuf_imports.insert(
            index,
            Imported {
                width,
                height,
                texture,
            },
        );
        DMABUF_DRAWN
    }

    /// Render the canvas into buffer `index` and hand the frame to GTK. The texture was imported
    /// once; this is the composite, the paper pass and a submit.
    pub fn present_dmabuf(&mut self, index: u32, paper: [f32; 4]) -> i32 {
        let Some(imported) = self.dmabuf_imports.get(&index) else {
            return DMABUF_NOT_IMPORTED;
        };
        let (width, height) = (imported.width, imported.height);
        let texture = imported.texture.clone();
        self.composite_full_canvas();
        let view = texture.create_view(&Default::default());
        if !self.draw_frame(&view, paper, true) {
            return DMABUF_DRAW_FAILED;
        }
        let _ = (width, height);
        DMABUF_DRAWN
    }

    /// Composite the whole canvas into the painting texture. The dmabuf path has no visible-rect
    /// bookkeeping: the buffer is the whole drawable, so every canvas tile may be sampled.
    fn composite_full_canvas(&mut self) {
        let (canvas_width, canvas_height) = self.layers.size();
        let region = (0, 0, canvas_width, canvas_height);
        self.layers.set_view(
            (
                region.0 as f32,
                region.1 as f32,
                region.2 as f32,
                region.3 as f32,
            ),
            1.0,
        );
        let pipeline = if self.layers.format() == crate::FrameFormat::Rgba8 {
            &self.composite_pipeline
        } else {
            &self.composite_hdr_pipeline
        };
        let composited = self.layers.composite_region(
            &self.device,
            &self.queue,
            pipeline,
            &self.layer_specs,
            region,
        );
        self.skip_texture = None;
        self.prepare_region((canvas_width, canvas_height), region);
        let Some(painting) = self.texture.as_ref() else {
            return;
        };
        let view = painting.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        match &composited {
            Some(source) => encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: source,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: painting,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: region.2,
                    height: region.3,
                    depth_or_array_layers: 1,
                },
            ),
            None => {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("dmabuf clear"),
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
    }
}

/// A headless proof that the import path works on a machine with a render node. It is `#[ignore]`d
/// because CI has no DRM device; run it on the target:
///
/// ```sh
/// cargo test -p drawkit-gpu --lib -- --ignored a_gbm_buffer
/// ```
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::ffi::c_void;
    use std::fs::OpenOptions;
    use std::os::fd::IntoRawFd;

    const GBM_FORMAT_XRGB8888: u32 = 0x3432_5258;
    const GBM_BO_USE_RENDERING: u32 = 4;

    #[link(name = "gbm")]
    unsafe extern "C" {
        fn gbm_create_device(fd: i32) -> *mut c_void;
        fn gbm_device_destroy(device: *mut c_void);
        fn gbm_bo_create(
            device: *mut c_void,
            width: u32,
            height: u32,
            format: u32,
            flags: u32,
        ) -> *mut c_void;
        fn gbm_bo_destroy(bo: *mut c_void);
        fn gbm_bo_get_fd(bo: *mut c_void) -> i32;
        fn gbm_bo_get_stride(bo: *mut c_void) -> u32;
        fn gbm_bo_get_offset(bo: *mut c_void, plane: i32) -> u32;
        fn gbm_bo_get_modifier(bo: *mut c_void) -> u64;
    }

    fn render_node() -> Option<std::fs::File> {
        for index in 128..136 {
            let path = format!("/dev/dri/renderD{index}");
            if let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) {
                return Some(file);
            }
        }
        None
    }

    #[test]
    #[ignore = "needs a DRM render node and a Vulkan device with dmabuf import"]
    fn a_gbm_buffer_imports_renders_and_reads_back() {
        use std::os::fd::FromRawFd;

        let Some(file) = render_node() else {
            eprintln!("dmabuf test: no /dev/dri/renderD* node");
            return;
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("no Vulkan adapter");
        let features = adapter.features();
        if !features.contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF) {
            eprintln!("dmabuf test: the adapter lacks VK_EXT_external_memory_dma_buf");
            return;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("dmabuf test"),
            required_features: wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("no device");

        let gbm = unsafe { gbm_create_device(file.as_raw_fd()) };
        assert!(!gbm.is_null(), "gbm_create_device failed");
        let (width, height) = (64u32, 64u32);
        let bo = unsafe {
            gbm_bo_create(
                gbm,
                width,
                height,
                GBM_FORMAT_XRGB8888,
                GBM_BO_USE_RENDERING,
            )
        };
        assert!(!bo.is_null(), "gbm_bo_create failed");
        let fd = unsafe { gbm_bo_get_fd(bo) };
        assert!(fd >= 0, "gbm_bo_get_fd failed");
        let stride = unsafe { gbm_bo_get_stride(bo) };
        let offset = u64::from(unsafe { gbm_bo_get_offset(bo, 0) });
        let modifier = unsafe { gbm_bo_get_modifier(bo) };

        let hal_desc = wgpu::hal::TextureDescriptor {
            label: Some("dmabuf test"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUses::COLOR_TARGET | wgpu::TextureUses::COPY_SRC,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: Vec::new(),
        };
        let hal_device = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }.expect("Vulkan");
        let hal_texture = unsafe {
            hal_device.texture_from_dmabuf_fd(
                unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) },
                &hal_desc,
                modifier,
                u64::from(stride),
                offset,
            )
        }
        .expect("import");
        drop(hal_device);
        let desc = wgpu::TextureDescriptor {
            label: Some("dmabuf test"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        let texture = unsafe {
            device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(
                hal_texture,
                &desc,
                wgpu::TextureUses::UNINITIALIZED,
            )
        };
        let view = texture.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(width * height * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
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
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::wgt::PollType::Wait {
                submission_index: None,
            })
            .expect("poll");
        let data = slice.get_mapped_range();
        // Bgra8Unorm: the red clear lands in byte 2.
        assert!(
            data[0..4] == [0, 0, 255, 255],
            "the imported buffer did not take the clear: {:?}",
            &data[0..4]
        );
        drop(data);
        readback.unmap();
        unsafe {
            gbm_bo_destroy(bo);
            gbm_device_destroy(gbm);
        }
    }
}
