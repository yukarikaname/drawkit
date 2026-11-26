use super::*;
use crate::fill::Mask;

mod begin;
mod cache;
pub(crate) use cache::SmartEdgeCache;

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
        self.finish_fill_animation();
        self.cancel_prediction();
        self.time_ms = time_ms;
        let (soft, color, under) = self.fill_mask(x, y, tolerance, color, options)?;
        let layer = self.document.active;
        let rect = Rect::new(0, 0, self.document.width(), self.document.height());
        let before = self.document.layers[layer].surface.snapshot(rect);
        if self.document.layers[layer].alpha_locked {
            self.document.layers[layer]
                .surface
                .blend_mask_locked_at(&soft, 0, 0, color);
        } else if under {
            self.document.layers[layer]
                .surface
                .blend_mask_under_at(&soft, 0, 0, color);
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
                    smart: options.smart,
                },
            );
        }
        Some(rect)
    }

    /// The prepared GPU fill's mask, one float per canvas pixel.
    pub fn gpu_fill_mask(&self) -> Option<&[f32]> {
        let animation = self.fill_animation.as_ref()?;
        if !animation.gpu {
            return None;
        }
        Some(animation.mask.data.as_slice())
    }

    /// Layer, region, colour and blending state of the prepared GPU fill.
    pub fn gpu_fill_state(&self) -> Option<(usize, Rect, Rgba, bool, bool)> {
        let animation = self.fill_animation.as_ref()?;
        if !animation.gpu {
            return None;
        }
        let alpha_locked = self
            .document
            .layers
            .get(animation.layer)
            .is_some_and(|layer| layer.alpha_locked);
        Some((
            animation.layer,
            animation.bounds,
            animation.color,
            animation.under,
            alpha_locked,
        ))
    }

    /// Record a fill the surface has already written into the resident tiles as one undo step and
    /// one tape event. The pixels stay on the GPU; the history keeps the rectangles, exactly as it
    /// does for a GPU stroke.
    pub fn finish_gpu_fill(&mut self, rects: &[Rect]) -> bool {
        let Some(animation) = self.fill_animation.as_ref() else {
            return false;
        };
        if !animation.gpu {
            return false;
        }
        let Some(animation) = self.fill_animation.take() else {
            return false;
        };
        if rects.is_empty() {
            return false;
        }
        self.record_gpu_step(animation.layer, rects);
        if self.recording_enabled {
            self.recording.push(
                animation.time_ms,
                Command::Fill {
                    layer: animation.layer,
                    x: (animation.tap.0 - 0.5) as usize,
                    y: (animation.tap.1 - 0.5) as usize,
                    tolerance: animation.tolerance,
                    color: animation.color,
                    close: animation.options.close,
                    expand: animation.options.expand,
                    feather: animation.options.feather,
                    smart: animation.options.smart,
                },
            );
        }
        true
    }

    /// Drop a GPU fill whose surface write failed or that the painter cancelled. No pixels and no
    /// history step exist yet.
    pub fn cancel_gpu_fill(&mut self) -> bool {
        let Some(animation) = self.fill_animation.as_ref() else {
            return false;
        };
        if !animation.gpu {
            return false;
        }
        self.fill_animation = None;
        true
    }

    /// Reveal the prepared fill up to `progress` (0...1). Pixels are written exactly once, in
    /// distance bands from the tap, so the result is the same as the instant fill.
    ///
    /// A GPU fill is applied in one pass by the surface, so there is nothing to reveal here.
    pub fn fill_animation_step(&mut self, progress: f32) -> Option<Rect> {
        let animation = self.fill_animation.as_mut()?;
        if animation.gpu {
            return None;
        }
        let radius = (progress.clamp(0.0, 1.0) * animation.max_distance).max(animation.applied);
        if radius <= animation.applied + 1e-4 {
            return None;
        }
        let bounds = animation.bounds;
        let mut ring = Mask::new(bounds.width, bounds.height);
        for y in 0..bounds.height {
            for x in 0..bounds.width {
                let value = animation.mask.get(bounds.x + x, bounds.y + y);
                if value <= 0.0 {
                    continue;
                }
                let dx = (bounds.x + x) as f32 + 0.5 - animation.tap.0;
                let dy = (bounds.y + y) as f32 + 0.5 - animation.tap.1;
                let distance = (dx * dx + dy * dy).sqrt();
                if distance > animation.applied && distance <= radius {
                    ring.set(x, y, value);
                }
            }
        }
        let layer = animation.layer;
        let color = animation.color;
        let under = animation.under;
        let alpha_locked = self.document.layers[layer].alpha_locked;
        let surface = &mut self.document.layers[layer].surface;
        if alpha_locked {
            surface.blend_mask_locked_at(&ring, bounds.x, bounds.y, color);
        } else if under {
            surface.blend_mask_under_at(&ring, bounds.x, bounds.y, color);
        } else {
            surface.blend_mask_at(&ring, bounds.x, bounds.y, color);
        }
        animation.applied = radius;
        Some(bounds)
    }

    /// Commit the animation as one pixel step and one tape fill.
    pub fn finish_fill_animation(&mut self) -> bool {
        let Some(mut animation) = self.fill_animation.take() else {
            return false;
        };
        // A GPU fill is finished by the surface, not here; dropping the plan leaves no history
        // step, which is right because nothing reached either the tiles or the CPU surface.
        if animation.gpu {
            return false;
        }
        if animation.applied + 1e-4 < animation.max_distance {
            self.fill_animation = Some(animation);
            self.fill_animation_step(1.0);
            animation = self.fill_animation.take().expect("animation was put back");
        }
        let after = self.document.layers[animation.layer]
            .surface
            .snapshot(animation.bounds);
        let serial = self.next_serial();
        self.history.push(
            serial,
            animation.layer,
            animation.bounds,
            animation.before,
            after,
        );
        self.layer_redo.clear();
        if self.recording_enabled {
            self.recording.push(
                animation.time_ms,
                Command::Fill {
                    layer: animation.layer,
                    x: (animation.tap.0 - 0.5) as usize,
                    y: (animation.tap.1 - 0.5) as usize,
                    tolerance: animation.tolerance,
                    color: animation.color,
                    close: animation.options.close,
                    expand: animation.options.expand,
                    feather: animation.options.feather,
                    smart: animation.options.smart,
                },
            );
        }
        true
    }

    pub fn cancel_fill_animation(&mut self) -> bool {
        let Some(animation) = self.fill_animation.take() else {
            return false;
        };
        if animation.gpu {
            // The surface refused the write or the painter cancelled; no pixel moved.
            return true;
        }
        self.document.layers[animation.layer]
            .surface
            .restore(animation.bounds, &animation.before);
        true
    }

    pub fn fill_animation_active(&self) -> bool {
        self.fill_animation.is_some()
    }

    /// A second fill of the same region should not shift its colour. When the seed of the fill is
    /// already an opaque colour within a few levels of the requested one, keep the existing colour;
    /// a deliberate change beyond that still paints.
    /// Keep the requested colour unless the active layer already holds an opaque colour within a
    /// hair of it at the tap. `existing` is that pixel.
    fn snapped_fill_colour(&self, existing: Rgba, requested: Rgba) -> Rgba {
        if !self.fill_snap {
            return requested;
        }
        let Some(layer) = self.document.layers.get(self.document.active) else {
            return requested;
        };
        if layer.kind != LayerKind::Paint {
            return requested;
        }
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

#[derive(Clone)]
pub(super) struct FillAnimation {
    layer: usize,
    bounds: Rect,
    mask: Mask,
    tap: (f32, f32),
    max_distance: f32,
    applied: f32,
    color: Rgba,
    under: bool,
    before: Vec<Rgba>,
    time_ms: u64,
    tolerance: f32,
    options: FillOptions,
    /// The surface applies this mask into the resident tiles; the core wrote nothing.
    gpu: bool,
}

fn mask_region(mask: &Mask, tap: (usize, usize)) -> Option<(Rect, f32)> {
    let mut min_x = usize::MAX;
    let mut min_y = usize::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut distance_squared = 0.0f32;
    // Within one row the squared distance is convex in x, so its maximum over the covered
    // columns is at the first or the last one. That keeps the scan to two distance tests per
    // row instead of one per pixel, which matters on a full-canvas fill.
    for y in 0..mask.height {
        let row = &mask.data[y * mask.width..(y + 1) * mask.width];
        let Some(first) = row.iter().position(|value| *value > 0.0) else {
            continue;
        };
        let last = row
            .iter()
            .rposition(|value| *value > 0.0)
            .expect("the row has a non-zero pixel");
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        min_x = min_x.min(first);
        max_x = max_x.max(last);
        let dy = y as f32 - tap.1 as f32;
        for x in [first, last] {
            let dx = x as f32 - tap.0 as f32;
            distance_squared = distance_squared.max(dx * dx + dy * dy);
        }
    }
    if min_x == usize::MAX {
        return None;
    }
    Some((
        Rect::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1),
        distance_squared.sqrt().max(1.0),
    ))
}

#[cfg(test)]
mod gpu_fill_tests;
#[cfg(test)]
mod smart_cache_tests;
