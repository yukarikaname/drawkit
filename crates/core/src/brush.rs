//! A round brush, stamped along the segment between two input samples.
//!
//! The tablet reports points; a brush draws a line. Stamping a disc along the segment at a
//! fraction of its radius is the cheapest thing that has no gaps and no banding: gaps come from
//! spacing larger than the disc, banding from a falloff with hard steps.
//!
//! A preset changes three things and nothing else: how much of the disc is solid, how much opacity
//! one segment lays down, and how much pressure changes the size. That is enough for a pencil, an
//! ink pen, a marker and an airbrush without a texture pipeline.

use crate::color::{Rgba, over};
use crate::fill::Mask;
use crate::surface::Surface;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushKind {
    SoftRound,
    Pencil,
    Ink,
    Marker,
    Airbrush,
}

impl BrushKind {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Pencil,
            2 => Self::Ink,
            3 => Self::Marker,
            4 => Self::Airbrush,
            _ => Self::SoftRound,
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::SoftRound => 0,
            Self::Pencil => 1,
            Self::Ink => 2,
            Self::Marker => 3,
            Self::Airbrush => 4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::SoftRound => "Soft round",
            Self::Pencil => "Pencil",
            Self::Ink => "Ink",
            Self::Marker => "Marker",
            Self::Airbrush => "Airbrush",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Brush {
    pub kind: BrushKind,
    /// 0 is a soft falloff to the rim, 1 is a solid disc with an anti-aliased edge.
    pub hardness: f32,
    /// Opacity laid down by one segment before pressure.
    pub flow: f32,
    /// Radius at zero pressure, as a fraction of the nominal radius.
    pub pressure_radius: f32,
    /// How much opacity is lost at zero pressure, as a fraction of `flow`.
    pub pressure_alpha: f32,
}

impl Brush {
    pub const fn soft_round() -> Self {
        Self {
            kind: BrushKind::SoftRound,
            hardness: 0.0,
            flow: 1.0,
            pressure_radius: 0.25,
            pressure_alpha: 0.0,
        }
    }

    pub const fn for_kind(kind: BrushKind) -> Self {
        match kind {
            BrushKind::SoftRound => Self::soft_round(),
            BrushKind::Pencil => Self {
                kind,
                hardness: 0.85,
                flow: 0.9,
                pressure_radius: 0.55,
                pressure_alpha: 0.35,
            },
            BrushKind::Ink => Self {
                kind,
                hardness: 1.0,
                flow: 1.0,
                pressure_radius: 0.8,
                pressure_alpha: 0.0,
            },
            BrushKind::Marker => Self {
                kind,
                hardness: 0.95,
                flow: 0.35,
                pressure_radius: 0.9,
                pressure_alpha: 0.0,
            },
            BrushKind::Airbrush => Self {
                kind,
                hardness: 0.0,
                flow: 0.07,
                pressure_radius: 0.6,
                pressure_alpha: 0.2,
            },
        }
    }

    /// The radius a sample of pressure `pressure` actually paints with.
    ///
    /// A pen at its lightest still leaves a mark - the floor is a fraction of the size, not zero -
    /// because a line that disappears at the start of every stroke is a line with gaps in it.
    pub fn radius_at(&self, radius: f32, pressure: f32) -> f32 {
        let p = pressure.clamp(0.0, 1.0);
        radius * (self.pressure_radius + (1.0 - self.pressure_radius) * p)
    }

    /// The alpha one segment lays down. Ink ignores pressure; a pencil at a light touch is faint.
    pub fn alpha_at(&self, pressure: f32) -> f32 {
        let p = pressure.clamp(0.0, 1.0);
        self.flow * (1.0 - self.pressure_alpha * (1.0 - p))
    }
}

fn coverage(distance: f32, radius: f32, hardness: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    let t = (1.0 - distance / radius).clamp(0.0, 1.0);
    // Keep a sliver of falloff even for the hardest brush, or the rim turns into a staircase.
    let edge = (1.0 - hardness).max(0.04);
    if t >= edge {
        1.0
    } else {
        let u = t / edge;
        u * u * (3.0 - 2.0 * u)
    }
}

/// One stamped disc, centred at a fractional pixel.
pub fn stamp(surface: &mut Surface, centre: (f32, f32), radius: f32, hardness: f32, color: Rgba) {
    if radius <= 0.0 || color[3] <= 0.0 {
        return;
    }
    let x0 = (centre.0 - radius).floor().max(0.0) as usize;
    let y0 = (centre.1 - radius).floor().max(0.0) as usize;
    let x1 = ((centre.0 + radius).ceil() as isize).max(0) as usize;
    let y1 = ((centre.1 + radius).ceil() as isize).max(0) as usize;
    for y in y0..=y1.min(surface.height().saturating_sub(1)) {
        for x in x0..=x1.min(surface.width().saturating_sub(1)) {
            let dx = x as f32 - centre.0;
            let dy = y as f32 - centre.1;
            let amount = coverage((dx * dx + dy * dy).sqrt(), radius, hardness);
            if amount <= 0.0 {
                continue;
            }
            let existing = surface.get(x, y);
            surface.set(
                x,
                y,
                over([color[0], color[1], color[2], color[3] * amount], existing),
            );
        }
    }
}

/// The coverage of one segment, and where its top-left corner sits on the canvas.
///
/// Both the brush and the eraser want this shape: stamp the discs, take the maximum, composite
/// once. The only difference is whether that composite adds colour or removes it.
fn segment_mask(
    width: usize,
    height: usize,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    hardness: f32,
) -> Option<(Mask, usize, usize)> {
    if radius <= 0.0 || width == 0 || height == 0 {
        return None;
    }
    let min_x = (from.0.min(to.0) - radius).floor().max(0.0) as usize;
    let min_y = (from.1.min(to.1) - radius).floor().max(0.0) as usize;
    let max_x = ((from.0.max(to.0) + radius).ceil().max(0.0) as usize).min(width.saturating_sub(1));
    let max_y =
        ((from.1.max(to.1) + radius).ceil().max(0.0) as usize).min(height.saturating_sub(1));
    if min_x > max_x || min_y > max_y {
        return None;
    }
    let mut mask = Mask::new(max_x - min_x + 1, max_y - min_y + 1);

    let mut plot = |cx: f32, cy: f32| {
        let x0 = ((cx - radius).floor().max(0.0) as usize).max(min_x);
        let y0 = ((cy - radius).floor().max(0.0) as usize).max(min_y);
        let x1 = ((cx + radius).ceil().max(0.0) as usize).min(max_x);
        let y1 = ((cy + radius).ceil().max(0.0) as usize).min(max_y);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let amount = coverage((dx * dx + dy * dy).sqrt(), radius, hardness);
                if amount <= 0.0 {
                    continue;
                }
                let mx = x - min_x;
                let my = y - min_y;
                mask.set(mx, my, mask.get(mx, my).max(amount));
            }
        }
    };

    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let distance = (dx * dx + dy * dy).sqrt();
    let spacing = (radius * 0.25).max(0.5);
    let steps = (distance / spacing).ceil() as usize;
    if steps == 0 {
        plot(to.0, to.1);
    } else {
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            plot(from.0 + dx * t, from.1 + dy * t);
        }
    }
    if mask.is_empty() {
        None
    } else {
        Some((mask, min_x, min_y))
    }
}

/// A brush stroke from one sample to the next.
///
/// The discs are collected into a coverage mask first and composited once. Stamping each disc
/// straight onto the layer composites its overlaps again and again, so the soft rim a single disc
/// draws becomes a hard, darker band twenty discs wide. The maximum coverage is the brush's
/// coverage; two separate strokes still accumulate, because they are two separate composites.
pub fn stroke(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    hardness: f32,
    color: Rgba,
) {
    if color[3] <= 0.0 {
        return;
    }
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        from,
        to,
        radius,
        hardness,
    ) {
        surface.blend_mask_at(&mask, x, y, color);
    }
}

/// A stroke on an alpha-locked layer: the existing coverage decides what may be painted, and the
/// stroke never changes the layer's transparency.
pub fn stroke_locked(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    hardness: f32,
    color: Rgba,
) {
    if color[3] <= 0.0 {
        return;
    }
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        from,
        to,
        radius,
        hardness,
    ) {
        surface.blend_mask_locked_at(&mask, x, y, color);
    }
}

/// Drag paint from where the brush was to where it is now.
///
/// The source is sampled from a snapshot of the whole touched rectangle, so a pixel written earlier
/// in the same segment never becomes the source for a later one. `strength` is how much of the
/// carried colour lands, and the falloff uses the same hardness as the brush.
pub fn smudge(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    hardness: f32,
    strength: f32,
) {
    let strength = strength.clamp(0.0, 1.0);
    if radius <= 0.0 || strength <= 0.0 {
        return;
    }
    let Some((mask, min_x, min_y)) = segment_mask(
        surface.width(),
        surface.height(),
        from,
        to,
        radius,
        hardness,
    ) else {
        return;
    };
    let left = (from.0.min(to.0) - radius).floor().max(0.0) as usize;
    let top = (from.1.min(to.1) - radius).floor().max(0.0) as usize;
    let right = ((from.0.max(to.0) + radius).ceil().max(0.0) as usize + 1).min(surface.width());
    let bottom = ((from.1.max(to.1) + radius).ceil().max(0.0) as usize + 1).min(surface.height());
    let region = crate::surface::Rect::new(
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    );
    if region.is_empty() {
        return;
    }
    let saved = surface.snapshot(region);
    let dx = from.0 - to.0;
    let dy = from.1 - to.1;
    for my in 0..mask.height {
        for mx in 0..mask.width {
            let amount = mask.get(mx, my) * strength;
            if amount <= 0.0 {
                continue;
            }
            let x = min_x + mx;
            let y = min_y + my;
            let sx = x as f32 + dx;
            let sy = y as f32 + dy;
            if sx < region.x as f32
                || sy < region.y as f32
                || sx >= (region.x + region.width) as f32
                || sy >= (region.y + region.height) as f32
            {
                continue;
            }
            let source = saved[(sy as usize - region.y) * region.width + (sx as usize - region.x)];
            if source[3] <= 0.0 {
                continue;
            }
            let destination = surface.get(x, y);
            surface.set(
                x,
                y,
                over(
                    [source[0], source[1], source[2], source[3] * amount],
                    destination,
                ),
            );
        }
    }
    surface.mark(region);
}

/// Remove alpha along the same segment shape. Colour is left alone: an eraser is a subtraction
/// from the alpha channel, not a bucket of transparent paint.
pub fn erase(surface: &mut Surface, from: (f32, f32), to: (f32, f32), radius: f32, hardness: f32) {
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        from,
        to,
        radius,
        hardness,
    ) {
        surface.erase_mask_at(&mask, x, y);
    }
}

#[cfg(test)]
mod tests;
