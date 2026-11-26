//! Perspective guides for a shell: infer from one layer or from the visible composite.
//!
//! Each recovered point is eight floats, in order:
//!
//! | index | meaning |
//! |-------|---------|
//! | 0 | `x`, document pixels |
//! | 1 | `y`, document pixels |
//! | 2 | `at_infinity`, `1.0` or `0.0` |
//! | 3 | `direction_degrees`, `[0, 180)`; meaningful only at infinity |
//! | 4 | `confidence`, supporting lines divided by considered lines |
//! | 5 | `support`, number of supporting lines |
//! | 6 | `lines`, number of considered lines |
//! | 7 | reserved, `0.0` |
//!
//! `capacity` is a point count, not a float count. A null output, a zero capacity, an invalid
//! handle, or a buffer too short for all recovered points is refused with 0 and nothing is
//! written.

use std::os::raw::c_void;

use super::editor;
use crate::tools::perspective::PerspectiveOptions;

const POINT_FLOATS: usize = 8;

/// Infer up to three vanishing points from a layer's alpha.
///
/// `layer` is an index into the layer list; `u32::MAX` reads the visible composite instead. A fill
/// or group layer has no line art and returns 0. Use the default [`PerspectiveOptions`].
///
/// Returns the number of points packed, or 0 when the input has fewer than two usable lines, the
/// buffer is null or short, or the handle is invalid.
///
/// # Safety
/// `out` must point at `capacity * 8` writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_perspective_infer_layer(
    handle: *mut c_void,
    layer: u32,
    out: *mut f32,
    capacity: u32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || capacity == 0 {
        return 0;
    }
    let layer = if layer == u32::MAX {
        None
    } else {
        Some(layer as usize)
    };
    let Some(perspective) = editor.infer_perspective(layer, PerspectiveOptions::default()) else {
        return 0;
    };
    let count = perspective.points.len();
    if count > capacity as usize {
        return 0;
    }
    // SAFETY: the caller promised room for `capacity` points of eight floats each.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, capacity as usize * POINT_FLOATS) };
    for (index, point) in perspective.points.iter().enumerate() {
        let values = &mut buffer[index * POINT_FLOATS..(index + 1) * POINT_FLOATS];
        values[0] = point.x;
        values[1] = point.y;
        values[2] = if point.at_infinity { 1.0 } else { 0.0 };
        values[3] = point.direction_degrees;
        values[4] = point.confidence;
        values[5] = point.support as f32;
        values[6] = point.lines as f32;
        values[7] = 0.0;
    }
    count as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Editor;

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

    fn line(mask: &mut [u8], width: usize, height: usize, from: (f32, f32), to: (f32, f32)) {
        let dx = to.0 - from.0;
        let dy = to.1 - from.1;
        let steps = dx.hypot(dy).ceil().max(1.0) as usize;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            stamp(
                mask,
                width,
                height,
                (from.0 + dx * t).round() as i32,
                (from.1 + dy * t).round() as i32,
            );
        }
    }

    fn ray(mask: &mut [u8], width: usize, height: usize, point: (f32, f32), degrees: f32) {
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
        line(
            mask,
            width,
            height,
            (point.0 + cos * low, point.1 + sin * low),
            (point.0 + cos * high, point.1 + sin * high),
        );
    }

    #[test]
    fn packs_the_documented_layout_and_refuses_a_short_buffer() {
        let (width, height) = (256usize, 256usize);
        let mut editor = Editor::new(width, height, 1 << 20);
        let mut mask = vec![0u8; width * height];
        for point in [(80.0f32, 100.0f32), (176.0, 100.0)] {
            for index in 0..5 {
                ray(&mut mask, width, height, point, 15.0 + index as f32 * 35.0);
            }
        }
        for y in 0..height {
            for x in 0..width {
                if mask[y * width + x] > 0 {
                    editor.document.layers[0]
                        .surface
                        .set(x, y, [0.0, 0.0, 0.0, 1.0]);
                }
            }
        }
        let handle = (&mut editor as *mut Editor).cast::<c_void>();
        let mut short = [0f32; 8];
        // SAFETY: the buffer is one point, which cannot hold two recovered points.
        assert_eq!(
            unsafe { dk_perspective_infer_layer(handle, u32::MAX, short.as_mut_ptr(), 1) },
            0
        );
        let mut out = [0f32; 3 * POINT_FLOATS];
        // SAFETY: the buffer holds the maximum number of points.
        let count = unsafe { dk_perspective_infer_layer(handle, u32::MAX, out.as_mut_ptr(), 3) };
        assert_eq!(count, 2);
        for point in out
            .as_chunks::<POINT_FLOATS>()
            .0
            .iter()
            .take(count as usize)
        {
            assert!(point[0].is_finite() && point[1].is_finite());
            assert!(point[0] >= 0.0 && point[0] < width as f32);
            assert!(point[3] >= 0.0 && point[3] < 180.0);
            assert!(point[4] > 0.2 && point[4] <= 1.0);
            assert!(point[5] >= point[6] * point[4] - 0.01);
            assert_eq!(point[7], 0.0);
        }
        // SAFETY: a null pointer is refused before it is read.
        assert_eq!(
            unsafe { dk_perspective_infer_layer(handle, u32::MAX, std::ptr::null_mut(), 3) },
            0
        );
    }
}
