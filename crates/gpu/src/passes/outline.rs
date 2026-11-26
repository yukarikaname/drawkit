/// Darken the silhouette of an RGBA8 frame. This runs after readback, so the CPU and GPU share the
/// same one-pixel boundary rule instead of duplicating it in WGSL.
pub(crate) fn darken(pixels: &mut [u8], width: usize, height: usize, radius: u8) {
    let alpha: Vec<u8> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3])
        .collect();
    let radius = radius as isize;
    let mut line = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if alpha[y * width + x] < 128 {
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
                        || alpha[ny as usize * width + nx as usize] < 128
                    {
                        edge = true;
                        break;
                    }
                }
                if edge {
                    break;
                }
            }
            if edge {
                line.push(y * width + x);
            }
        }
    }
    for index in line {
        pixels[index * 4..index * 4 + 4].copy_from_slice(&[20, 20, 26, 255]);
    }
}
