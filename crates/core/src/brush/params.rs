//! The parameter names the shells edit.

use super::Brush;

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
            _ => {}
        }
    }
}
