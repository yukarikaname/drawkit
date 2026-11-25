//! A round brush, stamped along the segment between two input samples.
//!
//! The tablet reports points; a brush draws a line. Stamping a disc along the segment at a
//! fraction of its radius is the cheapest thing that has no gaps and no banding: gaps come from
//! spacing larger than the disc, banding from a falloff with hard steps.
//!
//! A preset changes three things and nothing else: how much of the disc is solid, how much opacity
//! one segment lays down, and how much pressure changes the size. That is enough for a pencil, an
//! ink pen, a marker and an airbrush without a texture pipeline.

mod params;
mod preset;
mod preview;
mod segment;
mod shape;
mod stamp;
pub use preset::BrushPreset;
pub use preview::preview;
pub(crate) use segment::segment_mask;
pub use stamp::{smudge, stamp};

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
    Bristle,
    Spray,
    Chalk,
    Watercolor,
}

impl BrushKind {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Pencil,
            2 => Self::Ink,
            3 => Self::Marker,
            4 => Self::Airbrush,
            5 => Self::Bristle,
            6 => Self::Spray,
            7 => Self::Chalk,
            8 => Self::Watercolor,
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
            Self::Bristle => 5,
            Self::Spray => 6,
            Self::Chalk => 7,
            Self::Watercolor => 8,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::SoftRound => "Soft round",
            Self::Pencil => "Pencil",
            Self::Ink => "Ink",
            Self::Marker => "Marker",
            Self::Airbrush => "Airbrush",
            Self::Bristle => "Bristle",
            Self::Spray => "Spray",
            Self::Chalk => "Chalk",
            Self::Watercolor => "Watercolor",
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
    /// Stamp spacing as a fraction of the radius.
    pub spacing: f32,
    /// Random stamp offset as a fraction of the radius.
    pub jitter: f32,
    /// Stamp rotation in degrees.
    pub angle: f32,
    /// Short-axis length as a fraction of the radius; one is a circle.
    pub roundness: f32,
    /// Random per-pixel coverage reduction, zero is a clean stamp.
    pub grain: f32,
    /// Stable paper grain, sampled in canvas coordinates.
    pub paper: f32,
    /// Tip texture, stronger towards the rim of each dab.
    pub texture: f32,
    /// Colour picked up from the paint already under the stroke.
    pub wet: f32,
    /// Strength of a second, smaller tip offset perpendicular to the stroke.
    pub dual: f32,
    /// Size of the second tip as a fraction of the primary radius.
    pub dual_scale: f32,
}

impl Brush {
    pub const fn for_kind(kind: BrushKind) -> Self {
        match kind {
            BrushKind::SoftRound => Self::soft_round(),
            BrushKind::Pencil => Self {
                kind,
                hardness: 0.85,
                flow: 0.9,
                pressure_radius: 0.55,
                pressure_alpha: 0.35,
                spacing: 0.18,
                jitter: 0.02,
                angle: 0.0,
                roundness: 0.9,
                grain: 0.1,
                paper: 0.18,
                texture: 0.2,
                wet: 0.0,
                dual: 0.0,
                dual_scale: 0.7,
            },
            BrushKind::Ink => Self {
                kind,
                hardness: 1.0,
                flow: 1.0,
                pressure_radius: 0.8,
                pressure_alpha: 0.0,
                spacing: 0.12,
                jitter: 0.0,
                angle: 0.0,
                roundness: 1.0,
                grain: 0.0,
                paper: 0.0,
                texture: 0.0,
                wet: 0.0,
                dual: 0.0,
                dual_scale: 0.7,
            },
            BrushKind::Marker => Self {
                kind,
                hardness: 0.95,
                flow: 0.85,
                pressure_radius: 0.9,
                pressure_alpha: 0.0,
                spacing: 0.2,
                jitter: 0.02,
                angle: 20.0,
                roundness: 0.85,
                grain: 0.05,
                paper: 0.0,
                texture: 0.08,
                wet: 0.0,
                dual: 0.0,
                dual_scale: 0.7,
            },
            BrushKind::Airbrush => Self {
                kind,
                hardness: 0.0,
                flow: 0.35,
                pressure_radius: 0.6,
                pressure_alpha: 0.2,
                spacing: 0.3,
                jitter: 0.1,
                angle: 0.0,
                roundness: 1.0,
                grain: 0.0,
                paper: 0.0,
                texture: 0.12,
                wet: 0.0,
                dual: 0.0,
                dual_scale: 0.7,
            },
            BrushKind::Bristle => Self {
                kind,
                hardness: 0.75,
                flow: 0.75,
                pressure_radius: 0.7,
                pressure_alpha: 0.2,
                spacing: 0.12,
                jitter: 0.18,
                angle: 35.0,
                roundness: 0.35,
                grain: 0.35,
                paper: 0.1,
                texture: 0.45,
                wet: 0.0,
                dual: 0.25,
                dual_scale: 0.45,
            },
            BrushKind::Spray => Self {
                kind,
                hardness: 0.15,
                flow: 0.60,
                pressure_radius: 0.5,
                pressure_alpha: 0.1,
                spacing: 1.1,
                jitter: 0.8,
                angle: 0.0,
                roundness: 1.0,
                grain: 0.5,
                paper: 0.05,
                texture: 0.5,
                wet: 0.0,
                dual: 0.0,
                dual_scale: 0.7,
            },
            BrushKind::Chalk => Self {
                kind,
                hardness: 0.9,
                flow: 0.70,
                pressure_radius: 0.6,
                pressure_alpha: 0.3,
                spacing: 0.25,
                jitter: 0.25,
                angle: 15.0,
                roundness: 0.7,
                grain: 0.7,
                paper: 0.35,
                texture: 0.55,
                wet: 0.0,
                dual: 0.0,
                dual_scale: 0.7,
            },
            BrushKind::Watercolor => Self {
                kind,
                hardness: 0.0,
                flow: 0.45,
                pressure_radius: 0.4,
                pressure_alpha: 0.1,
                spacing: 0.18,
                jitter: 0.15,
                angle: 0.0,
                roundness: 0.9,
                grain: 0.25,
                paper: 0.2,
                texture: 0.2,
                wet: 0.45,
                dual: 0.0,
                dual_scale: 0.7,
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

/// A brush stroke from one sample to the next.
///
/// The discs are collected into a coverage mask first and composited once. Stamping each disc
/// straight onto the layer composites its overlaps again and again, so the soft rim a single disc
/// draws becomes a hard, darker band twenty discs wide. The maximum coverage is the brush's
/// coverage; two separate strokes still accumulate, because they are two separate composites.
pub fn stroke_with(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    brush: &Brush,
    color: Rgba,
    seed: u64,
) {
    let mut phase = -1.0;
    if color[3] <= 0.0 {
        return;
    }
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        (from, to),
        radius,
        brush,
        seed,
        &mut phase,
    ) {
        surface.blend_mask_at(&mask, x, y, color);
    }
}

/// The same stroke with a spacing phase carried across pointer samples.
pub fn stroke_with_phase(
    surface: &mut Surface,
    points: ((f32, f32), (f32, f32)),
    radius: f32,
    brush: &Brush,
    color: Rgba,
    seed: u64,
    phase: &mut f32,
) {
    let (from, to) = points;
    if color[3] <= 0.0 {
        return;
    }
    surface.set_stroke_wet(brush.wet);
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        (from, to),
        radius,
        brush,
        seed,
        phase,
    ) {
        surface.blend_stroke_mask_at(&mask, x, y, color, crate::surface::StrokeMask::Paint);
    }
}

/// The old round-brush entry point, kept for callers that only have a hardness.
pub fn stroke(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    hardness: f32,
    color: Rgba,
) {
    stroke_with(
        surface,
        from,
        to,
        radius,
        &Brush::from_hardness(hardness),
        color,
        0,
    );
}

/// A stroke on an alpha-locked layer: the existing coverage decides what may be painted, and the
/// stroke never changes the layer's transparency.
pub fn stroke_locked_with(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    brush: &Brush,
    color: Rgba,
    seed: u64,
) {
    let mut phase = -1.0;
    if color[3] <= 0.0 {
        return;
    }
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        (from, to),
        radius,
        brush,
        seed,
        &mut phase,
    ) {
        surface.blend_mask_locked_at(&mask, x, y, color);
    }
}

pub fn stroke_locked_with_phase(
    surface: &mut Surface,
    points: ((f32, f32), (f32, f32)),
    radius: f32,
    brush: &Brush,
    color: Rgba,
    seed: u64,
    phase: &mut f32,
) {
    let (from, to) = points;
    if color[3] <= 0.0 {
        return;
    }
    surface.set_stroke_wet(brush.wet);
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        (from, to),
        radius,
        brush,
        seed,
        phase,
    ) {
        surface.blend_stroke_mask_at(&mask, x, y, color, crate::surface::StrokeMask::Locked);
    }
}

pub fn stroke_locked(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    hardness: f32,
    color: Rgba,
) {
    stroke_locked_with(
        surface,
        from,
        to,
        radius,
        &Brush::from_hardness(hardness),
        color,
        0,
    );
}

/// Remove alpha along the same segment shape. Colour is left alone: an eraser is a subtraction
/// from the alpha channel, not a bucket of transparent paint.
pub fn erase_with(
    surface: &mut Surface,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    brush: &Brush,
    seed: u64,
) {
    let mut phase = -1.0;
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        (from, to),
        radius,
        brush,
        seed,
        &mut phase,
    ) {
        surface.erase_mask_at(&mask, x, y);
    }
}

pub fn erase_with_phase(
    surface: &mut Surface,
    points: ((f32, f32), (f32, f32)),
    radius: f32,
    brush: &Brush,
    seed: u64,
    phase: &mut f32,
) {
    let (from, to) = points;
    if let Some((mask, x, y)) = segment_mask(
        surface.width(),
        surface.height(),
        (from, to),
        radius,
        brush,
        seed,
        phase,
    ) {
        surface.blend_stroke_mask_at(&mask, x, y, [0.0; 4], crate::surface::StrokeMask::Erase);
    }
}

pub fn erase(surface: &mut Surface, from: (f32, f32), to: (f32, f32), radius: f32, hardness: f32) {
    erase_with(
        surface,
        from,
        to,
        radius,
        &Brush::from_hardness(hardness),
        0,
    );
}

#[cfg(test)]
mod tests;
