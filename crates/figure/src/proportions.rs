//! Body proportions as continuous parameters.
//!
//! Realistic, anime and chibi are not three models: they are the same skeleton with different head
//! counts, limb weights and widths. Keeping them as numbers means a painter can move between them
//! and keep the pose, and means the mesh can be generated from one description.

use serde::{Deserialize, Serialize};

/// The measurements a mannequin is built from. Lengths are in metres; multipliers are `1.0` for the
/// neutral figure.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proportions {
    /// Sole to crown.
    pub height: f32,
    /// Height divided by head height. 7.5 is realistic, 7.0 anime, 4.5 chibi.
    pub head_units: f32,
    /// Shoulder width (deltoid to deltoid) as a fraction of height.
    pub shoulder_width: f32,
    /// Hip width (joint to joint) as a fraction of height.
    pub hip_width: f32,
    /// Multiplier on every limb radius.
    pub limb_thickness: f32,
    /// Chest depth and width multiplier.
    pub chest: f32,
    /// Waist width multiplier.
    pub waist: f32,
    pub arms: f32,
    pub legs: f32,
    /// Multiplier on head height, for the stylised larger head.
    pub head_size: f32,
    pub hands: f32,
    pub feet: f32,
}

/// Common starting points. They are values of the same parameters, not separate skeletons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    RealisticNeutral,
    RealisticMale,
    RealisticFemale,
    Anime,
    Chibi,
}

impl Proportions {
    pub fn preset(preset: Preset) -> Self {
        use Preset::*;
        let mut proportions = Self {
            height: 1.75,
            head_units: 7.5,
            shoulder_width: 0.235,
            hip_width: 0.18,
            limb_thickness: 1.0,
            chest: 1.0,
            waist: 1.0,
            arms: 1.0,
            legs: 1.0,
            head_size: 1.0,
            hands: 1.0,
            feet: 1.0,
        };
        match preset {
            RealisticNeutral => {}
            RealisticMale => {
                proportions.shoulder_width = 0.25;
                proportions.hip_width = 0.175;
                proportions.chest = 1.1;
                proportions.limb_thickness = 1.05;
            }
            RealisticFemale => {
                proportions.shoulder_width = 0.22;
                proportions.hip_width = 0.20;
                proportions.chest = 1.05;
                proportions.waist = 0.9;
                proportions.limb_thickness = 0.95;
            }
            Anime => {
                proportions.height = 1.70;
                proportions.head_units = 7.0;
                proportions.head_size = 1.1;
                proportions.shoulder_width = 0.21;
                proportions.hip_width = 0.175;
                proportions.limb_thickness = 0.8;
                proportions.chest = 0.95;
                proportions.waist = 0.85;
                proportions.arms = 0.98;
                proportions.legs = 1.05;
                proportions.hands = 0.95;
                proportions.feet = 0.95;
            }
            Chibi => {
                proportions.height = 1.2;
                proportions.head_units = 4.5;
                proportions.head_size = 1.2;
                proportions.shoulder_width = 0.20;
                proportions.hip_width = 0.17;
                proportions.limb_thickness = 0.9;
                proportions.chest = 0.9;
                proportions.waist = 0.9;
                proportions.arms = 0.9;
                proportions.legs = 0.9;
                proportions.hands = 0.9;
                proportions.feet = 0.9;
            }
        }
        proportions
    }

    pub fn head_height(&self) -> f32 {
        self.height / self.head_units * self.head_size
    }

    /// Hip joint height: the legs are a little over half the body.
    pub fn leg_length(&self) -> f32 {
        self.height * 0.53 * self.legs
    }

    /// Shoulder to wrist, before the hand.
    pub fn arm_length(&self) -> f32 {
        self.height * 0.44 * self.arms
    }

    pub fn shoulder_span(&self) -> f32 {
        self.height * self.shoulder_width
    }

    pub fn hip_span(&self) -> f32 {
        self.height * self.hip_width
    }

    pub fn foot_length(&self) -> f32 {
        self.height * 0.10 * self.feet
    }

    pub fn limb_radius(&self) -> f32 {
        self.height * 0.032 * self.limb_thickness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_realistic_head_is_about_one_seventh_of_the_body() {
        let proportions = Proportions::preset(Preset::RealisticNeutral);
        let ratio = proportions.head_height() / proportions.height;
        assert!((ratio - 1.0 / 7.5).abs() < 1e-5, "ratio {ratio}");
    }

    #[test]
    fn chibi_has_a_relatively_larger_head_than_realistic() {
        let realistic = Proportions::preset(Preset::RealisticNeutral);
        let chibi = Proportions::preset(Preset::Chibi);
        assert!(chibi.head_units < realistic.head_units);
        assert!(chibi.head_height() / chibi.height > realistic.head_height() / realistic.height);
    }

    #[test]
    fn proportions_survive_json() {
        let proportions = Proportions::preset(Preset::Anime);
        let text = serde_json::to_string(&proportions).unwrap();
        let back: Proportions = serde_json::from_str(&text).unwrap();
        assert_eq!(back, proportions);
    }
}
