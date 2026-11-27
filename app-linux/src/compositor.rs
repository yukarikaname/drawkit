// The painting document behind the GTK canvas. GTK never owns pixels: the core Editor keeps the
// history and the CPU cache, the Rust surface keeps the resident layers and the live stroke on the
// GPU, and the GBM buffer in `dmabuf.rs` is what the compositor scans out.
//
// There is one presentation path on Linux: render into the GBM buffer and hand it to GTK as a
// dmabuf texture. If any piece is missing (no Wayland, no Vulkan, no importable buffer) the canvas
// stays empty and the status bar says why.

use std::path::PathBuf;

use drawkit_core::color::Rgba;
use drawkit_core::fill::FillOptions;
use drawkit_core::surface::Rect;
use drawkit_core::tools::Editor;

use crate::canvas::Tool;
use crate::paint::{PaintSurface, StrokeStart, Target};

pub(crate) struct State {
    pub(crate) editor: Editor,
    /// The surface that owns the resident layers, the stroke preview and the paper pass.
    pub(crate) paint: Option<PaintSurface>,
    pub(crate) gpu_ready: bool,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) size: f64,
    pub(crate) opacity: f64,
    pub(crate) colour: Rgba,
    pub(crate) tool: Tool,
    pub(crate) last: Option<(f32, f32)>,
    pub(crate) time: u64,
    /// The one honest sentence about why the canvas is not showing, if it is not.
    pub(crate) error: Option<String>,
    /// The resident stack's count and canvas size, so a structural change knows when to rebuild.
    pub(crate) surface_layers: usize,
    pub(crate) surface_size: (u32, u32),
    gpu_stroke: bool,
    pub(crate) gpu_stale: bool,
    pub(crate) file: Option<PathBuf>,
}

impl State {
    pub(crate) fn new() -> Self {
        let width = 1024;
        let height = 1024;
        Self {
            editor: Editor::new(width as usize, height as usize, 512 << 20),
            paint: None,
            gpu_ready: false,
            width,
            height,
            size: 18.0,
            opacity: 1.0,
            colour: [0.10, 0.09, 0.08, 1.0],
            tool: Tool::Brush,
            last: None,
            time: 0,
            error: Some("waiting for a window".to_string()),
            surface_layers: 0,
            surface_size: (0, 0),
            gpu_stroke: false,
            gpu_stale: false,
            file: None,
        }
    }

    /// Take the surface of a realized window. The surface is only a device/adapter handle here;
    /// the pixels leave through the dmabuf buffer.
    pub(crate) fn attach(&mut self, target: &Target) {
        match PaintSurface::new(target, self.width, self.height) {
            Ok(paint) => {
                self.paint = Some(paint);
                self.surface_layers = 0;
                self.surface_size = (0, 0);
                self.gpu_ready = self.sync_gpu();
                if self.gpu_ready {
                    self.error = None;
                } else {
                    self.error = Some("the layer stack cannot represent this document".to_string());
                }
            }
            Err(reason) => self.error = Some(reason),
        }
    }

    fn fail(&mut self, reason: &str) {
        self.error = Some(reason.to_string());
    }

    fn ink(&self) -> [f32; 4] {
        [
            self.colour[0],
            self.colour[1],
            self.colour[2],
            self.colour[3] * self.opacity as f32,
        ]
    }

    /// Start a stroke. A brush on a ready surface stays there: the core only records the events
    /// and the coverage accumulates in the preview.
    pub(crate) fn begin_stroke(&mut self, point: (f32, f32)) {
        let eligible = self.gpu_ready && self.tool == Tool::Brush;
        if eligible {
            self.editor.defer_raster = true;
        } else {
            self.materialize();
        }
        let time = self.time;
        self.editor.begin_stroke(time);
        if eligible {
            let (width, height, ink) = (self.width, self.height, self.ink());
            let map = self.editor.brush_texture.clone();
            let brush = self.editor.brush;
            let secondary = drawkit_core::brush::Brush::for_kind(
                drawkit_core::brush::BrushKind::from_i32(brush.dual_kind as i32),
            );
            let seed = self.editor.stroke_seed().unwrap_or(0);
            let Some(paint) = self.paint.as_mut() else {
                return;
            };
            match map {
                Some(texture) => paint.set_brush_texture(
                    texture.width as u32,
                    texture.height as u32,
                    &texture.data,
                ),
                None => paint.clear_brush_texture(),
            }
            paint.stroke_begin(&StrokeStart {
                width,
                height,
                colour: ink,
                flow: brush.flow,
                pressure_alpha: brush.pressure_alpha,
                paper: brush.paper,
                tip: brush.texture,
                dual: [
                    brush.dual,
                    brush.dual_scale,
                    secondary.hardness,
                    secondary.roundness,
                    secondary.angle,
                    secondary.paper,
                    secondary.texture,
                ],
                wet: brush.wet,
                jitter: brush.jitter,
                grain: brush.grain,
                dual_jitter: secondary.jitter,
                dual_grain: secondary.grain,
                seed,
            });
            self.gpu_stroke = true;
        }
        self.segment(point, point);
    }

    /// Finish the stroke on whichever path it started.
    pub(crate) fn end_stroke(&mut self) {
        self.editor.end_stroke();
        if !self.gpu_stroke {
            return;
        }
        self.gpu_stroke = false;
        self.editor.defer_raster = false;
        if !self.editor.begin_tile_commit() {
            self.editor.flush_deferred();
            return;
        }
        let active = self.editor.document.active;
        let ink = self.ink();
        let flow = self.editor.brush.flow;
        let Some(paint) = self.paint.as_mut() else {
            return;
        };
        let committed = paint.stroke_commit(active, [ink[0], ink[1], ink[2]], flow);
        paint.stroke_end();
        let rects: Vec<Rect> = committed
            .into_iter()
            .map(|(x, y, width, height)| {
                Rect::new(x as usize, y as usize, width as usize, height as usize)
            })
            .collect();
        self.editor.end_gpu_commit(&rects);
        if !rects.is_empty() {
            self.gpu_stale = true;
        }
    }

    /// Read the pixels a stroke or fill left on the GPU into the core's CPU surface. History,
    /// save and export read that copy; a live stroke never does.
    pub(crate) fn materialize(&mut self) {
        if !self.gpu_stale {
            return;
        }
        self.gpu_stale = false;
        let Some(paint) = self.paint.as_mut() else {
            return;
        };
        let tiles = paint.materialize();
        for tile in tiles {
            let (x, y, width, height) = tile.rect;
            self.editor.apply_tiles(
                tile.layer,
                Rect::new(x as usize, y as usize, width as usize, height as usize),
                &tile.pixels,
            );
        }
    }

    pub(crate) fn segment(&mut self, from: (f32, f32), to: (f32, f32)) {
        let radius = (self.size / 2.0) as f32;
        let colour = self.ink();
        let time = self.time;
        if self.gpu_stroke {
            self.editor.stroke_at(from, to, radius, 1.0, colour, time);
            self.time = self.time.wrapping_add(1);
            let brush = self.editor.brush;
            let hardness = brush.param(0).unwrap_or(0.8);
            let spacing = brush.param(4).unwrap_or(0.1);
            let roundness = brush.param(7).unwrap_or(1.0);
            let angle = brush.param(6).unwrap_or(0.0);
            let (width, height) = (self.width as usize, self.height as usize);
            let (from, to) = self.editor.last_segment().unwrap_or((from, to));
            let (variants, count) = self.editor.mirror.variants(width, height, from, to);
            if let Some(paint) = self.paint.as_mut() {
                for variant in &variants[..count] {
                    paint.stroke_segment(
                        variant.0, variant.1, radius, hardness, spacing, roundness, angle,
                    );
                }
            }
            return;
        }
        if self.tool == Tool::Eraser {
            self.editor.erase_at(from, to, radius, 1.0, time);
        } else {
            self.editor.stroke_at(from, to, radius, 1.0, colour, time);
        }
        self.time = self.time.wrapping_add(1);
        let Some(rect) = self.editor.document.take_dirty() else {
            return;
        };
        if !self.gpu_ready {
            return;
        }
        let active = self.editor.document.active;
        let Some(layer) = self.editor.document.layers.get(active) else {
            return;
        };
        let pixels = crate::pixels::layer_rgba(layer, rect);
        if let Some(paint) = self.paint.as_mut() {
            paint.upload_layer(
                active,
                rect.x as u32,
                rect.y as u32,
                rect.width as u32,
                rect.height as u32,
                &pixels,
            );
        }
    }

    /// A fill written straight into the resident tiles: one pass, one undo step.
    pub(crate) fn gpu_fill(&mut self, x: usize, y: usize) -> bool {
        if self.paint.is_none() || !self.gpu_ready {
            return false;
        }
        let time = now_ms();
        let colour = self.colour;
        let options = FillOptions::default();
        if self
            .editor
            .begin_gpu_fill(x, y, 0.12, colour, options, time)
            .is_none()
        {
            return false;
        }
        let Some((layer, _bounds, fill, under, alpha_locked)) = self.editor.gpu_fill_state() else {
            self.editor.cancel_gpu_fill();
            return false;
        };
        let (width, height) = (self.width, self.height);
        let applied = {
            let Some(paint) = self.paint.as_mut() else {
                self.editor.cancel_gpu_fill();
                return false;
            };
            let Some(mask) = self.editor.gpu_fill_mask() else {
                self.editor.cancel_gpu_fill();
                return false;
            };
            paint.apply_fill(
                layer,
                mask,
                width,
                height,
                [fill[0], fill[1], fill[2]],
                fill[3],
                alpha_locked,
                under,
            )
        };
        let Some((x, y, width, height)) = applied else {
            self.editor.cancel_gpu_fill();
            self.fail("fill found no pixels");
            return false;
        };
        self.editor.finish_gpu_fill(&[Rect::new(
            x as usize,
            y as usize,
            width as usize,
            height as usize,
        )]);
        self.gpu_stale = true;
        true
    }

    /// Render the canvas into a GBM buffer. `fd` is owned by the call; the return code is the
    /// surface's, mapped to the sentence the status bar shows.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn present_dmabuf(
        &mut self,
        fd: i32,
        width: u32,
        height: u32,
        stride: u32,
        offset: u64,
        modifier: u64,
        paper: [f32; 4],
    ) -> i32 {
        let Some(paint) = self.paint.as_mut() else {
            self.fail("no canvas surface");
            return -1;
        };
        let code = paint.present_dmabuf(fd, width, height, stride, offset, modifier, paper);
        match code {
            1 => self.error = None,
            0 => self.fail("the canvas has no layers"),
            -2 => self.fail("the device is not Vulkan"),
            -3 => self.fail("VK_EXT_external_memory_dma_buf is missing"),
            -4 => self.fail("the driver refused the dmabuf (modifier or stride)"),
            -5 => self.fail("the paper pass could not be encoded"),
            _ => self.fail("the dmabuf present failed"),
        }
        code
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}
