use super::*;
use crate::record::Event;
use crate::shape::{self, ShapeKind};

struct StrokeStyle {
    radius: f32,
    pressure: f32,
    color: Rgba,
    tilt: [f32; 2],
    azimuth: f32,
    roll: f32,
    time: u64,
}

impl Editor {
    /// The ideal path the last stroke snaps to, without painting it.
    ///
    /// A resident-surface shell paints strokes on the GPU and has no CPU pixels to rewrite, so it
    /// takes this path and replays it through the same stroke the live preview used.
    pub fn shape_path(&self, snap: bool) -> Option<Vec<(f32, f32)>> {
        let (points, _) = self.last_stroke()?;
        let guess = shape::classify(&points);
        if guess.kind == ShapeKind::Free || guess.confidence < 0.80 {
            return None;
        }
        let ideal = shape::ideal_points(guess.kind, &points, snap);
        (ideal.len() >= 2).then_some(ideal)
    }

    /// Replace the last finished stroke with the ideal line, ellipse or rectangle the tiny model
    /// recognises. A freehand gesture, a low-confidence guess or an eraser group is left alone.
    pub fn auto_shape(&mut self, snap: bool) -> bool {
        if !self.auto_shape && !self.quick_shape {
            return false;
        }
        let Some(ideal) = self.shape_path(snap) else {
            return false;
        };
        let Some((_, style)) = self.last_stroke() else {
            return false;
        };
        if self.undo().is_none() {
            return false;
        }
        // The ideal path is already stable: stabilisation would pull the ends in and leave a
        // gap, and a deferred session would only record the shape instead of painting it.
        let deferred = self.defer_raster;
        let stabilization = self.stabilization;
        let dedupe = self.stroke_dedupe;
        let prediction = self.prediction_enabled;
        self.defer_raster = false;
        self.stabilization = 0.0;
        self.stroke_dedupe = false;
        self.prediction_enabled = false;
        self.begin_stroke(style.time + 1);
        let dynamics = Dynamics {
            pressure: style.pressure,
            tilt: style.tilt,
            azimuth: style.azimuth,
            roll: style.roll,
        };
        let mut last = ideal[0];
        for (step, point) in ideal.iter().enumerate().skip(1) {
            if *point == last {
                continue;
            }
            self.stroke_sample(
                last,
                *point,
                style.radius,
                dynamics,
                style.color,
                style.time + 1 + step as u64,
            );
            last = *point;
        }
        self.end_stroke();
        self.defer_raster = deferred;
        self.stabilization = stabilization;
        self.stroke_dedupe = dedupe;
        self.prediction_enabled = prediction;
        true
    }

    /// The points and the last sample's style of the newest stroke group.
    fn last_stroke(&self) -> Option<(Vec<(f32, f32)>, StrokeStyle)> {
        let events = self.recording.events();
        let group = events.last()?.group?;
        let mut points = Vec::new();
        let mut style: Option<StrokeStyle> = None;
        let mut group_events: Vec<&Event> = events
            .iter()
            .rev()
            .take_while(|event| event.group == Some(group))
            .collect();
        group_events.reverse();
        for event in group_events {
            let Command::Stroke {
                from,
                to,
                radius,
                pressure,
                color,
                tilt,
                azimuth,
                roll,
                ..
            } = event.command
            else {
                return None;
            };
            if points.last() != Some(&from) {
                points.push(from);
            }
            points.push(to);
            style = Some(StrokeStyle {
                radius,
                pressure,
                color,
                tilt,
                azimuth,
                roll,
                time: event.time_ms,
            });
        }
        Some((points, style?))
    }
}
