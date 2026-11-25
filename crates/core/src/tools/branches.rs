//! Experimental history branches: keep the path an undo abandoned.
//!
//! The undo stack is linear. When the painter undoes a few steps and then draws again, the redo
//! path is normally thrown away. With branches enabled, the state just before the first undo of a
//! sequence is kept as a dormant branch. It can be selected again, and edits made on it create
//! further children.
//!
//! A branch is a full snapshot, not a delta. That is expensive, so the feature is off by default,
//! is disabled for animation frames, and trims the oldest branch when the budget or the count is
//! exceeded. Nothing here is written to `.dkp`: branches are a session experiment.

use super::*;

/// A branch is a document, its tape and the history that led to it.
#[derive(Clone)]
pub(super) struct BranchState {
    document: Document,
    history: History,
    recording: Recording,
    layer_undo: Vec<LayerAction>,
    layer_redo: Vec<LayerAction>,
    next_serial: u64,
    next_group: u64,
    next_seed: u64,
}

impl BranchState {
    fn capture(editor: &Editor) -> Self {
        Self {
            document: editor.document.clone(),
            history: editor.history.clone(),
            recording: editor.recording.clone(),
            layer_undo: editor.layer_undo.clone(),
            layer_redo: editor.layer_redo.clone(),
            next_serial: editor.next_serial,
            next_group: editor.next_group,
            next_seed: editor.next_seed,
        }
    }

    /// The transient fields (a stroke in flight, a prediction, a replay cache) belong to the live
    /// editor, not to the saved state.
    fn restore(self, editor: &mut Editor) {
        editor.document = self.document;
        editor.history = self.history;
        editor.recording = self.recording;
        editor.layer_undo = self.layer_undo;
        editor.layer_redo = self.layer_redo;
        editor.next_serial = self.next_serial;
        editor.next_group = self.next_group;
        editor.next_seed = self.next_seed;
        editor.stroke = None;
        editor.pending_stroke = None;
        editor.tile_commit = None;
        editor.prediction = None;
        editor.predictor.clear();
        editor.fill_animation = None;
        editor.selection = None;
        editor.smoothed = None;
    }

    fn bytes(&self) -> usize {
        let surfaces: usize = self
            .document
            .layers
            .iter()
            .map(|layer| layer.surface.allocated_bytes())
            .sum();
        let events = std::mem::size_of_val(self.recording.events());
        surfaces + self.history.bytes() + events
    }
}

#[derive(Clone)]
pub(super) struct HistoryBranch {
    id: u64,
    parent: Option<u64>,
    state: BranchState,
    bytes: usize,
}

/// Dormant branches may keep half a gigabyte; past that the oldest alternative goes.
const BRANCH_BUDGET: usize = 512 << 20;
const BRANCH_LIMIT: usize = 8;

impl Editor {
    pub fn set_history_branches(&mut self, enabled: bool) {
        self.branches_enabled = enabled;
        if !enabled {
            self.clear_history_branches();
        }
    }

    pub fn history_branches_enabled(&self) -> bool {
        self.branches_enabled && self.frames.len() <= 1
    }

    pub fn clear_history_branches(&mut self) {
        self.branches.clear();
        self.branch_tip = None;
        self.current_branch = None;
    }

    pub fn history_branch_count(&self) -> usize {
        self.branches.len()
    }

    /// `(id, parent, current, bytes)` for the palette.
    pub fn history_branch(&self, index: usize) -> Option<(u64, Option<u64>, bool, usize)> {
        let branch = self.branches.get(index)?;
        Some((
            branch.id,
            branch.parent,
            Some(branch.id) == self.current_branch,
            branch.bytes,
        ))
    }

    pub fn current_history_branch(&self) -> Option<u64> {
        self.current_branch
    }

    /// Whether an undo sequence is open and its abandoned path would become a branch on the next
    /// edit. The shell can show a marker for it.
    pub fn pending_history_branch(&self) -> bool {
        self.branch_tip.is_some()
    }

    /// Switch to a stored branch. The current state is saved first, so switching back works even
    /// when it was the live path and had never been stored.
    pub fn switch_history_branch(&mut self, id: u64) -> bool {
        if Some(id) == self.current_branch || !self.branches.iter().any(|b| b.id == id) {
            return false;
        }
        self.save_current_branch();
        let Some(target) = self
            .branches
            .iter()
            .find(|branch| branch.id == id)
            .map(|branch| branch.state.clone())
        else {
            return false;
        };
        let bytes = target.bytes();
        target.restore(self);
        self.current_branch = Some(id);
        self.branch_tip = None;
        if let Some(branch) = self.branches.iter_mut().find(|branch| branch.id == id) {
            branch.bytes = bytes;
        }
        self.trim_branches();
        true
    }

    fn save_current_branch(&mut self) {
        let state = BranchState::capture(self);
        let bytes = state.bytes();
        if let Some(current) = self.current_branch
            && let Some(branch) = self.branches.iter_mut().find(|b| b.id == current)
        {
            branch.state = state;
            branch.bytes = bytes;
            return;
        }
        let id = self.take_branch_id();
        self.branches.push(HistoryBranch {
            id,
            parent: self.current_branch,
            state,
            bytes,
        });
        self.current_branch = Some(id);
    }

    fn take_branch_id(&mut self) -> u64 {
        let id = self.next_branch;
        self.next_branch = self.next_branch.wrapping_add(1);
        id
    }

    /// Called before a new edit clears the redo stacks. The snapshot taken at the first undo is
    /// the abandoned future; it becomes a branch now.
    pub(super) fn begin_new_edit(&mut self) {
        if !self.history_branches_enabled() {
            return;
        }
        let Some(tip) = self.branch_tip.take() else {
            return;
        };
        if !self.can_redo() {
            return;
        }
        let bytes = tip.bytes();
        let id = self.take_branch_id();
        self.branches.push(HistoryBranch {
            id,
            parent: self.current_branch,
            state: tip,
            bytes,
        });
        self.trim_branches();
    }

    /// Remember the state before the first undo of a sequence. That state is where the redo path
    /// ends, and it is the only snapshot with the tape still intact.
    pub(super) fn note_undo(&mut self) {
        if !self.history_branches_enabled() || self.branch_tip.is_some() {
            return;
        }
        if !self.can_redo() {
            self.branch_tip = Some(BranchState::capture(self));
        }
    }

    /// Reaching the end of the redo path means the snapshot and the live state agree again.
    pub(super) fn note_redo(&mut self) {
        if self.branch_tip.is_some() && !self.can_redo() {
            self.branch_tip = None;
        }
    }

    fn trim_branches(&mut self) {
        while self.branches.len() > 1 {
            let total: usize = self.branches.iter().map(|branch| branch.bytes).sum();
            if self.branches.len() <= BRANCH_LIMIT && total <= BRANCH_BUDGET {
                break;
            }
            let Some(victim) = self
                .branches
                .iter()
                .position(|branch| Some(branch.id) != self.current_branch)
            else {
                break;
            };
            let removed = self.branches.remove(victim);
            for branch in &mut self.branches {
                if branch.parent == Some(removed.id) {
                    branch.parent = removed.parent;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stroke(editor: &mut Editor, from: (f32, f32), to: (f32, f32)) {
        editor.begin_stroke(0);
        editor.stroke_at(from, to, 6.0, 1.0, [1.0, 0.0, 0.0, 1.0], 0);
        editor.end_stroke();
    }

    fn ink(editor: &Editor, x: usize, y: usize) -> f32 {
        editor.document.layers[0].surface.get(x, y)[3]
    }

    #[test]
    fn an_edit_after_undo_keeps_the_abandoned_path() {
        let mut editor = Editor::new(64, 64, 1 << 20);
        editor.brush = crate::brush::Brush::soft_round();
        editor.set_history_branches(true);
        stroke(&mut editor, (30.0, 10.0), (30.0, 50.0));
        assert!(ink(&editor, 30, 30) > 0.5);
        assert!(editor.undo().is_some());
        assert!(editor.pending_history_branch());
        assert_eq!(ink(&editor, 30, 30), 0.0);
        // A different stroke on the undone state drops the redo path, but not the branch.
        stroke(&mut editor, (10.0, 30.0), (50.0, 30.0));
        assert_eq!(editor.history_branch_count(), 1);
        assert!(ink(&editor, 30, 30) > 0.5);
        let (id, parent, current, _) = editor.history_branch(0).unwrap();
        assert_eq!(parent, None);
        assert!(!current);
        // The branch holds the first, vertical stroke.
        assert!(editor.switch_history_branch(id));
        assert!(ink(&editor, 30, 30) > 0.5, "the vertical stroke came back");
        assert_eq!(ink(&editor, 10, 30), 0.0);
        // And the live path was saved, so it can be selected too.
        assert_eq!(editor.history_branch_count(), 2);
        let saved = (0..editor.history_branch_count())
            .filter_map(|index| editor.history_branch(index))
            .find(|(_, _, current, _)| !current)
            .map(|(id, _, _, _)| id)
            .unwrap();
        assert!(editor.switch_history_branch(saved));
        assert!(ink(&editor, 10, 30) > 0.5);
    }

    #[test]
    fn redoing_the_whole_path_does_not_leave_a_branch() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        editor.set_history_branches(true);
        stroke(&mut editor, (8.0, 16.0), (24.0, 16.0));
        editor.undo();
        editor.redo();
        assert!(!editor.pending_history_branch());
        stroke(&mut editor, (16.0, 8.0), (16.0, 24.0));
        assert_eq!(editor.history_branch_count(), 0);
    }

    #[test]
    fn disabling_branches_forgets_them() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        editor.set_history_branches(true);
        stroke(&mut editor, (8.0, 16.0), (24.0, 16.0));
        editor.undo();
        stroke(&mut editor, (16.0, 8.0), (16.0, 24.0));
        assert_eq!(editor.history_branch_count(), 1);
        editor.set_history_branches(false);
        assert_eq!(editor.history_branch_count(), 0);
    }
}
