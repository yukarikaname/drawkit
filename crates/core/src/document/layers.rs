use super::*;

impl Document {
    pub fn add_layer(&mut self, name: impl Into<String>) -> usize {
        let parent = if self.is_group(self.active) {
            Some(self.active)
        } else {
            self.layers[self.active].parent
        };
        let at = (self.active + 1).min(self.layers.len());
        let mut layer = Layer::new(name, self.width, self.height);
        layer.parent = parent;
        self.layers.insert(at, layer);
        self.active = at;
        at
    }

    /// Add a solid fill layer above the active item.
    pub fn add_fill_layer(&mut self, name: impl Into<String>, color: Rgba) -> usize {
        let parent = if self.is_group(self.active) {
            Some(self.active)
        } else {
            self.layers[self.active].parent
        };
        let at = (self.active + 1).min(self.layers.len());
        let mut layer = Layer::fill(name, color, self.width, self.height);
        layer.parent = parent;
        self.layers.insert(at, layer);
        self.active = at;
        at
    }

    /// Add an empty group above the active item, as a sibling of the active item's group.
    pub fn add_group(&mut self, name: impl Into<String>) -> usize {
        let parent = if self.is_group(self.active) {
            Some(self.active)
        } else {
            self.layers[self.active].parent
        };
        let at = (self.active + 1).min(self.layers.len());
        let mut group = Layer::group(name, self.width, self.height);
        group.parent = parent;
        self.layers.insert(at, group);
        self.active = at;
        at
    }

    /// Whether `index` sits inside the subtree rooted at `root`.
    fn is_inside(&self, index: usize, root: usize) -> bool {
        let mut current = self.layers.get(index).and_then(|layer| layer.parent);
        for _ in 0..self.layers.len() {
            match current {
                Some(parent) if parent == root => return true,
                Some(parent) => current = self.layers.get(parent).and_then(|layer| layer.parent),
                None => return false,
            }
        }
        false
    }

    /// The end of the block rooted at `index`. A group's descendants follow it in the vector, so
    /// moving a group moves its children with it. A project that does not keep that invariant is
    /// left alone rather than scrambled.
    fn block_end(&self, index: usize) -> Option<usize> {
        if index >= self.layers.len() {
            return None;
        }
        if !self.is_group(index) {
            return Some(index + 1);
        }
        let size = (index..self.layers.len())
            .filter(|&other| other == index || self.is_inside(other, index))
            .count();
        let end = index + size;
        if end > self.layers.len() {
            return None;
        }
        if (index..end).all(|other| other == index || self.is_inside(other, index)) {
            Some(end)
        } else {
            None
        }
    }

    fn sibling(&self, index: usize, up: bool) -> Option<usize> {
        let parent = self.layers.get(index)?.parent;
        if up {
            (index + 1..self.layers.len()).find(|&other| self.layers[other].parent == parent)
        } else {
            (0..index)
                .rev()
                .find(|&other| self.layers[other].parent == parent)
        }
    }

    /// Swap two adjacent sibling blocks and fix every parent link and the active index.
    fn swap_adjacent(&mut self, first: usize, middle: usize, end: usize) {
        let first_len = middle - first;
        let second_len = end - middle;
        self.layers[first..end].rotate_left(first_len);
        for layer in &mut self.layers[first..first + second_len] {
            if let Some(parent) = layer.parent
                && parent >= first
                && parent < end
            {
                layer.parent = Some(parent - first_len);
            }
        }
        for layer in &mut self.layers[first + second_len..end] {
            if let Some(parent) = layer.parent
                && parent >= first
                && parent < end
            {
                layer.parent = Some(parent + second_len);
            }
        }
        if self.active >= first && self.active < end {
            self.active = if self.active < middle {
                self.active + second_len
            } else {
                self.active - first_len
            };
        }
    }

    /// Move the layer or group at `index` by `steps` among its siblings. Positive is towards the
    /// front of the painting, which is up in the palette.
    pub fn move_layer(&mut self, index: usize, steps: i32) -> Option<(usize, i32)> {
        if index >= self.layers.len() || steps == 0 {
            return None;
        }
        let mut current = index;
        let mut applied = 0;
        for _ in 0..steps.unsigned_abs() {
            let Some(other) = self.sibling(current, steps > 0) else {
                break;
            };
            let (Some(current_end), Some(other_end)) =
                (self.block_end(current), self.block_end(other))
            else {
                break;
            };
            // The two sibling blocks have to touch; if they do not, the tree is not stored as
            // contiguous blocks and swapping them would scramble the parent links.
            let (first, middle, end, moved_first) = if current < other {
                if current_end != other {
                    break;
                }
                (current, other, other_end, true)
            } else {
                if other_end != current {
                    break;
                }
                (other, current, current_end, false)
            };
            let second_len = end - middle;
            self.swap_adjacent(first, middle, end);
            current = if moved_first {
                first + second_len
            } else {
                first
            };
            applied += if steps > 0 { 1 } else { -1 };
        }
        (applied != 0).then_some((current, applied))
    }

    /// Remove the active item, reporting what left and what was created in its place. Undo needs
    /// both halves: the removed subtree to put back, and a fallback layer to take away again.
    pub fn remove_active_collect(&mut self) -> Option<LayerRemoval> {
        if self.layers.len() <= 1 {
            return None;
        }
        let before = self.active;
        let mut doomed = vec![self.active];
        if self.is_group(self.active) {
            self.collect_children(self.active, &mut doomed);
        }
        doomed.sort_unstable();
        doomed.dedup();
        let mut removed = Vec::with_capacity(doomed.len());
        for index in doomed.iter().rev() {
            if *index < self.layers.len() {
                removed.push((*index, self.layers.remove(*index)));
            }
        }
        removed.reverse();
        let mut inserted = Vec::new();
        if !self
            .layers
            .iter()
            .any(|layer| layer.kind == LayerKind::Paint)
        {
            let layer = Layer::new("Layer 1", self.width, self.height);
            self.layers.push(layer);
            let index = self.layers.len() - 1;
            inserted.push((index, self.layers[index].clone()));
        }
        self.active = self.active.min(self.layers.len() - 1);
        if !self.can_paint()
            && let Some(index) = self
                .layers
                .iter()
                .position(|layer| layer.kind == LayerKind::Paint)
        {
            self.active = index;
        }
        Some(LayerRemoval {
            removed,
            inserted,
            active_before: before,
            active_after: self.active,
        })
    }

    /// Remove the active item. Removing a group removes everything inside it.
    pub fn remove_active(&mut self) -> bool {
        self.remove_active_collect().is_some()
    }

    fn collect_children(&self, group: usize, out: &mut Vec<usize>) {
        for (index, layer) in self.layers.iter().enumerate() {
            if layer.parent == Some(group) {
                out.push(index);
                self.collect_children(index, out);
            }
        }
    }
}
