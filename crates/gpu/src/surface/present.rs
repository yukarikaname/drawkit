//! Compositing the resident layer stack and drawing it into the swapchain.

use super::*;

/// `DRAWKIT_PRESENT_TRACE=1` prints where a present spends its time. A pen sample pays the whole
/// present cost, so a regression here is felt as tablet lag.
pub(super) fn trace_on() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var("DRAWKIT_PRESENT_TRACE").is_ok())
}

impl CanvasSurface {
    /// Map the paper rectangle back to canvas pixels so eviction keeps what is on screen.
    fn update_residency(&mut self, paper: [f32; 4]) {
        let (width, height) = self.layers.size();
        let drawable = (self.config.width as f32, self.config.height as f32);
        let (px, py, pw, ph) = (paper[0], paper[1], paper[2], paper[3]);
        if pw <= 0.0 || ph <= 0.0 || width == 0 || height == 0 {
            return;
        }
        let x0 = px.max(0.0);
        let y0 = py.max(0.0);
        let x1 = (px + pw).min(drawable.0);
        let y1 = (py + ph).min(drawable.1);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let scale_x = width as f32 / pw;
        let scale_y = height as f32 / ph;
        let view = (
            (x0 - px) * scale_x,
            (y0 - py) * scale_y,
            (x1 - x0) * scale_x,
            (y1 - y0) * scale_y,
        );
        self.layers.set_view(view, pw / width as f32);
    }

    /// Composite the resident layers on the GPU and draw the result. No readback happens; the
    /// finished texture is copied into the painting texture and sampled by the paper shader.
    /// 1 means drawn, 2 means composited but the swapchain was not available (an occluded
    /// window), 0 means the layer path could not run and the shell should use its CPU pixels.
    pub fn present_layers(&mut self, paper: [f32; 4]) -> i32 {
        let (width, height) = self.layers.size();
        if width == 0 || height == 0 {
            return 0;
        }
        self.update_residency(paper);
        let pipeline = if self.layers.format() == crate::FrameFormat::Rgba8 {
            &self.composite_pipeline
        } else {
            &self.composite_hdr_pipeline
        };
        let started = std::time::Instant::now();
        let composited =
            self.layers
                .composite_texture(&self.device, &self.queue, pipeline, &self.layer_specs);
        let composed_at = std::time::Instant::now();
        self.skip_texture = None;
        if self.stroke.is_erase()
            && let Some(layer) = self.stroke.preview_layer()
        {
            self.skip_texture = self.layers.composite_skip(
                &self.device,
                &self.queue,
                pipeline,
                &self.layer_specs,
                layer,
            );
        }
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
        let copied_at = std::time::Instant::now();
        let drawn = self.draw(paper);
        if trace_on() {
            let done = std::time::Instant::now();
            eprintln!(
                "DRAWKIT_GPU_PRESENT composite {:.2} ms, copy {:.2} ms, draw {:.2} ms, canvas {}x{}",
                (composed_at - started).as_secs_f64() * 1000.0,
                (copied_at - composed_at).as_secs_f64() * 1000.0,
                (done - copied_at).as_secs_f64() * 1000.0,
                width,
                height
            );
        }
        if drawn { 1 } else { 2 }
    }
}

impl CanvasSurface {
    /// A replay compositor on this surface's device, so a scrubbed frame can be presented without
    /// a readback or an upload.
    pub fn new_replay(&self, width: u32, height: u32) -> Option<crate::GpuCompositor> {
        let format = if self.painting_format == wgpu::TextureFormat::Rgba16Float {
            crate::FrameFormat::Rgba16
        } else {
            crate::FrameFormat::Rgba8
        };
        crate::GpuCompositor::new_with_device(
            self.device.clone(),
            self.queue.clone(),
            width,
            height,
            format,
        )
    }

    /// Copy a replay compositor's finished frame into the painting texture and draw it.
    pub fn present_replay(
        &mut self,
        compositor: &mut crate::GpuCompositor,
        paper: [f32; 4],
    ) -> i32 {
        let Some(source) = compositor.output_texture() else {
            return 0;
        };
        let (width, height) = compositor.size();
        if width == 0 || height == 0 {
            return 0;
        }
        self.prepare(width, height);
        let Some(texture) = self.texture.as_ref() else {
            return 0;
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source,
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
        self.queue.submit(Some(encoder.finish()));
        self.skip_texture = None;
        if self.draw(paper) { 1 } else { 2 }
    }
}
