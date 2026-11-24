//! The rest pose, built from proportions.
//!
//! A joint stores its offset from its parent, not an absolute position: a pose rotates a bone and
//! the child joints follow, so offsets are the only length information the rig needs. Left-side
//! joints live at negative X, which makes the mirror a sign flip.

use glam::Vec3;

use crate::joint::Joint;
use crate::proportions::Proportions;

#[derive(Clone, Copy, Debug)]
pub struct RestJoint {
    pub parent: Option<Joint>,
    /// From the parent's rest position, or from the world origin for the root.
    pub offset: Vec3,
}

pub struct Skeleton {
    joints: [RestJoint; Joint::COUNT],
}

impl Skeleton {
    /// An A-pose humanoid. The arms hang 40 degrees from vertical so a painter can see the armpit
    /// and the elbow without rotating the figure first.
    pub fn humanoid(proportions: &Proportions) -> Self {
        let h = proportions.height;
        let leg = proportions.leg_length();
        let arm = proportions.arm_length();
        let upper_arm = arm * 0.52;
        let forearm = arm * 0.38;
        let thigh = leg * 0.50;
        let shin = leg * 0.42;
        let foot = proportions.foot_length();
        let shoulder = proportions.shoulder_span();
        let hip = proportions.hip_span();
        let arm_dir = Vec3::new(-0.643, -0.766, 0.0).normalize();

        let mut joints = [RestJoint {
            parent: None,
            offset: Vec3::ZERO,
        }; Joint::COUNT];
        let mut place = |joint: Joint, parent: Option<Joint>, offset: Vec3| {
            joints[joint.index()] = RestJoint { parent, offset };
        };

        place(Joint::Pelvis, None, Vec3::new(0.0, leg, 0.0));
        place(
            Joint::Spine1,
            Some(Joint::Pelvis),
            Vec3::new(0.0, 0.06 * h, 0.0),
        );
        place(
            Joint::Spine2,
            Some(Joint::Spine1),
            Vec3::new(0.0, 0.06 * h, 0.0),
        );
        place(
            Joint::Chest,
            Some(Joint::Spine2),
            Vec3::new(0.0, 0.07 * h, 0.0),
        );
        place(
            Joint::Neck,
            Some(Joint::Chest),
            Vec3::new(0.0, 0.075 * h, 0.0),
        );
        place(
            Joint::Head,
            Some(Joint::Neck),
            Vec3::new(0.0, 0.055 * h, 0.0),
        );

        let clavicle = shoulder * 0.22;
        let upper_arm_x = shoulder * 0.28;
        place(
            Joint::ClavicleL,
            Some(Joint::Chest),
            Vec3::new(-clavicle, 0.005 * h, 0.0),
        );
        place(
            Joint::UpperArmL,
            Some(Joint::ClavicleL),
            Vec3::new(-upper_arm_x, -0.005 * h, 0.0),
        );
        place(Joint::ForearmL, Some(Joint::UpperArmL), arm_dir * upper_arm);
        place(Joint::HandL, Some(Joint::ForearmL), arm_dir * forearm);
        place(
            Joint::ClavicleR,
            Some(Joint::Chest),
            Vec3::new(clavicle, 0.005 * h, 0.0),
        );
        place(
            Joint::UpperArmR,
            Some(Joint::ClavicleR),
            Vec3::new(upper_arm_x, -0.005 * h, 0.0),
        );
        place(
            Joint::ForearmR,
            Some(Joint::UpperArmR),
            Vec3::new(-arm_dir.x, arm_dir.y, 0.0) * upper_arm,
        );
        place(
            Joint::HandR,
            Some(Joint::ForearmR),
            Vec3::new(-arm_dir.x, arm_dir.y, 0.0) * forearm,
        );

        let hip_x = hip * 0.5;
        place(
            Joint::ThighL,
            Some(Joint::Pelvis),
            Vec3::new(-hip_x, -0.01 * h, 0.0),
        );
        place(
            Joint::ShinL,
            Some(Joint::ThighL),
            Vec3::new(0.0, -thigh, 0.0),
        );
        place(Joint::FootL, Some(Joint::ShinL), Vec3::new(0.0, -shin, 0.0));
        place(
            Joint::ToeL,
            Some(Joint::FootL),
            Vec3::new(0.0, -0.01 * h, foot * 0.6),
        );
        place(
            Joint::ThighR,
            Some(Joint::Pelvis),
            Vec3::new(hip_x, -0.01 * h, 0.0),
        );
        place(
            Joint::ShinR,
            Some(Joint::ThighR),
            Vec3::new(0.0, -thigh, 0.0),
        );
        place(Joint::FootR, Some(Joint::ShinR), Vec3::new(0.0, -shin, 0.0));
        place(
            Joint::ToeR,
            Some(Joint::FootR),
            Vec3::new(0.0, -0.01 * h, foot * 0.6),
        );

        Self { joints }
    }

    pub fn rest(&self, joint: Joint) -> RestJoint {
        self.joints[joint.index()]
    }

    pub fn parent(&self, joint: Joint) -> Option<Joint> {
        self.joints[joint.index()].parent
    }

    pub fn offset(&self, joint: Joint) -> Vec3 {
        self.joints[joint.index()].offset
    }

    /// The direction of the bone ending at `joint` in its parent's rest frame.
    pub fn rest_direction(&self, joint: Joint) -> Vec3 {
        self.offset(joint).normalize_or_zero()
    }

    pub fn bone_length(&self, joint: Joint) -> f32 {
        self.offset(joint).length()
    }

    /// Absolute rest position, accumulated from the root.
    pub fn rest_position(&self, joint: Joint) -> Vec3 {
        let mut position = Vec3::ZERO;
        let mut current = joint;
        loop {
            let rest = self.rest(current);
            position += rest.offset;
            match rest.parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proportions::Preset;

    fn realistic() -> Skeleton {
        Skeleton::humanoid(&Proportions::preset(Preset::RealisticNeutral))
    }

    #[test]
    fn the_parent_is_always_earlier_than_the_child() {
        let skeleton = realistic();
        for joint in Joint::ALL {
            if let Some(parent) = skeleton.parent(joint) {
                assert!(
                    parent.index() < joint.index(),
                    "{} comes before its parent {}",
                    joint.name(),
                    parent.name()
                );
            }
        }
    }

    #[test]
    fn left_and_right_are_mirror_images() {
        let skeleton = realistic();
        for joint in Joint::ALL {
            let left = skeleton.rest_position(joint);
            let right = skeleton.rest_position(joint.mirror());
            assert!(
                (left.x + right.x).abs() < 1e-5,
                "{} is not mirrored",
                joint.name()
            );
            assert!(
                (left.y - right.y).abs() < 1e-5,
                "{} changed height",
                joint.name()
            );
            assert!(
                (left.z - right.z).abs() < 1e-5,
                "{} changed depth",
                joint.name()
            );
        }
    }

    #[test]
    fn the_crown_reaches_the_configured_height() {
        let proportions = Proportions::preset(Preset::RealisticNeutral);
        let skeleton = Skeleton::humanoid(&proportions);
        let crown = skeleton.rest_position(Joint::Head).y + proportions.head_height();
        let ratio = crown / proportions.height;
        assert!(
            (0.95..=1.01).contains(&ratio),
            "the crown is at {ratio} of the height"
        );
    }

    #[test]
    fn the_hands_hang_below_the_pelvis_in_the_rest_pose() {
        let skeleton = realistic();
        let hand = skeleton.rest_position(Joint::HandL).y;
        let pelvis = skeleton.rest_position(Joint::Pelvis).y;
        assert!(hand < pelvis, "hand {hand} is not below pelvis {pelvis}");
    }
}
