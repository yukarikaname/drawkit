use super::*;

impl Editor {
    /// Flood the region around `x, y` on the active layer.
    ///
    /// The fill is decided on the **composite** - the ink over the paper - but written to the active
    /// layer, and the mask carries the coverage read back out of the composite's anti-aliasing. That
    /// is the difference between a fill that reaches the line and one that leaves a pale rim of
    /// paper around it.
    pub fn fill(&mut self, x: usize, y: usize, tolerance: f32, color: Rgba) -> Option<Rect> {
        self.fill_at(x, y, tolerance, color, self.time_ms)
    }

    pub fn fill_at(
        &mut self,
        x: usize,
        y: usize,
        tolerance: f32,
        color: Rgba,
        time_ms: u64,
    ) -> Option<Rect> {
        self.fill_with(x, y, tolerance, color, FillOptions::default(), time_ms)
    }

    pub fn fill_with(
        &mut self,
        x: usize,
        y: usize,
        tolerance: f32,
        color: Rgba,
        options: FillOptions,
        time_ms: u64,
    ) -> Option<Rect> {
        self.cancel_prediction();
        self.time_ms = time_ms;
        if !self.document.can_paint() {
            return None;
        }
        let options = options.clamped();
        let (width, height) = (self.document.width(), self.document.height());
        let rect = Rect::new(0, 0, width, height);
        let composite: Vec<Rgba> = self
            .document
            .composite_rect(rect)
            .into_iter()
            .map(|pixel| over(pixel, PAPER))
            .collect();
        let sealed = close_ink(&composite, width, height, PAPER, options.close);
        let mask = flood(&sealed, width, height, (x, y), tolerance);
        if mask.is_empty() {
            return None;
        }
        let color = self.snapped_fill_colour(x, y, color);
        let grown = grow(&mask, options.expand);
        let core = erode(&mask, 1);
        let soft = edge_coverage(&composite, &grown, &core, PAPER);
        let soft = feather(&soft, options.feather);

        let layer = self.document.active;
        let before = self.document.layers[layer].surface.snapshot(rect);
        if self.document.layers[layer].alpha_locked {
            self.document.layers[layer]
                .surface
                .blend_mask_locked_at(&soft, 0, 0, color);
        } else {
            self.document.layers[layer].surface.blend_mask(&soft, color);
        }
        let after = self.document.layers[layer].surface.snapshot(rect);
        self.record_pixel_step(layer, rect, before, after);
        if self.recording_enabled {
            self.recording.push(
                time_ms,
                Command::Fill {
                    layer,
                    x,
                    y,
                    tolerance,
                    color,
                    close: options.close,
                    expand: options.expand,
                    feather: options.feather,
                },
            );
        }
        Some(rect)
    }

    /// A second fill of the same region should not shift its colour. When the seed of the fill is
    /// already an opaque colour within a few levels of the requested one, keep the existing colour;
    /// a deliberate change beyond that still paints.
    fn snapped_fill_colour(&self, x: usize, y: usize, requested: Rgba) -> Rgba {
        if !self.fill_snap {
            return requested;
        }
        let Some(layer) = self.document.layers.get(self.document.active) else {
            return requested;
        };
        if layer.kind != LayerKind::Paint {
            return requested;
        }
        let existing = layer.surface.get(x, y);
        if existing[3] < 0.99 {
            return requested;
        }
        let distance = ((existing[0] - requested[0]).powi(2)
            + (existing[1] - requested[1]).powi(2)
            + (existing[2] - requested[2]).powi(2))
        .sqrt();
        if distance <= 0.05 {
            [existing[0], existing[1], existing[2], requested[3]]
        } else {
            requested
        }
    }
}
