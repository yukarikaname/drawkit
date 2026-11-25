//! PNG-sequence animation interchange.
//!
//! The native `.dkp` keeps the tape per frame. This directory format is the lossy bridge to
//! animation software that does not read DrawKit: a `animation.json` manifest and one PNG per
//! frame. Layers are flattened on export and become one paint layer per frame on import.

use std::path::Path;

use aseprite::{AsepriteFile, ColorMode, Pixels};
use serde_json::json;

use super::*;

const MANIFEST: &str = "animation.json";

impl Editor {
    /// Write every frame as `frame-0001.png` plus `animation.json`.
    pub fn export_animation(&self, directory: &Path) -> bool {
        let width = self.document.width();
        let height = self.document.height();
        if width == 0 || height == 0 || std::fs::create_dir_all(directory).is_err() {
            return false;
        }
        let mut frames = Vec::new();
        for index in 0..self.frame_count() {
            let Some((pixels, _, _)) = self.frame_rgba(index) else {
                return false;
            };
            let name = format!("frame-{:04}.png", index + 1);
            let path = directory.join(&name);
            if !write_png(&path, width, height, &pixels) {
                return false;
            }
            frames.push(json!({
                "file": name,
                "duration_ms": 42,
            }));
        }
        let manifest = json!({
            "format": "drawkit-animation",
            "version": 1,
            "width": width,
            "height": height,
            "fps": 24,
            "loop": true,
            "activeFrame": self.active_frame(),
            "frames": frames,
        });
        std::fs::write(
            directory.join(MANIFEST),
            serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
        )
        .is_ok()
    }

    /// Write every frame as a separate Aseprite layer. The open format is the second animation
    /// bridge; DrawKit's layer tree is flattened into one cel per frame.
    pub fn export_aseprite(&self, path: &Path) -> bool {
        let width = self.document.width();
        let height = self.document.height();
        if width == 0 || height == 0 || width > u16::MAX as usize || height > u16::MAX as usize {
            return false;
        }
        let mut file = AsepriteFile::new(width as u16, height as u16, ColorMode::Rgba);
        for index in 0..self.frame_count() {
            let Some((pixels, _, _)) = self.frame_rgba(index) else {
                return false;
            };
            let layer = file.add_layer(&format!("Frame {}", index + 1));
            let frame = file.add_frame(42);
            let mut bytes = Vec::with_capacity(width * height * 4);
            for pixel in &pixels {
                let alpha = pixel[3].clamp(0.0, 1.0);
                bytes.push((pixel[0].clamp(0.0, 1.0) * 255.0).round() as u8);
                bytes.push((pixel[1].clamp(0.0, 1.0) * 255.0).round() as u8);
                bytes.push((pixel[2].clamp(0.0, 1.0) * 255.0).round() as u8);
                bytes.push((alpha * 255.0).round() as u8);
            }
            let Ok(pixels) = Pixels::new(bytes, width as u16, height as u16, ColorMode::Rgba)
            else {
                return false;
            };
            if file.set_cel(layer, frame, pixels, 0, 0).is_err() {
                return false;
            }
        }
        let Ok(mut output) = std::fs::File::create(path) else {
            return false;
        };
        file.write_to(&mut output).is_ok()
    }

    /// Read a directory written by `export_animation`. Each PNG becomes a one-layer frame.
    pub fn open_animation(directory: &Path) -> Option<Editor> {
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join(MANIFEST)).ok()?).ok()?;
        let width = manifest["width"].as_u64()? as usize;
        let height = manifest["height"].as_u64()? as usize;
        let entries = manifest["frames"].as_array()?;
        if width == 0 || height == 0 || entries.is_empty() {
            return None;
        }
        let limit = 256 << 20;
        let mut editor = Editor::new(width, height, limit);
        let mut frames = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let name = entry["file"].as_str()?;
            let imported = drawkit_formats::read_image(&directory.join(name)).ok()?;
            if imported.width != width || imported.height != height {
                return None;
            }
            let mut document = Document::new(width, height);
            document.color_space = crate::color_space::ColorSpace::from_i32(imported.color_space);
            let source = imported.layers.first()?;
            let layer = &mut document.layers[0];
            layer.name = source.name.clone();
            for y in 0..height {
                for x in 0..width {
                    let base = (y * source.width + x) * 4;
                    if base + 3 >= source.rgba.len() {
                        continue;
                    }
                    let alpha = source.rgba[base + 3] as f32 / 255.0;
                    if alpha <= 0.0 {
                        continue;
                    }
                    layer.surface.set(
                        x,
                        y,
                        [
                            source.rgba[base] as f32 / 255.0,
                            source.rgba[base + 1] as f32 / 255.0,
                            source.rgba[base + 2] as f32 / 255.0,
                            alpha,
                        ],
                    );
                }
            }
            layer.surface.mark(Rect::new(0, 0, width, height));
            if index == 0 {
                editor.document = document;
            } else {
                frames.push(Some(frames::Frame {
                    document,
                    history: History::new(limit),
                    recording: Recording::default(),
                    time_ms: 0,
                }));
            }
            if index == 0 {
                frames.insert(0, None);
            }
        }
        editor.frames = frames;
        editor.active_frame = 0;
        editor.clear_history();
        editor.recording = Recording::default();
        editor.recording_enabled = true;
        Some(editor)
    }
}

fn write_png(path: &Path, width: usize, height: usize, pixels: &[Rgba]) -> bool {
    let Ok(file) = std::fs::File::create(path) else {
        return false;
    };
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let Ok(mut writer) = encoder.write_header() else {
        return false;
    };
    let mut bytes = Vec::with_capacity(width * height * 4);
    for pixel in pixels {
        let alpha = pixel[3].clamp(0.0, 1.0);
        bytes.push((pixel[0].clamp(0.0, 1.0) * 255.0).round() as u8);
        bytes.push((pixel[1].clamp(0.0, 1.0) * 255.0).round() as u8);
        bytes.push((pixel[2].clamp(0.0, 1.0) * 255.0).round() as u8);
        bytes.push((alpha * 255.0).round() as u8);
    }
    writer.write_image_data(&bytes).is_ok()
}
