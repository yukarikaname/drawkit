//! The parameter names the shells edit.

use super::{Brush, BrushKind};

impl Brush {
    /// Read one editable parameter by the index the shell uses.
    pub fn param(&self, id: u8) -> Option<f32> {
        Some(match id {
            0 => self.hardness,
            1 => self.flow,
            2 => self.pressure_radius,
            3 => self.pressure_alpha,
            4 => self.spacing,
            5 => self.jitter,
            6 => self.angle,
            7 => self.roundness,
            8 => self.grain,
            9 => self.paper,
            10 => self.texture,
            11 => self.wet,
            12 => self.dual,
            13 => self.dual_scale,
            14 => self.dual_kind as f32,
            _ => return None,
        })
    }

    /// Change one parameter, clamped to the range the engine can use.
    pub fn set_param(&mut self, id: u8, value: f32) {
        match id {
            0 => self.hardness = value.clamp(0.0, 1.0),
            1 => self.flow = value.clamp(0.01, 1.0),
            2 => self.pressure_radius = value.clamp(0.0, 1.0),
            3 => self.pressure_alpha = value.clamp(0.0, 1.0),
            4 => self.spacing = value.clamp(0.02, 2.0),
            5 => self.jitter = value.clamp(0.0, 2.0),
            6 => self.angle = value,
            7 => self.roundness = value.clamp(0.05, 1.0),
            8 => self.grain = value.clamp(0.0, 1.0),
            9 => self.paper = value.clamp(0.0, 1.0),
            10 => self.texture = value.clamp(0.0, 1.0),
            11 => self.wet = value.clamp(0.0, 1.0),
            12 => self.dual = value.clamp(0.0, 1.0),
            13 => self.dual_scale = value.clamp(0.1, 2.0),
            14 => self.dual_kind = value.clamp(0.0, 8.0) as u8,
            _ => {}
        }
    }
}

impl Brush {
    /// A copy whose tip follows the pen. Tilt squashes the oval along the lean direction; azimuth
    /// and roll rotate it. A mouse reports zero and the brush stays exactly as configured.
    pub fn with_input(&self, tilt: [f32; 2], azimuth: f32, roll: f32) -> Self {
        let raw = (tilt[0] * tilt[0] + tilt[1] * tilt[1]).sqrt();
        if raw <= 0.001 && roll.abs() <= 0.001 {
            return *self;
        }
        // Tablets report normalized tilt on Apple and radians/degrees elsewhere. Anything above
        // one is treated as degrees from vertical, which keeps all three shells in 0...1.
        let strength = if raw <= 1.0 {
            raw
        } else {
            (raw / 60.0).min(1.0)
        };
        let direction = if raw > 0.01 {
            tilt[1].atan2(tilt[0])
        } else {
            azimuth
        };
        let mut shaped = *self;
        shaped.angle += direction.to_degrees() + roll.to_degrees();
        shaped.roundness = (self.roundness * (1.0 - 0.85 * strength)).max(0.08);
        shaped
    }
}

impl Brush {
    pub const fn soft_round() -> Self {
        Self {
            kind: BrushKind::SoftRound,
            hardness: 0.0,
            flow: 1.0,
            pressure_radius: 0.25,
            pressure_alpha: 0.0,
            spacing: 0.25,
            jitter: 0.0,
            angle: 0.0,
            roundness: 1.0,
            grain: 0.0,
            paper: 0.0,
            texture: 0.0,
            wet: 0.0,
            dual: 0.0,
            dual_scale: 0.7,
            dual_kind: 0,
        }
    }

    /// The round falloff the old call sites used, with no spatial randomness.
    pub fn from_hardness(hardness: f32) -> Self {
        Self {
            hardness: hardness.clamp(0.0, 1.0),
            ..Self::soft_round()
        }
    }
}

impl BrushKind {
    pub fn all() -> [Self; 9] {
        [
            Self::SoftRound,
            Self::Pencil,
            Self::Ink,
            Self::Marker,
            Self::Airbrush,
            Self::Bristle,
            Self::Spray,
            Self::Chalk,
            Self::Watercolor,
        ]
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::BrushKind;

    #[test]
    fn tilting_the_pen_squashes_the_tip() {
        let brush = Brush::soft_round();
        let shaped = brush.with_input([1.0, 0.0], 0.0, 0.0);
        assert!(shaped.roundness < 0.2);
        assert!((shaped.angle - 0.0).abs() < 0.01);
    }

    #[test]
    fn an_upright_pen_keeps_the_configured_tip() {
        let brush = Brush::for_kind(BrushKind::Bristle);
        let shaped = brush.with_input([0.0, 0.0], 0.0, 0.0);
        assert_eq!(shaped.roundness, brush.roundness);
        assert_eq!(shaped.angle, brush.angle);
    }
}
