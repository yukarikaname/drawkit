//! A small three-light rig for the reference figure.
//!
//! The CPU renderer and the GPU shader share these numbers: a key light, a fill light, a rim
//! light and an ambient term. A drawing mannequin needs readable planes, so the lights are
//! directional and untone-mapped; nothing here pretends to be a physically based material.

use glam::Vec3;

/// One directional light. `direction` points from the surface toward the light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub direction: Vec3,
    /// Linear RGB tint; white is neutral.
    pub color: Vec3,
    pub intensity: f32,
}

impl Light {
    pub const NONE: Self = Self {
        direction: Vec3::Y,
        color: Vec3::ONE,
        intensity: 0.0,
    };

    pub fn new(direction: Vec3, color: Vec3, intensity: f32) -> Self {
        let direction = if direction.length_squared() > 1e-8 {
            direction.normalize()
        } else {
            Vec3::Y
        };
        Self {
            direction,
            color,
            intensity: intensity.max(0.0),
        }
    }

    /// The direction an azimuth/elevation pair names, with zero elevation on the horizon and
    /// zero azimuth toward +Z.
    pub fn from_angles(azimuth: f32, elevation: f32, color: Vec3, intensity: f32) -> Self {
        let (sin_az, cos_az) = azimuth.sin_cos();
        let (sin_el, cos_el) = elevation.sin_cos();
        Self::new(
            Vec3::new(cos_el * sin_az, sin_el, cos_el * cos_az),
            color,
            intensity,
        )
    }

    /// White at neutral warmth; a negative warmth cools the tint, a positive one warms it.
    pub fn tint(warmth: f32) -> Vec3 {
        let warmth = warmth.clamp(-1.0, 1.0);
        if warmth < 0.0 {
            Vec3::new(1.0 + warmth * 0.25, 1.0 + warmth * 0.12, 1.0)
        } else {
            Vec3::new(1.0, 1.0 - warmth * 0.18, 1.0 - warmth * 0.35)
        }
    }

    fn rgb(self) -> Vec3 {
        self.color * self.intensity
    }

    fn diffuse(self, normal: Vec3) -> Vec3 {
        normal.dot(self.direction).max(0.0) * self.rgb()
    }

    fn rim(self, normal: Vec3, view: Vec3) -> Vec3 {
        let facing = normal.dot(view).clamp(0.0, 1.0);
        let edge = (1.0 - facing).powi(3);
        edge * self.rgb()
    }
}

/// The whole rig. The defaults reproduce the single light the mannequin had before the rig.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    pub key: Light,
    pub fill: Light,
    pub rim: Light,
    pub ambient: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            key: Light::from_angles(-0.719, 1.014, Vec3::ONE, 0.70),
            fill: Light::NONE,
            rim: Light::NONE,
            ambient: 0.30,
        }
    }
}

impl Lighting {
    pub fn new(key: Light, fill: Light, rim: Light, ambient: f32) -> Self {
        Self {
            key,
            fill,
            rim,
            ambient: ambient.max(0.0),
        }
    }

    /// Shade an albedo at a surface normal, with `view` pointing toward the camera.
    pub fn shade(&self, normal: Vec3, view: Vec3, albedo: Vec3) -> Vec3 {
        let light = self.key.diffuse(normal) + self.fill.diffuse(normal);
        let ambient = Vec3::splat(self.ambient);
        (albedo * (light + ambient) + self.rim.rim(normal, view)).clamp(Vec3::ZERO, Vec3::ONE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surface_facing_the_key_light_is_brighter_than_one_facing_away() {
        let lighting = Lighting::default();
        let albedo = Vec3::splat(0.8);
        let toward = lighting.shade(lighting.key.direction, Vec3::Z, albedo);
        let away = lighting.shade(-lighting.key.direction, Vec3::Z, albedo);
        assert!(toward.length() > away.length());
        assert!(away.x > 0.0, "ambient keeps the far side readable");
    }

    #[test]
    fn a_rim_light_brightens_the_silhouette_only() {
        let lighting = Lighting {
            rim: Light::from_angles(0.0, 0.0, Vec3::ONE, 0.5),
            ..Lighting::default()
        };
        let albedo = Vec3::ZERO;
        let centre = lighting.shade(Vec3::Z, Vec3::Z, albedo);
        let edge = lighting.shade(Vec3::X, Vec3::Z, albedo);
        assert!(edge.x > centre.x);
    }

    #[test]
    fn warmth_cools_and_warms_the_tint() {
        assert!(Light::tint(-1.0).z > Light::tint(-1.0).x);
        assert!(Light::tint(1.0).x > Light::tint(1.0).z);
    }
}
