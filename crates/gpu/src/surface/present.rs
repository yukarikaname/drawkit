//! Compositing the resident layer stack and drawing it into the swapchain.

use super::*;

impl CanvasSurface {
    /// Composite the resident layers on the GPU and draw the result. No readback happens; the
    /// finished texture is copied into the painting texture and sampled by the paper shader.
    /// 1 means drawn, 2 means composited but the swapchain was not available (an occluded
    /// window), 0 means the layer path could not run and the shell should use its CPU pixels.
    pub fn present_layers(&mut self, paper: [f32; 4]) -> i32 {
        let (width, height) = self.layers.size();
        if width == 0 || height == 0 {
            return 0;
        }
        let pipeline = if self.layers.format() == crate::FrameFormat::Rgba8 {
            &self.composite_pipeline
        } else {
            &self.composite_hdr_pipeline
        };
        let composited =
            self.layers
                .composite_texture(&self.device, &self.queue, pipeline, &self.layer_specs);
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
        if self.draw(paper) { 1 } else { 2 }
    }
}
