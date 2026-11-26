//! Preparing a fill: the mask, the selection clip and the state the shell then writes.

use super::*;
use crate::fill::Mask;

impl Editor {
    /// Build the soft coverage mask for a fill without writing it. The selection, gap closing,
    /// growth, edge coverage and feather all live here so the instant and animated paths agree.
    pub(super) fn fill_mask(
        &mut self,
        x: usize,
        y: usize,
        tolerance: f32,
        color: Rgba,
        options: FillOptions,
    ) -> Option<(Mask, Rgba, bool)> {
        if !self.document.can_paint() {
            return None;
        }
        if !self.inside_selection(x, y) {
            return None;
        }
        let options = options.clamped();
        let (width, height) = (self.document.width(), self.document.height());
        let rect = Rect::new(0, 0, width, height);
        // The plan shades the straight-alpha composite with the paper as it reads it, so the
        // canvas-sized frame `composite_rect` returns is the only copy in flight.
        let compose_started = std::time::Instant::now();
        let composite = self.document.composite_rect(rect);
        if std::env::var("DRAWKIT_FILL_TRACE").is_ok() {
            eprintln!("DRAWKIT_FILL_COMPOSE {:?}", compose_started.elapsed());
        }
        let layer = self.document.active;
        let under = self.document.layers[layer].surface.get(x, y)[3] < 0.5;
        let soft = if options.smart {
            let edges = self.smart_edges(&composite, width, height);
            crate::fill::mask_for_flat_cached(
                &composite,
                (width, height),
                (x, y),
                tolerance,
                options,
                under,
                edges,
            )?
        } else {
            crate::fill::mask_for_flat(
                &composite,
                (width, height),
                (x, y),
                tolerance,
                options,
                under,
            )?
        };
        let color = self.snapped_fill_colour(
            self.document.layers[self.document.active].surface.get(x, y),
            color,
        );
        Some((soft, color, under))
    }

    /// Start a progressive fill. The mask is ready, but no pixel is written until the shell calls
    /// `fill_animation_step`; `fill_animation_finish` records the one history step and tape event.
    pub fn begin_fill_animation(
        &mut self,
        x: usize,
        y: usize,
        tolerance: f32,
        color: Rgba,
        options: FillOptions,
        time_ms: u64,
    ) -> Option<Rect> {
        self.prepare_fill(x, y, tolerance, color, options, time_ms, false)
    }

    /// Build the mask for a surface that applies it on the GPU. Nothing is written here: the
    /// shell uploads the returned mask into the resident tiles and then calls `finish_gpu_fill`.
    /// The caller must materialise pending GPU pixels before this: the mask is decided from the
    /// document's composite, which is only a cache of the resident layers.
    pub fn begin_gpu_fill(
        &mut self,
        x: usize,
        y: usize,
        tolerance: f32,
        color: Rgba,
        options: FillOptions,
        time_ms: u64,
    ) -> Option<Rect> {
        self.prepare_fill(x, y, tolerance, color, options, time_ms, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_fill(
        &mut self,
        x: usize,
        y: usize,
        tolerance: f32,
        color: Rgba,
        options: FillOptions,
        time_ms: u64,
        gpu: bool,
    ) -> Option<Rect> {
        self.finish_fill_animation();
        self.cancel_prediction();
        self.time_ms = time_ms;
        let (mask, color, under) = self.fill_mask(x, y, tolerance, color, options)?;
        self.begin_from_mask(mask, color, under, x, y, tolerance, options, time_ms, gpu)
    }

    /// Build the mask from the composite the shell composed on the GPU.
    ///
    /// The live stack keeps no CPU pixels, so the core cannot compose the frame a fill's flood
    /// needs. The shell reads it back instead, which is the same premultiplied RGBA8 frame the
    /// tape replay floods over, so a live fill and a reconstructed one agree. `under` and `seed`
    /// describe the active layer at the tap; only the resident tiles know them.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_gpu_fill_from(
        &mut self,
        x: usize,
        y: usize,
        tolerance: f32,
        color: Rgba,
        options: FillOptions,
        time_ms: u64,
        composite: &[u8],
        under: bool,
        seed: Rgba,
    ) -> Option<Rect> {
        self.finish_fill_animation();
        self.cancel_prediction();
        self.time_ms = time_ms;
        if !self.document.can_paint() || !self.inside_selection(x, y) {
            return None;
        }
        let options = options.clamped();
        let (width, height) = (self.document.width(), self.document.height());
        let mask = crate::fill_mask::mask_from_composite(
            composite,
            width,
            height,
            (x, y),
            tolerance,
            options,
            under,
        )?;
        let color = self.snapped_fill_colour(seed, color);
        self.begin_from_mask(mask, color, under, x, y, tolerance, options, time_ms, true)
    }

    /// Whether the tap is inside the current selection, if there is one.
    pub(super) fn inside_selection(&self, x: usize, y: usize) -> bool {
        match self.selection {
            None => true,
            Some(selection) => {
                x >= selection.x
                    && y >= selection.y
                    && x < selection.x + selection.width
                    && y < selection.y + selection.height
            }
        }
    }

    /// The half of a fill both entry points share: the selection clip, the layer check and the
    /// prepared animation the shell then writes.
    #[allow(clippy::too_many_arguments)]
    fn begin_from_mask(
        &mut self,
        mut mask: Mask,
        color: Rgba,
        under: bool,
        x: usize,
        y: usize,
        tolerance: f32,
        options: FillOptions,
        time_ms: u64,
        gpu: bool,
    ) -> Option<Rect> {
        if let Some(selection) = self.selection {
            for py in 0..mask.height {
                for px in 0..mask.width {
                    if px < selection.x
                        || py < selection.y
                        || px >= selection.x + selection.width
                        || py >= selection.y + selection.height
                    {
                        mask.set(px, py, 0.0);
                    }
                }
            }
        }
        let layer = self.document.active;
        if self.document.layers[layer].kind != crate::document::LayerKind::Paint {
            return None;
        }
        let (bounds, max_distance) = mask_region(&mask, (x, y))?;
        // The GPU path writes nothing on the CPU, so there is nothing to restore on cancel.
        let before = if gpu {
            Vec::new()
        } else {
            self.document.layers[layer].surface.snapshot(bounds)
        };
        self.fill_animation = Some(FillAnimation {
            layer,
            bounds,
            mask,
            tap: (x as f32 + 0.5, y as f32 + 0.5),
            max_distance,
            applied: -1.0,
            color,
            under,
            before,
            time_ms,
            tolerance,
            options: options.clamped(),
            gpu,
        });
        Some(bounds)
    }
}
