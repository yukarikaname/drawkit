//! Recording what the painter did, so it can be played back.
//!
//! The tape is the *commands*, not the pixels: a stroke is four numbers and a colour, and an hour of
//! painting is a few thousand of them - kilobytes, not gigabytes. That makes replay cheap enough to
//! scrub, and it is the same tape a learned predictor would want: a stroke is training data exactly
//! as it was drawn.
//!
//! Undo edits the tape as well as the canvas. A recording that replays the stroke the painter took
//! back is not a recording of what they made.

use crate::brush::BrushKind;
use crate::color::Rgba;
mod edit;

pub use edit::TapeEdit;

use drawkit_formats::BrushScale;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Stroke {
        layer: usize,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        color: Rgba,
        brush: BrushKind,
        seed: u64,
        tilt: [f32; 2],
        azimuth: f32,
        roll: f32,
        scale: BrushScale,
    },
    Fill {
        layer: usize,
        x: usize,
        y: usize,
        tolerance: f32,
        color: Rgba,
        close: usize,
        expand: usize,
        feather: usize,
        /// Gap closing also used the local edge model.
        smart: bool,
    },
    Erase {
        layer: usize,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        brush: BrushKind,
        seed: u64,
        tilt: [f32; 2],
        azimuth: f32,
        roll: f32,
        scale: BrushScale,
    },
    Smudge {
        layer: usize,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        pressure: f32,
        strength: f32,
        brush: BrushKind,
    },
    Clear {
        layer: usize,
        color: Rgba,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub time_ms: u64,
    /// Segments of one pointer stroke share a group; undo removes the group, not one segment.
    pub group: Option<u64>,
    pub command: Command,
}

#[derive(Default, Clone)]
pub struct Recording {
    events: Vec<Event>,
    /// What undo took off the tape, one entry per undo step and oldest step first. The group of a
    /// stroke stays together so redo puts the same events back, in the same order.
    undone: Vec<Vec<Event>>,
    revision: u64,
}

impl Recording {
    /// The rich stroke tape, for `.dkp` and replay at another output size.
    ///
    /// The tape is a view of the recording; undo edits the recording, so the tape follows it.
    pub fn tape(
        &self,
        width: usize,
        height: usize,
        layers: usize,
    ) -> drawkit_formats::TapeDocument {
        use drawkit_formats::{TapeCommand, TapeDocument, TapeEvent};
        let events = self
            .events
            .iter()
            .map(|event| TapeEvent {
                time_ms: event.time_ms,
                group: event.group,
                command: match event.command {
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
                        scale,
                    } => TapeCommand::Stroke {
                        layer: layer as u32,
                        from: [from.0, from.1],
                        to: [to.0, to.1],
                        radius,
                        pressure,
                        color,
                        brush: brush.as_i32() as u8,
                        seed,
                        tilt,
                        azimuth,
                        roll,
                        scale: scale.as_u8(),
                    },
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
                        scale,
                    } => TapeCommand::Erase {
                        layer: layer as u32,
                        from: [from.0, from.1],
                        to: [to.0, to.1],
                        radius,
                        pressure,
                        brush: brush.as_i32() as u8,
                        seed,
                        tilt,
                        azimuth,
                        roll,
                        scale: scale.as_u8(),
                    },
                    Command::Fill {
                        layer,
                        x,
                        y,
                        tolerance,
                        color,
                        close,
                        expand,
                        feather,
                        smart,
                    } => TapeCommand::Fill {
                        layer: layer as u32,
                        x: x as u32,
                        y: y as u32,
                        tolerance,
                        color,
                        close: close as u32,
                        expand: expand as u32,
                        feather: feather as u32,
                        smart,
                    },
                    Command::Smudge {
                        layer,
                        from,
                        to,
                        radius,
                        pressure,
                        strength,
                        brush,
                    } => TapeCommand::Smudge {
                        layer: layer as u32,
                        from: [from.0, from.1],
                        to: [to.0, to.1],
                        radius,
                        pressure,
                        strength,
                        brush: brush.as_i32() as u8,
                    },
                    Command::Clear { layer, color } => TapeCommand::Clear {
                        layer: layer as u32,
                        color,
                    },
                },
            })
            .collect();
        TapeDocument {
            width,
            height,
            layers,
            color_space: 0,
            dynamic_range: 0,
            layer_info: Vec::new(),
            active: None,
            events,
            frames: Vec::new(),
            active_frame: 0,
            fps: 12,
            frame_durations: Vec::new(),
            loop_enabled: true,
            loop_start: 0,
            loop_end: 0,
            brushes: Vec::new(),
            textures: Vec::new(),
        }
    }

    /// Rebuild the recording from a saved tape so opening a project and saving it again does not
    /// drop the strokes that were already there.
    pub fn from_tape(events: &[drawkit_formats::TapeEvent]) -> Self {
        let mut recording = Self::default();
        for event in events {
            let command = match event.command {
                drawkit_formats::TapeCommand::Stroke {
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
                    scale,
                } => Command::Stroke {
                    layer: layer as usize,
                    from: (from[0], from[1]),
                    to: (to[0], to[1]),
                    radius,
                    pressure,
                    color,
                    brush: BrushKind::from_i32(brush as i32),
                    seed,
                    tilt,
                    azimuth,
                    roll,
                    scale: BrushScale::from_u8(scale),
                },
                drawkit_formats::TapeCommand::Erase {
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
                    scale,
                } => Command::Erase {
                    layer: layer as usize,
                    from: (from[0], from[1]),
                    to: (to[0], to[1]),
                    radius,
                    pressure,
                    brush: BrushKind::from_i32(brush as i32),
                    seed,
                    tilt,
                    azimuth,
                    roll,
                    scale: BrushScale::from_u8(scale),
                },
                drawkit_formats::TapeCommand::Fill {
                    layer,
                    x,
                    y,
                    tolerance,
                    color,
                    close,
                    expand,
                    feather,
                    smart,
                } => Command::Fill {
                    layer: layer as usize,
                    x: x as usize,
                    y: y as usize,
                    tolerance,
                    color,
                    close: close as usize,
                    expand: expand as usize,
                    feather: feather as usize,
                    smart,
                },
                drawkit_formats::TapeCommand::Smudge {
                    layer,
                    from,
                    to,
                    radius,
                    pressure,
                    strength,
                    brush,
                } => Command::Smudge {
                    layer: layer as usize,
                    from: (from[0], from[1]),
                    to: (to[0], to[1]),
                    radius,
                    pressure,
                    strength,
                    brush: BrushKind::from_i32(brush as i32),
                },
                drawkit_formats::TapeCommand::Clear { layer, color } => Command::Clear {
                    layer: layer as usize,
                    color,
                },
            };
            recording.push_grouped(event.time_ms, event.group, command);
        }
        recording
    }

    pub fn push(&mut self, time_ms: u64, command: Command) {
        self.push_grouped(time_ms, None, command);
    }

    /// Add a command that belongs to a pointer stroke. The group lets undo remove every segment of
    /// the stroke instead of leaving a shortened one on the tape.
    pub fn push_grouped(&mut self, time_ms: u64, group: Option<u64>, command: Command) {
        // A new command is a new branch: the events an undo took off this tape are gone.
        self.undone.clear();
        // Monotonic: a tape that goes backwards is a scrub that breaks every frame after it.
        let time_ms = self
            .events
            .last()
            .map_or(time_ms, |last| time_ms.max(last.time_ms));
        self.events.push(Event {
            time_ms,
            group,
            command,
        });
        self.revision = self.revision.wrapping_add(1);
    }

    /// Drop the last command - or the whole stroke it belongs to, because the painter undid it.
    pub fn undo(&mut self) -> bool {
        let Some(last) = self.events.last() else {
            return false;
        };
        let group = last.group;
        self.revision = self.revision.wrapping_add(1);
        let start = match group {
            Some(group) => {
                let mut start = self.events.len();
                while start > 0 && self.events[start - 1].group == Some(group) {
                    start -= 1;
                }
                start
            }
            None => self.events.len() - 1,
        };
        let removed: Vec<Event> = self.events.drain(start..).collect();
        self.undone.push(removed);
        true
    }

    /// Put the events the last undo removed back on the tape. Their times are clamped to the last
    /// event so a redo after a later recording still replays in order.
    pub fn redo(&mut self) -> bool {
        let Some(events) = self.undone.pop() else {
            return false;
        };
        let mut time_ms = self.events.last().map_or(0, |last| last.time_ms);
        for mut event in events {
            time_ms = time_ms.max(event.time_ms);
            event.time_ms = time_ms;
            self.events.push(event);
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.undone.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    /// Bumped on every edit so a cached replay knows it is stale.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn duration_ms(&self) -> u64 {
        self.events.last().map_or(0, |event| event.time_ms)
    }
}

#[cfg(test)]
mod tests;
