//! The recording as a flat event stream a host renderer can replay.
//!
//! The core already replays its tape with the CPU brush. This surface lets a shell drive the same
//! events through its own GPU compositor: the fields are the ones a stamp needs, resolved from the
//! recorded `Command`. Events the GPU cannot reproduce yet (fill, smudge, clear) are reported with
//! their own kind so the shell can fall back to the core replay.

use std::os::raw::c_void;

use super::editor;
use crate::record::Command;

/// One recorded event. `kind` is 0 stroke, 1 erase, 2 fill, 3 smudge, 4 clear.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DkReplayEvent {
    pub time_ms: u64,
    /// `u64::MAX` when the segment is not part of a stroke group.
    pub group: u64,
    pub seed: u64,
    pub kind: u32,
    pub layer: u32,
    pub brush: u32,
    pub from_x: f32,
    pub from_y: f32,
    pub to_x: f32,
    pub to_y: f32,
    pub radius: f32,
    pub pressure: f32,
    pub color: [f32; 4],
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub roll: f32,
    pub azimuth: f32,
    pub tolerance: f32,
    pub strength: f32,
    /// Fill seed pixel; zero for brush events.
    pub x: u32,
    pub y: u32,
    pub close: u32,
    pub expand: u32,
    pub feather: u32,
}

impl DkReplayEvent {
    fn from_command(event: &crate::record::Event) -> Self {
        let mut out = Self {
            time_ms: event.time_ms,
            group: event.group.unwrap_or(u64::MAX),
            ..Default::default()
        };
        match event.command {
            Command::Stroke {
                layer,
                from,
                to,
                radius,
                pressure,
                color,
                brush,
                seed,
                tilt,
                azimuth,
                roll,
            } => {
                out.kind = 0;
                out.layer = layer as u32;
                out.brush = brush.as_i32() as u32;
                out.seed = seed;
                out.from_x = from.0;
                out.from_y = from.1;
                out.to_x = to.0;
                out.to_y = to.1;
                out.radius = radius;
                out.pressure = pressure;
                out.color = color;
                out.tilt_x = tilt[0];
                out.tilt_y = tilt[1];
                out.azimuth = azimuth;
                out.roll = roll;
            }
            Command::Erase {
                layer,
                from,
                to,
                radius,
                pressure,
                brush,
                seed,
                tilt,
                azimuth,
                roll,
            } => {
                out.kind = 1;
                out.layer = layer as u32;
                out.brush = brush.as_i32() as u32;
                out.seed = seed;
                out.from_x = from.0;
                out.from_y = from.1;
                out.to_x = to.0;
                out.to_y = to.1;
                out.radius = radius;
                out.pressure = pressure;
                out.tilt_x = tilt[0];
                out.tilt_y = tilt[1];
                out.azimuth = azimuth;
                out.roll = roll;
            }
            Command::Fill {
                layer,
                x,
                y,
                tolerance,
                color,
                close,
                expand,
                feather,
            } => {
                out.kind = 2;
                out.layer = layer as u32;
                out.x = x as u32;
                out.y = y as u32;
                out.tolerance = tolerance;
                out.color = color;
                out.close = close as u32;
                out.expand = expand as u32;
                out.feather = feather as u32;
            }
            Command::Smudge {
                layer,
                from,
                to,
                radius,
                pressure,
                strength,
                brush,
            } => {
                out.kind = 3;
                out.layer = layer as u32;
                out.brush = brush.as_i32() as u32;
                out.from_x = from.0;
                out.from_y = from.1;
                out.to_x = to.0;
                out.to_y = to.1;
                out.radius = radius;
                out.pressure = pressure;
                out.strength = strength;
            }
            Command::Clear { layer, color } => {
                out.kind = 4;
                out.layer = layer as u32;
                out.color = color;
            }
        }
        out
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_replay_event_count(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.recording.events().len() as u32)
}

/// Write event `index` to `out`. Returns 0 for a null pointer or an index past the tape.
///
/// # Safety
/// `out` must point at one writable `DkReplayEvent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_replay_event(
    handle: *mut c_void,
    index: u32,
    out: *mut DkReplayEvent,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let Some(event) = editor.recording.events().get(index as usize) else {
        return 0;
    };
    // SAFETY: the shell passed room for one event.
    unsafe { *out = DkReplayEvent::from_command(event) };
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Editor;

    #[test]
    fn a_stroke_event_round_trips_through_the_c_side() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        editor.recording_enabled = true;
        editor.begin_stroke(10);
        editor.stroke_at((2.0, 3.0), (20.0, 21.0), 4.0, 0.5, [1.0, 0.0, 0.0, 1.0], 10);
        editor.end_stroke();
        let handle = Box::into_raw(Box::new(editor)) as *mut c_void;
        assert_eq!(dk_replay_event_count(handle), 1);
        let mut event = DkReplayEvent::default();
        assert_eq!(unsafe { dk_replay_event(handle, 0, &mut event) }, 1);
        assert_eq!(event.kind, 0);
        assert_eq!(event.from_x, 2.0);
        assert_eq!(event.to_x, 20.0);
        assert_eq!(event.radius, 4.0);
        assert_eq!(event.color, [1.0, 0.0, 0.0, 1.0]);
        assert!(event.group != u64::MAX);
        // SAFETY: the handle came from `Box::into_raw` above and is freed once.
        unsafe { drop(Box::from_raw(handle as *mut Editor)) };
    }
}
