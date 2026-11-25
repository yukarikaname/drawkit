// The final draw: paper shader, rotation uniform and swapchain acquisition.

use super::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    paper: [f32; 4],
    canvas: [f32; 2],
    /// Stops of display exposure for the extended-range path; zero for eight-bit.
    exposure: f32,
    /// View rotation in radians, clockwise in drawable pixels.
    angle: f32,
    /// x: display mode, 0 linear Display P3, 1 linear extended sRGB, 2 BT.2100 PQ.
    mode: [f32; 4],
    /// x: preview enabled, yzw: preview colour.
    preview: [f32; 4],
    /// How much of the paint under the stroke the preview colour picks up.
    preview_wet: f32,
    preview_erase: f32,
    /// Round the struct to the shader's 16-byte alignment.
    _pad: [f32; 2],
}

impl CanvasSurface {
    pub(super) fn draw(&mut self, paper: [f32; 4]) -> bool {
        let Some(texture) = self.texture.clone() else {
            return false;
        };
        self.draw_from(&texture, paper, self.texture_size)
    }

    /// Draw `texture` as the painting. A resident composite is drawn directly, without copying the
    /// full canvas into another texture first.
    pub(super) fn draw_from(
        &mut self,
        texture: &wgpu::Texture,
        paper: [f32; 4],
        size: (u32, u32),
    ) -> bool {
        let (canvas_width, canvas_height) = size;
        let view = texture.create_view(&Default::default());
        let skip_view = self.skip_texture.as_ref().map_or_else(
            || texture.create_view(&Default::default()),
            |skip| skip.create_view(&Default::default()),
        );
        let params = Params {
            paper,
            canvas: [canvas_width as f32, canvas_height as f32],
            exposure: if self.hdr { self.exposure } else { 0.0 },
            angle: self.rotation,
            mode: [self.color_mode, 0.0, 0.0, 0.0],
            preview: self
                .stroke
                .color()
                .map_or([0.0; 4], |color| [1.0, color[0], color[1], color[2]]),
            preview_wet: self.stroke.wet(),
            preview_erase: if self.stroke.is_erase() { 1.0 } else { 0.0 },
            _pad: [0.0; 2],
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self
                        .stroke
                        .coverage()
                        .map_or(self.stroke_dummy.as_entire_binding(), |buffer| {
                            buffer.as_entire_binding()
                        }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&skip_view),
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
        let surround = if self.color_mode >= 1.5 {
            // BT.2100 PQ: the chrome is authored in SDR light and encoded to the same signal.
            let encode = |value: f32| {
                let nits = value * 203.0;
                let m1 = 0.159_301_76_f32;
                let m2 = 78.843_75_f32;
                let c1 = 0.835_937_5_f32;
                let c2 = 18.851_563_f32;
                let c3 = 18.687_5_f32;
                let y = (nits / 10_000.0).max(0.0).powf(m1);
                ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
            };
            [encode(0.012), encode(0.011), encode(0.010)]
        } else if self.hdr {
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
                            r: surround[0] as f64,
                            g: surround[1] as f64,
                            b: surround[2] as f64,
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
        self.queue.present(frame);
        true
    }
}
