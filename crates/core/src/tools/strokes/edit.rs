//! Recolouring and deleting one recorded stroke.
//!
//! The tape is the source of truth, so an edit is a re-render, not a pixel overlay: the
//! anti-aliased edge of the old colour has to go too. Rebuilding the whole layer is correct but
//! throws away every other stroke's pixels; the writer map remembers which tiles each group
//! wrote, so only the tiles that stroke can reach are rebuilt.
//!
//! A tile can hold several groups. If one of them is re-rendered, every group painted on that
//! tile has to be replayed with it, or the result would be a blend of old and new pixels. Starting
//! from the edited group's tiles and closing over every writer that shares a tile gives exactly
//! the set to rebuild. Erase groups are writers too, so a tile the eraser crossed is replayed from
//! its recorded segments. Any pixel in that set the tape cannot explain (an imported raster, a
//! fill, a direct erase with no group) makes the replay differ from the layer and the edit is
//! refused, as before.

use std::collections::{HashMap, HashSet};

use super::super::*;
use crate::record::TapeEdit;
use crate::surface::{Surface, TILE};

/// A tile coordinate as the writer map reports it: column then row, in `TILE`-pixel units.
type Tile = (u32, u32);

impl Editor {
    /// Replace the ink of one recorded stroke. The stroke is re-rendered from the tape, so its
    /// anti-aliased edge takes the new colour with it instead of leaving a fringe.
    pub fn recolor_stroke(&mut self, group: u64, color: Rgba) -> bool {
        let Some(before) = self.recording.group_color(group) else {
            return false;
        };
        if before == color {
            return true;
        }
        self.apply_stroke_edit(
            group,
            TapeEdit::Recolor {
                group,
                before,
                after: color,
            },
        )
    }

    /// Remove one recorded stroke and every segment it owns.
    pub fn delete_stroke(&mut self, group: u64) -> bool {
        let events = self.recording.group_events(group);
        if events.is_empty() {
            return false;
        }
        self.apply_stroke_edit(group, TapeEdit::Removed { group, events })
    }

    /// The layer a group was painted on.
    fn group_layer(&self, group: u64) -> Option<usize> {
        self.recording.events().iter().find_map(|event| {
            (event.group == Some(group))
                .then(|| command_layer(event.command))
                .flatten()
        })
    }

    /// Re-render the tiles the edited group can affect, then swap the edit into the tape. A layer
    /// with pixels the tape did not paint is refused: rebuilding its tiles from strokes would
    /// erase them.
    fn apply_stroke_edit(&mut self, group: u64, edit: TapeEdit) -> bool {
        let Some(layer) = self.group_layer(group) else {
            return false;
        };
        let Some((tiles, groups)) = self.edit_region(layer, group) else {
            return false;
        };
        let (width, height) = (self.document.width(), self.document.height());
        let Some(bounds) = tile_bounds(&tiles, width, height) else {
            return false;
        };
        let alpha_locked = self.document.layers[layer].alpha_locked;
        // Replay only the groups that wrote these tiles. The closure includes every group sharing
        // a tile, so this is the same picture a whole-layer replay would produce there.
        let Some(original) = self.tape_region(layer, None, alpha_locked, Some(&groups)) else {
            return false;
        };
        let current = self.document.layers[layer].surface.snapshot(bounds);
        // Each affected tile is rebuilt whole, so every pixel in it must come from the tape. An
        // imported raster or a fill shows up as a mismatch and keeps the old refusal.
        for &tile in &tiles {
            let Some(rect) = tile_rect(tile, width, height) else {
                continue;
            };
            let tape = original.snapshot(rect);
            let ink = self.document.layers[layer].surface.snapshot(rect);
            if !similar(&tape, &ink) {
                // A 4K document refused edits the same tape accepted at 2K, and the refusal says
                // nothing about which pixel stopped it. `DRAWKIT_EDIT_TRACE=1` names the tile and
                // the first pixel that differs, which is how a read-back rounding difference
                // gets told apart from a tile that holds paint the tape cannot explain.
                if std::env::var("DRAWKIT_EDIT_TRACE").is_ok() {
                    let first = tape.iter().zip(&ink).position(|(a, b)| a != b).unwrap_or(0);
                    eprintln!(
                        "DRAWKIT_EDIT mismatch tile {tile:?} rect {rect:?} at {first}: tape {:?} ink {:?}",
                        tape.get(first),
                        ink.get(first)
                    );
                }
                return false;
            }
        }
        // The edited replay is a second full scratch surface; do not hold both at once.
        drop(original);
        let Some(edited) = self.tape_region(layer, Some(&edit), alpha_locked, Some(&groups)) else {
            return false;
        };
        self.recording.apply_edit(&edit, true);
        // A removed group has no events left to replay, so its writer entry would only send a
        // later edit's closure through tiles nothing can explain. The pixels are already gone.
        if let TapeEdit::Removed { group, .. } = &edit {
            self.document.layers[layer].surface.forget_writer(*group);
        }
        if let Some(target) = self.document.layers.get_mut(layer) {
            for &tile in &tiles {
                let Some(rect) = tile_rect(tile, width, height) else {
                    continue;
                };
                // Writing the whole tile replaces it: the old stroke is cleared and the replay
                // does not blend over pixels that are still there outside the edited group.
                let pixels = edited.snapshot(rect);
                target.surface.write_pixels(rect, &pixels);
            }
        }
        let after = self.document.layers[layer].surface.snapshot(bounds);
        // A stroke edit invalidates the tile journal and the older pixel steps: their rectangles
        // no longer describe this layer, and the GPU journal cannot keep earlier steps through a
        // re-render. The edit becomes one undoable step on a new baseline, with the tape change
        // attached.
        self.history.clear();
        self.layer_undo.clear();
        self.layer_redo.clear();
        let serial = self.next_serial();
        self.history
            .push_tape(serial, layer, bounds, current, after, edit);
        true
    }

    /// The tiles a stroke edit has to rebuild, and the groups to replay into them. Seeds with the
    /// edited group's tiles, then keeps adding every group that shares a tile with the set: a
    /// later stroke over the edited one has to be re-blended, and an earlier one has to be laid
    /// down again under it.
    fn edit_region(&self, layer: usize, group: u64) -> Option<(HashSet<Tile>, HashSet<u64>)> {
        let surface = &self.document.layers.get(layer)?.surface;
        let seed = surface.tiles_for_group(group)?;
        let mut tiles: HashSet<Tile> = seed.iter().copied().collect();
        let mut groups: HashSet<u64> = HashSet::from([group]);
        let mut by_tile: HashMap<Tile, Vec<u64>> = HashMap::new();
        for (writer, written) in surface.writer_groups() {
            for &tile in written {
                by_tile.entry(tile).or_default().push(writer);
            }
        }
        let mut frontier: Vec<Tile> = tiles.iter().copied().collect();
        while let Some(tile) = frontier.pop() {
            let Some(writers) = by_tile.get(&tile) else {
                continue;
            };
            for &writer in writers {
                if !groups.insert(writer) {
                    continue;
                }
                let Some(written) = surface.tiles_for_group(writer) else {
                    continue;
                };
                for &next in written {
                    if tiles.insert(next) {
                        frontier.push(next);
                    }
                }
            }
        }
        Some((tiles, groups))
    }

    /// Replay one layer from the tape into a scratch surface. Only brush events are reproduced;
    /// fill, smudge and clear are context-dependent and make the layer ineligible. `groups` limits
    /// the replay to the groups a tile edit needs; `None` replays the whole layer.
    fn tape_region(
        &self,
        layer: usize,
        edit: Option<&TapeEdit>,
        alpha_locked: bool,
        groups: Option<&HashSet<u64>>,
    ) -> Option<Surface> {
        let (width, height) = (self.document.width(), self.document.height());
        let mut scratch = Surface::new(width, height);
        let mut phase = 0.0;
        let mut current_group = u64::MAX;
        for event in self.recording.events() {
            if command_layer(event.command) != Some(layer) {
                continue;
            }
            let wanted = match groups {
                Some(groups) => event.group.is_some_and(|group| groups.contains(&group)),
                None => true,
            };
            if !wanted || edit.is_some_and(|edit| edit.removes(event.group)) {
                continue;
            }
            let mut command = event.command;
            match &mut command {
                Command::Stroke { color, .. } => {
                    if let Some(TapeEdit::Recolor { group, after, .. }) = edit
                        && event.group == Some(*group)
                    {
                        *color = *after;
                    }
                }
                Command::Erase { .. } => {}
                _ => return None,
            }
            if event.group != Some(current_group) {
                phase = 0.0;
                current_group = event.group.unwrap_or(u64::MAX);
            }
            if !self.stamp(&mut scratch, command, alpha_locked, &mut phase) {
                return None;
            }
        }
        Some(scratch)
    }

    fn stamp(
        &self,
        surface: &mut Surface,
        command: Command,
        alpha_locked: bool,
        phase: &mut f32,
    ) -> bool {
        match command {
            Command::Stroke {
                from,
                to,
                radius,
                pressure,
                color,
                brush: kind,
                seed,
                tilt,
                azimuth,
                roll,
                ..
            } => {
                let pen = self
                    .brushes
                    .get(kind as usize)
                    .copied()
                    .unwrap_or_else(|| crate::brush::Brush::for_kind(kind))
                    .with_input(tilt, azimuth, roll);
                let painted = pen.radius_at(radius, pressure);
                let ink = [
                    color[0],
                    color[1],
                    color[2],
                    color[3] * pen.alpha_at(pressure),
                ];
                let rect = self.bounds(from, to, painted);
                if alpha_locked {
                    crate::brush::stroke_locked_with_phase(
                        surface,
                        (from, to),
                        painted,
                        &pen,
                        ink,
                        seed,
                        phase,
                    );
                } else {
                    crate::brush::stroke_with_phase(
                        surface,
                        (from, to),
                        painted,
                        &pen,
                        ink,
                        seed,
                        phase,
                    );
                }
                surface.mark(rect);
                true
            }
            Command::Erase {
                from,
                to,
                radius,
                pressure,
                brush: kind,
                seed,
                tilt,
                azimuth,
                roll,
                strength,
                ..
            } => {
                let pen = self
                    .brushes
                    .get(kind as usize)
                    .copied()
                    .unwrap_or_else(|| crate::brush::Brush::for_kind(kind))
                    .with_input(tilt, azimuth, roll);
                let painted = pen.radius_at(radius, pressure);
                let rect = self.bounds(from, to, painted);
                // The strength is part of the event: replay removes what the live segment did,
                // not a full erase.
                crate::brush::erase_with_phase(
                    surface,
                    (from, to),
                    painted,
                    &pen,
                    seed,
                    phase,
                    strength,
                );
                surface.mark(rect);
                true
            }
            _ => false,
        }
    }
}

fn command_layer(command: Command) -> Option<usize> {
    match command {
        Command::Stroke { layer, .. }
        | Command::Erase { layer, .. }
        | Command::Fill { layer, .. }
        | Command::Smudge { layer, .. }
        | Command::Clear { layer, .. } => Some(layer),
    }
}

fn tile_rect((tx, ty): Tile, width: usize, height: usize) -> Option<Rect> {
    let x = tx as usize * TILE;
    let y = ty as usize * TILE;
    if x >= width || y >= height {
        return None;
    }
    Some(Rect::new(x, y, TILE.min(width - x), TILE.min(height - y)))
}

/// The bounding rectangle of a tile set, for one history snapshot. The tiles may be scattered,
/// but the step stores one rectangle; pixels outside the set are unchanged by the edit.
fn tile_bounds(tiles: &HashSet<Tile>, width: usize, height: usize) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for &tile in tiles {
        let Some(rect) = tile_rect(tile, width, height) else {
            continue;
        };
        bounds = Some(bounds.map_or(rect, |current| current.union(&rect)));
    }
    bounds
}

/// Whether the layer region is explained by the tape. Partial-coverage pixels are skipped: the
/// GPU stamp and the CPU replay differ on the edge by design, and an imported base shows up in the
/// flat interior or in the transparent part of the rectangle instead.
fn similar(tape: &[Rgba], current: &[Rgba]) -> bool {
    mismatch(tape, current).is_none()
}

/// The first pixel the comparison rejects, for the trace: `similar` is the same question with no
/// interest in where the answer came from.
fn mismatch(tape: &[Rgba], current: &[Rgba]) -> Option<(usize, Rgba, Rgba)> {
    if tape.len() != current.len() {
        return Some((0, [0.0; 4], [0.0; 4]));
    }
    tape.iter().zip(current).enumerate().find_map(|(at, (a, b))| {
        if !differs(a, b) {
            return None;
        }
        Some((at, *a, *b))
    })
}

/// Whether two pixels are far enough apart to call the layer unexplainable. A partly covered pixel
/// is compared through its visible contribution only, and its straight colour is left alone: it is
/// a blend of paint and paper, and a re-render never lands on exactly the same number there.
fn differs(a: &Rgba, b: &Rgba) -> bool {
    if a[3] > 0.02 && a[3] < 0.98 {
        return false;
    }
    // Straight RGB under zero alpha is leftover from the tile read-back and says nothing about
    // what the layer shows.
    let left = [a[0] * a[3], a[1] * a[3], a[2] * a[3], a[3]];
    let right = [b[0] * b[3], b[1] * b[3], b[2] * b[3], b[3]];
    (0..4).any(|channel| (left[channel] - right[channel]).abs() > 0.05)
}

#[cfg(test)]
mod tests;
