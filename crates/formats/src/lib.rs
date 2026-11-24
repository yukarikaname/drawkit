//! Krita and Photoshop documents, read into layers of straight RGBA8.
//!
//! Both formats are containers for layers this editor does not have (groups, filters, vector
//! layers, masks). The reader keeps what the painting core can paint: raster layers, their names,
//! visibility, opacity and offset. Everything else is flattened away or reported as unsupported.
//!
//! Krita's tiles are planar B, G, R, A, optionally LZF-compressed. Photoshop's layers are
//! interleaved RGBA and already uncompressed by `ag-psd`. The same crate carries the stroke tape
//! container (`.dkp`), which is the lossless source the raster formats are exported from.

use std::fmt;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

mod dkp;
mod exif;
pub mod exr;
mod kra;
mod palette;
mod profile;
mod psd;
mod raster;

pub use dkp::{
    BrushScale, TapeCommand, TapeDocument, TapeEvent, TapeLayer, TapeOutput, read_dkp,
    read_dkp_bytes, write_dkp,
};
pub use kra::{read_kra, read_kra_bytes, write_kra};
pub use palette::{PaletteEntry, PaletteFormat, format_for_path, read_palette, write_palette};
pub use profile::detect as detect_color_space;
pub use psd::{read_psd, read_psd_bytes, write_psd};
pub use raster::{RasterFormat, write_raster, write_raster_by_extension};

/// A format error with a sentence a person can read.
#[derive(Debug)]
pub struct Error(String);

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Read a PNG or JPEG as one raster layer. Used when a drawing is opened as an image rather than
/// as a document.
pub fn read_image(path: &Path) -> Result<ImportedDocument, Error> {
    let bytes =
        std::fs::read(path).map_err(|e| error(format!("reading {}: {e}", path.display())))?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|e| error(format!("decoding {}: {e}", path.display())))?;
    let rgba = decoded.to_rgba8();
    let (source_width, source_height) = rgba.dimensions();
    let (width, height, pixels) = exif::apply(
        source_width as usize,
        source_height as usize,
        &rgba.into_raw(),
        exif::orientation(&bytes),
    );
    Ok(ImportedDocument {
        width,
        height,
        color_space: profile::detect_bytes(&bytes),
        layers: vec![ImportedLayer {
            name: "Image".into(),
            visible: true,
            opacity: 1.0,
            x: 0,
            y: 0,
            width,
            height,
            rgba: pixels,
        }],
    })
}

pub(crate) fn error(message: impl Into<String>) -> Error {
    Error::new(message)
}

#[derive(Clone, Debug)]
pub struct ImportedLayer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ImportedDocument {
    pub width: usize,
    pub height: usize,
    /// Working colour space, using `drawkit_core::document::ColorSpace::as_i32` numbering.
    pub color_space: i32,
    /// Bottom layer first.
    pub layers: Vec<ImportedLayer>,
}

/// One layer to write. Straight RGBA8, document-sized; bottom-up in `ExportDocument::layers`.
#[derive(Clone, Debug)]
pub struct ExportLayer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ExportDocument {
    pub width: usize,
    pub height: usize,
    pub layers: Vec<ExportLayer>,
}

fn zip_entry(zip: &mut ZipArchive<Cursor<&[u8]>>, names: &[String]) -> Option<Vec<u8>> {
    for name in names {
        if let Ok(mut entry) = zip.by_name(name) {
            let mut bytes = Vec::new();
            if entry.read_to_end(&mut bytes).is_ok() {
                return Some(bytes);
            }
        }
    }
    None
}

fn decode_png(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>), Error> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| error(format!("reading the merged PNG: {e}")))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|e| error(format!("decoding the merged PNG: {e}")))?;
    let width = info.width as usize;
    let height = info.height as usize;
    let channels = info.color_type.samples();
    let mut rgba = vec![0_u8; width * height * 4];
    for (source, target) in buffer
        .chunks_exact(channels)
        .zip(rgba.as_chunks_mut::<4>().0)
    {
        match channels {
            1 => target.copy_from_slice(&[source[0], source[0], source[0], 255]),
            2 => target.copy_from_slice(&[source[0], source[0], source[0], source[1]]),
            3 => target.copy_from_slice(&[source[0], source[1], source[2], 255]),
            _ => target.copy_from_slice(&source[..4]),
        }
    }
    Ok((width, height, rgba))
}

fn number<T: std::str::FromStr>(value: &str) -> Option<T> {
    value.trim().parse().ok()
}

#[cfg(test)]
mod tests;
