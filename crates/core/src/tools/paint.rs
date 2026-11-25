use super::*;

impl Editor {
    /// Start one pointer stroke. The shell calls this on pointer-down and `end_stroke` on
    /// pointer-up; the segments in between become one undo step and one tape group.
    pub fn begin_stroke(&mut self, time_ms: u64) {
        self.finish_fill_animation();
        if self.stroke.is_some() {
            return;
        }
        self.cancel_prediction();
        self.predictor.clear();
        self.time_ms = time_ms;
        self.smoothed = None;
        self.stroke = Some(StrokeSession {
            group: self.next_group,
            seed: self.take_seed(),
            history_depth: self.history.depth().0,
            layer: self.document.active,
            phases: [0.0; 4],
            last: None,
            deferred: self.defer_raster && self.recording_enabled,
        });
        self.next_group += 1;
        if let Some(layer) = self.document.layers.get_mut(self.document.active) {
            layer.surface.begin_stroke_accumulation();
            layer.surface.set_brush_texture(self.brush_texture.clone());
        }
    }

    /// Rasterise a group that the GPU preview recorded without touching the CPU surface.
    fn rasterize_deferred(&mut self, group: u64, layer: usize) {
        let commands: Vec<Command> = self
            .recording
            .events()
            .iter()
            .filter(|event| event.group == Some(group))
            .map(|event| event.command)
            .collect();
        if commands.is_empty() {
            return;
        }
        let mut union = Rect::new(0, 0, 0, 0);
        for command in &commands {
            if let Command::Stroke {
                from, to, radius, ..
            }
            | Command::Erase {
                from, to, radius, ..
            } = command
            {
                union = union.union(&self.bounds(*from, *to, *radius));
            }
        }
        if union.is_empty() {
            return;
        }
        let Some(target) = self.document.layers.get(layer) else {
            return;
        };
        let before = target.surface.snapshot(union);
        if let Some(target) = self.document.layers.get_mut(layer) {
            target.surface.begin_stroke_accumulation();
            target.surface.set_brush_texture(self.brush_texture.clone());
        }
        let mut phase = 0.0;
        for command in commands {
            super::replay::apply_replay_command(self, command, &mut phase);
        }
        let after = self.document.layers[layer].surface.snapshot(union);
        self.record_pixel_step(layer, union, before, after);
    }

    pub fn end_stroke(&mut self) {
        self.cancel_prediction();
        self.predictor.clear();
        let Some(session) = self.stroke.take() else {
            return;
        };
        self.smoothed = None;
        if session.deferred {
            self.rasterize_deferred(session.group, session.layer);
        }
        if let Some(layer) = self.document.layers.get_mut(session.layer) {
            layer.surface.end_stroke_accumulation();
        }
        if self.document.active != session.layer {
            return;
        }
        let depth = self.history.depth().0;
        if depth > session.history_depth {
            self.history
                .merge_last(depth - session.history_depth, &mut self.document);
        }
    }

    /// Remove the tentative segment ahead of the pen, if any. The pixels go back to the state the
    /// last real sample left, without touching history or the tape.
    pub fn cancel_prediction(&mut self) {
        let Some(prediction) = self.prediction.take() else {
            return;
        };
        if let Some(layer) = self.document.layers.get_mut(prediction.layer) {
            for (rect, pixels) in prediction.saved {
                layer.surface.restore(rect, &pixels);
            }
        }
    }

    /// Paint a segment for the preview only, keeping what it replaced so the next real sample can
    /// retract it.
    fn paint_prediction(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        dynamics: Dynamics,
        color: Rgba,
        erase: bool,
    ) {
        let brush = self
            .brush
            .with_input(dynamics.tilt, dynamics.azimuth, dynamics.roll);
        let painted = brush.radius_at(radius, dynamics.pressure);
        let alpha = brush.alpha_at(dynamics.pressure);
        let ink = [color[0], color[1], color[2], color[3] * alpha];
        let seed = self.stroke.as_ref().map_or(0, |session| session.seed);
        let (width, height) = (self.document.width(), self.document.height());
        let (variants, count) = self.mirror.variants(width, height, from, to);
        let layer = self.document.active;
        let mut saved = Vec::with_capacity(count);
        let alpha_locked = self.document.layers[layer].alpha_locked;
        for &(variant_from, variant_to) in &variants[..count] {
            let rect = self.bounds(variant_from, variant_to, painted);
            let surface = &mut self.document.layers[layer].surface;
            let before = surface.snapshot(rect);
            if erase {
                brush::erase_with(surface, variant_from, variant_to, painted, &brush, seed);
            } else if alpha_locked {
                brush::stroke_locked_with(
                    surface,
                    variant_from,
                    variant_to,
                    painted,
                    &brush,
                    ink,
                    seed,
                );
            } else {
                brush::stroke_with(
                    surface,
                    variant_from,
                    variant_to,
                    painted,
                    &brush,
                    ink,
                    seed,
                );
            }
            surface.mark(rect);
            saved.push((rect, before));
        }
        self.prediction = Some(Prediction { layer, saved });
    }

    /// Extrapolate the next few milliseconds and show that segment. Kept short: a prediction that
    /// runs away at a corner is worse than the latency it hides.
    pub(super) fn extend_prediction(
        &mut self,
        to: (f32, f32),
        radius: f32,
        dynamics: Dynamics,
        color: Rgba,
        erase: bool,
        time_ms: u64,
    ) {
        if !self.prediction_enabled || self.stroke.is_none() {
            return;
        }
        self.predictor.push(time_ms as f64 / 1000.0, [to.0, to.1]);
        let now = time_ms as f64 / 1000.0;
        let Some(predicted) = self.predictor.predict(now, 0.008) else {
            return;
        };
        let dx = predicted[0] - to.0;
        let dy = predicted[1] - to.1;
        let distance = (dx * dx + dy * dy).sqrt();
        let limit = (radius * 4.0).max(6.0);
        if distance < 0.75 || distance > limit {
            return;
        }
        self.paint_prediction(
            to,
            (predicted[0], predicted[1]),
            radius,
            dynamics,
            color,
            erase,
        );
    }

    pub(super) fn bounds(&self, from: (f32, f32), to: (f32, f32), radius: f32) -> Rect {
        let spread = radius * (1.0 + self.brush.jitter.max(0.0));
        let margin = spread.ceil() as usize + 2;
        let left = from.0.min(to.0).floor().max(0.0) as usize;
        let top = from.1.min(to.1).floor().max(0.0) as usize;
        let right = (from.0.max(to.0).ceil().max(0.0) as usize + margin).min(self.document.width());
        let bottom =
            (from.1.max(to.1).ceil().max(0.0) as usize + margin).min(self.document.height());
        let x = left.saturating_sub(margin);
        let y = top.saturating_sub(margin);
        Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
    }

    pub fn stroke(&mut self, from: (f32, f32), to: (f32, f32), radius: f32, color: Rgba) -> Rect {
        self.stroke_at(from, to, radius, 1.0, color, self.time_ms)
    }

    /// A seed per stroke, saved with the tape so a replay is the same random stream.
    pub(super) fn take_seed(&mut self) -> u64 {
        let mut value = self.next_seed;
        self.next_seed = self.next_seed.wrapping_add(0x9E3779B97F4A7C15);
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
        value ^ (value >> 31)
    }

    pub(super) fn smooth_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
    ) -> ((f32, f32), (f32, f32)) {
        if self.stabilization <= 0.0 {
            return (from, to);
        }
        let step = 1.0 - self.stabilization;
        let previous = self.smoothed.unwrap_or(from);
        let next = (
            previous.0 + (to.0 - previous.0) * step,
            previous.1 + (to.1 - previous.1) * step,
        );
        self.smoothed = Some(next);
        (previous, next)
    }

    pub fn stroke_at(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        color: Rgba,
        time_ms: u64,
    ) -> Rect {
        self.stroke_sample(
            from,
            to,
            radius,
            Dynamics::pressure(pressure),
            color,
            time_ms,
        )
    }

    /// Paint one segment with the full tablet input. The angles are recorded for the brush engine
    /// that will use them; the current round brush only reads pressure.
    pub fn stroke_sample(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        dynamics: Dynamics,
        color: Rgba,
        time_ms: u64,
    ) -> Rect {
        self.time_ms = time_ms;
        self.cancel_prediction();
        if !self.document.can_paint() {
            return Rect::new(0, 0, 0, 0);
        }
        let (from, to) = self.smooth_segment(from, to);
        let brush = self
            .brush
            .with_input(dynamics.tilt, dynamics.azimuth, dynamics.roll);
        let painted = brush.radius_at(radius, dynamics.pressure);
        let alpha = brush.alpha_at(dynamics.pressure);
        let ink = [color[0], color[1], color[2], color[3] * alpha];
        if self.stroke_dedupe
            && self
                .stroke
                .as_ref()
                .and_then(|session| session.last)
                .is_some_and(|last| last.repeats(from, to, painted, ink, dynamics))
        {
            return Rect::new(0, 0, 0, 0);
        }
        let (width, height) = (self.document.width(), self.document.height());
        let (variants, count) = self.mirror.variants(width, height, from, to);
        let layer = self.document.active;
        let session = self
            .stroke
            .as_ref()
            .map(|session| (session.group, session.seed));
        let group = session.map(|session| session.0);
        let seed = session.map_or_else(|| self.take_seed(), |session| session.1);
        let mut union = Rect::new(0, 0, 0, 0);
        let mut phases = self
            .stroke
            .as_ref()
            .map_or([0.0; 4], |session| session.phases);
        if let Some(session) = self.stroke.as_mut() {
            session.last = Some(LastSegment {
                from,
                to,
                painted,
                ink,
                dynamics,
            });
        }
        if self.stroke.as_ref().is_some_and(|session| session.deferred) {
            for &(variant_from, variant_to) in &variants[..count] {
                union = union.union(&self.bounds(variant_from, variant_to, painted));
                if self.recording_enabled {
                    self.recording.push_grouped(
                        time_ms,
                        group,
                        Command::Stroke {
                            layer,
                            from: variant_from,
                            to: variant_to,
                            radius,
                            pressure: dynamics.pressure,
                            color,
                            brush: self.brush.kind,
                            seed,
                            tilt: dynamics.tilt,
                            azimuth: dynamics.azimuth,
                            roll: dynamics.roll,
                        },
                    );
                }
            }
            return union;
        }
        for (variant, &(variant_from, variant_to)) in variants[..count].iter().enumerate() {
            let rect = self.bounds(variant_from, variant_to, painted);
            let alpha_locked = self.document.layers[layer].alpha_locked;
            let surface = &mut self.document.layers[layer].surface;
            let before = surface.snapshot(rect);
            if alpha_locked {
                brush::stroke_locked_with_phase(
                    surface,
                    (variant_from, variant_to),
                    painted,
                    &brush,
                    ink,
                    seed,
                    &mut phases[variant],
                );
            } else {
                brush::stroke_with_phase(
                    surface,
                    (variant_from, variant_to),
                    painted,
                    &brush,
                    ink,
                    seed,
                    &mut phases[variant],
                );
            }
            let after = surface.snapshot(rect);
            self.record_pixel_step(layer, rect, before, after);
            union = union.union(&rect);
            if self.recording_enabled {
                self.recording.push_grouped(
                    time_ms,
                    group,
                    Command::Stroke {
                        layer,
                        from: variant_from,
                        to: variant_to,
                        radius,
                        pressure: dynamics.pressure,
                        color,
                        brush: self.brush.kind,
                        seed,
                        tilt: dynamics.tilt,
                        azimuth: dynamics.azimuth,
                        roll: dynamics.roll,
                    },
                );
            }
        }
        self.extend_prediction(to, radius, dynamics, color, false, time_ms);
        union
    }
}
