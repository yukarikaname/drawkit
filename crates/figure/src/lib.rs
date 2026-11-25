//! A parametric drawing mannequin: skeleton, proportions, pose and picking.
//!
//! This is not a character rig. A drawing figure exists to answer "where is the elbow" while the
//! painter draws, so it needs joint positions and a silhouette, not skinning, cloth or a face. The
//! joints are a fixed humanoid; the proportions are parameters; the pose is one quaternion per
//! joint. Everything here is pure math and can be tested without a window or a GPU.

pub mod figure;
pub mod ik;
pub mod joint;
pub mod light;
pub mod light_model;
pub mod mesh;
pub mod pick;
pub mod pose;
pub mod poses;
pub mod proportions;
pub mod render;
pub mod skeleton;

pub use figure::Figure;
pub use ik::solve_two_bone;
pub use joint::Joint;
pub use light::{Light, Lighting};
pub use light_model::LightingEstimate;
pub use mesh::{Mesh, Vertex, mannequin};
pub use pick::pick_joint;
pub use pose::{Pose, mirror_rotation};
pub use poses::PosePreset;
pub use proportions::{Preset, Proportions};
pub use render::Camera;
pub use skeleton::Skeleton;
