//! The undo record for pixels that never leave the GPU.
//!
//! A commit replaces the textures of every tile it touches. Those replaced textures are the undo
//! record; this type only orders them, groups segments and enforces the memory budget. Both the
//! window surface and the headless compositor keep one.

use super::TileSwap;

pub(crate) struct JournalStep {
    pub(crate) layer: usize,
    pub(crate) rects: Vec<(u32, u32, u32, u32)>,
    pub(crate) swaps: Vec<TileSwap>,
    pub(crate) undone: Vec<TileSwap>,
    pub(crate) bytes: usize,
}

pub(crate) struct TileJournal {
    undo: Vec<JournalStep>,
    redo: Vec<JournalStep>,
    open: Option<JournalStep>,
    bytes: usize,
    budget: usize,
}

impl TileJournal {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            open: None,
            bytes: 0,
            budget,
        }
    }

    /// Record one commit. Returns false when there was nothing to swap.
    pub(crate) fn push(
        &mut self,
        layer: usize,
        rects: Vec<(u32, u32, u32, u32)>,
        swaps: Vec<TileSwap>,
        bytes: usize,
    ) -> bool {
        if swaps.is_empty() {
            return false;
        }
        self.redo.clear();
        self.bytes = self.bytes.saturating_add(bytes);
        self.undo.push(JournalStep {
            layer,
            rects,
            swaps,
            undone: Vec::new(),
            bytes,
        });
        self.trim();
        true
    }

    /// Start collecting segment commits into one step, for a smudge.
    pub(crate) fn begin_group(&mut self, layer: usize) {
        self.open = Some(JournalStep {
            layer,
            rects: Vec::new(),
            swaps: Vec::new(),
            undone: Vec::new(),
            bytes: 0,
        });
    }

    pub(crate) fn grouping(&self) -> bool {
        self.open.is_some()
    }

    /// Append to the open group. Returns every rectangle in the group so far.
    pub(crate) fn push_grouped(
        &mut self,
        rects: &[(u32, u32, u32, u32)],
        swaps: Vec<TileSwap>,
        bytes: usize,
    ) -> Option<Vec<(u32, u32, u32, u32)>> {
        let open = self.open.as_mut()?;
        open.rects.extend_from_slice(rects);
        open.swaps.extend(swaps);
        open.bytes = open.bytes.saturating_add(bytes);
        Some(open.rects.clone())
    }

    /// Close the open group and return its rectangles once it is an undo step.
    pub(crate) fn end_group(&mut self) -> Option<Vec<(u32, u32, u32, u32)>> {
        let step = self.open.take()?;
        if step.swaps.is_empty() {
            return None;
        }
        let rects = step.rects.clone();
        self.redo.clear();
        self.bytes = self.bytes.saturating_add(step.bytes);
        self.undo.push(step);
        self.trim();
        Some(rects)
    }

    pub(crate) fn begin_undo(&mut self) -> Option<JournalStep> {
        let step = self.undo.pop()?;
        self.bytes = self.bytes.saturating_sub(step.bytes);
        Some(step)
    }

    /// The rectangles the next undo would restore, without taking the step.
    pub(crate) fn undo_rects(&self) -> Option<&[(u32, u32, u32, u32)]> {
        self.undo.last().map(|step| step.rects.as_slice())
    }

    /// The rectangles the next redo would restore, without taking the step.
    pub(crate) fn redo_rects(&self) -> Option<&[(u32, u32, u32, u32)]> {
        self.redo.last().map(|step| step.rects.as_slice())
    }

    pub(crate) fn finish_undo(&mut self, step: JournalStep) {
        self.redo.push(step);
    }

    pub(crate) fn begin_redo(&mut self) -> Option<JournalStep> {
        let step = self.redo.pop()?;
        self.bytes = self.bytes.saturating_add(step.bytes);
        Some(step)
    }

    pub(crate) fn finish_redo(&mut self, step: JournalStep) {
        self.undo.push(step);
    }

    /// Drop every step and hand back the tile textures it was holding. After this no step can
    /// restore anything, so each one is free for reuse.
    pub(crate) fn take_textures(&mut self) -> Vec<TileSwap> {
        let mut out = Vec::new();
        let mut collect = |step: JournalStep| {
            out.extend(step.swaps);
            out.extend(step.undone);
        };
        for step in self.undo.drain(..) {
            collect(step);
        }
        for step in self.redo.drain(..) {
            collect(step);
        }
        if let Some(step) = self.open.take() {
            collect(step);
        }
        self.bytes = 0;
        out
    }

    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.open = None;
        self.bytes = 0;
    }

    fn trim(&mut self) {
        while self.bytes > self.budget && self.undo.len() > 1 {
            let dropped = self.undo.remove(0);
            self.bytes = self.bytes.saturating_sub(dropped.bytes);
        }
    }
}
