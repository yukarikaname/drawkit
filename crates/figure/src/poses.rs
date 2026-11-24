//! A few poses worth having under a menu.
//!
//! They are local rotations, so they apply to any proportions and keep the figure's body type. A
//! preset is a starting point, not a keyframe: after choosing one the painter keeps rotating joints.

use glam::Vec3;

use crate::joint::Joint;
use crate::pose::Pose;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PosePreset {
    APose,
    TPose,
    ArmsUp,
    Contrapposto,
}

impl PosePreset {
    pub const ALL: [PosePreset; 4] = [
        PosePreset::APose,
        PosePreset::TPose,
        PosePreset::ArmsUp,
        PosePreset::Contrapposto,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PosePreset::APose => "A-pose",
            PosePreset::TPose => "T-pose",
            PosePreset::ArmsUp => "Arms up",
            PosePreset::Contrapposto => "Contrapposto",
        }
    }

    /// The rotations that make the pose. Angles are around the world axes in the parent's frame,
    /// which is why the left and right arms take opposite signs.
    pub fn pose(self) -> Pose {
        let mut pose = Pose::identity();
        match self {
            PosePreset::APose => {}
            PosePreset::TPose => {
                pose.rotate(Joint::UpperArmL, Vec3::Z, -50.0_f32.to_radians());
                pose.rotate(Joint::UpperArmR, Vec3::Z, 50.0_f32.to_radians());
            }
            PosePreset::ArmsUp => {
                pose.rotate(Joint::UpperArmL, Vec3::Z, -110.0_f32.to_radians());
                pose.rotate(Joint::UpperArmR, Vec3::Z, 110.0_f32.to_radians());
            }
            PosePreset::Contrapposto => {
                pose.rotate(Joint::Pelvis, Vec3::Z, 4.0_f32.to_radians());
                pose.rotate(Joint::Spine2, Vec3::Z, -3.0_f32.to_radians());
                pose.rotate(Joint::Neck, Vec3::Z, 2.0_f32.to_radians());
                pose.rotate(Joint::ThighR, Vec3::Z, -3.0_f32.to_radians());
                pose.rotate(Joint::ShinR, Vec3::X, 4.0_f32.to_radians());
            }
        }
        pose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proportions::{Preset, Proportions};
    use crate::skeleton::Skeleton;

    fn rig() -> Skeleton {
        Skeleton::humanoid(&Proportions::preset(Preset::RealisticNeutral))
    }

    #[test]
    fn a_t_pose_puts_the_hands_at_shoulder_height() {
        let skeleton = rig();
        let world = PosePreset::TPose.pose().world(&skeleton);
        let shoulder = world[Joint::UpperArmL.index()].position.y;
        let hand = world[Joint::HandL.index()].position.y;
        assert!(
            (hand - shoulder).abs() < 0.05,
            "hand {hand} vs shoulder {shoulder}"
        );
    }

    #[test]
    fn arms_up_puts_the_hands_above_the_head() {
        let skeleton = rig();
        let world = PosePreset::ArmsUp.pose().world(&skeleton);
        let head = world[Joint::Head.index()].position.y;
        let hand = world[Joint::HandL.index()].position.y;
        assert!(hand > head, "hand {hand} is below head {head}");
    }

    #[test]
    fn every_preset_has_a_name_and_two_of_them_differ() {
        assert_eq!(PosePreset::ALL.len(), 4);
        assert_ne!(PosePreset::APose.pose(), PosePreset::TPose.pose());
        assert!(
            PosePreset::ALL
                .iter()
                .all(|preset| !preset.name().is_empty())
        );
    }
}
