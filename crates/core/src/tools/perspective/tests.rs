use std::time::Instant;

use super::*;

const CANVAS: usize = 1024;

fn blank() -> Vec<u8> {
    vec![0u8; CANVAS * CANVAS]
}

fn stamp(mask: &mut [u8], width: usize, height: usize, x: i32, y: i32) {
    for dy in -1..=1 {
        for dx in -1..=1 {
            let nx = x + dx;
            let ny = y + dy;
            if nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < height {
                mask[ny as usize * width + nx as usize] = 255;
            }
        }
    }
}

fn segment(mask: &mut [u8], width: usize, height: usize, from: (f32, f32), to: (f32, f32)) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let length = dx.hypot(dy);
    if length < 0.5 {
        return;
    }
    let steps = (length * 2.0).ceil().max(1.0) as usize;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = (from.0 + dx * t).round() as i32;
        let y = (from.1 + dy * t).round() as i32;
        stamp(mask, width, height, x, y);
    }
}

/// A full line through `point` at `degrees`, clipped to the canvas.
fn ray_line(mask: &mut [u8], width: usize, height: usize, point: (f32, f32), degrees: f32) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let mut low = f32::NEG_INFINITY;
    let mut high = f32::INFINITY;
    for (position, direction, limit) in [
        (point.0, cos, (width - 1) as f32),
        (point.1, sin, (height - 1) as f32),
    ] {
        if direction.abs() < 1e-6 {
            if position < 0.0 || position > limit {
                return;
            }
        } else {
            let a = (0.0 - position) / direction;
            let b = (limit - position) / direction;
            low = low.max(a.min(b));
            high = high.min(a.max(b));
        }
    }
    if !low.is_finite() || !high.is_finite() || high - low < 1.0 {
        return;
    }
    let from = (point.0 + cos * low, point.1 + sin * low);
    let to = (point.0 + cos * high, point.1 + sin * high);
    segment(mask, width, height, from, to);
}

fn fan(
    mask: &mut [u8],
    width: usize,
    height: usize,
    point: (f32, f32),
    centre_degrees: f32,
    spread_degrees: f32,
    count: usize,
) {
    for index in 0..count {
        let offset = if count > 1 {
            (index as f32 / (count - 1) as f32 - 0.5) * spread_degrees
        } else {
            0.0
        };
        ray_line(mask, width, height, point, centre_degrees + offset);
    }
}

fn towards(from: (f32, f32), to: (f32, f32)) -> f32 {
    (to.1 - from.1).atan2(to.0 - from.0).to_degrees()
}

fn next_random(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

#[test]
fn one_point_family_recovers_the_centre() {
    let mut mask = blank();
    for index in 0..12 {
        ray_line(
            &mut mask,
            CANVAS,
            CANVAS,
            (512.0, 512.0),
            index as f32 * 15.0,
        );
    }
    let result = infer(&mask, CANVAS, CANVAS, PerspectiveOptions::default())
        .expect("twelve lines through one point");
    let point = &result.points[0];
    assert!(!point.at_infinity, "{point:?}");
    assert!((point.x - 512.0).hypot(point.y - 512.0) <= 8.0, "{point:?}");
    assert!(point.confidence > 0.6, "{point:?}");
}

#[test]
fn two_families_recover_two_points_and_the_horizon() {
    let mut mask = blank();
    let left = (280.0, 360.0);
    let right = (744.0, 360.0);
    fan(
        &mut mask,
        CANVAS,
        CANVAS,
        left,
        towards(left, (512.0, 512.0)),
        80.0,
        6,
    );
    fan(
        &mut mask,
        CANVAS,
        CANVAS,
        right,
        towards(right, (512.0, 512.0)),
        80.0,
        6,
    );
    let result =
        infer(&mask, CANVAS, CANVAS, PerspectiveOptions::default()).expect("two line families");
    let finite: Vec<_> = result
        .points
        .iter()
        .filter(|point| !point.at_infinity)
        .collect();
    assert!(finite.len() >= 2, "{:?}", result.points);
    for expected in [left, right] {
        assert!(
            finite
                .iter()
                .any(|point| (point.x - expected.0).hypot(point.y - expected.1) <= 12.0),
            "missing {expected:?}: {:?}",
            result.points
        );
    }
    let horizon = result.horizon.expect("two finite points make a horizon");
    for point in finite.iter().take(2) {
        let residual = horizon.a * point.x + horizon.b * point.y + horizon.c;
        assert!(residual.abs() < 0.5, "{horizon:?} misses {point:?}");
    }
}

#[test]
fn three_families_land_in_their_quadrants() {
    let mut mask = blank();
    let expected = [(220.0, 240.0), (804.0, 240.0), (512.0, 860.0)];
    for point in expected {
        fan(
            &mut mask,
            CANVAS,
            CANVAS,
            point,
            towards(point, (512.0, 512.0)),
            70.0,
            5,
        );
    }
    let result =
        infer(&mask, CANVAS, CANVAS, PerspectiveOptions::default()).expect("three line families");
    let finite: Vec<_> = result
        .points
        .iter()
        .filter(|point| !point.at_infinity)
        .collect();
    assert!(finite.len() >= 3, "{:?}", result.points);
    let quadrants = [(true, true), (false, true), (true, false)];
    for (expected, quadrant) in expected.iter().zip(quadrants) {
        assert!(
            finite.iter().any(|point| {
                (point.x - expected.0).hypot(point.y - expected.1) <= 12.0
                    && (point.x < 512.0) == quadrant.0
                    && (point.y < 512.0) == quadrant.1
            }),
            "missing {expected:?} in {quadrant:?}: {:?}",
            result.points
        );
    }
}

#[test]
fn a_parallel_family_is_a_point_at_infinity() {
    let mut mask = blank();
    let angle = 30.0f32;
    let (sin, cos) = angle.to_radians().sin_cos();
    for offset in -300..=300 {
        if offset % 75 != 0 {
            continue;
        }
        let point = (512.0 - sin * offset as f32, 512.0 + cos * offset as f32);
        ray_line(&mut mask, CANVAS, CANVAS, point, angle);
    }
    let result = infer(&mask, CANVAS, CANVAS, PerspectiveOptions::default())
        .expect("parallel lines are still lines");
    let point = result
        .points
        .iter()
        .find(|point| point.at_infinity)
        .expect("the family is parallel");
    let difference = (point.direction_degrees - angle + 90.0).rem_euclid(180.0) - 90.0;
    assert!(difference.abs() <= 3.0, "{point:?}");
    assert!(point.confidence > 0.6, "{point:?}");
}

#[test]
fn specks_do_not_move_the_recovered_point() {
    let mut mask = blank();
    for index in 0..12 {
        ray_line(
            &mut mask,
            CANVAS,
            CANVAS,
            (512.0, 512.0),
            index as f32 * 15.0,
        );
    }
    let inked = mask.iter().filter(|&&value| value > 0).count();
    let mut state = 0x9E37_79B9u32;
    for _ in 0..inked / 5 {
        let x = (next_random(&mut state) as usize) % CANVAS;
        let y = (next_random(&mut state) as usize) % CANVAS;
        mask[y * CANVAS + x] = 255;
    }
    let result = infer(&mask, CANVAS, CANVAS, PerspectiveOptions::default())
        .expect("the lines are still there");
    assert!(
        result
            .points
            .iter()
            .any(|point| !point.at_infinity && (point.x - 512.0).hypot(point.y - 512.0) <= 8.0),
        "{:?}",
        result.points
    );
}

#[test]
fn empty_and_single_line_masks_return_none() {
    assert!(infer(&[], 0, 0, PerspectiveOptions::default()).is_none());
    assert!(infer(&[0u8; 16], 4, 4, PerspectiveOptions::default()).is_none());
    let blank = blank();
    assert!(infer(&blank, CANVAS, CANVAS, PerspectiveOptions::default()).is_none());
    let mut one = blank;
    ray_line(&mut one, CANVAS, CANVAS, (512.0, 512.0), 20.0);
    assert!(infer(&one, CANVAS, CANVAS, PerspectiveOptions::default()).is_none());
    assert!(infer(&[0u8; 10], CANVAS, CANVAS, PerspectiveOptions::default()).is_none());
}

#[test]
fn two_runs_are_identical() {
    let mut mask = blank();
    for index in 0..12 {
        ray_line(
            &mut mask,
            CANVAS,
            CANVAS,
            (512.0, 512.0),
            index as f32 * 15.0,
        );
    }
    let first = infer(&mask, CANVAS, CANVAS, PerspectiveOptions::default()).unwrap();
    let second = infer(&mask, CANVAS, CANVAS, PerspectiveOptions::default()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn editor_reads_a_layer_alpha() {
    let mut editor = Editor::new(CANVAS, CANVAS, 1 << 20);
    let mut mask = blank();
    for index in 0..12 {
        ray_line(
            &mut mask,
            CANVAS,
            CANVAS,
            (512.0, 512.0),
            index as f32 * 15.0,
        );
    }
    for y in 0..CANVAS {
        for x in 0..CANVAS {
            if mask[y * CANVAS + x] > 0 {
                editor.document.layers[0]
                    .surface
                    .set(x, y, [0.0, 0.0, 0.0, 1.0]);
            }
        }
    }
    let result = editor
        .infer_perspective(Some(0), PerspectiveOptions::default())
        .expect("the layer holds line art");
    let point = &result.points[0];
    assert!((point.x - 512.0).hypot(point.y - 512.0) <= 8.0, "{point:?}");
}

#[test]
#[ignore = "timing benchmark; run with --release"]
fn four_k_inference_stays_under_three_hundred_ms() {
    let (width, height) = (3840usize, 2160usize);
    let mut mask = vec![0u8; width * height];
    let vps = [(900.0, 700.0), (2900.0, 700.0), (1920.0, 2000.0)];
    for point in vps {
        fan(
            &mut mask,
            width,
            height,
            point,
            towards(point, (width as f32 * 0.5, height as f32 * 0.5)),
            140.0,
            30,
        );
    }
    let start = Instant::now();
    let result = infer(&mask, width, height, PerspectiveOptions::default());
    let elapsed = start.elapsed();
    eprintln!(
        "4K perspective inference: {elapsed:?}, {} points",
        result
            .as_ref()
            .map_or(0, |perspective| perspective.points.len())
    );
    assert!(result.is_some());
    assert!(elapsed.as_millis() < 300, "4K inference took {elapsed:?}");
}
