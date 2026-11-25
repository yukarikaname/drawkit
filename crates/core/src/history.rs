//! Undo, in pixels and in bytes.
//!
//! The wrong implementation copies the whole canvas for every stroke: at 4K that is 130 MB a step,
//! the third stroke is a gigabyte, and the fourth is a swap file. The right one copies the rectangle
//! the stroke touched, and only that - which is why the surface hands out a dirty rectangle in the
//! first place.
//!
//! A step stores before *and* after: undo restores the one, redo the other, and neither has to
//! re-run the tool that made the change.

use crate::color::Rgba;
use crate::document::Document;
use crate::surface::Rect;

/// One changed rectangle of a step. A tool that works in segments records one piece per segment
/// or per tile; nothing has to be expanded to their bounding box.
#[derive(Clone)]
struct Piece {
    rect: Rect,
    before: Vec<Rgba>,
    after: Vec<Rgba>,
}

#[derive(Clone)]
pub struct Step {
    /// Orders this pixel step against layer-structure steps.
    serial: u64,
    layer: usize,
    pieces: Vec<Piece>,
}

impl Step {
    /// What this step costs the memory budget. Four floats a pixel, twice.
    fn bytes(&self) -> usize {
        self.pieces
            .iter()
            .map(|piece| (piece.before.len() + piece.after.len()) * std::mem::size_of::<Rgba>())
            .sum()
    }

    /// The rectangle a caller should refresh: every piece's union.
    fn bounds(&self) -> Rect {
        let mut bounds = self
            .pieces
            .first()
            .map_or(Rect::new(0, 0, 0, 0), |p| p.rect);
        for piece in self.pieces.iter().skip(1) {
            bounds = bounds.union(&piece.rect);
        }
        bounds
    }
}

#[derive(Clone)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    limit_bytes: usize,
    bytes: usize,
}

impl History {
    pub fn new(limit_bytes: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit_bytes,
            bytes: 0,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Forget every step, for a newly loaded document. The limit survives.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.bytes = 0;
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Record a change that has already been applied.
    ///
    /// `before` is the region as it was, `after` as it is now. Calling this clears the redo stack:
    /// a new edit after an undo is a new branch, and the branch that was abandoned is gone.
    pub fn push(
        &mut self,
        serial: u64,
        layer: usize,
        rect: Rect,
        before: Vec<Rgba>,
        after: Vec<Rgba>,
    ) {
        self.push_pieces(serial, layer, vec![(rect, before, after)]);
    }

    /// Record a change made of several rectangles, such as the tiles a GPU commit returned. The
    /// pieces are stored as they are: nothing is expanded to their bounding box.
    pub fn push_pieces(
        &mut self,
        serial: u64,
        layer: usize,
        pieces: Vec<(Rect, Vec<Rgba>, Vec<Rgba>)>,
    ) {
        let pieces: Vec<Piece> = pieces
            .into_iter()
            .filter(|(rect, before, after)| {
                !rect.is_empty() && !before.is_empty() && before.len() == after.len()
            })
            .map(|(rect, before, after)| Piece {
                rect,
                before,
                after,
            })
            .collect();
        if pieces.is_empty() {
            return;
        }
        self.redo.clear();
        let step = Step {
            serial,
            layer,
            pieces,
        };
        self.bytes += step.bytes();
        self.undo.push(step);
        // Oldest first: the step that is most expensive to keep is the one furthest from what the
        // user is doing.
        while self.bytes > self.limit_bytes && self.undo.len() > 1 {
            let dropped = self.undo.remove(0);
            self.bytes -= dropped.bytes();
        }
    }

    /// Merge the last `count` steps into one, because they are segments of a single stroke.
    ///
    /// The union's before image is assembled from the segments in order, keeping the earliest value
    /// for a pixel several segments touched. The after image comes from the document itself: after
    /// all the segments, that is exactly the last state.
    pub fn merge_last(&mut self, count: usize, document: &mut Document) -> bool {
        if count < 2 || count > self.undo.len() {
            return false;
        }
        let start = self.undo.len() - count;
        let layer = self.undo[start].layer;
        if self.undo[start..].iter().any(|step| step.layer != layer) {
            return false;
        }
        let mut union = self.undo[start].bounds();
        for step in &self.undo[start + 1..] {
            union = union.union(&step.bounds());
        }
        if union.is_empty() {
            return false;
        }
        let Some(target) = document.layers.get(layer) else {
            return false;
        };
        let mut before = vec![[0.0; 4]; union.width * union.height];
        let mut filled = vec![false; union.width * union.height];
        for step in &self.undo[start..] {
            for piece in &step.pieces {
                for y in piece.rect.y..(piece.rect.y + piece.rect.height) {
                    for x in piece.rect.x..(piece.rect.x + piece.rect.width) {
                        let union_index = (y - union.y) * union.width + (x - union.x);
                        if filled[union_index] {
                            continue;
                        }
                        let step_index = (y - piece.rect.y) * piece.rect.width + (x - piece.rect.x);
                        before[union_index] = piece.before[step_index];
                        filled[union_index] = true;
                    }
                }
            }
        }
        let after = target.surface.snapshot(union);
        let serial = self.undo[start..].last().map_or(0, |step| step.serial);
        let removed: usize = self.undo[start..].iter().map(Step::bytes).sum();
        self.undo.truncate(start);
        self.bytes -= removed;
        self.push_pieces(serial, layer, vec![(union, before, after)]);
        true
    }

    /// Put the last change back, returning the rectangle that changed.
    pub fn undo(&mut self, document: &mut Document) -> Option<Rect> {
        let step = self.undo.pop()?;
        if let Some(layer) = document.layers.get_mut(step.layer) {
            for piece in &step.pieces {
                layer.surface.restore(piece.rect, &piece.before);
            }
        }
        self.bytes -= step.bytes();
        let rect = step.bounds();
        self.redo.push(step);
        Some(rect)
    }

    /// Apply the last undone change again.
    pub fn redo(&mut self, document: &mut Document) -> Option<Rect> {
        let step = self.redo.pop()?;
        if let Some(layer) = document.layers.get_mut(step.layer) {
            for piece in &step.pieces {
                layer.surface.restore(piece.rect, &piece.after);
            }
        }
        self.bytes += step.bytes();
        let rect = step.bounds();
        self.undo.push(step);
        Some(rect)
    }

    /// Every step, for a "history" panel.
    pub fn depth(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    pub fn top_serial(&self) -> u64 {
        self.undo.last().map_or(0, |step| step.serial)
    }

    pub fn redo_serial(&self) -> u64 {
        self.redo.last().map_or(0, |step| step.serial)
    }

    /// A new action on another timeline makes the abandoned branch invalid.
    pub fn clear_redo(&mut self) {
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document_with_dot(color: Rgba) -> (Document, Rect, Vec<Rgba>) {
        let mut document = Document::new(16, 16);
        let rect = Rect::new(2, 2, 4, 4);
        let before = document.layers[0].surface.snapshot(rect);
        for y in 2..6 {
            for x in 2..6 {
                document.layers[0].surface.set(x, y, color);
            }
        }
        (document, rect, before)
    }

    #[test]
    fn undo_restores_what_was_there_and_redo_puts_it_back() {
        let (mut document, rect, before) = document_with_dot([1.0, 0.0, 0.0, 1.0]);
        let after = document.layers[0].surface.snapshot(rect);
        let mut history = History::new(1 << 20);
        history.push(1, 0, rect, before, after);

        history.undo(&mut document).unwrap();
        assert_eq!(document.layers[0].surface.get(3, 3), [0.0; 4]);

        history.redo(&mut document).unwrap();
        assert_eq!(document.layers[0].surface.get(3, 3), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn merging_segments_makes_one_step_that_undoes_the_whole_stroke() {
        let mut document = Document::new(16, 16);
        let original = document.layers[0].surface.snapshot(Rect::new(0, 0, 16, 16));
        let mut history = History::new(1 << 20);
        for (rect, color) in [
            (Rect::new(2, 2, 4, 4), [1.0, 0.0, 0.0, 1.0]),
            (Rect::new(4, 4, 4, 4), [1.0, 0.0, 0.0, 1.0]),
            (Rect::new(6, 6, 4, 4), [1.0, 0.0, 0.0, 1.0]),
        ] {
            let before = document.layers[0].surface.snapshot(rect);
            for y in rect.y..rect.y + rect.height {
                for x in rect.x..rect.x + rect.width {
                    document.layers[0].surface.set(x, y, color);
                }
            }
            let after = document.layers[0].surface.snapshot(rect);
            history.push(1, 0, rect, before, after);
        }
        assert_eq!(history.depth().0, 3);
        assert!(history.merge_last(3, &mut document));
        assert_eq!(history.depth().0, 1);
        history.undo(&mut document).unwrap();
        assert_eq!(
            document.layers[0].surface.snapshot(Rect::new(0, 0, 16, 16)),
            original,
            "undo left part of the stroke behind"
        );
    }

    #[test]
    fn a_new_edit_drops_the_redo_branch() {
        let (mut document, rect, before) = document_with_dot([1.0, 0.0, 0.0, 1.0]);
        let after = document.layers[0].surface.snapshot(rect);
        let mut history = History::new(1 << 20);
        history.push(1, 0, rect, before.clone(), after.clone());
        history.undo(&mut document).unwrap();
        history.push(1, 0, rect, before, after);
        assert!(!history.can_redo());
    }

    #[test]
    fn the_oldest_steps_are_dropped_at_the_budget() {
        let (mut document, rect, before) = document_with_dot([1.0, 0.0, 0.0, 1.0]);
        let after = document.layers[0].surface.snapshot(rect);
        // Room for a couple of steps and no more. One step is four floats a pixel, twice, over a
        // 4x4 rectangle: 512 bytes.
        let limit = 1_500;
        let mut history = History::new(limit);
        for _ in 0..8 {
            history.push(1, 0, rect, before.clone(), after.clone());
        }
        let (depth, _) = history.depth();
        assert!(depth < 8, "the budget did not trim anything");
        assert!(
            history.bytes() <= limit,
            "{} bytes over the {} budget",
            history.bytes(),
            limit
        );
        // And what is left still undoes.
        assert!(history.undo(&mut document).is_some());
    }
}
