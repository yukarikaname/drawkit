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
    /// Replace the last finished stroke with the ideal line, ellipse or rectangle the tiny model
    /// recognises. A freehand gesture, a low-confidence guess or an eraser group is left alone.
    pub fn auto_shape(&mut self) -> bool {
        if !self.auto_shape && !self.quick_shape {
            return false;
        }
        let events = self.recording.events();
        let Some(group) = events.last().and_then(|event| event.group) else {
            return false;
        };
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
                brush,
                tilt,
                azimuth,
                roll,
                ..
            } = event.command
            else {
                return false;
            };
            if points.last() != Some(&from) {
                points.push(from);
            }
            points.push(to);
            let _ = brush;
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
        let Some(style) = style else {
            return false;
        };
        let guess = shape::classify(&points);
        if guess.kind == ShapeKind::Free || guess.confidence < 0.80 {
            return false;
        }
        let ideal = shape::ideal_points(guess.kind, &points);
        if ideal.len() < 2 || self.undo().is_none() {
            return false;
        }
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
        true
    }
}
