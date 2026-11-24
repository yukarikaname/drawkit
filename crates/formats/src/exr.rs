//! Linear OpenEXR output.
//!
//! An 8-bit PNG cannot hold a highlight above paper white, so an HDR painting leaves through EXR
//! instead. The values are premultiplied linear Display P3, the same numbers the extended-range
//! surface receives, and the file is tagged with the P3 primaries so another app reads them right.

use std::path::Path;

use exr::meta::attribute::Chromaticities;
use exr::prelude::*;

fn display_p3() -> Chromaticities {
    Chromaticities {
        red: Vec2(0.680, 0.320),
        green: Vec2(0.265, 0.690),
        blue: Vec2(0.150, 0.060),
        white: Vec2(0.3127, 0.3290),
    }
}

/// Write premultiplied linear RGBA as a 32-bit float EXR.
pub fn write(
    path: impl AsRef<Path>,
    width: usize,
    height: usize,
    rgba: &[[f32; 4]],
) -> std::result::Result<(), String> {
    if width == 0 || height == 0 || rgba.len() < width * height {
        return Err("empty image".to_string());
    }
    let channels = SpecificChannels::rgba(|Vec2(x, y)| {
        let pixel: [f32; 4] = rgba[y * width + x];
        (pixel[0], pixel[1], pixel[2], pixel[3])
    });
    let layer = Layer::new(
        (width, height),
        LayerAttributes::named("Canvas"),
        Encoding::SMALL_LOSSLESS,
        channels,
    );
    let mut image = Image::from_layer(layer);
    image.attributes.chromaticities = Some(display_p3());
    image
        .write()
        .to_file(path)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("drawkit-hdr-{}-{stamp}.exr", std::process::id()))
    }

    #[test]
    fn a_round_trip_keeps_highlights_above_one() {
        let mut pixels = vec![[0.0_f32; 4]; 4 * 4];
        pixels[5] = [4.0, 2.0, 0.5, 1.0];
        pixels[6] = [0.25, 0.5, 0.75, 0.5];
        let path = temp_path();
        write(&path, 4, 4, &pixels).unwrap();

        let image = read_first_rgba_layer_from_file(
            &path,
            |resolution, _| vec![[0.0_f32; 4]; resolution.width() * resolution.height()],
            |pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
                pixels[position.y() * 4 + position.x()] = [r, g, b, a];
            },
        )
        .unwrap();
        let _ = std::fs::remove_file(&path);

        let read = &image.layer_data.channel_data.pixels;
        assert!((read[5][0] - 4.0).abs() < 1e-4, "{:?}", read[5]);
        assert!((read[5][1] - 2.0).abs() < 1e-4);
        assert!((read[6][3] - 0.5).abs() < 1e-4);
    }
}
