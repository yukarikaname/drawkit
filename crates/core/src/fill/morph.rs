//! Square morphology as two 1-D passes.
//!
//! `grow` and `erode` used to touch a 3x3 neighbourhood once per radius step. A square
//! structuring element is separable, so the same result costs two passes over the image, and a
//! binary mask needs only a running count instead of nine compares per pixel. The out-of-bounds
//! rules are the ones the pixel loops had: grow ignores outside pixels, erode treats them as
//! empty.

use std::collections::VecDeque;

/// Dilation by a (2r+1) square of a 0/1 map, ignoring pixels outside the image.
pub(crate) fn dilate_binary(source: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    if radius == 0 || width == 0 || height == 0 {
        return source.to_vec();
    }
    if radius == 1 {
        return dilate_one(source, width, height);
    }
    let mut rows = vec![0u8; width * height];
    for y in 0..height {
        let row = &source[y * width..(y + 1) * width];
        let out = &mut rows[y * width..(y + 1) * width];
        let mut count = 0usize;
        for value in row.iter().take(radius.min(width - 1) + 1) {
            count += usize::from(*value != 0);
        }
        for x in 0..width {
            out[x] = u8::from(count != 0);
            if x + radius + 1 < width {
                count += usize::from(row[x + radius + 1] != 0);
            }
            if x >= radius {
                count -= usize::from(row[x - radius] != 0);
            }
        }
    }
    let mut out = vec![0u8; width * height];
    let mut counts = vec![0usize; width];
    for y in 0..=radius.min(height - 1) {
        let row = &rows[y * width..(y + 1) * width];
        for (count, value) in counts.iter_mut().zip(row) {
            *count += usize::from(*value != 0);
        }
    }
    for y in 0..height {
        let out_row = &mut out[y * width..(y + 1) * width];
        for (value, count) in out_row.iter_mut().zip(&counts) {
            *value = u8::from(*count != 0);
        }
        if y + radius + 1 < height {
            let row = &rows[(y + radius + 1) * width..(y + radius + 2) * width];
            for (count, value) in counts.iter_mut().zip(row) {
                *count += usize::from(*value != 0);
            }
        }
        if y >= radius {
            let row = &rows[(y - radius) * width..(y - radius + 1) * width];
            for (count, value) in counts.iter_mut().zip(row) {
                *count -= usize::from(*value != 0);
            }
        }
    }
    out
}

/// Erosion by a (2r+1) square of a 0/1 map, with pixels outside the image counting as empty.
pub(crate) fn erode_binary(source: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    if radius == 0 || width == 0 || height == 0 {
        return source.to_vec();
    }
    if radius == 1 {
        return erode_one(source, width, height);
    }
    let span = 2 * radius + 1;
    let mut rows = vec![0u8; width * height];
    for y in 0..height {
        let row = &source[y * width..(y + 1) * width];
        let out = &mut rows[y * width..(y + 1) * width];
        let mut count = 0usize;
        for value in row.iter().take(radius.min(width - 1) + 1) {
            count += usize::from(*value != 0);
        }
        for x in 0..width {
            let full = x >= radius && x + radius < width;
            out[x] = u8::from(full && count == span);
            if x + radius + 1 < width {
                count += usize::from(row[x + radius + 1] != 0);
            }
            if x >= radius {
                count -= usize::from(row[x - radius] != 0);
            }
        }
    }
    let mut out = vec![0u8; width * height];
    let mut counts = vec![0usize; width];
    for y in 0..=radius.min(height - 1) {
        let row = &rows[y * width..(y + 1) * width];
        for (count, value) in counts.iter_mut().zip(row) {
            *count += usize::from(*value != 0);
        }
    }
    for y in 0..height {
        let full = y >= radius && y + radius < height;
        let out_row = &mut out[y * width..(y + 1) * width];
        for (value, count) in out_row.iter_mut().zip(&counts) {
            *value = u8::from(full && *count == span);
        }
        if y + radius + 1 < height {
            let row = &rows[(y + radius + 1) * width..(y + radius + 2) * width];
            for (count, value) in counts.iter_mut().zip(row) {
                *count += usize::from(*value != 0);
            }
        }
        if y >= radius {
            let row = &rows[(y - radius) * width..(y - radius + 1) * width];
            for (count, value) in counts.iter_mut().zip(row) {
                *count -= usize::from(*value != 0);
            }
        }
    }
    out
}

/// Maximum over a (2r+1) square, ignoring pixels outside the image.
pub(crate) fn grow_into(
    source: &[f32],
    width: usize,
    height: usize,
    radius: usize,
    out: &mut [f32],
) {
    if radius == 0 || width == 0 || height == 0 {
        out[..width * height].copy_from_slice(&source[..width * height]);
        return;
    }
    let mut rows = vec![0.0f32; width * height];
    let mut window = VecDeque::with_capacity(2 * radius + 1);
    for y in 0..height {
        let row = &source[y * width..(y + 1) * width];
        let out_row = &mut rows[y * width..(y + 1) * width];
        window.clear();
        let mut next = 0usize;
        for (x, out) in out_row.iter_mut().enumerate() {
            let high = (x + radius).min(width - 1);
            while next <= high {
                let value = row[next];
                while let Some(&back) = window.back() {
                    if row[back] <= value {
                        window.pop_back();
                    } else {
                        break;
                    }
                }
                window.push_back(next);
                next += 1;
            }
            let low = x.saturating_sub(radius);
            while let Some(&front) = window.front() {
                if front < low {
                    window.pop_front();
                } else {
                    break;
                }
            }
            *out = row[*window.front().expect("the window covers x")];
        }
    }
    let mut windows: Vec<VecDeque<usize>> = (0..width).map(|_| VecDeque::new()).collect();
    let mut next = 0usize;
    for y in 0..height {
        let high = (y + radius).min(height - 1);
        while next <= high {
            let offset = next * width;
            for (x, queue) in windows.iter_mut().enumerate() {
                let value = rows[offset + x];
                while let Some(&back) = queue.back() {
                    if rows[back * width + x] <= value {
                        queue.pop_back();
                    } else {
                        break;
                    }
                }
                queue.push_back(next);
            }
            next += 1;
        }
        let low = y.saturating_sub(radius);
        for (x, queue) in windows.iter_mut().enumerate() {
            while let Some(&front) = queue.front() {
                if front < low {
                    queue.pop_front();
                } else {
                    break;
                }
            }
            out[y * width + x] = rows[*queue.front().expect("the window covers y") * width + x];
        }
    }
}

/// Radius one is the default for both gap closing and the fill's growth, so its three-tap form
/// is worth spelling out: two passes of or/and instead of a running count.
fn dilate_one(source: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rows = vec![0u8; width * height];
    for y in 0..height {
        let row = &source[y * width..(y + 1) * width];
        let out = &mut rows[y * width..(y + 1) * width];
        if width < 3 {
            out.copy_from_slice(row);
            continue;
        }
        out[0] = row[0] | row[1];
        for x in 1..width - 1 {
            out[x] = row[x - 1] | row[x] | row[x + 1];
        }
        out[width - 1] = row[width - 2] | row[width - 1];
    }
    let mut out = vec![0u8; width * height];
    if height < 3 {
        out.copy_from_slice(&rows);
        return out;
    }
    for x in 0..width {
        out[x] = rows[x] | rows[width + x];
        out[(height - 1) * width + x] =
            rows[(height - 2) * width + x] | rows[(height - 1) * width + x];
    }
    for y in 1..height - 1 {
        let previous = &rows[(y - 1) * width..y * width];
        let current = &rows[y * width..(y + 1) * width];
        let next = &rows[(y + 1) * width..(y + 2) * width];
        let out_row = &mut out[y * width..(y + 1) * width];
        for x in 0..width {
            out_row[x] = previous[x] | current[x] | next[x];
        }
    }
    out
}

fn erode_one(source: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rows = vec![0u8; width * height];
    if width < 3 {
        return rows;
    }
    for y in 0..height {
        let row = &source[y * width..(y + 1) * width];
        let out = &mut rows[y * width..(y + 1) * width];
        for x in 1..width - 1 {
            out[x] = row[x - 1] & row[x] & row[x + 1];
        }
    }
    let mut out = vec![0u8; width * height];
    if height < 3 {
        return out;
    }
    for y in 1..height - 1 {
        let previous = &rows[(y - 1) * width..y * width];
        let current = &rows[y * width..(y + 1) * width];
        let next = &rows[(y + 1) * width..(y + 2) * width];
        let out_row = &mut out[y * width..(y + 1) * width];
        for x in 1..width - 1 {
            out_row[x] = previous[x] & current[x] & next[x];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(source: &[u8], width: usize, height: usize, radius: usize, grow: bool) -> Vec<u8> {
        let mut out = vec![0u8; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut value = !grow;
                for dy in -(radius as isize)..=radius as isize {
                    for dx in -(radius as isize)..=radius as isize {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        let inside =
                            nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < height;
                        if !inside {
                            if !grow {
                                value = false;
                            }
                            continue;
                        }
                        let sample = source[ny as usize * width + nx as usize] != 0;
                        value = if grow {
                            value || sample
                        } else {
                            value && sample
                        };
                    }
                }
                out[y * width + x] = u8::from(value);
            }
        }
        out
    }

    #[test]
    fn the_fast_and_window_paths_agree() {
        let (width, height) = (7usize, 5usize);
        let mut source = vec![0u8; width * height];
        source[2 * width + 3] = 1;
        source[3 * width + 4] = 1;
        source[width + 1] = 1;
        for radius in 1..=3 {
            assert_eq!(
                dilate_binary(&source, width, height, radius),
                naive(&source, width, height, radius, true),
                "dilate radius {radius}"
            );
            assert_eq!(
                erode_binary(&source, width, height, radius),
                naive(&source, width, height, radius, false),
                "erode radius {radius}"
            );
        }
    }
}
