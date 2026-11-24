//! EXIF orientation, so a phone photo opens the way it was taken.
//!
//! The pixels of a JPEG are usually stored in sensor order and the file carries an orientation
//! tag telling the viewer to rotate them. The decoder does not apply it, and a drawing app that
//! ignores it opens a portrait photo on its side.

/// The orientation of a JPEG or PNG (EXIF tag 0x0112), or 1 when there is none.
pub fn orientation(bytes: &[u8]) -> u8 {
    let tiff = if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg_exif(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png_exif(bytes)
    } else {
        None
    };
    tiff.and_then(parse_tiff).unwrap_or(1)
}

/// Apply an EXIF orientation to straight RGBA8. Returns the new size and pixels.
pub fn apply(width: usize, height: usize, rgba: &[u8], orientation: u8) -> (usize, usize, Vec<u8>) {
    let orientation = orientation.clamp(1, 8);
    if orientation == 1 || width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return (width, height, rgba.to_vec());
    }
    let (out_width, out_height) = if orientation >= 5 {
        (height, width)
    } else {
        (width, height)
    };
    let mut out = vec![0u8; out_width * out_height * 4];
    for y in 0..out_height {
        for x in 0..out_width {
            let (sx, sy) = source(x, y, width, height, orientation);
            let from = (sy * width + sx) * 4;
            let to = (y * out_width + x) * 4;
            out[to..to + 4].copy_from_slice(&rgba[from..from + 4]);
        }
    }
    (out_width, out_height, out)
}

/// Destination pixel to source pixel. The numbering is the EXIF one.
fn source(x: usize, y: usize, width: usize, height: usize, orientation: u8) -> (usize, usize) {
    match orientation {
        1 => (x, y),
        2 => (width - 1 - x, y),
        3 => (width - 1 - x, height - 1 - y),
        4 => (x, height - 1 - y),
        5 => (y, x),
        6 => (y, height - 1 - x),
        7 => (width - 1 - y, height - 1 - x),
        8 => (width - 1 - y, x),
        _ => (x, y),
    }
}

fn jpeg_exif(bytes: &[u8]) -> Option<&[u8]> {
    let mut position = 2;
    while position + 4 <= bytes.len() {
        if bytes[position] != 0xFF {
            position += 1;
            continue;
        }
        let marker = bytes[position + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            position += 2;
            continue;
        }
        let length = u16::from_be_bytes([bytes[position + 2], bytes[position + 3]]) as usize;
        if length < 2 || position + 2 + length > bytes.len() {
            break;
        }
        let payload = &bytes[position + 4..position + 2 + length];
        if marker == 0xE1 && payload.starts_with(b"Exif\0\0") {
            return Some(&payload[6..]);
        }
        position += 2 + length;
    }
    None
}

fn png_exif(bytes: &[u8]) -> Option<&[u8]> {
    let mut position = 8;
    while position + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[position..position + 4].try_into().ok()?) as usize;
        let kind = &bytes[position + 4..position + 8];
        let start = position + 8;
        let end = start.checked_add(length)?;
        if end + 4 > bytes.len() {
            return None;
        }
        if kind == b"eXIf" {
            return Some(&bytes[start..end]);
        }
        position = end + 4;
    }
    None
}

fn parse_tiff(tiff: &[u8]) -> Option<u8> {
    if tiff.len() < 8 {
        return None;
    }
    let little = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |offset: usize| -> Option<u16> {
        let bytes: [u8; 2] = tiff.get(offset..offset + 2)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    };
    let u32_at = |offset: usize| -> Option<u32> {
        let bytes: [u8; 4] = tiff.get(offset..offset + 4)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    };
    if u16_at(2)? != 42 {
        return None;
    }
    let directory = u32_at(4)? as usize;
    let count = u16_at(directory)? as usize;
    if count > 512 {
        return None;
    }
    for index in 0..count {
        let entry = directory + 2 + index * 12;
        let tag = u16_at(entry)?;
        if tag != 0x0112 {
            continue;
        }
        let kind = u16_at(entry + 2)?;
        let amount = u32_at(entry + 4)?;
        if kind != 3 || amount == 0 {
            return None;
        }
        return Some(u16_at(entry + 8)? as u8);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: usize, height: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for y in 0..height {
            for x in 0..width {
                out.extend_from_slice(&[(y * 10 + x) as u8, 0, 0, 255]);
            }
        }
        out
    }

    #[test]
    fn the_eight_orientations_move_the_pixels_where_exif_says() {
        let (width, height) = (2, 3);
        let source = image(width, height);
        let pixel = |data: &[u8], index: usize| data[index * 4];

        let (w, h, out) = apply(width, height, &source, 6);
        assert_eq!((w, h), (3, 2));
        // Rotating [[0,1],[10,11],[20,21]] clockwise gives [[20,10,0],[21,11,1]].
        assert_eq!(pixel(&out, 0), 20);
        assert_eq!(pixel(&out, 2), 0);
        assert_eq!(pixel(&out, 3), 21);

        let (_, _, out) = apply(width, height, &source, 8);
        // Counter-clockwise gives [[1,11,21],[0,10,20]].
        assert_eq!(pixel(&out, 0), 1);
        assert_eq!(pixel(&out, 3), 0);

        let (_, _, out) = apply(width, height, &source, 2);
        assert_eq!(pixel(&out, 0), 1);
        let (_, _, out) = apply(width, height, &source, 3);
        assert_eq!(pixel(&out, 0), 21);
        let (_, _, out) = apply(width, height, &source, 5);
        assert_eq!(pixel(&out, 0), 0);
        assert_eq!(pixel(&out, 1), 10);
    }

    fn tiff(orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff
    }

    #[test]
    fn a_jpeg_app1_segment_carries_the_orientation() {
        let tiff = tiff(6);
        let mut jpeg = vec![0xFF, 0xD8];
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        jpeg.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        jpeg.extend_from_slice(&[0xFF, 0xDA]);
        assert_eq!(orientation(&jpeg), 6);
    }

    #[test]
    fn opening_a_portrait_jpeg_gives_a_portrait_image() {
        use image::{ExtendedColorType, ImageEncoder};
        let mut jpeg = Vec::new();
        let rgb = vec![128u8; 3 * 2 * 3];
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .write_image(&rgb, 3, 2, ExtendedColorType::Rgb8)
            .unwrap();
        let tiff = tiff(6);
        let mut app1 = vec![0xFF, 0xE1];
        app1.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);
        jpeg.splice(2..2, app1);

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("drawkit-exif-{}-{stamp}.jpg", std::process::id()));
        std::fs::write(&path, &jpeg).unwrap();
        let document = crate::read_image(&path).expect("the jpeg opens");
        let _ = std::fs::remove_file(&path);
        assert_eq!((document.width, document.height), (2, 3));
    }

    #[test]
    fn no_tag_means_upright() {
        assert_eq!(orientation(b"not an image"), 1);
        assert_eq!(orientation(&[0xFF, 0xD8, 0xFF, 0xD9]), 1);
    }
}
