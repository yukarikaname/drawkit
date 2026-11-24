//! Replaying a stroke tape at another output scale.
//!
//! This is the smallest useful piece of resolution independence: the same recorded samples are
//! painted into a new document at `scale`, with a choice of whether the brush radius follows the
//! canvas or stays at its recorded output size. The brush engine is still the current round brush;
//! the geometry, pressure and seed are the tape's.
//!
//! Fills and clears replay too, because they are part of the painting the tape recorded.

use crate::brush::{self, Brush, BrushKind};
use crate::color::{Rgba, over};
use crate::document::{Layer, LayerKind};
use crate::fill::{FillOptions, close_ink, edge_coverage, erode, feather, flood, grow};
use crate::surface::Rect;
use crate::tools::{Editor, PAPER};
use drawkit_formats::{BrushScale, TapeCommand, TapeDocument, TapeOutput};

/// Paint a tape into a fresh editor. The result has one layer; history and recording are empty.
pub fn replay_tape(tape: &TapeDocument, output: TapeOutput) -> Editor {
    replay_tape_with(tape, output, false)
}

/// `use_rasters` is for opening a project: a cached layer already contains its final pixels, so its
/// old events must not be applied again. Replaying a tape at another scale keeps the strokes.
pub fn replay_tape_with(tape: &TapeDocument, output: TapeOutput, use_rasters: bool) -> Editor {
    let scale = output.scale.clamp(0.01, 16.0);
    let width = ((tape.width as f32 * scale).round() as usize).max(1);
    let height = ((tape.height as f32 * scale).round() as usize).max(1);
    let mut editor = Editor::new(width, height, 1);
    editor.recording_enabled = false;
    editor.document.color_space = crate::document::ColorSpace::from_i32(tape.color_space);
    editor.document.dynamic_range = crate::document::DynamicRange::from_i32(tape.dynamic_range);
    let mut cached = std::collections::HashSet::new();
    let raster_scale = (scale - 1.0).abs() < 1e-4;
    if tape.layer_info.is_empty() {
        while editor.document.layers.len() < tape.layers.max(1) {
            let name = format!("Layer {}", editor.document.layers.len() + 1);
            editor.document.add_layer(name);
        }
    } else {
        let mut layers = Vec::with_capacity(tape.layer_info.len());
        for (index, info) in tape.layer_info.iter().enumerate() {
            let mut layer = match info.kind {
                1 => Layer::group(&info.name, width, height),
                2 => Layer::fill(&info.name, info.fill, width, height),
                _ => Layer::new(&info.name, width, height),
            };
            if raster_scale && let Some(raster) = &info.raster {
                let rect = info
                    .raster_rect
                    .map_or(Rect::new(0, 0, width, height), |r| {
                        Rect::new(r[0] as usize, r[1] as usize, r[2] as usize, r[3] as usize)
                    });
                if let Ok(surface) = crate::layer_png::decode_layer(raster, width, height, rect) {
                    layer.surface = surface;
                    if use_rasters {
                        cached.insert(index);
                    }
                }
            }
            layer.visible = info.visible;
            layer.opacity = info.opacity.clamp(0.0, 1.0);
            layer.blend = crate::document::BlendMode::from_i32(info.blend as i32);
            layer.locked = info.locked;
            layer.alpha_locked = info.alpha_locked;
            layer.color_tag = info.color_tag;
            layer.clipping = info.clipping;
            layer.parent = info.parent.map(|parent| parent as usize);
            layer.expanded = info.expanded;
            layer.fill = info.fill;
            layers.push(layer);
        }
        if layers.is_empty() {
            layers.push(Layer::new("Layer 1", width, height));
        }
        if !layers.iter().any(|layer| layer.kind == LayerKind::Paint) {
            layers.push(Layer::new("Layer 1", width, height));
        }
        editor.document.layers = layers;
        editor.document.active = tape
            .active
            .map(|active| (active as usize).min(editor.document.layers.len() - 1))
            .unwrap_or(editor.document.layers.len() - 1);
    }

    for event in &tape.events {
        let event_layer = match &event.command {
            TapeCommand::Stroke { layer, .. }
            | TapeCommand::Erase { layer, .. }
            | TapeCommand::Fill { layer, .. }
            | TapeCommand::Smudge { layer, .. }
            | TapeCommand::Clear { layer, .. } => *layer as usize,
        };
        if cached.contains(&event_layer) {
            continue;
        }
        match &event.command {
            TapeCommand::Stroke {
                layer,
                from,
                to,
                radius,
                pressure,
                color,
                brush,
                ..
            } => {
                let radius = match output.brush_scale {
                    BrushScale::Canvas => radius * scale,
                    BrushScale::Screen => *radius,
                };
                let pen = Brush::for_kind(BrushKind::from_i32(*brush as i32));
                let painted = pen.radius_at(radius, *pressure);
                let ink = [
                    color[0],
                    color[1],
                    color[2],
                    color[3] * pen.alpha_at(*pressure),
                ];
                paint_segment(
                    &mut editor,
                    *layer as usize,
                    (*from, *to),
                    painted,
                    Some(ink),
                    pen.hardness,
                    scale,
                );
            }
            TapeCommand::Erase {
                layer,
                from,
                to,
                radius,
                pressure,
                brush,
                ..
            } => {
                let radius = match output.brush_scale {
                    BrushScale::Canvas => radius * scale,
                    BrushScale::Screen => *radius,
                };
                let pen = Brush::for_kind(BrushKind::from_i32(*brush as i32));
                let painted = pen.radius_at(radius, *pressure);
                paint_segment(
                    &mut editor,
                    *layer as usize,
                    (*from, *to),
                    painted,
                    None,
                    pen.hardness,
                    scale,
                );
            }
            TapeCommand::Fill {
                layer,
                x,
                y,
                tolerance,
                color,
                close,
                expand,
                feather,
            } => {
                replay_fill(
                    &mut editor,
                    *layer as usize,
                    (*x as f32 * scale).round() as usize,
                    (*y as f32 * scale).round() as usize,
                    *tolerance,
                    *color,
                    FillOptions {
                        close: *close as usize,
                        expand: *expand as usize,
                        feather: *feather as usize,
                    },
                );
            }
            TapeCommand::Smudge {
                layer,
                from,
                to,
                radius,
                pressure,
                strength,
                brush,
            } => {
                let radius = match output.brush_scale {
                    BrushScale::Canvas => radius * scale,
                    BrushScale::Screen => *radius,
                };
                let pen = Brush::for_kind(BrushKind::from_i32(*brush as i32));
                let painted = pen.radius_at(radius, *pressure);
                smudge_segment(
                    &mut editor,
                    *layer as usize,
                    (*from, *to),
                    painted,
                    pen.hardness,
                    *strength,
                    scale,
                );
            }
            TapeCommand::Clear { layer, color } => {
                let (width, height) = (editor.document.width(), editor.document.height());
                let layer = (*layer as usize).min(editor.document.layers.len() - 1);
                let surface = &mut editor.document.layers[layer].surface;
                for y in 0..height {
                    for x in 0..width {
                        surface.set(x, y, *color);
                    }
                }
                surface.mark(Rect::new(0, 0, width, height));
            }
        }
    }
    editor
}

fn paint_segment(
    editor: &mut Editor,
    layer: usize,
    points: ([f32; 2], [f32; 2]),
    painted: f32,
    color: Option<Rgba>,
    hardness: f32,
    scale: f32,
) {
    let (from, to) = points;
    let from = (from[0] * scale, from[1] * scale);
    let to = (to[0] * scale, to[1] * scale);
    let margin = painted.ceil() as usize + 2;
    let width = editor.document.width();
    let height = editor.document.height();
    let left = from.0.min(to.0).floor().max(0.0) as usize;
    let top = from.1.min(to.1).floor().max(0.0) as usize;
    let right = (from.0.max(to.0).ceil().max(0.0) as usize + margin).min(width);
    let bottom = (from.1.max(to.1).ceil().max(0.0) as usize + margin).min(height);
    let x = left.saturating_sub(margin);
    let y = top.saturating_sub(margin);
    let rect = Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y));
    let layer = layer.min(editor.document.layers.len() - 1);
    let surface = &mut editor.document.layers[layer].surface;
    match color {
        Some(color) => brush::stroke(surface, from, to, painted, hardness, color),
        None => brush::erase(surface, from, to, painted, hardness),
    }
    surface.mark(rect);
}

/// The replay twin of a live smudge: same geometry and strength, no history or tape.
fn smudge_segment(
    editor: &mut Editor,
    layer: usize,
    points: ([f32; 2], [f32; 2]),
    painted: f32,
    hardness: f32,
    strength: f32,
    scale: f32,
) {
    let (from, to) = points;
    let from = (from[0] * scale, from[1] * scale);
    let to = (to[0] * scale, to[1] * scale);
    let margin = painted.ceil() as usize + 2;
    let width = editor.document.width();
    let height = editor.document.height();
    let left = from.0.min(to.0).floor().max(0.0) as usize;
    let top = from.1.min(to.1).floor().max(0.0) as usize;
    let right = (from.0.max(to.0).ceil().max(0.0) as usize + margin).min(width);
    let bottom = (from.1.max(to.1).ceil().max(0.0) as usize + margin).min(height);
    let x = left.saturating_sub(margin);
    let y = top.saturating_sub(margin);
    let rect = Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y));
    let layer = layer.min(editor.document.layers.len().saturating_sub(1));
    let surface = &mut editor.document.layers[layer].surface;
    brush::smudge(surface, from, to, painted, hardness, strength);
    surface.mark(rect);
}

fn replay_fill(
    editor: &mut Editor,
    layer: usize,
    x: usize,
    y: usize,
    tolerance: f32,
    color: Rgba,
    options: FillOptions,
) {
    let options = options.clamped();
    let (width, height) = (editor.document.width(), editor.document.height());
    let rect = Rect::new(0, 0, width, height);
    let composite: Vec<Rgba> = editor
        .document
        .composite_rect(rect)
        .into_iter()
        .map(|pixel| over(pixel, PAPER))
        .collect();
    let sealed = close_ink(&composite, width, height, PAPER, options.close);
    let mask = flood(&sealed, width, height, (x, y), tolerance);
    let grown = grow(&mask, options.expand);
    let core = erode(&mask, 1);
    let soft = edge_coverage(&composite, &grown, &core, PAPER);
    let soft = feather(&soft, options.feather);
    let layer = layer.min(editor.document.layers.len() - 1);
    let surface = &mut editor.document.layers[layer].surface;
    surface.blend_mask(&soft, color);
    surface.mark(rect);
}

#[cfg(test)]
mod tests;
