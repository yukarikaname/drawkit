//! Two-bone IK for the elbow and the knee.
//!
//! Posing by dragging the hand is the whole interaction: the painter does not want to rotate an
//! upper arm and then a forearm until the hand happens to land. The law of cosines gives the elbow
//! position for a target and a pole; two aim rotations put the bones there.

use glam::{Quat, Vec3};

use crate::joint::Joint;
use crate::pose::{Pose, WorldJoint};
use crate::skeleton::Skeleton;

/// Place the chain `upper -> lower -> end` so `end` reaches `target`, bending toward `pole`.
///
/// Returns whether the target was within reach. An unreachable target leaves the chain stretched
/// toward it instead of leaving the last good pose; a painter can see that the limb is at its
/// limit, which is information.
pub fn solve_two_bone(
    pose: &mut Pose,
    skeleton: &Skeleton,
    upper: Joint,
    lower: Joint,
    end: Joint,
    target: Vec3,
    pole: Vec3,
) -> bool {
    let l1 = skeleton.bone_length(lower);
    let l2 = skeleton.bone_length(end);
    if l1 <= 1e-6 || l2 <= 1e-6 {
        return false;
    }
    let world = pose.world(skeleton);
    let root = world[upper.index()].position;
    let to_target = target - root;
    let distance = to_target.length();
    if distance <= 1e-6 {
        return false;
    }
    let reach = l1 + l2;
    let reachable = distance <= reach - 1e-4;
    let d = distance.min(reach - 1e-4).max((l1 - l2).abs() + 1e-4);
    let direction = to_target / distance;

    // The elbow lies in the plane through the root, the target and the pole. A pole on the line
    // (or no pole at all) leaves the plane undefined, so pick any plane rather than a NaN.
    let mut side = pole - root;
    side -= direction * side.dot(direction);
    let perpendicular = if side.length_squared() < 1e-8 {
        direction.any_orthonormal_vector()
    } else {
        side.normalize()
    };

    let cos = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let elbow = root + direction * (l1 * cos) + perpendicular * (l1 * sin);
    let end_target = root + direction * d;

    let world = pose.world(skeleton);
    if !aim(pose, skeleton, upper, lower, &world, elbow - root) {
        return false;
    }
    let world = pose.world(skeleton);
    let lower_position = world[lower.index()].position;
    aim(
        pose,
        skeleton,
        lower,
        end,
        &world,
        end_target - lower_position,
    );
    reachable
}

/// Rotate `joint` so its bone points along `desired` in world space.
fn aim(
    pose: &mut Pose,
    skeleton: &Skeleton,
    joint: Joint,
    child: Joint,
    world: &[WorldJoint],
    desired: Vec3,
) -> bool {
    let rest = skeleton.rest(joint);
    // The bone a joint rotates is the offset to its child, not the offset it was reached by: the
    // upper arm rotates the shoulder-to-elbow vector, not the clavicle-to-shoulder one.
    let rest_direction = skeleton.offset(child).normalize_or_zero();
    if rest_direction == Vec3::ZERO || desired.length_squared() < 1e-10 {
        return false;
    }
    let parent_rotation = rest
        .parent
        .map_or(Quat::IDENTITY, |parent| world[parent.index()].rotation);
    let local_desired = parent_rotation.conjugate() * desired.normalize();
    pose.set(joint, rotation_between(rest_direction, local_desired));
    true
}

/// The smallest rotation taking `from` to `to`. `Quat::from_rotation_arc` is undefined for
/// opposite vectors, which is exactly the pose a straight arm or leg is dragged into.
pub(crate) fn rotation_between(from: Vec3, to: Vec3) -> Quat {
    let dot = from.dot(to).clamp(-1.0, 1.0);
    if dot > 1.0 - 1e-6 {
        return Quat::IDENTITY;
    }
    if dot < -1.0 + 1e-6 {
        return Quat::from_axis_angle(from.any_orthonormal_vector(), std::f32::consts::PI);
    }
    Quat::from_axis_angle(from.cross(to).normalize(), dot.acos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joint::Joint;
    use crate::proportions::{Preset, Proportions};

    fn rig() -> Skeleton {
        Skeleton::humanoid(&Proportions::preset(Preset::RealisticNeutral))
    }

    #[test]
    fn a_reachable_target_puts_the_hand_on_it() {
        let skeleton = rig();
        let mut pose = Pose::identity();
        let world = pose.world(&skeleton);
        let root = world[Joint::UpperArmL.index()].position;
        let reach = skeleton.bone_length(Joint::ForearmL) + skeleton.bone_length(Joint::HandL);
        let target = root + Vec3::new(-0.2, -0.4, 0.3).normalize() * (reach * 0.85);
        let pole = root + Vec3::new(0.0, -0.5, -0.5);
        assert!(solve_two_bone(
            &mut pose,
            &skeleton,
            Joint::UpperArmL,
            Joint::ForearmL,
            Joint::HandL,
            target,
            pole
        ));
        let hand = pose.world(&skeleton)[Joint::HandL.index()].position;
        assert!(
            (hand - target).length() < 1e-3,
            "hand {hand} missed {target}"
        );
    }

    #[test]
    fn an_unreachable_target_leaves_the_arm_stretched_toward_it() {
        let skeleton = rig();
        let mut pose = Pose::identity();
        let root = pose.world(&skeleton)[Joint::UpperArmL.index()].position;
        let target = root + Vec3::new(0.0, -1.0, 0.0) * 10.0;
        let pole = root + Vec3::new(0.0, 0.0, -1.0);
        assert!(!solve_two_bone(
            &mut pose,
            &skeleton,
            Joint::UpperArmL,
            Joint::ForearmL,
            Joint::HandL,
            target,
            pole
        ));
        let hand = pose.world(&skeleton)[Joint::HandL.index()].position;
        let reach = skeleton.bone_length(Joint::ForearmL) + skeleton.bone_length(Joint::HandL);
        assert!(((hand - root).length() - reach).abs() < 1e-3);
    }

    #[test]
    fn the_knee_bends_forward_for_a_forward_target() {
        let skeleton = rig();
        let mut pose = Pose::identity();
        let world = pose.world(&skeleton);
        let hip = world[Joint::ThighL.index()].position;
        let reach = skeleton.bone_length(Joint::ShinL) + skeleton.bone_length(Joint::FootL);
        let target = hip + Vec3::new(0.0, -reach * 0.7, 0.10);
        let pole = hip + Vec3::new(0.0, -0.2, 0.5);
        assert!(solve_two_bone(
            &mut pose,
            &skeleton,
            Joint::ThighL,
            Joint::ShinL,
            Joint::FootL,
            target,
            pole
        ));
        let knee = pose.world(&skeleton)[Joint::ShinL.index()].position;
        assert!(knee.z > hip.z, "the knee bent backwards: {knee} vs {hip}");
    }
}
