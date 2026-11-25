//! The `.dkp` stroke tape: the lossless source a painting is exported from.
//!
//! A tape is a ZIP with a small manifest and one binary stream of events. Pixel formats (KRA, PSD,
//! PNG) are derivatives of this file; this one keeps the samples, pressure, tilt, seed and brush
//! radius that the pixels were made from.

mod events;

use events::{decode_events, encode_events};
use std::io::{Cursor, Read, Write};
use std::path::Path;

use serde_json::json;
use zip::{ZipArchive, ZipWriter};

use crate::{Error, error};

const MIMETYPE: &str = "application/x-drawkit-strokes";
const MAGIC: &[u8; 4] = b"DKP0";
const MAGIC_V2: &[u8; 4] = b"DKP1";
const MAGIC_V3: &[u8; 4] = b"DKP2";
const MAX_EVENTS: usize = 16_000_000;

mod scale;
pub use scale::{BrushScale, TapeOutput};

#[derive(Clone, Debug)]
pub struct TapeDocument {
    pub width: usize,
    pub height: usize,
    pub layers: usize,
    pub color_space: i32,
    pub dynamic_range: i32,
    /// Layer tree metadata. Empty in version 1 and 2 tapes.
    pub layer_info: Vec<TapeLayer>,
    /// The layer the palette had selected when the project was saved.
    pub active: Option<u32>,
    pub events: Vec<TapeEvent>,
    /// Animation frames as complete nested tapes. A single-frame project leaves this empty.
    pub frames: Vec<TapeDocument>,
    /// Index into `frames`; the top-level tape is the active frame for older readers.
    pub active_frame: usize,
    /// Playback rate for the animation timeline.
    pub fps: u16,
    /// Per-frame hold in milliseconds. Empty means every frame uses `1000 / fps`.
    pub frame_durations: Vec<u32>,
    /// Whether playback wraps at the end of the loop range.
    pub loop_enabled: bool,
    /// First frame of the loop range.
    pub loop_start: usize,
    /// One past the last frame, or 0 for the end of the timeline.
    pub loop_end: usize,
    /// The nine editable brush slots. Empty means the built-in presets.
    pub brushes: Vec<TapeBrush>,
    /// One small grayscale paper/tip texture, row-major.
    pub textures: Vec<TapeTexture>,
}

#[derive(Clone, Debug)]
pub struct TapeTexture {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct TapeBrush {
    pub kind: u8,
    pub hardness: f32,
    pub flow: f32,
    pub pressure_radius: f32,
    pub pressure_alpha: f32,
    pub spacing: f32,
    pub jitter: f32,
    pub angle: f32,
    pub roundness: f32,
    pub grain: f32,
    pub paper: f32,
    pub texture: f32,
    pub wet: f32,
    pub dual: f32,
    pub dual_scale: f32,
    pub dual_kind: u8,
}

/// One row of the layer tree as it was saved. The stroke events say what was painted; this says
/// how the palette was arranged around it.
#[derive(Clone, Debug)]
pub struct TapeLayer {
    pub name: String,
    /// 0 paint, 1 group, 2 fill.
    pub kind: u8,
    pub parent: Option<u32>,
    pub visible: bool,
    pub opacity: f32,
    pub blend: u8,
    pub locked: bool,
    pub alpha_locked: bool,
    pub clipping: bool,
    pub expanded: bool,
    pub reference: bool,
    pub color_tag: u8,
    pub fill: [f32; 4],
    /// PNG bytes for a layer that is not reproducible from strokes (an import or a bake).
    pub raster: Option<Vec<u8>>,
    /// Where the cached PNG sits on the canvas. `None` means the whole canvas, which is how
    /// caches written before this field existed are read.
    pub raster_rect: Option<[u32; 4]>,
}

#[derive(Clone, Debug)]
pub struct TapeEvent {
    pub time_ms: u64,
    pub group: Option<u64>,
    pub command: TapeCommand,
}

#[derive(Clone, Debug)]
pub enum TapeCommand {
    Stroke {
        layer: u32,
        from: [f32; 2],
        to: [f32; 2],
        radius: f32,
        pressure: f32,
        color: [f32; 4],
        brush: u8,
        seed: u64,
        tilt: [f32; 2],
        azimuth: f32,
        roll: f32,
        scale: u8,
    },
    Erase {
        layer: u32,
        from: [f32; 2],
        to: [f32; 2],
        radius: f32,
        pressure: f32,
        brush: u8,
        seed: u64,
        tilt: [f32; 2],
        azimuth: f32,
        roll: f32,
        scale: u8,
    },
    Fill {
        layer: u32,
        x: u32,
        y: u32,
        tolerance: f32,
        color: [f32; 4],
        close: u32,
        expand: u32,
        feather: u32,
    },
    Smudge {
        layer: u32,
        from: [f32; 2],
        to: [f32; 2],
        radius: f32,
        pressure: f32,
        strength: f32,
        brush: u8,
    },
    Clear {
        layer: u32,
        color: [f32; 4],
    },
}

/// Write a tape to memory. The manifest and event stream are deflated; a layer cache is already a
/// compressed PNG, so it is stored as-is.
pub fn write_dkp(document: &TapeDocument) -> Result<Vec<u8>, Error> {
    if document.width == 0
        || document.height == 0
        || document.width > 32_768
        || document.height > 32_768
    {
        return Err(error("the tape canvas size is out of range"));
    }
    let layer_info = document
        .layer_info
        .iter()
        .map(|layer| {
            json!({
                "name": layer.name,
                "kind": layer.kind,
                "parent": layer.parent.map_or(-1, |parent| parent as i32),
                "visible": layer.visible,
                "opacity": layer.opacity,
                "blend": layer.blend,
                "locked": layer.locked,
                "alphaLocked": layer.alpha_locked,
                "clipping": layer.clipping,
                "expanded": layer.expanded,
                "reference": layer.reference,
                "colorTag": layer.color_tag,
                "fill": layer.fill,
                "raster": layer.raster.is_some(),
                "rasterRect": layer.raster_rect.map(|rect| rect.to_vec()),
            })
        })
        .collect::<Vec<_>>();
    let manifest = json!({
        "format": "drawkit-strokes",
        "version": 5,
        "width": document.width,
        "height": document.height,
        "layers": document.layers.max(1),
        "colorSpace": document.color_space,
        "dynamicRange": document.dynamic_range,
        "layerInfo": layer_info,
        "active": document.active.map_or(-1, |active| active as i32),
        "events": document.events.len(),
        "eventEncoding": "DKP1",
        "frames": document.frames.len(),
        "activeFrame": document.active_frame,
        "fps": document.fps,
        "frameDurations": document.frame_durations,
        "loopEnabled": document.loop_enabled,
        "loopStart": document.loop_start,
        "loopEnd": document.loop_end,
        "textures": document.textures.iter().map(|texture| json!({
            "width": texture.width,
            "height": texture.height,
            "data": texture.data,
        })).collect::<Vec<_>>(),
        "brushes": document.brushes.iter().map(|brush| json!({
            "kind": brush.kind,
            "hardness": brush.hardness,
            "flow": brush.flow,
            "pressureRadius": brush.pressure_radius,
            "pressureAlpha": brush.pressure_alpha,
            "spacing": brush.spacing,
            "jitter": brush.jitter,
            "angle": brush.angle,
            "roundness": brush.roundness,
            "grain": brush.grain,
            "paper": brush.paper,
            "texture": brush.texture,
            "wet": brush.wet,
            "dual": brush.dual,
            "dualScale": brush.dual_scale,
            "dualKind": brush.dual_kind,
        })).collect::<Vec<_>>(),
    });
    let manifest = serde_json::to_vec(&manifest).map_err(|e| error(e.to_string()))?;
    let strokes = encode_events(document)?;

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mimetype", stored)
        .map_err(|e| error(e.to_string()))?;
    zip.write_all(MIMETYPE.as_bytes())
        .map_err(|e| error(e.to_string()))?;
    zip.start_file("manifest.json", deflated)
        .map_err(|e| error(e.to_string()))?;
    zip.write_all(&manifest).map_err(|e| error(e.to_string()))?;
    // The event stream is regular floats and small integers, so deflate cuts it roughly in half;
    // a finished 4K painting carries a few megabytes of tape.
    zip.start_file("strokes.bin", deflated)
        .map_err(|e| error(e.to_string()))?;
    zip.write_all(&strokes).map_err(|e| error(e.to_string()))?;
    for (index, frame) in document.frames.iter().enumerate() {
        let bytes = write_dkp(frame)?;
        zip.start_file(format!("frame-{index}.dkp"), stored)
            .map_err(|e| error(e.to_string()))?;
        zip.write_all(&bytes).map_err(|e| error(e.to_string()))?;
    }
    for (index, layer) in document.layer_info.iter().enumerate() {
        if let Some(raster) = &layer.raster {
            // PNG already contains a deflate stream; compressing it again costs save time and
            // saves almost nothing.
            zip.start_file(format!("layer-{index}.png"), stored)
                .map_err(|e| error(e.to_string()))?;
            zip.write_all(raster).map_err(|e| error(e.to_string()))?;
        }
    }
    Ok(zip.finish().map_err(|e| error(e.to_string()))?.into_inner())
}

pub fn read_dkp(path: &Path) -> Result<TapeDocument, Error> {
    let bytes =
        std::fs::read(path).map_err(|e| error(format!("reading {}: {e}", path.display())))?;
    read_dkp_bytes(&bytes)
}

pub fn read_dkp_bytes(bytes: &[u8]) -> Result<TapeDocument, Error> {
    let mut zip =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| error(format!("opening the tape: {e}")))?;
    let mut manifest = String::new();
    zip.by_name("manifest.json")
        .map_err(|e| error(format!("the tape has no manifest: {e}")))?
        .read_to_string(&mut manifest)
        .map_err(|e| error(e.to_string()))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .map_err(|e| error(format!("the manifest is not JSON: {e}")))?;
    let width = manifest["width"]
        .as_u64()
        .ok_or_else(|| error("the manifest has no width"))? as usize;
    let height = manifest["height"]
        .as_u64()
        .ok_or_else(|| error("the manifest has no height"))? as usize;
    let layers = manifest["layers"].as_u64().unwrap_or(1) as usize;
    let color_space = manifest["colorSpace"].as_i64().unwrap_or(0) as i32;
    let dynamic_range = manifest["dynamicRange"].as_i64().unwrap_or(0) as i32;
    let layer_info: Vec<TapeLayer> = manifest["layerInfo"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| TapeLayer {
                    name: row["name"].as_str().unwrap_or("Layer").to_string(),
                    kind: row["kind"].as_u64().unwrap_or(0) as u8,
                    parent: row["parent"]
                        .as_i64()
                        .filter(|parent| *parent >= 0)
                        .map(|parent| parent as u32),
                    visible: row["visible"].as_bool().unwrap_or(true),
                    opacity: row["opacity"].as_f64().unwrap_or(1.0) as f32,
                    blend: row["blend"].as_u64().unwrap_or(0) as u8,
                    locked: row["locked"].as_bool().unwrap_or(false),
                    alpha_locked: row["alphaLocked"].as_bool().unwrap_or(false),
                    clipping: row["clipping"].as_bool().unwrap_or(false),
                    expanded: row["expanded"].as_bool().unwrap_or(true),
                    reference: row["reference"].as_bool().unwrap_or(false),
                    color_tag: row["colorTag"].as_u64().unwrap_or(0) as u8,
                    fill: row["fill"]
                        .as_array()
                        .map(|fill| {
                            let mut channels = [0.0; 4];
                            for (slot, value) in channels.iter_mut().zip(fill) {
                                *slot = value.as_f64().unwrap_or(0.0) as f32;
                            }
                            channels
                        })
                        .unwrap_or([0.0; 4]),
                    raster: None,
                    raster_rect: row["rasterRect"].as_array().map(|rect| {
                        let mut values = [0_u32; 4];
                        for (slot, value) in values.iter_mut().zip(rect) {
                            *slot = value.as_u64().unwrap_or(0) as u32;
                        }
                        values
                    }),
                })
                .collect()
        })
        .unwrap_or_default();
    if width == 0 || height == 0 || width > 32_768 || height > 32_768 {
        return Err(error("the tape canvas size is out of range"));
    }
    let mut data = Vec::new();
    zip.by_name("strokes.bin")
        .map_err(|e| error(format!("the tape has no stroke stream: {e}")))?
        .read_to_end(&mut data)
        .map_err(|e| error(e.to_string()))?;
    let version = manifest["version"].as_u64().unwrap_or(1);
    let events = decode_events(&data, version)?;
    let mut layer_info = layer_info;
    for (index, layer) in layer_info.iter_mut().enumerate() {
        let name = format!("layer-{index}.png");
        if let Ok(mut entry) = zip.by_name(&name) {
            let mut bytes = Vec::new();
            if entry.read_to_end(&mut bytes).is_ok() {
                layer.raster = Some(bytes);
            }
        }
    }
    let frame_count = manifest["frames"].as_u64().unwrap_or(0) as usize;
    let active_frame = manifest["activeFrame"].as_u64().unwrap_or(0) as usize;
    let fps = manifest["fps"].as_u64().unwrap_or(12) as u16;
    let frame_durations = manifest["frameDurations"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as u32)
                .collect()
        })
        .unwrap_or_default();
    let loop_enabled = manifest["loopEnabled"].as_bool().unwrap_or(true);
    let loop_start = manifest["loopStart"].as_u64().unwrap_or(0) as usize;
    let loop_end = manifest["loopEnd"].as_u64().unwrap_or(0) as usize;
    let textures = manifest["textures"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let width = value["width"].as_u64()? as usize;
                    let height = value["height"].as_u64()? as usize;
                    let data = value["data"]
                        .as_array()?
                        .iter()
                        .map(|byte| byte.as_u64().unwrap_or(0) as u8)
                        .collect();
                    Some(TapeTexture {
                        width,
                        height,
                        data,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let brushes = manifest["brushes"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    let number = |key: &str, fallback: f32| {
                        value[key].as_f64().map_or(fallback, |number| number as f32)
                    };
                    TapeBrush {
                        kind: value["kind"].as_u64().unwrap_or(0) as u8,
                        hardness: number("hardness", 0.0),
                        flow: number("flow", 1.0),
                        pressure_radius: number("pressureRadius", 0.25),
                        pressure_alpha: number("pressureAlpha", 0.0),
                        spacing: number("spacing", 0.25),
                        jitter: number("jitter", 0.0),
                        angle: number("angle", 0.0),
                        roundness: number("roundness", 1.0),
                        grain: number("grain", 0.0),
                        paper: number("paper", 0.0),
                        texture: number("texture", 0.0),
                        wet: number("wet", 0.0),
                        dual: number("dual", 0.0),
                        dual_scale: number("dualScale", 0.7),
                        dual_kind: value["dualKind"].as_u64().unwrap_or(0) as u8,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let mut frames = Vec::with_capacity(frame_count);
    for index in 0..frame_count {
        let name = format!("frame-{index}.dkp");
        if let Ok(mut entry) = zip.by_name(&name) {
            let mut bytes = Vec::new();
            if entry.read_to_end(&mut bytes).is_ok()
                && let Ok(frame) = read_dkp_bytes(&bytes)
            {
                frames.push(frame);
            }
        }
    }
    Ok(TapeDocument {
        width,
        height,
        layers: layers.max(1),
        color_space,
        dynamic_range,
        layer_info,
        active: manifest["active"]
            .as_i64()
            .filter(|active| *active >= 0)
            .map(|active| active as u32),
        events,
        frames,
        active_frame,
        fps,
        frame_durations,
        loop_enabled,
        loop_start,
        loop_end,
        brushes,
        textures,
    })
}
