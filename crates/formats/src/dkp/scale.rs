//! How a brush radius is resolved when a tape is rendered at another scale.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrushScale {
    /// The radius is in document units and follows the canvas.
    #[default]
    Canvas,
    /// The radius is in output pixels and stays the same size.
    Screen,
}

impl BrushScale {
    pub fn as_u8(self) -> u8 {
        match self {
            BrushScale::Canvas => 0,
            BrushScale::Screen => 1,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => BrushScale::Screen,
            _ => BrushScale::Canvas,
        }
    }

    /// Resolve a recorded radius in output pixels.
    pub fn resolve(self, radius: f32, pixels_per_unit: f32) -> f32 {
        match self {
            BrushScale::Canvas => radius * pixels_per_unit,
            BrushScale::Screen => radius,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TapeOutput {
    pub scale: f32,
    pub brush_scale: BrushScale,
}

impl Default for TapeOutput {
    fn default() -> Self {
        Self {
            scale: 1.0,
            brush_scale: BrushScale::Canvas,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_canvas_radius_scales_and_a_screen_radius_does_not() {
        assert_eq!(BrushScale::Canvas.resolve(20.0, 0.25), 5.0);
        assert_eq!(BrushScale::Screen.resolve(20.0, 0.25), 20.0);
        assert_eq!(BrushScale::Canvas.resolve(20.0, 2.0), 40.0);
        assert_eq!(BrushScale::Screen.resolve(20.0, 2.0), 20.0);
    }

    #[test]
    fn the_tape_byte_round_trips() {
        assert_eq!(BrushScale::from_u8(0), BrushScale::Canvas);
        assert_eq!(BrushScale::from_u8(1), BrushScale::Screen);
        assert_eq!(BrushScale::Screen.as_u8(), 1);
    }
}
