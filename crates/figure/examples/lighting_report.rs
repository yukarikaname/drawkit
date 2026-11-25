//! Split a multi-direction lighting sheet, estimate the key light for each cell, and render a
//! shaded sphere with the proposed rig. The result is a visual check, not a test fixture.
//!
//!   cargo run --release -p drawkit-figure --example lighting_report -- \
//!     training/lighting/eval/pixiv_69048188.jpg training/lighting/eval/report.png 2 4

use std::env;

use drawkit_figure::light_model;
use drawkit_figure::{Camera, Mesh, Vertex};
use glam::Vec3;
use image::{Rgb, RgbImage};

fn sphere(radius: f32) -> Mesh {
    let rings = 24;
    let sectors = 32;
    let mut vertices = Vec::new();
    for ring in 0..=rings {
        let phi = std::f32::consts::PI * ring as f32 / rings as f32;
        for sector in 0..=sectors {
            let theta = std::f32::consts::TAU * sector as f32 / sectors as f32;
            let normal = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
            vertices.push(Vertex {
                position: (normal * radius).to_array(),
                normal: normal.to_array(),
            });
        }
    }
    let mut indices = Vec::new();
    for ring in 0..rings {
        for sector in 0..sectors {
            let a = ring * (sectors + 1) + sector;
            let b = a + sectors + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Mesh { vertices, indices }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("usage: lighting_report <sheet> <out.png> <rows> <cols>");
        return;
    }
    let sheet = image::open(&args[0]).expect("sheet").to_rgb8();
    let rows: u32 = args[2].parse().expect("rows");
    let cols: u32 = args[3].parse().expect("cols");
    let cell_w = sheet.width() / cols;
    let cell_h = sheet.height() / rows;
    let sphere_size = cell_w.min(240);
    let label = 28;
    let stride = cell_h + sphere_size + label;
    let mut report = RgbImage::new(sheet.width(), rows * stride);
    let mesh = sphere(1.0);
    for row in 0..rows {
        let strip =
            image::imageops::crop_imm(&sheet, 0, row * cell_h, sheet.width(), cell_h).to_image();
        image::imageops::overlay(&mut report, &strip, 0, (row * stride) as i64);
        for col in 0..cols {
            let cell =
                image::imageops::crop_imm(&sheet, col * cell_w, row * cell_h, cell_w, cell_h)
                    .to_image();
            let cell = subject_crop(&cell);
            let gray = cell
                .pixels()
                .map(|pixel| {
                    (0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32)
                        / 255.0
                })
                .collect::<Vec<_>>();
            eprintln!("cell {row},{col} crop {}x{}", cell.width(), cell.height());
            let Some(estimate) =
                light_model::estimate(&gray, cell.width() as usize, cell.height() as usize)
            else {
                eprintln!("cell {row},{col}: no estimate");
                continue;
            };
            let lighting = estimate.lighting;
            println!(
                "cell {row},{col}: az {:+.1}° el {:+.1}° I {:.2} ambient {:.2} warmth {:+.2}",
                estimate
                    .direction
                    .z
                    .atan2(estimate.direction.x)
                    .to_degrees(),
                estimate.direction.y.asin().to_degrees(),
                estimate.intensity,
                estimate.ambient,
                estimate.warmth
            );
            let camera = Camera::orbit(Vec3::ZERO, 3.0, 0.0, 0.0);
            let pixels = drawkit_figure::render::render_with_lighting(
                &mesh,
                &camera,
                sphere_size as usize,
                sphere_size as usize,
                [0.82, 0.82, 0.82],
                &lighting,
            );
            let x = col * cell_w + (cell_w - sphere_size) / 2;
            let y = row * stride + cell_h + label;
            for (index, pixel) in pixels.iter().enumerate() {
                let px = index as u32 % sphere_size;
                let py = index as u32 / sphere_size;
                if let Some(target) = report.get_pixel_mut_checked(x + px, y + py) {
                    let alpha = pixel[3];
                    *target = Rgb([
                        (pixel[0] * 255.0 * alpha + 240.0 * (1.0 - alpha)) as u8,
                        (pixel[1] * 255.0 * alpha + 240.0 * (1.0 - alpha)) as u8,
                        (pixel[2] * 255.0 * alpha + 240.0 * (1.0 - alpha)) as u8,
                    ]);
                }
            }
            // The predicted direction as a screen-space arrow: x is right, y is up in the image.
            let arrow_x = estimate.direction.x;
            let arrow_y = -estimate.direction.y;
            let length = (arrow_x * arrow_x + arrow_y * arrow_y).sqrt().max(1e-4);
            let reach = sphere_size as f32 * 0.32;
            let cx = x as f32 + sphere_size as f32 / 2.0;
            let cy = y as f32 + sphere_size as f32 / 2.0;
            let tip = (cx + arrow_x / length * reach, cy + arrow_y / length * reach);
            draw_line(&mut report, (cx, cy), tip, [230, 90, 40]);
            draw_line(
                &mut report,
                tip,
                (
                    tip.0 - arrow_x / length * 12.0 + arrow_y / length * 8.0,
                    tip.1 - arrow_y / length * 12.0 - arrow_x / length * 8.0,
                ),
                [230, 90, 40],
            );
            draw_line(
                &mut report,
                tip,
                (
                    tip.0 - arrow_x / length * 12.0 - arrow_y / length * 8.0,
                    tip.1 - arrow_y / length * 12.0 + arrow_x / length * 8.0,
                ),
                [230, 90, 40],
            );
        }
    }
    report.save(&args[1]).expect("report");
    eprintln!("wrote {}", args[1]);
}

/// Crop to the non-background subject with a small margin, the way a reference crop is framed.
fn subject_crop(cell: &RgbImage) -> RgbImage {
    let luma = |pixel: &Rgb<u8>| {
        0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32
    };
    let mut border = Vec::new();
    for x in 0..cell.width() {
        border.push(luma(cell.get_pixel(x, 0)));
        border.push(luma(cell.get_pixel(x, cell.height() - 1)));
    }
    for y in 0..cell.height() {
        border.push(luma(cell.get_pixel(0, y)));
        border.push(luma(cell.get_pixel(cell.width() - 1, y)));
    }
    border.sort_by(|a, b| a.total_cmp(b));
    let background = border[border.len() / 2];
    let mut min_x = cell.width();
    let mut min_y = cell.height();
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..cell.height() {
        for x in 0..cell.width() {
            if (luma(cell.get_pixel(x, y)) - background).abs() > 28.0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x >= max_x || min_y >= max_y {
        return cell.clone();
    }
    let pad = ((max_x - min_x).max(max_y - min_y) / 10).max(4);
    let x = min_x.saturating_sub(pad);
    let y = min_y.saturating_sub(pad);
    let width = (max_x + pad).min(cell.width() - 1) - x + 1;
    let height = (max_y + pad).min(cell.height() - 1) - y + 1;
    image::imageops::crop_imm(cell, x, y, width, height).to_image()
}

fn draw_line(image: &mut RgbImage, from: (f32, f32), to: (f32, f32), color: [u8; 3]) {
    let steps = ((to.0 - from.0).abs().max((to.1 - from.1).abs()) as i32).max(1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = from.0 + (to.0 - from.0) * t;
        let y = from.1 + (to.1 - from.1) * t;
        for oy in -1..=1 {
            for ox in -1..=1 {
                let (px, py) = (x as i32 + ox, y as i32 + oy);
                if px >= 0
                    && py >= 0
                    && let Some(pixel) = image.get_pixel_mut_checked(px as u32, py as u32)
                {
                    *pixel = Rgb(color);
                }
            }
        }
    }
}
