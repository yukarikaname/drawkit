//! Which working space an image is in.
//!
//! A P3 or Adobe RGB file opened as sRGB looks washed out. The pixels themselves do not say which
//! space they are in; the ICC profile does. This reads the profile from a PNG `iCCP` chunk or a
//! JPEG `APP2` segment and matches its RGB primaries against the three spaces the core knows.

use std::path::Path;

/// The values match `drawkit_core::document::ColorSpace::as_i32`.
pub const SRGB: i32 = 0;
pub const DISPLAY_P3: i32 = 1;
pub const ADOBE_RGB: i32 = 2;

/// sRGB, Display P3 and Adobe RGB primaries as CIE xy.
const SRGB_GAMUT: [(f32, f32); 3] = [(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)];
const P3_GAMUT: [(f32, f32); 3] = [(0.680, 0.320), (0.265, 0.690), (0.150, 0.060)];
const ADOBE_GAMUT: [(f32, f32); 3] = [(0.64, 0.33), (0.21, 0.71), (0.15, 0.06)];

/// The colour space of an image file, or sRGB when there is no usable profile.
pub fn detect(path: &Path) -> i32 {
    let Ok(bytes) = std::fs::read(path) else {
        return SRGB;
    };
    detect_bytes(&bytes)
}

pub fn detect_bytes(bytes: &[u8]) -> i32 {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png_profile(bytes);
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return jpeg_profile(bytes);
    }
    SRGB
}

fn png_profile(bytes: &[u8]) -> i32 {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let Ok(reader) = decoder.read_info() else {
        return SRGB;
    };
    let Some(profile) = reader.info().icc_profile.as_ref() else {
        return SRGB;
    };
    classify(profile)
}

/// Concatenate the ICC chunks of a JPEG's APP2 segments and classify the profile.
fn jpeg_profile(bytes: &[u8]) -> i32 {
    let mut profile = Vec::new();
    let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
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
        if marker == 0xE2 && payload.starts_with(b"ICC_PROFILE\0") && payload.len() > 14 {
            chunks.push((payload[12], payload[14..].to_vec()));
        }
        position += 2 + length;
    }
    if chunks.is_empty() {
        return SRGB;
    }
    chunks.sort_by_key(|(sequence, _)| *sequence);
    for (_, data) in chunks {
        profile.extend_from_slice(&data);
    }
    classify(&profile)
}

/// Match the profile's `rXYZ`/`gXYZ`/`bXYZ` tags to a known gamut.
pub(crate) fn classify(profile: &[u8]) -> i32 {
    let Some(gamut) = primaries(profile) else {
        return SRGB;
    };
    let spaces = [
        (SRGB_GAMUT, SRGB),
        (P3_GAMUT, DISPLAY_P3),
        (ADOBE_GAMUT, ADOBE_RGB),
    ];
    let mut best = (f32::MAX, SRGB);
    for (candidate, space) in spaces {
        let distance: f32 = candidate
            .iter()
            .zip(gamut.iter())
            .map(|(a, b)| {
                let dx = a.0 - b.0;
                let dy = a.1 - b.1;
                dx * dx + dy * dy
            })
            .sum();
        if distance < best.0 {
            best = (distance, space);
        }
    }
    // A profile with primaries unlike all three is not something we can honour; sRGB is the
    // documented fallback rather than guessing a wide space and shifting every colour.
    if best.0 < 0.006 { best.1 } else { SRGB }
}

/// The RGB chromaticities from an ICC profile, in CIE xy.
fn primaries(profile: &[u8]) -> Option<[(f32, f32); 3]> {
    if profile.len() < 132 {
        return None;
    }
    let count = u32::from_be_bytes(profile[128..132].try_into().ok()?) as usize;
    if count == 0 || count > 256 || 132 + count * 12 > profile.len() {
        return None;
    }
    let mut xyz = [None; 3];
    for index in 0..count {
        let entry = 132 + index * 12;
        let signature = &profile[entry..entry + 4];
        let offset = u32::from_be_bytes(profile[entry + 4..entry + 8].try_into().ok()?) as usize;
        let size = u32::from_be_bytes(profile[entry + 8..entry + 12].try_into().ok()?) as usize;
        if size < 20 || offset + size > profile.len() {
            continue;
        }
        let slot = match signature {
            b"rXYZ" => 0,
            b"gXYZ" => 1,
            b"bXYZ" => 2,
            _ => continue,
        };
        let data = &profile[offset..offset + size];
        if &data[..4] != b"XYZ " {
            continue;
        }
        let value = |at: usize| -> f32 {
            i32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as f32 / 65_536.0
        };
        xyz[slot] = Some([value(8), value(12), value(16)]);
    }
    let [Some(red), Some(green), Some(blue)] = xyz else {
        return None;
    };
    let chromaticity = |value: [f32; 3]| -> Option<(f32, f32)> {
        let sum = value[0] + value[1] + value[2];
        if sum.abs() < 1e-6 {
            return None;
        }
        Some((value[0] / sum, value[1] / sum))
    };
    Some([
        chromaticity(red)?,
        chromaticity(green)?,
        chromaticity(blue)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ICC profile with a tag table and three XYZ tags.
    fn profile(gamut: [(f32, f32); 3]) -> Vec<u8> {
        let mut header = vec![0u8; 128];
        header[..4].copy_from_slice(&((128 + 4 + 3 * 12 + 3 * 20) as u32).to_be_bytes());
        header[16..20].copy_from_slice(b"RGB ");
        let mut bytes = header;
        bytes.extend_from_slice(&3u32.to_be_bytes());
        for (index, name) in [b"rXYZ", b"gXYZ", b"bXYZ"].into_iter().enumerate() {
            let offset = 128 + 4 + 3 * 12 + index * 20;
            bytes.extend_from_slice(name);
            bytes.extend_from_slice(&(offset as u32).to_be_bytes());
            bytes.extend_from_slice(&20u32.to_be_bytes());
        }
        for (x, y) in gamut {
            bytes.extend_from_slice(b"XYZ ");
            bytes.extend_from_slice(&[0; 4]);
            let xyz = [x / y, 1.0, (1.0 - x - y) / y];
            for value in xyz {
                bytes.extend_from_slice(&((value * 65_536.0).round() as i32).to_be_bytes());
            }
        }
        bytes
    }

    #[test]
    fn the_three_gamuts_are_told_apart() {
        assert_eq!(classify(&profile(SRGB_GAMUT)), SRGB);
        assert_eq!(classify(&profile(P3_GAMUT)), DISPLAY_P3);
        assert_eq!(classify(&profile(ADOBE_GAMUT)), ADOBE_RGB);
    }

    #[test]
    fn junk_and_unknown_primaries_fall_back_to_srgb() {
        assert_eq!(classify(b"not a profile"), SRGB);
        let odd = [(0.80, 0.20), (0.10, 0.80), (0.05, 0.05)];
        assert_eq!(classify(&profile(odd)), SRGB);
    }

    #[test]
    fn a_jpeg_profile_is_found_before_the_scan_data() {
        let icc = profile(P3_GAMUT);
        let mut jpeg = vec![0xFF, 0xD8];
        jpeg.extend_from_slice(&[0xFF, 0xE2]);
        let length = (2 + 14 + icc.len()) as u16;
        jpeg.extend_from_slice(&length.to_be_bytes());
        jpeg.extend_from_slice(b"ICC_PROFILE\0");
        jpeg.extend_from_slice(&[1, 1]);
        jpeg.extend_from_slice(&icc);
        jpeg.extend_from_slice(&[0xFF, 0xDA]);
        assert_eq!(jpeg_profile(&jpeg), DISPLAY_P3);
    }
}
