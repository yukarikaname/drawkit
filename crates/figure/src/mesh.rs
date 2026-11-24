//! A mannequin mesh generated from the skeleton, not sculpted.
//!
//! The parts are capsules and balls: a ball at every joint is the classic wooden figure, and it
//! makes the joints readable as joints. Nothing is skinned, so changing a proportion or a pose is
//! a rebuild of a few thousand vertices, not a weight-painting problem.

use glam::Vec3;
use std::f32::consts::{PI, TAU};

use crate::joint::Joint;
use crate::pose::Pose;
use crate::proportions::Proportions;
use crate::skeleton::Skeleton;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn append(&mut self, other: Mesh) {
        let offset = self.vertices.len() as u32;
        self.vertices.extend(other.vertices);
        self.indices
            .extend(other.indices.into_iter().map(|index| index + offset));
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// The axis-aligned box the mesh occupies, for framing the camera and for tests.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let first = self.vertices.first()?;
        let mut min = Vec3::from_array(first.position);
        let mut max = min;
        for vertex in &self.vertices {
            let position = Vec3::from_array(vertex.position);
            min = min.min(position);
            max = max.max(position);
        }
        Some((min, max))
    }
}

pub fn sphere(center: Vec3, radius: f32, rings: usize, segments: usize) -> Mesh {
    let mut mesh = Mesh::default();
    for ring in 0..=rings {
        let phi = ring as f32 / rings as f32 * PI;
        let normal_y = phi.cos();
        let ring_radius = phi.sin();
        for segment in 0..=segments {
            let theta = segment as f32 / segments as f32 * TAU;
            let normal = Vec3::new(
                ring_radius * theta.cos(),
                normal_y,
                ring_radius * theta.sin(),
            );
            mesh.vertices.push(Vertex {
                position: (center + normal * radius).to_array(),
                normal: normal.to_array(),
            });
        }
    }
    for ring in 0..rings {
        for segment in 0..segments {
            let row = (segments + 1) as u32;
            let a = ring as u32 * row + segment as u32;
            let b = a + 1;
            let c = a + row;
            let d = c + 1;
            mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    mesh
}

/// A tapered cylinder from `a` to `b`; its ends are open, because a capsule's balls cover them.
pub fn taper(a: Vec3, b: Vec3, radius_a: f32, radius_b: f32, segments: usize) -> Mesh {
    let axis = b - a;
    let length = axis.length();
    if length < 1e-6 {
        return Mesh::default();
    }
    let direction = axis / length;
    let u = direction.any_orthonormal_vector();
    let v = direction.cross(u);
    let mut mesh = Mesh::default();
    for end in 0..=1 {
        let (center, radius) = if end == 0 {
            (a, radius_a)
        } else {
            (b, radius_b)
        };
        for segment in 0..=segments {
            let theta = segment as f32 / segments as f32 * TAU;
            let normal = u * theta.cos() + v * theta.sin();
            mesh.vertices.push(Vertex {
                position: (center + normal * radius).to_array(),
                normal: normal.to_array(),
            });
        }
    }
    let row = (segments + 1) as u32;
    for segment in 0..segments as u32 {
        mesh.indices.extend_from_slice(&[
            segment,
            row + segment,
            segment + 1,
            segment + 1,
            row + segment,
            row + segment + 1,
        ]);
    }
    mesh
}

pub fn limb(a: Vec3, b: Vec3, radius_a: f32, radius_b: f32) -> Mesh {
    let mut mesh = taper(a, b, radius_a, radius_b, 12);
    mesh.append(sphere(a, radius_a, 6, 12));
    mesh.append(sphere(b, radius_b, 6, 12));
    mesh
}

/// The whole figure, posed. Joint balls are slightly larger than the limb that meets them.
pub fn mannequin(skeleton: &Skeleton, pose: &Pose, proportions: &Proportions) -> Mesh {
    let world = pose.world(skeleton);
    let at = |joint: Joint| world[joint.index()].position;
    let height = proportions.height;
    let radius = proportions.limb_radius();
    let head = proportions.head_height();
    let mut mesh = Mesh::default();

    mesh.append(sphere(
        at(Joint::Head) + Vec3::Y * head * 0.5,
        head * 0.46,
        12,
        18,
    ));
    mesh.append(limb(
        at(Joint::Neck),
        at(Joint::Head),
        height * 0.030,
        height * 0.032,
    ));
    mesh.append(limb(
        at(Joint::Chest),
        at(Joint::Neck),
        height * 0.050,
        height * 0.030,
    ));
    mesh.append(limb(
        at(Joint::Spine2),
        at(Joint::Chest),
        height * 0.085 * proportions.chest,
        height * 0.085 * proportions.chest,
    ));
    mesh.append(limb(
        at(Joint::Pelvis),
        at(Joint::Spine2),
        height * 0.075 * proportions.waist,
        height * 0.075 * proportions.chest,
    ));
    mesh.append(sphere(
        at(Joint::Pelvis) + Vec3::Y * height * 0.03,
        height * 0.075 * proportions.hip_width / 0.18,
        8,
        14,
    ));

    for (clavicle, upper, forearm, hand) in [
        (
            Joint::ClavicleL,
            Joint::UpperArmL,
            Joint::ForearmL,
            Joint::HandL,
        ),
        (
            Joint::ClavicleR,
            Joint::UpperArmR,
            Joint::ForearmR,
            Joint::HandR,
        ),
    ] {
        mesh.append(limb(at(clavicle), at(upper), radius * 0.6, radius * 0.9));
        mesh.append(limb(at(upper), at(forearm), radius * 0.9, radius * 0.75));
        mesh.append(limb(at(forearm), at(hand), radius * 0.75, radius * 0.65));
        mesh.append(sphere(at(hand), radius * 1.1, 6, 10));
    }

    for (thigh, shin, foot, toe) in [
        (Joint::ThighL, Joint::ShinL, Joint::FootL, Joint::ToeL),
        (Joint::ThighR, Joint::ShinR, Joint::FootR, Joint::ToeR),
    ] {
        mesh.append(limb(at(thigh), at(shin), radius * 1.35, radius * 0.95));
        mesh.append(limb(at(shin), at(foot), radius * 0.95, radius * 0.7));
        mesh.append(limb(at(foot), at(toe), radius * 0.8, radius * 0.55));
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proportions::{Preset, Proportions};

    fn rig() -> (Skeleton, Proportions) {
        let proportions = Proportions::preset(Preset::RealisticNeutral);
        (Skeleton::humanoid(&proportions), proportions)
    }

    #[test]
    fn every_index_points_at_a_vertex() {
        let (skeleton, proportions) = rig();
        let mesh = mannequin(&skeleton, &Pose::identity(), &proportions);
        assert!(!mesh.is_empty());
        assert!(
            mesh.indices
                .iter()
                .all(|index| (*index as usize) < mesh.vertices.len())
        );
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.position.iter().all(|value| value.is_finite())
                && (Vec3::from_array(vertex.normal).length() - 1.0).abs() < 1e-3
        }));
    }

    #[test]
    fn the_mesh_is_about_as_tall_as_the_configured_figure() {
        let (skeleton, proportions) = rig();
        let mesh = mannequin(&skeleton, &Pose::identity(), &proportions);
        let (min, max) = mesh.bounds().unwrap();
        let height = max.y - min.y;
        assert!(
            (height / proportions.height - 1.0).abs() < 0.12,
            "mesh height {height} vs {}",
            proportions.height
        );
    }

    #[test]
    fn raising_an_arm_changes_the_silhouette() {
        let (skeleton, proportions) = rig();
        let rest = mannequin(&skeleton, &Pose::identity(), &proportions);
        let mut pose = Pose::identity();
        pose.rotate(Joint::UpperArmL, Vec3::Z, -1.2);
        let raised = mannequin(&skeleton, &pose, &proportions);
        let rest_bounds = rest.bounds().unwrap();
        let raised_bounds = raised.bounds().unwrap();
        assert!(
            raised_bounds.0.x < rest_bounds.0.x - 0.05
                || raised_bounds.1.x > rest_bounds.1.x + 0.05,
            "the raised arm did not move the bounds"
        );
    }
}
