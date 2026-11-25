use super::*;

fn luminance(pixel: [f32; 3]) -> f32 {
    0.30 * pixel[0] + 0.59 * pixel[1] + 0.11 * pixel[2]
}

/// Give `pixel` the luminance `target`, keeping its hue and saturation. The result can leave the
/// unit cube after the shift, so clip by moving toward the target rather than clamping each channel
/// independently, which would change the hue.
fn with_luminance(pixel: [f32; 3], target: f32) -> [f32; 3] {
    let delta = target - luminance(pixel);
    let shifted = [pixel[0] + delta, pixel[1] + delta, pixel[2] + delta];
    let low = shifted[0].min(shifted[1]).min(shifted[2]);
    let high = shifted[0].max(shifted[1]).max(shifted[2]);
    if low < 0.0 {
        let scale = target / (target - low).max(1e-6);
        [
            target + (shifted[0] - target) * scale,
            target + (shifted[1] - target) * scale,
            target + (shifted[2] - target) * scale,
        ]
    } else if high > 1.0 {
        let scale = (1.0 - target) / (high - target).max(1e-6);
        [
            target + (shifted[0] - target) * scale,
            target + (shifted[1] - target) * scale,
            target + (shifted[2] - target) * scale,
        ]
    } else {
        shifted
    }
}

fn blend(mode: BlendMode, top: [f32; 3], bottom: [f32; 3]) -> [f32; 3] {
    if mode == BlendMode::Color {
        // The layer supplies hue and saturation; the painting below supplies the brightness.
        return with_luminance(top, luminance(bottom).clamp(0.0, 1.0));
    }
    if mode == BlendMode::Luminosity {
        return with_luminance(bottom, luminance(top).clamp(0.0, 1.0));
    }
    let channel = |index: usize| match mode {
        BlendMode::Normal => top[index],
        BlendMode::Multiply => top[index] * bottom[index],
        BlendMode::Screen => 1.0 - (1.0 - top[index]) * (1.0 - bottom[index]),
        BlendMode::Overlay => {
            if bottom[index] < 0.5 {
                2.0 * top[index] * bottom[index]
            } else {
                1.0 - 2.0 * (1.0 - top[index]) * (1.0 - bottom[index])
            }
        }
        BlendMode::Add => (top[index] + bottom[index]).min(1.0),
        BlendMode::Darken => top[index].min(bottom[index]),
        BlendMode::Lighten => top[index].max(bottom[index]),
        BlendMode::Color | BlendMode::Luminosity => unreachable!("handled above"),
    };
    [channel(0), channel(1), channel(2)]
}

impl Document {
    /// The pixels of `rect`, bottom layer up, with group and layer opacity applied.
    pub fn composite_rect(&self, rect: Rect) -> Vec<Rgba> {
        self.composite_rect_with(rect, false)
    }

    /// The same rect for a file: reference layers are on the canvas but not in the export.
    pub fn composite_rect_export(&self, rect: Rect) -> Vec<Rgba> {
        self.composite_rect_with(rect, true)
    }

    fn composite_rect_with(&self, rect: Rect, skip_references: bool) -> Vec<Rgba> {
        let mut out = vec![[0.0; 4]; rect.width * rect.height];
        self.composite_children(None, rect, &mut out, skip_references);
        out
    }

    /// The canvas resampled to `width` by `height` for a scrub preview. Nearest sampling keeps
    /// the cost at the size of the preview instead of the size of the painting; a 4K canvas at a
    /// 1024 preview costs a sixteenth as much per step.
    pub fn composite_scaled(&self, width: usize, height: usize) -> Vec<Rgba> {
        let mut out = vec![[0.0; 4]; width * height];
        if width == 0 || height == 0 || self.width == 0 || self.height == 0 {
            return out;
        }
        self.composite_sampled(
            None,
            width,
            height,
            &|x, y| (x * self.width / width, y * self.height / height),
            &mut out,
            false,
        );
        out
    }

    pub fn hdr(&self) -> bool {
        self.dynamic_range == DynamicRange::Hdr
    }

    /// The same rectangle in linear light, premultiplied and converted to extended linear
    /// Display P3. This is what an extended-range surface and a linear EXR file want; the 8-bit
    /// path above stays in the working space because that is what a normal screen shows.
    pub fn composite_linear_rect(&self, rect: Rect) -> Vec<Rgba> {
        self.composite_linear_rect_with(rect, false)
    }

    pub fn composite_linear_rect_export(&self, rect: Rect) -> Vec<Rgba> {
        self.composite_linear_rect_with(rect, true)
    }

    fn composite_linear_rect_with(&self, rect: Rect, skip_references: bool) -> Vec<Rgba> {
        let matrix = self.color_space.to_display_p3();
        self.composite_rect_with(rect, skip_references)
            .into_iter()
            .map(|pixel| {
                let alpha = pixel[3].clamp(0.0, 1.0);
                let r = self.color_space.decode(pixel[0]);
                let g = self.color_space.decode(pixel[1]);
                let b = self.color_space.decode(pixel[2]);
                let linear = [
                    matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b,
                    matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b,
                    matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b,
                ];
                [
                    linear[0].max(0.0) * alpha,
                    linear[1].max(0.0) * alpha,
                    linear[2].max(0.0) * alpha,
                    alpha,
                ]
            })
            .collect()
    }

    /// The nearest layer below `index` in the same group that is not itself clipped. That layer
    /// supplies the transparency a clipping mask shows through.
    fn clip_base_index(&self, parent: Option<usize>, index: usize) -> Option<usize> {
        (0..index).rev().find(|candidate| {
            self.layers[*candidate].parent == parent && !self.layers[*candidate].clipping
        })
    }

    /// The layer a clipped layer shows through. The GPU path can use a group base through its
    /// isolated buffer; the CPU walk still treats a group as no base.
    pub fn clip_base(&self, index: usize) -> Option<usize> {
        let layer = self.layers.get(index)?;
        if !layer.clipping {
            return None;
        }
        self.clip_base_index(layer.parent, index)
    }

    /// The alpha of one layer at a pixel, ignoring what is below it. A group contributes the
    /// coverage of its visible children.
    fn base_alpha(&self, index: usize, x: usize, y: usize) -> f32 {
        let Some(layer) = self.layers.get(index) else {
            return 0.0;
        };
        if !layer.visible {
            return 0.0;
        }
        match layer.kind {
            LayerKind::Paint => layer.surface.get(x, y)[3],
            LayerKind::Fill => layer.fill[3],
            LayerKind::Group => {
                let mut alpha = 0.0;
                for (child, child_layer) in self.layers.iter().enumerate() {
                    if child_layer.parent != Some(index) || !child_layer.visible {
                        continue;
                    }
                    let child_alpha = self.base_alpha(child, x, y);
                    alpha = 1.0 - (1.0 - alpha) * (1.0 - child_alpha);
                }
                alpha
            }
        }
    }

    fn composite_children(
        &self,
        parent: Option<usize>,
        rect: Rect,
        out: &mut [Rgba],
        skip_references: bool,
    ) {
        self.composite_sampled(
            parent,
            rect.width,
            rect.height,
            &|x, y| (rect.x + x, rect.y + y),
            out,
            skip_references,
        );
    }

    fn composite_sampled<F>(
        &self,
        parent: Option<usize>,
        width: usize,
        height: usize,
        sample: &F,
        out: &mut [Rgba],
        skip_references: bool,
    ) where
        F: Fn(usize, usize) -> (usize, usize),
    {
        for (index, layer) in self.layers.iter().enumerate() {
            if layer.parent != parent
                || !layer.visible
                || layer.opacity <= 0.0
                || (skip_references && layer.reference)
            {
                continue;
            }
            let clip_base = if layer.clipping {
                self.clip_base_index(parent, index)
            } else {
                None
            };
            match layer.kind {
                LayerKind::Paint => {
                    // Allocated tiles are the only place this layer can have ink, so a sparse
                    // layer costs its own footprint, not the whole canvas.
                    let Some(painted) = layer.surface.painted_bounds() else {
                        continue;
                    };
                    for oy in 0..height {
                        for ox in 0..width {
                            let (x, y) = sample(ox, oy);
                            if x < painted.x
                                || y < painted.y
                                || x >= painted.x + painted.width
                                || y >= painted.y + painted.height
                            {
                                continue;
                            }
                            let pixel = layer.surface.get(x, y);
                            if pixel[3] <= 0.0 {
                                continue;
                            }
                            let target = oy * width + ox;
                            let clip = clip_base.map_or(1.0, |base| self.base_alpha(base, x, y));
                            let alpha = pixel[3] * layer.opacity * clip;
                            if alpha <= 0.0 {
                                continue;
                            }
                            let backdrop = out[target];
                            let colour = blend(
                                layer.blend,
                                [pixel[0], pixel[1], pixel[2]],
                                [backdrop[0], backdrop[1], backdrop[2]],
                            );
                            out[target] = over([colour[0], colour[1], colour[2], alpha], backdrop);
                        }
                    }
                }
                LayerKind::Group => {
                    let mut group = vec![[0.0; 4]; width * height];
                    self.composite_sampled(
                        Some(index),
                        width,
                        height,
                        sample,
                        &mut group,
                        skip_references,
                    );
                    for (target, pixel) in group.into_iter().enumerate() {
                        if pixel[3] <= 0.0 {
                            continue;
                        }
                        let (x, y) = sample(target % width, target / width);
                        let clip = clip_base.map_or(1.0, |base| self.base_alpha(base, x, y));
                        let alpha = pixel[3] * layer.opacity * clip;
                        if alpha <= 0.0 {
                            continue;
                        }
                        let backdrop = out[target];
                        let colour = blend(
                            layer.blend,
                            [pixel[0], pixel[1], pixel[2]],
                            [backdrop[0], backdrop[1], backdrop[2]],
                        );
                        out[target] = over([colour[0], colour[1], colour[2], alpha], backdrop);
                    }
                }
                LayerKind::Fill => {
                    if layer.fill[3] <= 0.0 {
                        continue;
                    }
                    for oy in 0..height {
                        for ox in 0..width {
                            let (x, y) = sample(ox, oy);
                            if x >= self.width || y >= self.height {
                                continue;
                            }
                            let clip = clip_base.map_or(1.0, |base| self.base_alpha(base, x, y));
                            let alpha = layer.fill[3] * layer.opacity * clip;
                            if alpha <= 0.0 {
                                continue;
                            }
                            let target = oy * width + ox;
                            let backdrop = out[target];
                            let colour = blend(
                                layer.blend,
                                [layer.fill[0], layer.fill[1], layer.fill[2]],
                                [backdrop[0], backdrop[1], backdrop[2]],
                            );
                            out[target] = over([colour[0], colour[1], colour[2], alpha], backdrop);
                        }
                    }
                }
            }
        }
    }
}
