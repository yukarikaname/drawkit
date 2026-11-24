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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub time_ms: u64,
    /// Segments of one pointer stroke share a group; undo removes the group, not one segment.
    pub group: Option<u64>,
    pub command: Command,
}

#[derive(Default, Clone)]
pub struct Recording {
    events: Vec<Event>,
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
                    } => TapeCommand::Fill {
                        layer: layer as u32,
                        x: x as u32,
                        y: y as u32,
                        tolerance,
                        color,
                        close: close as u32,
                        expand: expand as u32,
                        feather: feather as u32,
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
                } => Command::Fill {
                    layer: layer as usize,
                    x: x as usize,
                    y: y as usize,
                    tolerance,
                    color,
                    close: close as usize,
                    expand: expand as usize,
                    feather: feather as usize,
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
        match group {
            Some(group) => {
                while self
                    .events
                    .last()
                    .is_some_and(|event| event.group == Some(group))
                {
                    self.events.pop();
                }
            }
            None => {
                self.events.pop();
            }
        }
        true
    }

    pub fn clear(&mut self) {
        self.events.clear();
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
mod tests {
    use super::*;

    #[test]
    fn time_never_goes_backwards() {
        let mut recording = Recording::default();
        recording.push(
            100,
            Command::Clear {
                layer: 0,
                color: [0.0; 4],
            },
        );
        recording.push(
            50,
            Command::Clear {
                layer: 0,
                color: [0.0; 4],
            },
        );
        assert_eq!(recording.duration_ms(), 100);
    }

    #[test]
    fn undo_drops_every_segment_of_a_grouped_stroke() {
        let mut recording = Recording::default();
        recording.push_grouped(
            1,
            Some(7),
            Command::Clear {
                layer: 0,
                color: [0.0; 4],
            },
        );
        recording.push_grouped(
            2,
            Some(7),
            Command::Clear {
                layer: 0,
                color: [0.0; 4],
            },
        );
        recording.push_grouped(
            3,
            Some(7),
            Command::Clear {
                layer: 0,
                color: [0.0; 4],
            },
        );
        recording.push(
            4,
            Command::Clear {
                layer: 0,
                color: [0.0; 4],
            },
        );
        assert!(recording.undo());
        assert_eq!(
            recording.events().len(),
            3,
            "the ungrouped command survived"
        );
        assert!(recording.undo());
        assert!(
            recording.is_empty(),
            "a segment of the stroke stayed on the tape"
        );
    }

    #[test]
    fn undo_drops_the_last_command() {
        let mut recording = Recording::default();
        recording.push(
            1,
            Command::Clear {
                layer: 0,
                color: [0.0; 4],
            },
        );
        recording.push(
            2,
            Command::Clear {
                layer: 0,
                color: [0.0; 4],
            },
        );
        assert!(recording.undo());
        assert_eq!(recording.events().len(), 1);
        assert_eq!(recording.duration_ms(), 1);
    }
}
