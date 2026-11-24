//! JPEG and WebP export of an opaque composite.
//!
//! The tape is the source, but a painter still wants a small file to send somewhere. Both encoders
//! take straight RGB8; the caller has already put the painting on paper.

use std::io::BufWriter;
use std::path::Path;

use image::{ExtendedColorType, ImageEncoder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterFormat {
    Jpeg,
    WebP,
}

/// Write `rgb` (three bytes per pixel) as a JPEG or lossless WebP.
pub fn write_raster(
    path: &Path,
    width: usize,
    height: usize,
    rgb: &[u8],
    format: RasterFormat,
    quality: u8,
) -> Result<(), String> {
    if width == 0 || height == 0 || rgb.len() < width * height * 3 {
        return Err("the image is empty".to_string());
    }
    let file = std::fs::File::create(path)
        .map_err(|error| format!("creating {}: {error}", path.display()))?;
    let writer = BufWriter::new(file);
    let encoded = match format {
        RasterFormat::Jpeg => {
            image::codecs::jpeg::JpegEncoder::new_with_quality(writer, quality.clamp(1, 100))
                .write_image(
                    &rgb[..width * height * 3],
                    width as u32,
                    height as u32,
                    ExtendedColorType::Rgb8,
                )
        }
        RasterFormat::WebP => image::codecs::webp::WebPEncoder::new_lossless(writer).write_image(
            &rgb[..width * height * 3],
            width as u32,
            height as u32,
            ExtendedColorType::Rgb8,
        ),
    };
    encoded.map_err(|error| format!("encoding {}: {error}", path.display()))
}

/// A small helper for a writer that only has a path: picks the format from the extension.
pub fn write_raster_by_extension(
    path: &Path,
    width: usize,
    height: usize,
    rgb: &[u8],
    quality: u8,
) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let format = match extension.as_str() {
        "webp" => RasterFormat::WebP,
        "jpg" | "jpeg" => RasterFormat::Jpeg,
        _ => return Err("export is neither JPEG nor WebP".to_string()),
    };
    write_raster(path, width, height, rgb, format, quality)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(extension: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "drawkit-raster-{}-{stamp}.{extension}",
            std::process::id()
        ))
    }

    fn pixels(width: usize, height: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                out.push((x * 40) as u8);
                out.push((y * 60) as u8);
                out.push(90);
            }
        }
        out
    }

    #[test]
    fn webp_round_trips_losslessly() {
        let (width, height) = (8, 6);
        let rgb = pixels(width, height);
        let path = temp("webp");
        write_raster(&path, width, height, &rgb, RasterFormat::WebP, 100).unwrap();
        let read = image::open(&path).unwrap().to_rgb8();
        assert_eq!((read.width(), read.height()), (width as u32, height as u32));
        assert_eq!(read.into_raw(), rgb);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn jpeg_writes_an_image_of_the_right_size() {
        let (width, height) = (16, 12);
        let rgb = pixels(width, height);
        let path = temp("jpg");
        write_raster(&path, width, height, &rgb, RasterFormat::Jpeg, 90).unwrap();
        let read = image::open(&path).unwrap().to_rgb8();
        assert_eq!((read.width(), read.height()), (width as u32, height as u32));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_wrong_size_is_rejected() {
        let path = temp("webp");
        assert!(write_raster(&path, 4, 4, &[0; 8], RasterFormat::WebP, 100).is_err());
        let _ = std::fs::remove_file(path);
    }
}
