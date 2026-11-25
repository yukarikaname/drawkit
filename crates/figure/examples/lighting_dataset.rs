//! Generate paired mannequin renders and light labels for the light estimator.
//!
//! The same CPU rasteriser the GPU pass is tested against draws each image, so the label is the
//! exact key direction and ambient term the shader used. Output is one little-endian file:
//!
//!   "DKLT" u32 version=1 u32 count u32 width u32 height u32 channels=1
//!   per sample: f32 gray[width * height], f32 labels[6]
//!
//! labels are key x, y, z (unit, toward the light), key intensity, ambient, warmth.
//!
//! Run with:
//!   cargo run --release -p drawkit-figure --example lighting_dataset -- 5000 96

use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use drawkit_figure::{Camera, Figure, Light, Lighting, PosePreset, Preset};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }

    fn unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    fn pick<T: Copy>(&mut self, values: &[T]) -> T {
        values[(self.next_u32() as usize) % values.len()]
    }
}

fn main() -> std::io::Result<()> {
    let mut args = env::args().skip(1);
    let count: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4000);
    let size: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(96);
    let path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("training/lighting/data/lighting.bin"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let presets = [
        Preset::RealisticNeutral,
        Preset::RealisticMale,
        Preset::RealisticFemale,
        Preset::Anime,
        Preset::Chibi,
    ];
    let poses = [
        PosePreset::APose,
        PosePreset::TPose,
        PosePreset::ArmsUp,
        PosePreset::Contrapposto,
    ];
    let mut rng = Rng::new(0x4d595df4d0f33173);
    let mut out = BufWriter::new(File::create(&path)?);
    out.write_all(b"DKLT")?;
    for value in [1_u32, count as u32, size as u32, size as u32, 1] {
        out.write_all(&value.to_le_bytes())?;
    }

    let mut image = vec![0.0f32; size * size];
    for index in 0..count {
        let mut figure = Figure::from_preset(rng.pick(&presets));
        figure.apply_pose_preset(rng.pick(&poses));

        let azimuth = rng.range(-std::f32::consts::PI, std::f32::consts::PI);
        let elevation = rng.range(0.05, 1.35);
        let intensity = rng.range(0.35, 1.25);
        let ambient = rng.range(0.10, 0.60);
        let warmth = rng.range(-1.0, 1.0);
        let key = Light::from_angles(azimuth, elevation, Light::tint(warmth), intensity);
        let lighting = Lighting::new(key, Light::NONE, Light::NONE, ambient);

        let portrait = rng.unit() < 0.5;
        let (height_factor, distance) = if portrait {
            (0.84, rng.range(1.6, 2.8))
        } else {
            (0.55, rng.range(4.0, 5.2))
        };
        let camera = Camera::orbit(
            glam::Vec3::new(0.0, figure.proportions.height * height_factor, 0.0),
            distance,
            rng.range(-0.18, 0.18),
            rng.range(-0.05, 0.20),
        );
        let albedo = rng.range(0.58, 0.92);
        let pixels = figure.render_with_lighting(&camera, size, size, [albedo; 3], &lighting);
        let paper = rng.range(0.72, 1.0);
        let vignette = rng.range(0.0, 0.10);
        let mut coverage = vec![0.0f32; size * size];
        for (pixel_index, pixel) in pixels.iter().enumerate() {
            let x = (pixel_index % size) as f32 / size as f32;
            let y = (pixel_index / size) as f32 / size as f32;
            let edge = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 1.35;
            let background = (paper - vignette * edge).clamp(0.0, 1.0);
            let alpha = pixel[3].clamp(0.0, 1.0);
            coverage[pixel_index] = alpha;
            let value = pixel[0].clamp(0.0, 1.0) * alpha + background * (1.0 - alpha);
            image[pixel_index] = value.clamp(0.0, 1.0);
        }
        // Anime lighting is read from bands and an outline, not from a smooth photograph. Outline
        // the silhouette and, three times in four, quantise the value into a few cel bands.
        let outline = rng.range(0.35, 0.85);
        let cel = rng.unit() < 0.75;
        let levels = 3.0 + (rng.next_u32() % 4) as f32;
        for y in 0..size {
            for x in 0..size {
                let index = y * size + x;
                let left = coverage[y * size + x.saturating_sub(1)];
                let right = coverage[y * size + (x + 1).min(size - 1)];
                let top = coverage[y.saturating_sub(1) * size + x];
                let bottom = coverage[(y + 1).min(size - 1) * size + x];
                let edge = (right - left)
                    .abs()
                    .max((bottom - top).abs())
                    .clamp(0.0, 1.0);
                let mut value = image[index] * (1.0 - edge * outline);
                if cel {
                    value = (value * levels).round() / levels;
                }
                let noise = (rng.unit() - 0.5) * 0.008;
                image[index] = (value + noise).clamp(0.0, 1.0);
            }
        }
        let labels = [
            key.direction.x,
            key.direction.y,
            key.direction.z,
            intensity,
            ambient,
            warmth,
        ];
        for value in &image {
            out.write_all(&value.to_le_bytes())?;
        }
        for value in labels {
            out.write_all(&value.to_le_bytes())?;
        }
        if (index + 1) % 250 == 0 {
            eprintln!("{}/{}", index + 1, count);
        }
    }
    out.flush()?;
    eprintln!("wrote {} samples to {}", count, path.display());
    Ok(())
}
