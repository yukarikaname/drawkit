use super::*;

#[derive(Clone)]
pub(super) struct ReplayCache {
    revision: u64,
    layers: usize,
    /// Visibility, opacity, blend, group and fill state. A change here invalidates the replay even
    /// when the tape did not move.
    signature: u64,
    time_ms: u64,
    index: usize,
    editor: Box<Editor>,
}

fn replay_base(source: &Editor) -> Editor {
    let (width, height) = (source.document.width(), source.document.height());
    let mut scratch = Editor::new(width, height, 1);
    scratch.recording_enabled = false;
    scratch.document.color_space = source.document.color_space;
    scratch.document.dynamic_range = source.document.dynamic_range;
    let painted: std::collections::HashSet<usize> = source
        .recording
        .events()
        .iter()
        .map(|event| match event.command {
            Command::Stroke { layer, .. }
            | Command::Erase { layer, .. }
            | Command::Fill { layer, .. }
            | Command::Smudge { layer, .. }
            | Command::Clear { layer, .. } => layer,
        })
        .collect();
    let mut layers = Vec::with_capacity(source.document.layers.len());
    for (index, layer) in source.document.layers.iter().enumerate() {
        let mut copy = match layer.kind {
            LayerKind::Group => Layer::group(&layer.name, width, height),
            LayerKind::Fill => Layer::fill(&layer.name, layer.fill, width, height),
            LayerKind::Paint => Layer::new(&layer.name, width, height),
        };
        copy.visible = layer.visible;
        copy.opacity = layer.opacity;
        copy.blend = layer.blend;
        copy.locked = layer.locked;
        copy.alpha_locked = layer.alpha_locked;
        copy.clipping = layer.clipping;
        copy.parent = layer.parent;
        copy.expanded = layer.expanded;
        copy.fill = layer.fill;
        if layer.kind == LayerKind::Paint && !painted.contains(&index) && !layer.surface.is_clear()
        {
            copy.surface = layer.surface.clone();
        }
        layers.push(copy);
    }
    scratch.document.layers = layers;
    scratch.document.active = source
        .document
        .active
        .min(scratch.document.layers.len().saturating_sub(1));
    scratch
}

/// The layer state that changes the picture without touching the tape.
fn layer_signature(editor: &Editor) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    editor.document.color_space.as_i32().hash(&mut hasher);
    editor.document.dynamic_range.as_i32().hash(&mut hasher);
    for layer in &editor.document.layers {
        layer.visible.hash(&mut hasher);
        layer.opacity.to_bits().hash(&mut hasher);
        layer.blend.as_i32().hash(&mut hasher);
        layer.parent.hash(&mut hasher);
        layer.alpha_locked.hash(&mut hasher);
        layer.clipping.hash(&mut hasher);
        match layer.kind {
            LayerKind::Paint => 0_u8.hash(&mut hasher),
            LayerKind::Group => 1_u8.hash(&mut hasher),
            LayerKind::Fill => 2_u8.hash(&mut hasher),
        }
        for channel in layer.fill {
            channel.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn apply_replay_command(scratch: &mut Editor, command: Command) {
    match command {
        Command::Stroke {
            layer,
            from,
            to,
            radius,
            pressure,
            color,
            brush: kind,
            ..
        } => {
            let pen = Brush::for_kind(kind);
            let painted = pen.radius_at(radius, pressure);
            let ink = [
                color[0],
                color[1],
                color[2],
                color[3] * pen.alpha_at(pressure),
            ];
            let rect = scratch.bounds(from, to, painted);
            let layer = layer.min(scratch.document.layers.len() - 1);
            let alpha_locked = scratch.document.layers[layer].alpha_locked;
            let surface = &mut scratch.document.layers[layer].surface;
            if alpha_locked {
                brush::stroke_locked(surface, from, to, painted, pen.hardness, ink);
            } else {
                brush::stroke(surface, from, to, painted, pen.hardness, ink);
            }
            scratch.document.layers[layer].surface.mark(rect);
        }
        Command::Erase {
            layer,
            from,
            to,
            radius,
            pressure,
            brush: kind,
            ..
        } => {
            let pen = Brush::for_kind(kind);
            let painted = pen.radius_at(radius, pressure);
            let rect = scratch.bounds(from, to, painted);
            let layer = layer.min(scratch.document.layers.len() - 1);
            brush::erase(
                &mut scratch.document.layers[layer].surface,
                from,
                to,
                painted,
                pen.hardness,
            );
            scratch.document.layers[layer].surface.mark(rect);
        }
        Command::Fill {
            layer,
            x,
            y,
            tolerance,
            color,
            close,
            expand,
            feather: feather_radius,
        } => {
            let (width, height) = (scratch.document.width(), scratch.document.height());
            let rect = Rect::new(0, 0, width, height);
            let composite: Vec<Rgba> = scratch
                .document
                .composite_rect(rect)
                .into_iter()
                .map(|pixel| over(pixel, PAPER))
                .collect();
            let sealed = close_ink(&composite, width, height, PAPER, close);
            let mask = flood(&sealed, width, height, (x, y), tolerance);
            let grown = grow(&mask, expand);
            let core = erode(&mask, 1);
            let soft = edge_coverage(&composite, &grown, &core, PAPER);
            let soft = feather(&soft, feather_radius);
            let layer = layer.min(scratch.document.layers.len() - 1);
            scratch.document.layers[layer]
                .surface
                .blend_mask(&soft, color);
        }
        Command::Smudge {
            layer,
            from,
            to,
            radius,
            pressure,
            strength,
            brush: kind,
        } => {
            let pen = Brush::for_kind(kind);
            let painted = pen.radius_at(radius, pressure);
            let rect = scratch.bounds(from, to, painted);
            let layer = layer.min(scratch.document.layers.len() - 1);
            let surface = &mut scratch.document.layers[layer].surface;
            brush::smudge(surface, from, to, painted, pen.hardness, strength);
            scratch.document.layers[layer].surface.mark(rect);
        }
        Command::Clear { layer, color } => {
            let (width, height) = (scratch.document.width(), scratch.document.height());
            let layer = layer.min(scratch.document.layers.len() - 1);
            let surface = &mut scratch.document.layers[layer].surface;
            for y in 0..height {
                for x in 0..width {
                    surface.set(x, y, color);
                }
            }
        }
    }
}

impl Editor {
    /// A fresh editor with the tape played up to `time_ms`.
    ///
    /// The stateless version is used by tests and one-off renders. The scrub path uses
    /// `replay_composite`, which keeps the last replayed state and only applies the events between
    /// the old and new positions.
    pub fn replay(&self, time_ms: u64) -> Editor {
        let mut scratch = replay_base(self);
        for event in self.recording.events() {
            if event.time_ms > time_ms {
                break;
            }
            apply_replay_command(&mut scratch, event.command);
        }
        scratch
    }

    /// The composite at `time_ms`, advancing a cached replay when the scrub moves forward.
    pub fn replay_composite(&mut self, time_ms: u64) -> Vec<Rgba> {
        self.advance_replay(time_ms);
        let cache = self
            .replay_cache
            .as_ref()
            .expect("the replay cache was just advanced");
        let (width, height) = (
            cache.editor.document.width(),
            cache.editor.document.height(),
        );
        cache
            .editor
            .document
            .composite_rect(Rect::new(0, 0, width, height))
    }

    /// The same replay, sampled down to the size of the preview. The canvas is still composited
    /// at full resolution in the cache; only the readout is smaller.
    pub fn replay_composite_scaled(
        &mut self,
        time_ms: u64,
        width: usize,
        height: usize,
    ) -> Vec<Rgba> {
        self.advance_replay(time_ms);
        let cache = self
            .replay_cache
            .as_ref()
            .expect("the replay cache was just advanced");
        cache
            .editor
            .document
            .composite_scaled(width.max(1), height.max(1))
    }

    /// The whole painting, resampled for a navigator or other small overview. Opaque RGBA8 with the
    /// paper behind it; the canvas itself stays transparent.
    pub fn overview(&self, width: usize, height: usize) -> Vec<u8> {
        if width == 0 || height == 0 {
            return Vec::new();
        }
        let pixels = self.document.composite_scaled(width, height);
        let mut out = Vec::with_capacity(pixels.len() * 4);
        for pixel in pixels {
            let shown = over(pixel, PAPER);
            for channel in shown {
                out.push((channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        out
    }

    /// One timelapse frame: the tape at `time_ms` over paper, as opaque BGRA for a video encoder.
    /// A frame that is smaller than the canvas is sampled down; H.264 has a size limit and a 4K
    /// timelapse does not need the 8K pixels.
    pub fn video_frame(&mut self, time_ms: u64, width: usize, height: usize) -> Vec<u8> {
        let (canvas_width, canvas_height) = (self.document.width(), self.document.height());
        let pixels = if width == canvas_width && height == canvas_height {
            self.replay_composite(time_ms)
        } else {
            self.replay_composite_scaled(time_ms, width.max(1), height.max(1))
        };
        let mut out = Vec::with_capacity(pixels.len() * 4);
        for pixel in pixels {
            let shown = over(pixel, PAPER);
            out.push((shown[2].clamp(0.0, 1.0) * 255.0).round() as u8);
            out.push((shown[1].clamp(0.0, 1.0) * 255.0).round() as u8);
            out.push((shown[0].clamp(0.0, 1.0) * 255.0).round() as u8);
            out.push(255);
        }
        out
    }

    fn advance_replay(&mut self, time_ms: u64) {
        let revision = self.recording.revision();
        let layers = self.document.layers.len();
        let signature = layer_signature(self);
        let restart = self.replay_cache.as_ref().is_none_or(|cache| {
            cache.revision != revision
                || cache.layers != layers
                || cache.signature != signature
                || time_ms < cache.time_ms
        });
        if restart {
            self.replay_cache = Some(Box::new(ReplayCache {
                revision,
                layers,
                signature,
                time_ms: 0,
                index: 0,
                editor: Box::new(replay_base(self)),
            }));
        }
        let events = self.recording.events();
        let cache = self
            .replay_cache
            .as_mut()
            .expect("replay cache was just created");
        while cache.index < events.len() {
            let event = &events[cache.index];
            if event.time_ms > time_ms {
                break;
            }
            apply_replay_command(&mut cache.editor, event.command);
            cache.index += 1;
        }
        cache.time_ms = time_ms;
    }
}
