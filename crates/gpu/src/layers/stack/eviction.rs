//! Eviction, resumable holes and the tiles the undo journal still needs.
//!
//! A backup-less stack cannot rebuild an evicted texture, so dropping one turns it into a hole
//! the shell must fill again. A tile the next journal step will swap out stays pinned instead.

use super::*;

impl LayerStack {
    /// Tiles the undo journal still needs resident. Eviction skips them even when the budget is
    /// otherwise exceeded, so a step can always be taken.
    pub(crate) fn set_pinned(&mut self, pins: impl IntoIterator<Item = (usize, (u32, u32))>) {
        self.pinned = pins.into_iter().collect();
    }

    /// A tile's canvas rectangle, clipped at the right and bottom edges.
    fn tile_rect(&self, (tx, ty): (u32, u32)) -> (u32, u32, u32, u32) {
        let x = tx * self.tile_width;
        let y = ty * self.tile_height;
        (
            x,
            y,
            self.tile_width.min(self.width - x),
            self.tile_height.min(self.height - y),
        )
    }

    /// Every tile a backup-less stack had to evict and the shell must upload again, as
    /// `(layer, x, y, width, height)`. Layers the shell never uploaded are not listed; those are
    /// covered by `layer_missing`. Ordered by layer and then by tile position.
    pub fn missing_rects(&self) -> Vec<(usize, u32, u32, u32, u32)> {
        let mut out = Vec::new();
        if self.backups {
            return out;
        }
        for (index, layer) in self.layers.iter().enumerate() {
            let Some(layer) = layer else { continue };
            if !layer.known {
                continue;
            }
            let mut coords: Vec<(u32, u32)> = layer
                .tiles
                .iter()
                .filter(|(_, tile)| tile.missing && tile.gpu.is_none())
                .map(|(coord, _)| *coord)
                .collect();
            coords.sort_unstable();
            for coord in coords {
                let (x, y, width, height) = self.tile_rect(coord);
                out.push((index, x, y, width, height));
            }
        }
        out
    }

    /// Drop the coldest GPU tiles until the stack fits, never dropping the tile being rendered.
    pub(super) fn evict_cold(&mut self, keep: Option<(u32, u32)>) {
        while self.resident_bytes > self.budget_bytes {
            let victim = self.pick_victim(keep);
            let Some((index, coord)) = victim else {
                break;
            };
            let backups = self.backups;
            let (freed, droppable) = self
                .layers
                .get_mut(index)
                .and_then(Option::as_mut)
                .and_then(|layer| layer.tiles.get_mut(&coord))
                .map_or((0, false), |tile| {
                    // Without a backup the pixels only exist in the core surface now, so mark
                    // the tile for a shell re-upload and repaint that hole in the next composite.
                    let droppable = !backups;
                    if droppable {
                        tile.missing = true;
                        tile.backup = Vec::new();
                    }
                    (tile.gpu.take().map_or(0, |gpu| gpu.bytes), droppable)
                });
            self.resident_bytes = self.resident_bytes.saturating_sub(freed);
            if droppable {
                self.dirty_tiles.insert(coord);
            }
        }
    }

    /// A tile the viewport policy is happy to lose, or the coldest tile when there is no view.
    /// The policy never offers an active or visible tile while a far one exists; when it offers
    /// nothing, the coldest resident tile is taken so memory cannot run away.
    fn pick_victim(&self, keep: Option<(u32, u32)>) -> Option<(usize, (u32, u32))> {
        if let Some(residency) = self.residency.as_ref() {
            for coord in residency.eviction_order(&self.dirty_tiles) {
                if Some(coord) == keep {
                    continue;
                }
                for (index, layer) in self.layers.iter().enumerate() {
                    let Some(layer) = layer else { continue };
                    if let Some(tile) = layer.tiles.get(&coord)
                        && tile.gpu.is_some()
                        && !tile.stale
                        && !self.pinned.contains(&(index, coord))
                    {
                        return Some((index, coord));
                    }
                }
            }
        }
        let mut victim: Option<(usize, (u32, u32), u64)> = None;
        for (index, layer) in self.layers.iter().enumerate() {
            let Some(layer) = layer else { continue };
            for (coord, tile) in &layer.tiles {
                if tile.gpu.is_none()
                    || tile.stale
                    || Some(*coord) == keep
                    || self.pinned.contains(&(index, *coord))
                {
                    continue;
                }
                if victim.is_none_or(|(_, _, used)| tile.used < used) {
                    victim = Some((index, *coord, tile.used));
                }
            }
        }
        victim.map(|(index, coord, _)| (index, coord))
    }
}
