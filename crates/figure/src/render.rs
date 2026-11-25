//! A CPU rasteriser for the mannequin.
//!
//! It is not here to be the final renderer. It is the reference image the GPU pass has to match,
//! it keeps the figure usable on a machine without a compute queue, and it makes the 3D part
//! testable: a test can assert that a sphere covers the centre pixel and not the corner.
//!
//! Flat triangles, a depth buffer, one directional light. A drawing figure needs readable planes,
//! not a physically based material.

use glam::Vec3;

use crate::light::Lighting;
use crate::mesh::Mesh;

/// Optional cartoon treatment for the reference figure: a few value bands and a silhouette line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStyle {
    pub toon: bool,
    pub outline: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
}

impl Camera {
    /// An orbit camera around `target`; yaw is around Y, pitch lifts the eye.
    pub fn orbit(target: Vec3, distance: f32, yaw: f32, pitch: f32) -> Self {
        let eye = target
            + Vec3::new(
                distance * pitch.cos() * yaw.sin(),
                distance * pitch.sin(),
                distance * pitch.cos() * yaw.cos(),
            );
        Self {
            eye,
            target,
            up: Vec3::Y,
            fov_y: 45.0_f32.to_radians(),
        }
    }
}

/// Straight RGBA in 0..1, top row first, transparent where the mesh does not cover.
pub fn render(
    mesh: &Mesh,
    camera: &Camera,
    width: usize,
    height: usize,
    color: [f32; 3],
) -> Vec<[f32; 4]> {
    render_with_lighting(mesh, camera, width, height, color, &Lighting::default())
}

/// The same rasteriser with an explicit rig. The GPU pass uses the same lights, so a changed
/// key, fill or rim must still agree on the silhouette and the approximate shade.
pub fn render_with_lighting(
    mesh: &Mesh,
    camera: &Camera,
    width: usize,
    height: usize,
    color: [f32; 3],
    lighting: &Lighting,
) -> Vec<[f32; 4]> {
    render_styled(
        mesh,
        camera,
        width,
        height,
        color,
        lighting,
        &RenderStyle::default(),
    )
}

pub fn render_styled(
    mesh: &Mesh,
    camera: &Camera,
    width: usize,
    height: usize,
    color: [f32; 3],
    lighting: &Lighting,
    style: &RenderStyle,
) -> Vec<[f32; 4]> {
    let mut pixels = vec![[0.0; 4]; width * height];
    if width == 0 || height == 0 || mesh.is_empty() {
        return pixels;
    }
    let forward = (camera.target - camera.eye).normalize_or_zero();
    let right = forward.cross(camera.up).normalize_or_zero();
    if forward == Vec3::ZERO || right == Vec3::ZERO {
        return pixels;
    }
    let up = right.cross(forward);
    let tan_half = (camera.fov_y * 0.5).tan();
    let aspect = width as f32 / height as f32;
    let albedo = Vec3::from_array(color);

    // Inverse depth: zero means untouched, and a larger value is nearer.
    let mut depth = vec![0.0_f32; width * height];

    for triangle in mesh.indices.as_chunks::<3>().0 {
        let (a, b, c) = (
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        );
        let world = [
            Vec3::from_array(mesh.vertices[a].position),
            Vec3::from_array(mesh.vertices[b].position),
            Vec3::from_array(mesh.vertices[c].position),
        ];
        let normals = [
            Vec3::from_array(mesh.vertices[a].normal),
            Vec3::from_array(mesh.vertices[b].normal),
            Vec3::from_array(mesh.vertices[c].normal),
        ];

        let mut screen = [(0.0_f32, 0.0_f32, 0.0_f32); 3];
        let mut visible = true;
        for (index, point) in world.iter().enumerate() {
            let relative = *point - camera.eye;
            let z = relative.dot(forward);
            if z <= 0.03 {
                visible = false;
                break;
            }
            let x = relative.dot(right) / (z * tan_half * aspect);
            let y = relative.dot(up) / (z * tan_half);
            screen[index] = (
                (x * 0.5 + 0.5) * width as f32,
                (1.0 - (y * 0.5 + 0.5)) * height as f32,
                1.0 / z,
            );
        }
        if !visible {
            continue;
        }
        let area = edge(screen[0], screen[1], screen[2]);
        if area.abs() < 1e-9 {
            continue;
        }
        let inverse_area = 1.0 / area;
        let min_x = screen
            .iter()
            .map(|s| s.0)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0)
            .min(width as f32 - 1.0) as usize;
        let max_x = screen
            .iter()
            .map(|s| s.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(0.0)
            .min(width as f32 - 1.0) as usize;
        let min_y = screen
            .iter()
            .map(|s| s.1)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0)
            .min(height as f32 - 1.0) as usize;
        let max_y = screen
            .iter()
            .map(|s| s.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(0.0)
            .min(height as f32 - 1.0) as usize;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let x = px as f32 + 0.5;
                let y = py as f32 + 0.5;
                let w0 = edge(screen[1], screen[2], (x, y, 0.0)) * inverse_area;
                let w1 = edge(screen[2], screen[0], (x, y, 0.0)) * inverse_area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let index = py * width + px;
                let inverse_depth = w0 * screen[0].2 + w1 * screen[1].2 + w2 * screen[2].2;
                if inverse_depth <= depth[index] {
                    continue;
                }
                depth[index] = inverse_depth;
                let mut normal =
                    (normals[0] * w0 + normals[1] * w1 + normals[2] * w2).normalize_or_zero();
                let point = world[0] * w0 + world[1] * w1 + world[2] * w2;
                if normal.dot(camera.eye - point) < 0.0 {
                    normal = -normal;
                }
                let view = (camera.eye - point).normalize_or_zero();
                let mut shade = lighting.shade(normal, view, albedo);
                if style.toon {
                    let value = shade.dot(Vec3::new(0.299, 0.587, 0.114));
                    let band = if value < 0.30 {
                        0.30
                    } else if value < 0.64 {
                        0.64
                    } else {
                        0.92
                    };
                    shade = Vec3::splat(band);
                }
                pixels[index] = [shade.x, shade.y, shade.z, 1.0];
            }
        }
    }
    if style.outline > 0 {
        outline(&mut pixels, width, height, style.outline);
    }
    pixels
}

/// Darken the silhouette by `width` pixels. The alpha copy keeps the line from growing inward.
fn outline(pixels: &mut [[f32; 4]], width: usize, height: usize, radius: u8) {
    let alpha: Vec<f32> = pixels.iter().map(|pixel| pixel[3]).collect();
    let radius = radius as isize;
    let mut line = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if alpha[index] < 0.5 {
                continue;
            }
            let mut edge = false;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx < 0
                        || ny < 0
                        || nx >= width as isize
                        || ny >= height as isize
                        || alpha[ny as usize * width + nx as usize] < 0.5
                    {
                        edge = true;
                    }
                    if edge {
                        break;
                    }
                }
                if edge {
                    break;
                }
            }
            if edge {
                line.push(index);
            }
        }
    }
    for index in line {
        pixels[index] = [0.08, 0.08, 0.10, 1.0];
    }
}

fn edge(a: (f32, f32, f32), b: (f32, f32, f32), point: (f32, f32, f32)) -> f32 {
    (point.0 - a.0) * (b.1 - a.1) - (point.1 - a.1) * (b.0 - a.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::sphere;
    use crate::proportions::Preset;

    fn camera() -> Camera {
        Camera {
            eye: Vec3::new(0.0, 0.0, 2.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: 45.0_f32.to_radians(),
        }
    }

    #[test]
    fn a_sphere_covers_the_centre_and_not_the_corner() {
        let mesh = sphere(Vec3::ZERO, 0.5, 12, 18);
        let pixels = render(&mesh, &camera(), 64, 64, [0.8, 0.6, 0.4]);
        assert!(
            pixels[32 * 64 + 32][3] > 0.9,
            "the sphere did not cover the centre"
        );
        assert_eq!(pixels[0][3], 0.0, "the corner should be empty");
    }

    #[test]
    fn the_shading_varies_across_the_sphere() {
        let mesh = sphere(Vec3::ZERO, 0.5, 12, 18);
        let pixels = render(&mesh, &camera(), 64, 64, [1.0, 1.0, 1.0]);
        let lit: Vec<f32> = pixels.iter().filter(|p| p[3] > 0.0).map(|p| p[0]).collect();
        assert!(lit.iter().any(|value| *value < 0.6), "the sphere is flat");
        assert!(lit.iter().any(|value| *value > 0.9), "nothing is lit");
    }

    #[test]
    fn the_toon_style_has_a_handful_of_values() {
        let mesh = sphere(Vec3::ZERO, 0.5, 12, 18);
        let style = RenderStyle {
            toon: true,
            outline: 0,
        };
        let pixels = render_styled(
            &mesh,
            &camera(),
            64,
            64,
            [1.0, 1.0, 1.0],
            &Lighting::default(),
            &style,
        );
        let mut values: Vec<u32> = pixels
            .iter()
            .filter(|pixel| pixel[3] > 0.5)
            .map(|pixel| (pixel[0] * 255.0).round() as u32)
            .collect();
        values.sort_unstable();
        values.dedup();
        assert!(values.len() <= 3, "toon shading has {} bands", values.len());
    }

    #[test]
    fn the_outline_darkens_the_silhouette() {
        let mesh = sphere(Vec3::ZERO, 0.5, 12, 18);
        let smooth = render(&mesh, &camera(), 64, 64, [1.0, 1.0, 1.0]);
        let outlined = render_styled(
            &mesh,
            &camera(),
            64,
            64,
            [1.0, 1.0, 1.0],
            &Lighting::default(),
            &RenderStyle {
                toon: false,
                outline: 1,
            },
        );
        let dark = |pixels: &[[f32; 4]]| {
            pixels
                .iter()
                .filter(|pixel| pixel[3] > 0.5 && pixel[0] < 0.2)
                .count()
        };
        assert_eq!(dark(&smooth), 0);
        assert!(dark(&outlined) > 10);
    }

    #[test]
    fn a_mannequin_fills_a_readable_part_of_the_frame() {
        let figure = crate::Figure::from_preset(Preset::RealisticNeutral);
        let camera = Camera::orbit(Vec3::new(0.0, 0.9, 0.0), 4.5, 0.3, 0.1);
        let pixels = figure.render(&camera, 128, 192, [0.7, 0.7, 0.7]);
        let covered = pixels.iter().filter(|pixel| pixel[3] > 0.5).count();
        assert!(covered > 1_000, "only {covered} pixels were drawn");
    }
}
