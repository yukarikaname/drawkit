use super::*;

impl Editor {
    pub fn erase(&mut self, from: (f32, f32), to: (f32, f32), radius: f32, pressure: f32) -> Rect {
        self.erase_at(from, to, radius, pressure, self.time_ms)
    }

    pub fn erase_at(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        time_ms: u64,
    ) -> Rect {
        self.erase_sample(from, to, radius, Dynamics::pressure(pressure), time_ms)
    }

    pub fn erase_sample(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        dynamics: Dynamics,
        time_ms: u64,
    ) -> Rect {
        self.time_ms = time_ms;
        self.cancel_prediction();
        if !self.document.can_paint() || self.document.active().alpha_locked {
            return Rect::new(0, 0, 0, 0);
        }
        let (from, to) = self.smooth_segment(from, to);
        let brush = self
            .brush
            .with_input(dynamics.tilt, dynamics.azimuth, dynamics.roll);
        let painted = brush.radius_at(radius, dynamics.pressure);
        // The eraser has no colour; a stroke and an erase on the same geometry are still
        // different actions because the sentinel does not match a stroke's ink.
        let ink = [0.0; 4];
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
        for (variant, &(variant_from, variant_to)) in variants[..count].iter().enumerate() {
            let rect = self.bounds(variant_from, variant_to, painted);
            let surface = &mut self.document.layers[layer].surface;
            let before = surface.snapshot(rect);
            brush::erase_with_phase(
                surface,
                (variant_from, variant_to),
                painted,
                &brush,
                seed,
                &mut phases[variant],
            );
            let after = surface.snapshot(rect);
            self.record_pixel_step(layer, rect, before, after);
            union = union.union(&rect);
            if self.recording_enabled {
                self.recording.push_grouped(
                    time_ms,
                    group,
                    Command::Erase {
                        layer,
                        from: variant_from,
                        to: variant_to,
                        radius,
                        pressure: dynamics.pressure,
                        brush: self.brush.kind,
                        seed,
                        tilt: dynamics.tilt,
                        azimuth: dynamics.azimuth,
                        roll: dynamics.roll,
                    },
                );
            }
        }
        if let Some(session) = self.stroke.as_mut() {
            session.phases = phases;
        }
        self.extend_prediction(to, radius, dynamics, [0.0; 4], true, time_ms);
        union
    }

    pub fn smudge(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        strength: f32,
    ) -> Rect {
        self.smudge_at(from, to, radius, pressure, strength, self.time_ms)
    }

    pub fn smudge_at(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        strength: f32,
        time_ms: u64,
    ) -> Rect {
        self.smudge_sample(
            from,
            to,
            radius,
            Dynamics::pressure(pressure),
            strength,
            time_ms,
        )
    }

    /// Pull existing paint along the pointer. Unlike a stroke this paints nothing new; it moves
    /// what is already on the layer, so the tape has to carry the same command to reproduce it.
    pub fn smudge_sample(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        dynamics: Dynamics,
        strength: f32,
        time_ms: u64,
    ) -> Rect {
        self.time_ms = time_ms;
        self.cancel_prediction();
        let strength = strength.clamp(0.0, 1.0);
        if !self.document.can_paint()
            || self.document.active().alpha_locked
            || strength <= 0.0
            || radius <= 0.0
        {
            return Rect::new(0, 0, 0, 0);
        }
        let (from, to) = self.smooth_segment(from, to);
        let painted = self.brush.radius_at(radius, dynamics.pressure);
        let (width, height) = (self.document.width(), self.document.height());
        let (variants, count) = self.mirror.variants(width, height, from, to);
        let layer = self.document.active;
        let group = self.stroke.as_ref().map(|session| session.group);
        let mut union = Rect::new(0, 0, 0, 0);
        for &(variant_from, variant_to) in &variants[..count] {
            let rect = self.bounds(variant_from, variant_to, painted);
            if self.stroke.as_ref().is_some_and(|session| session.deferred) {
                union = union.union(&rect);
                if self.recording_enabled {
                    self.recording.push_grouped(
                        time_ms,
                        group,
                        Command::Smudge {
                            layer,
                            from: variant_from,
                            to: variant_to,
                            radius,
                            pressure: dynamics.pressure,
                            strength,
                            brush: self.brush.kind,
                        },
                    );
                }
                continue;
            }
            let surface = &mut self.document.layers[layer].surface;
            let before = surface.snapshot(rect);
            brush::smudge(
                surface,
                variant_from,
                variant_to,
                painted,
                self.brush.hardness,
                strength,
            );
            let after = surface.snapshot(rect);
            self.record_pixel_step(layer, rect, before, after);
            union = union.union(&rect);
            if self.recording_enabled {
                self.recording.push_grouped(
                    time_ms,
                    group,
                    Command::Smudge {
                        layer,
                        from: variant_from,
                        to: variant_to,
                        radius,
                        pressure: dynamics.pressure,
                        strength,
                        brush: self.brush.kind,
                    },
                );
            }
        }
        union
    }
}
