//! Picking a recorded stroke. The tape groups the segments of one pointer stroke, so a group is
//! already an object; this is the hit test that turns a click into one.

use super::*;
use crate::record::TapeEdit;
use crate::surface::Surface;

impl Editor {
    /// The top-most stroke whose dab covers `point`, in document pixels. Erase strokes are not
    /// returned: they have no colour and an eraser gesture is not a paintable object.
    pub fn pick_stroke(&self, point: (f32, f32)) -> Option<u64> {
        for event in self.recording.events().iter().rev() {
            let Some(group) = event.group else { continue };
            let Command::Stroke {
                layer,
                from,
                to,
                radius,
                pressure,
                brush,
                ..
            } = event.command
            else {
                continue;
            };
            let Some(target) = self.document.layers.get(layer) else {
                continue;
            };
            if target.kind != LayerKind::Paint || !target.visible || target.locked {
                continue;
            }
            let pen = self
                .brushes
                .get(brush as usize)
                .copied()
                .unwrap_or_else(|| Brush::for_kind(brush));
            let painted = pen.radius_at(radius, pressure);
            if distance_to_segment(point, from, to) <= painted {
                return Some(group);
            }
        }
        None
    }

    /// The document rectangle a stroke's dabs can reach, including jitter and the paper margin.
    pub fn stroke_bounds(&self, group: u64) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        for event in self.recording.events() {
            if event.group != Some(group) {
                continue;
            }
            let Command::Stroke {
                from,
                to,
                radius,
                pressure,
                brush,
                ..
            } = event.command
            else {
                continue;
            };
            let pen = self
                .brushes
                .get(brush as usize)
                .copied()
                .unwrap_or_else(|| Brush::for_kind(brush));
            let painted = pen.radius_at(radius, pressure);
            let rect = self.bounds(from, to, painted);
            bounds = Some(bounds.map_or(rect, |current| current.union(&rect)));
        }
        bounds
    }
}

fn distance_to_segment(point: (f32, f32), from: (f32, f32), to: (f32, f32)) -> f32 {
    let segment = (to.0 - from.0, to.1 - from.1);
    let length = segment.0 * segment.0 + segment.1 * segment.1;
    if length <= f32::EPSILON {
        return distance(point, from);
    }
    let along = ((point.0 - from.0) * segment.0 + (point.1 - from.1) * segment.1) / length;
    let t = along.clamp(0.0, 1.0);
    distance(point, (from.0 + segment.0 * t, from.1 + segment.1 * t))
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

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

    /// Verify that the tape explains the pixels the stroke touches, then re-render the edit into
    /// the layer. A layer with an imported base or a fill/smudge fails the check, so rebuilding it
    /// from strokes can never erase pixels it did not paint.
    fn apply_stroke_edit(&mut self, group: u64, edit: TapeEdit) -> bool {
        let Some(layer) = self.group_layer(group) else {
            return false;
        };
        let Some(bounds) = self.stroke_bounds(group) else {
            return false;
        };
        let Some(current) = self
            .document
            .layers
            .get(layer)
            .map(|target| target.surface.snapshot(bounds))
        else {
            return false;
        };
        let alpha_locked = self.document.layers[layer].alpha_locked;
        let Some(original) = self.tape_region(layer, None, alpha_locked) else {
            return false;
        };
        if !similar(&original.snapshot(bounds), &current) {
            return false;
        }
        let Some(edited) = self.tape_region(layer, Some(&edit), alpha_locked) else {
            return false;
        };
        let after = edited.snapshot(bounds);
        self.recording.apply_edit(&edit, true);
        if let Some(target) = self.document.layers.get_mut(layer) {
            target.surface.write_pixels(bounds, &after);
        }
        // A stroke edit invalidates the tile journal and the older pixel steps: their rectangles
        // no longer describe this layer. The edit itself becomes one undoable step on a new
        // baseline, with the tape change attached.
        self.history.clear();
        self.layer_undo.clear();
        self.layer_redo.clear();
        let serial = self.next_serial();
        self.history
            .push_tape(serial, layer, bounds, current, after, edit);
        true
    }

    /// Replay one layer from the tape into a scratch surface. Only brush events are reproduced;
    /// fill, smudge and clear are context-dependent and make the layer ineligible.
    fn tape_region(
        &self,
        layer: usize,
        edit: Option<&TapeEdit>,
        alpha_locked: bool,
    ) -> Option<Surface> {
        let (width, height) = (self.document.width(), self.document.height());
        let mut scratch = Surface::new(width, height);
        let mut phase = 0.0;
        let mut current_group = u64::MAX;
        for event in self.recording.events() {
            if edit.is_some_and(|edit| edit.removes(event.group)) {
                continue;
            }
            if command_layer(event.command) != Some(layer) {
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
                crate::brush::erase_with_phase(surface, (from, to), painted, &pen, seed, phase);
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

/// Whether the layer region is explained by the tape. Partial-coverage pixels are skipped: the
/// GPU stamp and the CPU replay differ on the edge by design, and an imported base shows up in the
/// flat interior or in the transparent part of the rectangle instead.
fn similar(tape: &[Rgba], current: &[Rgba]) -> bool {
    if tape.len() != current.len() {
        return false;
    }
    tape.iter().zip(current).all(|(a, b)| {
        if a[3] > 0.02 && a[3] < 0.98 {
            return true;
        }
        // Compare the visible contribution. Straight RGB under zero alpha is leftover from the
        // tile read-back and says nothing about what the layer shows.
        let left = [a[0] * a[3], a[1] * a[3], a[2] * a[3], a[3]];
        let right = [b[0] * b[3], b[1] * b[3], b[2] * b[3], b[3]];
        (0..4).all(|channel| (left[channel] - right[channel]).abs() <= 0.05)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted_editor() -> Editor {
        let mut editor = Editor::new(64, 64, 1 << 20);
        editor.recording_enabled = true;
        editor.begin_stroke(0);
        editor.stroke_at(
            (10.0, 10.0),
            (40.0, 10.0),
            4.0,
            1.0,
            [1.0, 0.0, 0.0, 1.0],
            0,
        );
        editor.end_stroke();
        editor
    }

    #[test]
    fn a_stroke_is_picked_where_it_was_painted() {
        let editor = painted_editor();
        let group = editor
            .pick_stroke((25.0, 12.0))
            .expect("the stroke covers the point");
        assert!(editor.pick_stroke((25.0, 40.0)).is_none());
        let bounds = editor.stroke_bounds(group).expect("a stroke has bounds");
        assert!(bounds.x <= 6, "bounds start too late: {bounds:?}");
        assert!(
            bounds.x + bounds.width >= 44,
            "bounds end too early: {bounds:?}"
        );
    }

    #[test]
    fn a_stroke_is_recoloured_through_the_tape() {
        let mut editor = painted_editor();
        let group = editor.pick_stroke((25.0, 10.0)).expect("a stroke");
        let bounds = editor.stroke_bounds(group).expect("bounds");
        let mut edge = None;
        'scan: for y in bounds.y..bounds.y + bounds.height {
            for x in bounds.x..bounds.x + bounds.width {
                let pixel = editor.document.layers[0].surface.get(x, y);
                if pixel[3] > 0.1 && pixel[3] < 0.9 {
                    edge = Some((x, y, pixel[3]));
                    break 'scan;
                }
            }
        }
        let edge = edge.expect("the stroke has an anti-aliased edge");
        assert!(editor.recolor_stroke(group, [0.0, 0.0, 1.0, 1.0]));
        let pixel = editor.document.layers[0].surface.get(25, 10);
        assert!(pixel[2] > 0.9 && pixel[0] < 0.1, "pixel {pixel:?}");
        assert_eq!(
            editor.recording.group_color(group),
            Some([0.0, 0.0, 1.0, 1.0])
        );
        let softened = editor.document.layers[0].surface.get(edge.0, edge.1);
        assert!(
            (softened[3] - edge.2).abs() < 0.01,
            "the edge alpha changed: {} -> {}",
            edge.2,
            softened[3]
        );
        assert!(
            softened[2] > 0.9 && softened[0] < 0.1,
            "the edge kept the old colour: {softened:?}"
        );

        assert!(editor.undo().is_some());
        let pixel = editor.document.layers[0].surface.get(25, 10);
        assert!(pixel[0] > 0.9 && pixel[2] < 0.1, "undo pixel {pixel:?}");
        assert_eq!(
            editor.recording.group_color(group),
            Some([1.0, 0.0, 0.0, 1.0]),
            "undo left the tape recoloured"
        );

        assert!(editor.redo().is_some());
        assert_eq!(
            editor.recording.group_color(group),
            Some([0.0, 0.0, 1.0, 1.0])
        );
        let softened = editor.document.layers[0].surface.get(edge.0, edge.1);
        assert!(
            (softened[3] - edge.2).abs() < 0.01,
            "the edge alpha changed: {} -> {}",
            edge.2,
            softened[3]
        );
        assert!(
            softened[2] > 0.9 && softened[0] < 0.1,
            "the edge kept the old colour: {softened:?}"
        );
    }

    #[test]
    fn a_stroke_is_deleted_and_undo_restores_it() {
        let mut editor = painted_editor();
        let group = editor.pick_stroke((25.0, 10.0)).expect("a stroke");
        assert!(editor.delete_stroke(group));
        assert!(editor.recording.group_events(group).is_empty());
        assert_eq!(editor.document.layers[0].surface.get(25, 10), [0.0; 4]);

        assert!(editor.undo().is_some());
        assert!(
            !editor.recording.group_events(group).is_empty(),
            "tape not restored"
        );
        let pixel = editor.document.layers[0].surface.get(25, 10);
        assert!(pixel[0] > 0.9, "pixels not restored: {pixel:?}");

        assert!(editor.redo().is_some());
        assert!(editor.recording.group_events(group).is_empty());
    }

    #[test]
    fn a_layer_with_pixels_the_tape_did_not_paint_is_refused() {
        let mut editor = painted_editor();
        let group = editor.pick_stroke((25.0, 10.0)).expect("a stroke");
        editor.document.layers[0]
            .surface
            .set(25, 10, [0.0, 1.0, 0.0, 1.0]);
        assert!(!editor.recolor_stroke(group, [0.0, 0.0, 1.0, 1.0]));
        assert_eq!(
            editor.recording.group_color(group),
            Some([1.0, 0.0, 0.0, 1.0]),
            "a refused edit changed the tape"
        );
    }

    #[test]
    fn an_erase_stroke_is_not_pickable() {
        let mut editor = Editor::new(64, 64, 1 << 20);
        editor.recording_enabled = true;
        editor.begin_stroke(0);
        editor.erase_at((10.0, 10.0), (40.0, 10.0), 4.0, 1.0, 0);
        editor.end_stroke();
        assert!(editor.pick_stroke((25.0, 10.0)).is_none());
    }
}
