use super::*;

impl Editor {
    pub fn stroke_tape(&self) -> drawkit_formats::TapeDocument {
        self.build_tape(false)
    }

    /// A tape with a pixel cache for every paint layer. Opening it skips the replay, which is what
    /// keeps a project with thousands of strokes fast to open; the events are still there.
    fn build_tape(&self, cache_all: bool) -> drawkit_formats::TapeDocument {
        let mut tape = Self::tape_for(&self.document, &self.recording, cache_all);
        let mut frames = Vec::with_capacity(self.frames.len());
        for slot in &self.frames {
            let (document, recording) = slot
                .as_ref()
                .map(|frame| (&frame.document, &frame.recording))
                .unwrap_or((&self.document, &self.recording));
            frames.push(Self::tape_for(document, recording, cache_all));
        }
        tape.frames = frames;
        tape.active_frame = self.active_frame;
        tape
    }

    fn tape_for(
        document: &Document,
        recording: &Recording,
        cache_all: bool,
    ) -> drawkit_formats::TapeDocument {
        let mut tape = recording.tape(document.width(), document.height(), document.layers.len());
        tape.active = Some(document.active as u32);
        tape.color_space = document.color_space.as_i32();
        tape.dynamic_range = document.dynamic_range.as_i32();
        let painted: std::collections::HashSet<usize> = recording
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
        tape.layer_info = document
            .layers
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                let mut raster = None;
                let mut raster_rect = None;
                let cache = layer.kind == crate::document::LayerKind::Paint
                    && !layer.surface.is_clear()
                    && (cache_all || !painted.contains(&index));
                if cache
                    && let Some(rect) = layer.surface.painted_bounds()
                    && let Ok(bytes) =
                        crate::layer_png::encode_layer(&layer.surface, rect, document.hdr())
                {
                    raster = Some(bytes);
                    raster_rect = Some([
                        rect.x as u32,
                        rect.y as u32,
                        rect.width as u32,
                        rect.height as u32,
                    ]);
                }
                drawkit_formats::TapeLayer {
                    name: layer.name.clone(),
                    kind: match layer.kind {
                        crate::document::LayerKind::Paint => 0,
                        crate::document::LayerKind::Group => 1,
                        crate::document::LayerKind::Fill => 2,
                    },
                    parent: layer.parent.map(|parent| parent as u32),
                    visible: layer.visible,
                    opacity: layer.opacity,
                    blend: layer.blend.as_i32() as u8,
                    locked: layer.locked,
                    alpha_locked: layer.alpha_locked,
                    clipping: layer.clipping,
                    expanded: layer.expanded,
                    reference: layer.reference,
                    color_tag: layer.color_tag,
                    fill: layer.fill,
                    raster,
                    raster_rect,
                }
            })
            .collect();
        tape
    }

    /// Save the stroke tape, the lossless source the painting can be re-rendered from.
    pub fn save_tape(&self, path: &std::path::Path) -> bool {
        self.write_tape(path, true)
    }

    fn write_tape(&self, path: &std::path::Path, cache_all: bool) -> bool {
        let tape = self.build_tape(cache_all);
        match drawkit_formats::write_dkp(&tape) {
            Ok(bytes) => std::fs::write(path, bytes).is_ok(),
            Err(error) => {
                eprintln!("save tape: {error}");
                false
            }
        }
    }
}
