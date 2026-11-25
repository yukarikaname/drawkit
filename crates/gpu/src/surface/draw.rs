// The final draw: paper shader, rotation uniform and swapchain acquisition.

use super::*;

impl CanvasSurface {
    pub(super) fn draw(&mut self, paper: [f32; 4]) -> bool {
        let Some(texture) = self.texture.as_ref() else {
            return false;
        };
        let (canvas_width, canvas_height) = self.texture_size;
        let view = texture.create_view(&Default::default());
        let params = Params {
            paper,
            canvas: [canvas_width as f32, canvas_height as f32],
            exposure: if self.hdr { self.exposure } else { 0.0 },
            angle: self.rotation,
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
