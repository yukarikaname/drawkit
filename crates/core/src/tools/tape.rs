use super::*;

pub(crate) fn brush_to_tape(brush: &crate::brush::Brush) -> drawkit_formats::TapeBrush {
    drawkit_formats::TapeBrush {
        kind: brush.kind.as_i32() as u8,
        hardness: brush.hardness,
        flow: brush.flow,
        pressure_radius: brush.pressure_radius,
        pressure_alpha: brush.pressure_alpha,
        spacing: brush.spacing,
        jitter: brush.jitter,
        angle: brush.angle,
        roundness: brush.roundness,
        grain: brush.grain,
        paper: brush.paper,
        texture: brush.texture,
        wet: brush.wet,
        dual: brush.dual,
        dual_scale: brush.dual_scale,
        dual_kind: brush.dual_kind,
    }
}

pub(crate) fn brush_from_tape(brush: &drawkit_formats::TapeBrush) -> crate::brush::Brush {
    let mut result =
        crate::brush::Brush::for_kind(crate::brush::BrushKind::from_i32(brush.kind as i32));
    result.hardness = brush.hardness.clamp(0.0, 1.0);
    result.flow = brush.flow.clamp(0.01, 1.0);
    result.pressure_radius = brush.pressure_radius.clamp(0.0, 1.0);
    result.pressure_alpha = brush.pressure_alpha.clamp(0.0, 1.0);
    result.spacing = brush.spacing.clamp(0.02, 2.0);
    result.jitter = brush.jitter.clamp(0.0, 2.0);
    result.angle = brush.angle;
    result.roundness = brush.roundness.clamp(0.05, 1.0);
    result.grain = brush.grain.clamp(0.0, 1.0);
    result.paper = brush.paper.clamp(0.0, 1.0);
    result.texture = brush.texture.clamp(0.0, 1.0);
    result.wet = brush.wet.clamp(0.0, 1.0);
    result.dual = brush.dual.clamp(0.0, 1.0);
    result.dual_scale = brush.dual_scale.clamp(0.1, 2.0);
    result.dual_kind = brush.dual_kind.min(8);
    result
}

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
        tape.fps = self.fps;
        tape.loop_enabled = self.loop_enabled;
        tape.loop_start = self.loop_start;
        tape.loop_end = self.loop_end;
        tape.brushes = self.brushes.iter().map(brush_to_tape).collect();
        tape.textures = self
            .brush_texture
            .iter()
            .map(|texture| drawkit_formats::TapeTexture {
                width: texture.width,
                height: texture.height,
                data: texture.data.clone(),
            })
            .collect();
        tape.frame_durations = (0..self.frames.len())
            .map(|index| {
                if index == self.active_frame {
                    self.frame_duration
                } else {
                    self.frames[index]
                        .as_ref()
                        .map_or(self.frame_duration, |frame| frame.duration_ms)
                }
            })
            .collect();
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
                // An HDR layer is cached as float EXR, so highlights above one survive and the
                // open path does not have to replay its strokes with the CPU brush.
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
    pub fn save_tape(&mut self, path: &std::path::Path) -> bool {
        self.flush_deferred();
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

impl Editor {
    /// Build the editor and layer tree a project file describes, without replaying its events.
    ///
    /// A saved project carries a raster for every paint layer, so opening it only has to decode
    /// those rasters. Events are kept on the tape for a GPU replay or a later export.
    pub fn from_tape(tape: &drawkit_formats::TapeDocument) -> Editor {
        let (width, height) = (tape.width.max(1), tape.height.max(1));
        let mut editor = Editor::new(width, height, 1);
        editor.recording_enabled = false;
        if let Some(texture) = tape.textures.first() {
            editor.brush_texture = crate::brush::BrushTexture::new(
                texture.width,
                texture.height,
                texture.data.clone(),
            )
            .map(std::sync::Arc::new);
        }
        if !tape.brushes.is_empty() {
            for (index, slot) in editor.brushes.iter_mut().enumerate() {
                if let Some(stored) = tape.brushes.get(index) {
                    *slot = super::brush_from_tape(stored);
                }
            }
        }
        editor.document.color_space = crate::document::ColorSpace::from_i32(tape.color_space);
        editor.document.dynamic_range = crate::document::DynamicRange::from_i32(tape.dynamic_range);
        if tape.layer_info.is_empty() {
            while editor.document.layers.len() < tape.layers.max(1) {
                let name = format!("Layer {}", editor.document.layers.len() + 1);
                editor.document.add_layer(name);
            }
            return editor;
        }
        let mut layers = Vec::with_capacity(tape.layer_info.len());
        for info in &tape.layer_info {
            let mut layer = match info.kind {
                1 => Layer::group(&info.name, width, height),
                2 => Layer::fill(&info.name, info.fill, width, height),
                _ => Layer::new(&info.name, width, height),
            };
            if let Some(raster) = &info.raster {
                let rect = info
                    .raster_rect
                    .map_or(Rect::new(0, 0, width, height), |r| {
                        Rect::new(r[0] as usize, r[1] as usize, r[2] as usize, r[3] as usize)
                    });
                if let Ok(surface) = crate::layer_png::decode_layer(raster, width, height, rect) {
                    layer.surface = surface;
                }
            }
            layer.visible = info.visible;
            layer.opacity = info.opacity.clamp(0.0, 1.0);
            layer.blend = crate::document::BlendMode::from_i32(info.blend as i32);
            layer.locked = info.locked;
            layer.alpha_locked = info.alpha_locked;
            layer.reference = info.reference;
            layer.color_tag = info.color_tag;
            layer.clipping = info.clipping;
            layer.parent = info.parent.map(|parent| parent as usize);
            layer.expanded = info.expanded;
            layer.fill = info.fill;
            layers.push(layer);
        }
        if layers.is_empty() {
            layers.push(Layer::new("Layer 1", width, height));
        }
        if !layers.iter().any(|layer| layer.kind == LayerKind::Paint) {
            layers.push(Layer::new("Layer 1", width, height));
        }
        editor.document.layers = layers;
        editor.document.active = tape
            .active
            .map(|active| (active as usize).min(editor.document.layers.len() - 1))
            .unwrap_or(editor.document.layers.len() - 1);
        editor
    }
}
