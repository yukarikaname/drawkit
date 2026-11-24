//! The figure as one object: proportions, rest skeleton and pose together.
//!
//! Changing a proportion rebuilds the rest skeleton and keeps the pose, because the pose is stored
//! per joint and the joint list does not change. That is the property that makes a slider cheap.

use glam::Vec3;

use crate::ik::solve_two_bone;
use crate::joint::Joint;
use crate::mesh::{Mesh, mannequin};
use crate::pick::pick_joint;
use crate::pose::Pose;
use crate::proportions::{Preset, Proportions};
use crate::render::{self, Camera};
use crate::skeleton::Skeleton;

pub struct Figure {
    pub proportions: Proportions,
    pub skeleton: Skeleton,
    pub pose: Pose,
}

impl Figure {
    pub fn new(proportions: Proportions) -> Self {
        let skeleton = Skeleton::humanoid(&proportions);
        Self {
            proportions,
            skeleton,
            pose: Pose::identity(),
        }
    }

    pub fn from_preset(preset: Preset) -> Self {
        Self::new(Proportions::preset(preset))
    }

    pub fn set_proportions(&mut self, proportions: Proportions) {
        self.proportions = proportions;
        self.skeleton = Skeleton::humanoid(&self.proportions);
    }

    pub fn set_preset(&mut self, preset: Preset) {
        self.set_proportions(Proportions::preset(preset));
    }

    pub fn reset_pose(&mut self) {
        self.pose = Pose::identity();
    }

    pub fn apply_pose(&mut self, pose: Pose) {
        self.pose = pose;
    }

    pub fn apply_pose_preset(&mut self, preset: crate::PosePreset) {
        self.pose = preset.pose();
    }

    pub fn mirror(&mut self) {
        self.pose = self.pose.mirror();
    }

    pub fn rotate(&mut self, joint: Joint, axis: Vec3, radians: f32) {
        self.pose.rotate(joint, axis, radians);
    }

    pub fn solve_ik(
        &mut self,
        upper: Joint,
        lower: Joint,
        end: Joint,
        target: Vec3,
        pole: Vec3,
    ) -> bool {
        solve_two_bone(
            &mut self.pose,
            &self.skeleton,
            upper,
            lower,
            end,
            target,
            pole,
        )
    }

    /// Rotate one bone so its child joint points at `target`, which is how an elbow or a knee is
    /// dragged without moving the hand or the foot to a depth the pointer cannot express.
    pub fn aim(&mut self, joint: Joint, target: Vec3) -> bool {
        let child = match joint {
            Joint::UpperArmL => Joint::ForearmL,
            Joint::UpperArmR => Joint::ForearmR,
            Joint::ThighL => Joint::ShinL,
            Joint::ThighR => Joint::ShinR,
            _ => return false,
        };
        let rest = self.skeleton.rest(joint);
        let rest_direction = self.skeleton.offset(child).normalize_or_zero();
        let world = self.pose.world(&self.skeleton);
        let root = world[joint.index()].position;
        let desired = (target - root).normalize_or_zero();
        if rest_direction == Vec3::ZERO || desired == Vec3::ZERO {
            return false;
        }
        let parent_rotation = rest.parent.map_or(glam::Quat::IDENTITY, |parent| {
            world[parent.index()].rotation
        });
        let local = parent_rotation.conjugate() * desired;
        self.pose
            .set(joint, crate::ik::rotation_between(rest_direction, local));
        true
    }

    pub fn pick(&self, origin: Vec3, direction: Vec3, radius: f32) -> Option<(Joint, f32)> {
        pick_joint(
            &self.pose,
            &self.skeleton,
            origin,
            direction,
            f32::INFINITY,
            radius,
        )
    }

    pub fn mesh(&self) -> Mesh {
        mannequin(&self.skeleton, &self.pose, &self.proportions)
    }

    pub fn render(
        &self,
        camera: &Camera,
        width: usize,
        height: usize,
        color: [f32; 3],
    ) -> Vec<[f32; 4]> {
        render::render(&self.mesh(), camera, width, height, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_the_preset_keeps_the_pose() {
        let mut figure = Figure::from_preset(Preset::RealisticNeutral);
        figure.rotate(Joint::UpperArmL, Vec3::Z, 0.3);
        let rotation = figure.pose.get(Joint::UpperArmL);
        figure.set_preset(Preset::Anime);
        assert_eq!(figure.pose.get(Joint::UpperArmL), rotation);
        assert_eq!(figure.proportions.height, 1.70);
    }

    #[test]
    fn aiming_an_upper_arm_puts_the_elbow_on_the_ray() {
        let mut figure = Figure::from_preset(Preset::RealisticNeutral);
        let world = figure.pose.world(&figure.skeleton);
        let shoulder = world[Joint::UpperArmL.index()].position;
        let target = shoulder + Vec3::new(-0.3, -0.2, 0.4);
        assert!(figure.aim(Joint::UpperArmL, target));
        let elbow = figure.pose.world(&figure.skeleton)[Joint::ForearmL.index()].position;
        let wanted = (target - shoulder).normalize();
        let got = (elbow - shoulder).normalize();
        assert!((wanted - got).length() < 1e-3, "{wanted} vs {got}");
        assert!(
            !figure.aim(Joint::Head, target),
            "a joint with no aimable bone was moved"
        );
    }

    #[test]
    fn mirroring_swaps_the_arms() {
        let mut figure = Figure::from_preset(Preset::RealisticNeutral);
        figure.rotate(Joint::UpperArmL, Vec3::Z, 0.4);
        let left = figure.pose.get(Joint::UpperArmL);
        figure.mirror();
        assert_eq!(
            figure.pose.get(Joint::UpperArmR),
            crate::mirror_rotation(left)
        );
    }

    #[test]
    fn a_rendered_figure_has_a_body_and_empty_background() {
        let figure = Figure::from_preset(Preset::RealisticNeutral);
        let camera = Camera::orbit(Vec3::new(0.0, 0.9, 0.0), 4.5, 0.0, 0.0);
        let pixels = figure.render(&camera, 96, 144, [0.7, 0.7, 0.7]);
        assert!(pixels.iter().any(|pixel| pixel[3] > 0.5));
        assert!(pixels.iter().any(|pixel| pixel[3] == 0.0));
    }
}
