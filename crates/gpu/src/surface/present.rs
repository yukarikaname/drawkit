//! Compositing the resident layer stack and drawing it into the swapchain.

use super::*;

#[cfg(test)]
mod tests;

/// `DRAWKIT_PRESENT_TRACE=1` prints where a present spends its time. A pen sample pays the whole
/// present cost, so a regression here is felt as tablet lag.
pub(super) fn trace_on() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var("DRAWKIT_PRESENT_TRACE").is_ok())
}

/// The canvas rectangle the drawable shows, clamped to the canvas.
///
/// `paper` is the axis-aligned bounds of the sheet in drawable pixels and the shader rotates a
/// fragment back into sheet space before sampling it. The same transform maps the four drawable
/// corners into canvas pixels, and the result is their bounding box. Mapping the drawable rather
/// than the paper is what makes the surround outside the sheet clamp away instead of forcing a
/// full-canvas composite: a corner outside the paper lands outside the canvas and is clipped.
fn visible_rect(
    paper: [f32; 4],
    drawable: (u32, u32),
    canvas: (u32, u32),
    angle: f32,
) -> (u32, u32, u32, u32) {
    let (canvas_width, canvas_height) = canvas;
    if canvas_width == 0 || canvas_height == 0 {
        return (0, 0, 0, 0);
    }
    let (px, py, pw, ph) = (paper[0], paper[1], paper[2], paper[3]);
    if pw <= 0.0
        || ph <= 0.0
        || !px.is_finite()
        || !py.is_finite()
        || !pw.is_finite()
        || !ph.is_finite()
    {
        return (0, 0, canvas_width, canvas_height);
    }
    let angle = if angle.is_finite() { angle } else { 0.0 };
    let (centre_x, centre_y) = (px + pw * 0.5, py + ph * 0.5);
    let (drawable_width, drawable_height) = (drawable.0 as f32, drawable.1 as f32);
    let (cos, sin) = (angle.cos(), angle.sin());
    let mut min = (f32::INFINITY, f32::INFINITY);
    let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (x, y) in [
        (0.0, 0.0),
        (drawable_width, 0.0),
        (0.0, drawable_height),
        (drawable_width, drawable_height),
    ] {
        let ox = x - centre_x;
        let oy = y - centre_y;
        // Mirrors `paper_uv` in surface.wgsl: rotate the point back into sheet space.
        let local_x = ox * cos - oy * sin;
        let local_y = ox * sin + oy * cos;
        let canvas_x = (local_x / pw + 0.5) * canvas_width as f32;
        let canvas_y = (local_y / ph + 0.5) * canvas_height as f32;
        min = (min.0.min(canvas_x), min.1.min(canvas_y));
        max = (max.0.max(canvas_x), max.1.max(canvas_y));
    }
    let mut x0 = min.0.floor().clamp(0.0, canvas_width as f32) as u32;
    let mut y0 = min.1.floor().clamp(0.0, canvas_height as f32) as u32;
    let mut x1 = max.0.ceil().clamp(0.0, canvas_width as f32) as u32;
    let mut y1 = max.1.ceil().clamp(0.0, canvas_height as f32) as u32;
    // A paper entirely off screen still needs a texture to sample; one pixel costs nothing and
    // every fragment of the drawable is outside the sheet, so it is never read.
    if x1 <= x0 {
        x0 = x0.min(canvas_width - 1);
        x1 = x0 + 1;
    }
    if y1 <= y0 {
        y0 = y0.min(canvas_height - 1);
        y1 = y0 + 1;
    }
    (x0, y0, x1 - x0, y1 - y0)
}

impl CanvasSurface {
    /// Map the paper rectangle back to canvas pixels and tell eviction what is on screen. Returns
    /// the same rectangle for the present to composite.
    fn update_residency(&mut self, paper: [f32; 4]) -> (u32, u32, u32, u32) {
        let canvas = self.layers.size();
        let drawable = (self.config.width, self.config.height);
        let region = visible_rect(paper, drawable, canvas, self.rotation);
        // Device pixels per canvas pixel, the unit `set_view` wants for its eviction ring. Under
        // rotation `paper` is the turned sheet's bounds, so this is an estimate; it only shapes
        // the preload ring, not the pixels.
        let zoom = if paper[2] > 0.0 && canvas.0 > 0 {
            paper[2] / canvas.0 as f32
        } else {
            1.0
        };
        self.layers.set_view(
            (
                region.0 as f32,
                region.1 as f32,
                region.2 as f32,
                region.3 as f32,
            ),
            zoom,
        );
        region
    }

    /// Composite the visible part of the resident layers and draw it. No readback happens; the
    /// finished rectangle is copied into the painting texture, which is sized to that rectangle
    /// rather than to the canvas, and sampled by the paper shader.
    /// 1 means drawn, 2 means composited but the swapchain was not available (an occluded
    /// window), 0 means the layer path could not run and the shell should use its CPU pixels.
    pub fn present_layers(&mut self, paper: [f32; 4]) -> i32 {
        let (width, height) = self.layers.size();
        if width == 0 || height == 0 {
            return 0;
        }
        let region = self.update_residency(paper);
        let pipeline = if self.layers.format() == crate::FrameFormat::Rgba8 {
            &self.composite_pipeline
        } else {
            &self.composite_hdr_pipeline
        };
        let started = std::time::Instant::now();
        let composited = self.layers.composite_region(
            &self.device,
            &self.queue,
            pipeline,
            &self.layer_specs,
            region,
        );
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
                region,
            );
        }
        self.prepare_region((width, height), region);
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
                        width: region.2,
                        height: region.3,
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
                "DRAWKIT_GPU_PRESENT composite {:.2} ms, copy {:.2} ms, draw {:.2} ms, canvas {}x{} shown {}x{} at {},{}",
                (composed_at - started).as_secs_f64() * 1000.0,
                (copied_at - composed_at).as_secs_f64() * 1000.0,
                (done - copied_at).as_secs_f64() * 1000.0,
                width,
                height,
                region.2,
                region.3,
                region.0,
                region.1
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
