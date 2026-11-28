// The final draw: paper shader, rotation uniform and the render target.
//
// The swapchain path acquires its frame here. The dmabuf path imports a GBM buffer as a texture
// (surface/dmabuf.rs) and calls the same `draw_frame` with `imported = true`, which picks the
// pipeline built for that texture's `Bgra8Unorm` format instead of the window's.

use super::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    paper: [f32; 4],
    /// Where the painting texture's (0,0) sits in canvas pixels, then its size. A live present
    /// holds only the visible rectangle of the canvas, so sampling subtracts this origin.
    region: [f32; 4],
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
    _pad: [f32; 2],
}

impl CanvasSurface {
    pub(super) fn draw(&mut self, paper: [f32; 4]) -> bool {
        self.draw_from(paper, None)
    }

    /// The drawable rectangle a paper covers, clamped to the target. The surround is the clear
    /// colour, so only these pixels need a fragment; a 1024² sheet in a 1080x2400 window leaves
    /// most of the frame to the clear.
    fn paper_scissor(paper: [f32; 4], target: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
        let (width, height) = (target.0 as f32, target.1 as f32);
        if width <= 0.0 || height <= 0.0 || !paper.iter().all(|value| value.is_finite()) {
            return None;
        }
        let x0 = paper[0].floor().clamp(0.0, width) as u32;
        let y0 = paper[1].floor().clamp(0.0, height) as u32;
        let x1 = (paper[0] + paper[2]).ceil().clamp(0.0, width) as u32;
        let y1 = (paper[1] + paper[3]).ceil().clamp(0.0, height) as u32;
        (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
    }

    /// Present `paper`, sampling `source` instead of the painting texture when the stack has
    /// already composed exactly the visible rectangle.
    pub(super) fn draw_from(&mut self, paper: [f32; 4], source: Option<&wgpu::Texture>) -> bool {
        let acquire_started = std::time::Instant::now();
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
        let acquired_at = std::time::Instant::now();
        crate::surface::present::record_ms(3, acquire_started.elapsed().as_secs_f32() * 1000.0);
        let frame_view = frame.texture.create_view(&Default::default());
        let target = (self.config.width, self.config.height);
        let drawn = self.draw_frame(&frame_view, paper, false, source, target);
        crate::surface::present::record_ms(4, acquired_at.elapsed().as_secs_f32() * 1000.0);
        if !drawn {
            return false;
        }
        if crate::surface::present::trace_on() {
            eprintln!(
                "DRAWKIT_GPU_DRAW acquire {:.2} ms, encode {:.2} ms",
                (acquired_at - acquire_started).as_secs_f64() * 1000.0,
                (std::time::Instant::now() - acquired_at).as_secs_f64() * 1000.0
            );
        }
        self.queue.present(frame);
        true
    }

    /// Encode and submit the paper pass into `target`. A live present hands over the stack's
    /// composite so the frame does not pay for a copy into a second texture.
    pub(super) fn draw_frame(
        &mut self,
        target: &wgpu::TextureView,
        paper: [f32; 4],
        imported: bool,
        source: Option<&wgpu::Texture>,
        target_size: (u32, u32),
    ) -> bool {
        // A stroke that names its layer is composited into that layer's tile, so the frame already
        // holds it and the shader must not mix it over the top a second time. A shell that passes
        // no layer (or a stroke the stack cannot address) keeps the over-the-frame preview.
        let preview = self.preview_texture();
        let layered = preview.is_some();
        let Some(texture) = preview.as_ref().or(source).or(self.texture.as_ref()) else {
            return false;
        };
        let (canvas_width, canvas_height) = self.canvas_size;
        let region = self.texture_region;
        let stage = std::time::Instant::now();
        let view = texture.create_view(&Default::default());
        let skip_view = self.skip_texture.as_ref().map_or_else(
            || texture.create_view(&Default::default()),
            |skip| skip.create_view(&Default::default()),
        );
        crate::surface::present::record_ms(5, stage.elapsed().as_secs_f32() * 1000.0);
        let params = Params {
            paper,
            region: [
                region.0 as f32,
                region.1 as f32,
                region.2.max(1) as f32,
                region.3.max(1) as f32,
            ],
            canvas: [canvas_width as f32, canvas_height as f32],
            exposure: if self.hdr { self.exposure } else { 0.0 },
            angle: self.rotation,
            mode: [self.color_mode, 0.0, 0.0, 0.0],
            preview: if layered {
                [0.0; 4]
            } else {
                self.stroke
                    .color()
                    .map_or([0.0; 4], |color| [1.0, color[0], color[1], color[2]])
            },
            preview_wet: if layered { 0.0 } else { self.stroke.wet() },
            preview_erase: if layered || !self.stroke.is_erase() {
                0.0
            } else {
                1.0
            },
            _pad: [0.0; 2],
        };
        let buffered = std::time::Instant::now();
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("canvas surface params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        // A pipeline handle is a refcounted wgpu resource; cloning it ends the borrow of
        // `self` that `paper_pipeline` needs.
        let pipeline = self.paper_pipeline(imported).clone();
        let layout = pipeline.get_bind_group_layout(0);
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
        crate::surface::present::record_ms(6, buffered.elapsed().as_secs_f32() * 1000.0);
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
            [encode(0.0069), encode(0.0091), encode(0.0116)]
        } else if self.hdr {
            if self.color_mode < 0.5 {
                // ExtendedDisplayP3 is P3 primaries with the extended sRGB transfer, so the clear
                // is the encoded grey, not its linear value.
                [0.078, 0.090, 0.102]
            } else {
                // ExtendedSrgbLinear takes linear light.
                [0.0069, 0.0091, 0.0116]
            }
        } else {
            [0.078, 0.090, 0.102]
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas surface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
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
            if let Some((x, y, width, height)) = Self::paper_scissor(paper, target_size) {
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.set_scissor_rect(x, y, width, height);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        true
    }

    /// The window's pipeline, or the lazily built one for an imported `Bgra8Unorm` buffer.
    /// The paper pass. On Linux the surface's own format is the shell's dma-buf format — eight-bit
    /// Bgra or, for an HDR document, 10-bit PQ — so one pipeline covers both; elsewhere the window
    /// surface has the same format the pipeline was built for.
    fn paper_pipeline(&mut self, imported: bool) -> &wgpu::RenderPipeline {
        let _ = imported;
        &self.pipeline
    }
}
