//! Picking a joint with a ray, so the shell can drag one without reading the GPU back.
//!
//! Joints are spheres for this purpose. A painter's pointer is aiming at a handle, not at the
//! silhouette; a sphere around the joint is both easier to hit and easier to reason about than the
//! capsule mesh that will be drawn over it.

use glam::Vec3;

use crate::joint::Joint;
use crate::pose::Pose;
use crate::skeleton::Skeleton;

/// The nearest joint within `radius` of the ray `origin + t * direction`, and the ray distance.
pub fn pick_joint(
    pose: &Pose,
    skeleton: &Skeleton,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    radius: f32,
) -> Option<(Joint, f32)> {
    let direction = direction.try_normalize()?;
    let world = pose.world(skeleton);
    let mut best: Option<(Joint, f32)> = None;
    for joint in Joint::ALL {
        let center = world[joint.index()].position;
        if let Some(distance) = ray_sphere(origin, direction, center, radius)
            && distance <= max_distance
            && best.is_none_or(|(_, closest)| distance < closest)
        {
            best = Some((joint, distance));
        }
    }
    best
}

fn ray_sphere(origin: Vec3, direction: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let offset = origin - center;
    let projected = offset.dot(direction);
    let discriminant = projected * projected - (offset.length_squared() - radius * radius);
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let near = -projected - root;
    if near >= 0.0 {
        return Some(near);
    }
    let far = -projected + root;
    if far >= 0.0 { Some(0.0) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proportions::{Preset, Proportions};

    fn rig() -> (Skeleton, Pose) {
        let skeleton = Skeleton::humanoid(&Proportions::preset(Preset::RealisticNeutral));
        (skeleton, Pose::identity())
    }

    #[test]
    fn a_ray_through_the_head_picks_the_head() {
        let (skeleton, pose) = rig();
        let head = pose.world(&skeleton)[Joint::Head.index()].position;
        let origin = head + Vec3::new(0.0, 0.0, 1.0);
        let picked = pick_joint(&pose, &skeleton, origin, Vec3::NEG_Z, 2.0, 0.05);
        assert_eq!(picked.map(|(joint, _)| joint), Some(Joint::Head));
    }

    #[test]
    fn a_ray_that_misses_every_joint_picks_nothing() {
        let (skeleton, pose) = rig();
        let origin = Vec3::new(10.0, 10.0, 10.0);
        assert!(pick_joint(&pose, &skeleton, origin, Vec3::X, 100.0, 0.05).is_none());
    }

    #[test]
    fn the_nearer_of_two_joints_wins() {
        let (skeleton, pose) = rig();
        // Up the centre line: the pelvis is hit before the spine above it.
        let origin = Vec3::new(0.0, 0.5, 0.0);
        let picked = pick_joint(&pose, &skeleton, origin, Vec3::Y, 3.0, 0.08);
        assert_eq!(picked.map(|(joint, _)| joint), Some(Joint::Pelvis));
    }
}
