//! A pose is one local quaternion per joint.
//!
//! Local, because that is what a painter changes: rotate the elbow, and the hand follows. The world
//! transform is a single pass down the joint list, which is why the skeleton keeps parents before
//! children.

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::joint::Joint;
use crate::skeleton::Skeleton;

/// Reflect a rotation across the YZ plane.
pub fn mirror_rotation(rotation: Quat) -> Quat {
    Quat::from_xyzw(rotation.x, -rotation.y, -rotation.z, rotation.w)
}

#[derive(Clone, Copy, Debug)]
pub struct WorldJoint {
    pub position: Vec3,
    pub rotation: Quat,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub rotations: Vec<Quat>,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            rotations: vec![Quat::IDENTITY; Joint::COUNT],
        }
    }
}

impl Pose {
    pub fn identity() -> Self {
        Self::default()
    }

    pub fn get(&self, joint: Joint) -> Quat {
        self.rotations[joint.index()]
    }

    pub fn set(&mut self, joint: Joint, rotation: Quat) {
        self.rotations[joint.index()] = rotation;
    }

    /// Rotate a joint in its parent's frame, on top of whatever it already holds.
    pub fn rotate(&mut self, joint: Joint, axis: Vec3, radians: f32) {
        let delta = Quat::from_axis_angle(axis.normalize_or_zero(), radians);
        self.rotations[joint.index()] = self.rotations[joint.index()] * delta;
    }

    /// World positions and rotations, parents first.
    pub fn world(&self, skeleton: &Skeleton) -> Vec<WorldJoint> {
        let mut world = vec![
            WorldJoint {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY
            };
            Joint::COUNT
        ];
        for joint in Joint::ALL {
            let rest = skeleton.rest(joint);
            let local = self.get(joint);
            world[joint.index()] = match rest.parent {
                Some(parent) => {
                    let parent_world = world[parent.index()];
                    WorldJoint {
                        position: parent_world.position + parent_world.rotation * rest.offset,
                        rotation: parent_world.rotation * local,
                    }
                }
                None => WorldJoint {
                    position: rest.offset,
                    rotation: local,
                },
            };
        }
        world
    }

    /// The other side of the body, with the pose reflected across the YZ plane.
    ///
    /// A rotation is a pseudovector: mirroring the figure reverses the components along Y and Z
    /// and keeps the one along the mirror normal. Getting this wrong raises the wrong arm, which
    /// is visible immediately.
    pub fn mirror(&self) -> Self {
        let mut rotations = vec![Quat::IDENTITY; Joint::COUNT];
        for joint in Joint::ALL {
            rotations[joint.mirror().index()] = mirror_rotation(self.get(joint));
        }
        Self { rotations }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|error| error.to_string())
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let pose: Pose = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if pose.rotations.len() != Joint::COUNT {
            return Err(format!(
                "{} joints, expected {}",
                pose.rotations.len(),
                Joint::COUNT
            ));
        }
        Ok(pose)
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
    fn the_identity_pose_is_the_rest_pose() {
        let skeleton = rig();
        let world = Pose::identity().world(&skeleton);
        for joint in Joint::ALL {
            let rest = skeleton.rest_position(joint);
            let posed = world[joint.index()].position;
            assert!((rest - posed).length() < 1e-5, "{} moved", joint.name());
        }
    }

    #[test]
    fn rotating_the_elbow_moves_the_hand_and_not_the_shoulder() {
        let skeleton = rig();
        let mut pose = Pose::identity();
        let shoulder = pose.world(&skeleton)[Joint::UpperArmL.index()].position;
        let hand = pose.world(&skeleton)[Joint::HandL.index()].position;
        pose.rotate(Joint::ForearmL, Vec3::Z, 1.0);
        let world = pose.world(&skeleton);
        assert!((world[Joint::UpperArmL.index()].position - shoulder).length() < 1e-5);
        assert!((world[Joint::HandL.index()].position - hand).length() > 0.05);
    }

    #[test]
    fn mirroring_twice_returns_the_pose() {
        let mut pose = Pose::identity();
        pose.rotate(Joint::UpperArmL, Vec3::Z, 0.4);
        pose.rotate(Joint::ThighR, Vec3::X, -0.2);
        let back = pose.mirror().mirror();
        for joint in Joint::ALL {
            let a = pose.get(joint);
            let b = back.get(joint);
            assert!(
                (a.x - b.x).abs() < 1e-6
                    && (a.y - b.y).abs() < 1e-6
                    && (a.z - b.z).abs() < 1e-6
                    && (a.w - b.w).abs() < 1e-6,
                "{} changed",
                joint.name()
            );
        }
    }

    #[test]
    fn a_mirrored_arm_lands_where_the_reflection_says() {
        let skeleton = rig();
        let mut pose = Pose::identity();
        pose.rotate(Joint::UpperArmL, Vec3::Z, 0.5);
        pose.rotate(Joint::ForearmL, Vec3::X, 0.3);
        let left = pose.world(&skeleton)[Joint::HandL.index()].position;
        let right = pose.mirror().world(&skeleton)[Joint::HandR.index()].position;
        assert!((left.x + right.x).abs() < 1e-4, "{left} vs {right}");
        assert!((left.y - right.y).abs() < 1e-4, "{left} vs {right}");
        assert!((left.z - right.z).abs() < 1e-4, "{left} vs {right}");
    }

    #[test]
    fn a_pose_survives_json() {
        let mut pose = Pose::identity();
        pose.rotate(Joint::Spine2, Vec3::Y, 0.25);
        pose.rotate(Joint::ShinL, Vec3::X, 0.5);
        let text = pose.to_json().unwrap();
        let back = Pose::from_json(&text).unwrap();
        assert_eq!(back, pose);
    }

    #[test]
    fn a_short_pose_is_rejected() {
        let text = r#"{"rotations":[]}"#;
        assert!(Pose::from_json(text).is_err());
    }
}
