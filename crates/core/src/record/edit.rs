//! Stroke edits: the tape half of a recolour or a deletion.

use super::*;

/// One change to the tape that a history step has to undo with its pixels. Stroke edits replace
/// the recording, and a pixel-only step would leave the tape describing the old drawing.
#[derive(Clone, Debug)]
pub enum TapeEdit {
    /// Every segment of `group` had its ink replaced.
    Recolor {
        group: u64,
        before: Rgba,
        after: Rgba,
    },
    /// The events of `group` with the positions they occupied, for undo.
    Removed {
        group: u64,
        events: Vec<(usize, Event)>,
    },
}

impl TapeEdit {
    /// Whether the event belongs to the group this edit removes.
    pub fn removes(&self, group: Option<u64>) -> bool {
        matches!(self, TapeEdit::Removed { group: removed, .. } if group == Some(*removed))
    }
}

impl Recording {
    /// The colour of the first paint segment in a group.
    pub fn group_color(&self, group: u64) -> Option<Rgba> {
        self.events.iter().find_map(|event| {
            if event.group != Some(group) {
                return None;
            }
            match event.command {
                Command::Stroke { color, .. } => Some(color),
                _ => None,
            }
        })
    }

    /// The events of a group and where they sit, for a removal edit.
    pub fn group_events(&self, group: u64) -> Vec<(usize, Event)> {
        self.events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.group == Some(group))
            .map(|(index, event)| (index, *event))
            .collect()
    }

    /// Replace the ink of every paint segment in a group.
    pub fn recolor_group(&mut self, group: u64, color: Rgba) -> bool {
        let mut changed = false;
        for event in &mut self.events {
            if event.group != Some(group) {
                continue;
            }
            if let Command::Stroke { color: ink, .. } = &mut event.command
                && *ink != color
            {
                *ink = color;
                changed = true;
            }
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    /// Put a tape edit in place. `forward` applies it, the other direction undoes it.
    pub fn apply_edit(&mut self, edit: &TapeEdit, forward: bool) {
        // An edit rewrites the timeline under the undone events, so they can no longer be replayed
        // where they came from.
        self.undone.clear();
        match edit {
            TapeEdit::Recolor {
                group,
                before,
                after,
            } => {
                let color = if forward { *after } else { *before };
                self.recolor_group(*group, color);
            }
            TapeEdit::Removed { group, events } => {
                if forward {
                    self.events.retain(|event| event.group != Some(*group));
                } else {
                    for (index, event) in events {
                        let at = (*index).min(self.events.len());
                        self.events.insert(at, *event);
                    }
                }
                self.revision = self.revision.wrapping_add(1);
            }
        }
    }
}
