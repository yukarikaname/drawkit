use super::frames::Frame;
use super::*;

impl Editor {
    /// Open a project, a Krita/Photoshop document or a PNG/JPEG into a new editor sized to the file.
    ///
    /// The shell gets a handle whose canvas is the file's own size; it does not have to peek at the
    /// header or guess a maximum. Unknown extensions return `None`.
    pub fn open_file(path: &std::path::Path) -> Option<Editor> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "dkp" => {
                let tape = drawkit_formats::read_dkp(path).ok()?;
                let mut editor = crate::replay::replay_tape_with(
                    &tape,
                    drawkit_formats::TapeOutput::default(),
                    true,
                );
                editor.recording = crate::record::Recording::from_tape(&tape.events);
                editor.recording_enabled = true;
                editor.clear_history();
                if !tape.frames.is_empty() {
                    let active = tape.active_frame.min(tape.frames.len() - 1);
                    let mut frames = Vec::with_capacity(tape.frames.len());
                    for (index, frame_tape) in tape.frames.iter().enumerate() {
                        if index == active {
                            frames.push(None);
                            continue;
                        }
                        let mut frame = crate::replay::replay_tape_with(
                            frame_tape,
                            drawkit_formats::TapeOutput::default(),
                            true,
                        );
                        frame.recording = crate::record::Recording::from_tape(&frame_tape.events);
                        frame.recording_enabled = true;
                        frame.clear_history();
                        frames.push(Some(Frame {
                            document: frame.document,
                            history: frame.history,
                            recording: frame.recording,
                            time_ms: frame.time_ms,
                            duration_ms: frame_tape
                                .frame_durations
                                .get(index)
                                .copied()
                                .unwrap_or_else(|| 1000_u32 / u32::from(frame_tape.fps.max(1))),
                        }));
                    }
                    editor.frames = frames;
                    editor.active_frame = active;
                    editor.fps = tape.fps;
                    editor.loop_enabled = tape.loop_enabled;
                    editor.loop_start = tape.loop_start;
                    editor.loop_end = tape.loop_end;
                    editor.frame_duration = tape
                        .frame_durations
                        .get(active)
                        .copied()
                        .unwrap_or_else(|| 1000_u32 / u32::from(tape.fps.max(1)));
                }
                Some(editor)
            }
            "kra" | "psd" | "png" | "jpg" | "jpeg" | "webp" => {
                let imported = match extension.as_str() {
                    "kra" => drawkit_formats::read_kra(path).ok()?,
                    "psd" => drawkit_formats::read_psd(path).ok()?,
                    _ => drawkit_formats::read_image(path).ok()?,
                };
                let mut editor = Editor::new(imported.width, imported.height, 256 << 20);
                if !editor.import_document(&imported) {
                    return None;
                }
                Some(editor)
            }
            _ => None,
        }
    }

    /// Replace the document with an imported Krita or Photoshop document.
    ///
    /// The format crates have already flattened groups and offsets into canvas-sized layers. Here
    /// the image is fitted into this editor's fixed canvas: preserve aspect, centre, nearest
    /// sample. Import is a new document, so history and the recording tape start over.
    pub fn import_document(&mut self, imported: &drawkit_formats::ImportedDocument) -> bool {
        if imported.layers.is_empty() || imported.width == 0 || imported.height == 0 {
            return false;
        }
        let (document_width, document_height) = (self.document.width(), self.document.height());
        let scale = (document_width as f32 / imported.width as f32)
            .min(document_height as f32 / imported.height as f32);
        let target_width = (imported.width as f32 * scale).round().max(1.0) as usize;
        let target_height = (imported.height as f32 * scale).round().max(1.0) as usize;
        let offset_x = (document_width.saturating_sub(target_width)) / 2;
        let offset_y = (document_height.saturating_sub(target_height)) / 2;

        let mut layers = Vec::with_capacity(imported.layers.len());
        for spec in &imported.layers {
            let mut layer =
                crate::document::Layer::new(spec.name.clone(), document_width, document_height);
            layer.visible = spec.visible;
            layer.opacity = spec.opacity.clamp(0.0, 1.0);
            let source_width = spec.width.max(1);
            let source_height = spec.height.max(1);
            if spec.rgba.len() < source_width * source_height * 4 {
                continue;
            }
            for y in 0..target_height {
                let source_y = y * source_height / target_height;
                for x in 0..target_width {
                    let source_x = x * source_width / target_width;
                    let index = (source_y * source_width + source_x) * 4;
                    let pixel = &spec.rgba[index..index + 4];
                    if pixel[3] == 0 {
                        continue;
                    }
                    let target_x = offset_x + x;
                    let target_y = offset_y + y;
                    if target_x >= document_width || target_y >= document_height {
                        continue;
                    }
                    layer.surface.set(
                        target_x,
                        target_y,
                        [
                            pixel[0] as f32 / 255.0,
                            pixel[1] as f32 / 255.0,
                            pixel[2] as f32 / 255.0,
                            pixel[3] as f32 / 255.0,
                        ],
                    );
                }
            }
            layer
                .surface
                .mark(Rect::new(0, 0, document_width, document_height));
            layers.push(layer);
        }
        if layers.is_empty() {
            return false;
        }
        self.document.layers = layers;
        self.document.active = self.document.layers.len() - 1;
        self.document.color_space = crate::document::ColorSpace::from_i32(imported.color_space);
        self.clear_history();
        self.recording.clear();
        true
    }

    /// Save the document as a Krita file Krita can reopen.
    /// Export the visible painting over paper as JPEG or lossless WebP.
    pub fn export_raster(
        &self,
        path: &std::path::Path,
        format: drawkit_formats::RasterFormat,
        quality: u8,
    ) -> bool {
        let (width, height) = (self.document.width(), self.document.height());
        let pixels = self
            .document
            .composite_rect_export(Rect::new(0, 0, width, height));
        let mut rgb = Vec::with_capacity(width * height * 3);
        for pixel in pixels {
            let shown = over(pixel, PAPER);
            for channel in &shown[..3] {
                rgb.push((channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        drawkit_formats::write_raster(path, width, height, &rgb, format, quality).is_ok()
    }

    /// Turn an imported, opaque layer of ink on paper into transparent ink.
    ///
    /// This is the no-selection half of layer colour separation: the paper contributes white to
    /// every channel, so the least covered channel is how much of the ink is there. That coverage
    /// becomes alpha and the core's decontamination solves the colour back out. Only a layer with
    /// no recorded strokes is accepted - a painted layer's straight colour already is the ink, and
    /// running the same solve on it would darken its edges.
    pub fn remove_paper_from_layer(&mut self) -> bool {
        if !self.document.can_paint() {
            return false;
        }
        let layer_index = self.document.active;
        if self.recording.events().iter().any(|event| {
            matches!(
                event.command,
                Command::Stroke { layer, .. }
                    | Command::Erase { layer, .. }
                    | Command::Fill { layer, .. }
                    | Command::Smudge { layer, .. }
                    | Command::Clear { layer, .. }
                    if layer == layer_index
            )
        }) {
            return false;
        }
        let (width, height) = (self.document.width(), self.document.height());
        let rect = Rect::new(0, 0, width, height);
        let surface = &mut self.document.layers[layer_index].surface;
        let before = surface.snapshot(rect);
        for y in 0..height {
            for x in 0..width {
                let pixel = surface.get(x, y);
                let coverage = 1.0 - pixel[0].min(pixel[1]).min(pixel[2]);
                let result = if coverage <= 0.02 || pixel[3] <= 0.0 {
                    [0.0; 4]
                } else {
                    crate::color::decontaminate(pixel, PAPER, coverage)
                };
                surface.set(x, y, result);
            }
        }
        let after = surface.snapshot(rect);
        self.record_pixel_step(layer_index, rect, before, after);
        true
    }

    pub fn export_kra(&self, path: &std::path::Path) -> bool {
        let document = self.export_document();
        match drawkit_formats::write_kra(&document) {
            Ok(bytes) => std::fs::write(path, bytes).is_ok(),
            Err(error) => {
                eprintln!("export kra: {error}");
                false
            }
        }
    }

    /// Save the document as a layered Photoshop file.
    pub fn export_psd(&self, path: &std::path::Path) -> bool {
        let document = self.export_document();
        match drawkit_formats::write_psd(&document) {
            Ok(bytes) => std::fs::write(path, bytes).is_ok(),
            Err(error) => {
                eprintln!("export psd: {error}");
                false
            }
        }
    }

    fn export_document(&self) -> drawkit_formats::ExportDocument {
        let (width, height) = (self.document.width(), self.document.height());
        let layers = self
            .document
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| {
                layer.kind != crate::document::LayerKind::Group && !layer.reference
            })
            .map(|(index, layer)| {
                let mut rgba = Vec::with_capacity(width * height * 4);
                for y in 0..height {
                    for x in 0..width {
                        let pixel = if layer.kind == crate::document::LayerKind::Fill {
                            layer.fill
                        } else {
                            layer.surface.get(x, y)
                        };
                        rgba.push((pixel[0].clamp(0.0, 1.0) * 255.0).round() as u8);
                        rgba.push((pixel[1].clamp(0.0, 1.0) * 255.0).round() as u8);
                        rgba.push((pixel[2].clamp(0.0, 1.0) * 255.0).round() as u8);
                        rgba.push((pixel[3].clamp(0.0, 1.0) * 255.0).round() as u8);
                    }
                }
                drawkit_formats::ExportLayer {
                    name: layer.name.clone(),
                    visible: self.document.effective_visible(index),
                    opacity: self.document.effective_opacity(index),
                    rgba,
                }
            })
            .collect();
        drawkit_formats::ExportDocument {
            width,
            height,
            layers,
        }
    }

    /// Add a layer above the active one from premultiplied RGBA8.
    ///
    /// The input format is the one every renderer and window system produces; the surface wants
    /// straight alpha, so the colour is divided by alpha once on the way in. Transparent pixels are
    /// skipped instead of stored as black, which keeps the tile sparse and the edges clean.
    pub fn add_image_layer(&mut self, pixels: &[u8], width: usize, height: usize) -> bool {
        if width == 0 || height == 0 || pixels.len() < width * height * 4 {
            return false;
        }
        let name = format!("Reference {}", self.document.layers.len() + 1);
        self.document.add_layer(name);
        let layer = self.document.active;
        let surface = &mut self.document.layers[layer].surface;
        for y in 0..height.min(surface.height()) {
            for x in 0..width.min(surface.width()) {
                let index = (y * width + x) * 4;
                let alpha = pixels[index + 3] as f32 / 255.0;
                if alpha <= 0.0 {
                    continue;
                }
                let red = (pixels[index] as f32 / 255.0 / alpha).min(1.0);
                let green = (pixels[index + 1] as f32 / 255.0 / alpha).min(1.0);
                let blue = (pixels[index + 2] as f32 / 255.0 / alpha).min(1.0);
                surface.set(x, y, [red, green, blue, alpha]);
            }
        }
        surface.mark(Rect::new(
            0,
            0,
            width.min(surface.width()),
            height.min(surface.height()),
        ));
        true
    }

    /// A premultiplied thumbnail of one layer, for the layer palette. Averaging in premultiplied
    /// space keeps transparent pixels from pulling the colour of a mostly empty block towards
    /// black, which is the usual downscale bug.
    pub fn layer_thumbnail(&self, index: usize, width: usize, height: usize) -> Option<Vec<Rgba>> {
        let layer = self.document.layers.get(index)?;
        match layer.kind {
            crate::document::LayerKind::Group => return None,
            crate::document::LayerKind::Fill => {
                if width == 0 || height == 0 {
                    return None;
                }
                let mut out = vec![layer.fill; width * height];
                for pixel in &mut out {
                    pixel[3] *= layer.opacity;
                }
                return Some(out);
            }
            crate::document::LayerKind::Paint => {}
        }
        if width == 0 || height == 0 {
            return None;
        }
        let (document_width, document_height) = (self.document.width(), self.document.height());
        let mut sums = vec![[0.0_f32; 4]; width * height];
        let mut counts = vec![0_u32; width * height];
        // Four samples per output pixel on each axis. Walking all eight million pixels of an 8K
        // layer to make forty rows of preview is what made the palette feel slow after a stroke;
        // a fourth of that is still enough to keep a block's average.
        let stride_x = (document_width / (width * 4).max(1)).max(1);
        let stride_y = (document_height / (height * 4).max(1)).max(1);
        for y in (0..document_height).step_by(stride_y) {
            let target_y = y * height / document_height;
            for x in (0..document_width).step_by(stride_x) {
                let target_x = x * width / document_width;
                let pixel = layer.surface.get(x, y);
                let target = target_y * width + target_x;
                sums[target][0] += pixel[0] * pixel[3];
                sums[target][1] += pixel[1] * pixel[3];
                sums[target][2] += pixel[2] * pixel[3];
                sums[target][3] += pixel[3];
                counts[target] += 1;
            }
        }
        let mut out = vec![[0.0_f32; 4]; width * height];
        for (index, slot) in out.iter_mut().enumerate() {
            let count = counts[index].max(1) as f32;
            *slot = [
                sums[index][0] / count,
                sums[index][1] / count,
                sums[index][2] / count,
                sums[index][3] / count,
            ];
        }
        Some(out)
    }

    /// The colour visible at `x, y`: the composite over the paper, as straight RGBA.
    ///
    /// An eyedropper samples what the painter sees, not what the active layer stores. On a
    /// transparent layer those are different answers.
    pub fn pick(&self, x: usize, y: usize) -> Option<Rgba> {
        if x >= self.document.width() || y >= self.document.height() {
            return None;
        }
        let pixel = self.document.composite_rect(Rect::new(x, y, 1, 1))[0];
        Some(over(pixel, PAPER))
    }

    pub fn clear(&mut self, color: Rgba) {
        self.clear_at(color, self.time_ms)
    }

    pub fn clear_at(&mut self, color: Rgba, time_ms: u64) {
        self.cancel_prediction();
        self.time_ms = time_ms;
        if !self.document.can_paint() || self.document.active().alpha_locked {
            return;
        }
        let (width, height) = (self.document.width(), self.document.height());
        let rect = self.selection.unwrap_or(Rect::new(0, 0, width, height));
        let layer = self.document.active;
        let before = self.document.layers[layer].surface.snapshot(rect);
        let surface = &mut self.document.layers[layer].surface;
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                surface.set(x, y, color);
            }
        }
        let after = self.document.layers[layer].surface.snapshot(rect);
        self.record_pixel_step(layer, rect, before, after);
        if self.recording_enabled {
            self.recording.push(
                time_ms,
                Command::Clear {
                    layer: self.document.active,
                    color,
                },
            );
        }
    }

    /// The stroke tape plus the layer tree. This is the project: pixels come back from the
    /// strokes, and a layer that was imported or baked carries its pixels as a raster.
    /// A copy of everything a project file needs, without the undo stack. A background save uses
    /// it so the worker does not hold a second copy of every undo snapshot.
    pub fn save_snapshot(&self) -> Editor {
        Editor {
            document: self.document.clone(),
            history: History::new(1),
            recording: self.recording.clone(),
            recording_enabled: false,
            time_ms: self.time_ms,
            stroke: None,
            next_group: self.next_group,
            next_seed: self.next_seed,
            stabilization: self.stabilization,
            smoothed: None,
            mirror: self.mirror,
            brush: self.brush,
            brushes: self.brushes,
            brush_texture: self.brush_texture.clone(),
            fill_snap: self.fill_snap,
            next_serial: self.next_serial,
            layer_undo: self.layer_undo.clone(),
            layer_redo: self.layer_redo.clone(),
            stroke_dedupe: self.stroke_dedupe,
            prediction_enabled: false,
            auto_shape: self.auto_shape,
            quick_shape: self.quick_shape,
            predictor: Predictor::new(6),
            prediction: None,
            replay_cache: None,
            selection: None,
            fill_animation: None,
            frames: self.frames.clone(),
            active_frame: self.active_frame,
            fps: self.fps,
            frame_duration: self.frame_duration,
            loop_enabled: self.loop_enabled,
            loop_start: self.loop_start,
            loop_end: self.loop_end,
        }
    }
}
