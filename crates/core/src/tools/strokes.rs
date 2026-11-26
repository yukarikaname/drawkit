//! Picking a recorded stroke. The tape groups the segments of one pointer stroke, so a group is
//! already an object; this is the hit test that turns a click into one.

use super::*;

mod edit;

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
    fn an_erase_stroke_is_not_pickable() {
        let mut editor = Editor::new(64, 64, 1 << 20);
        editor.recording_enabled = true;
        editor.begin_stroke(0);
        editor.erase_at((10.0, 10.0), (40.0, 10.0), 4.0, 1.0, 0);
        editor.end_stroke();
        assert!(editor.pick_stroke((25.0, 10.0)).is_none());
    }
}
