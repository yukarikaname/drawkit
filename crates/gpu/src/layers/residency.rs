//! Which tiles deserve a GPU texture right now.
//!
//! Edited tiles are active, tiles under the viewport visible, the ring around it a preload
//! neighbour and the rest can stay on the CPU. The ranking is pure and deterministic.

use std::collections::HashSet;

/// How badly a tile wants to stay resident. The order is the eviction order, least wanted first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TilePriority {
    Active,
    Visible,
    Neighbour,
    Far,
}

/// A tile's rectangle in canvas pixels. Edge tiles are clipped to the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl TileRect {
    fn right(&self) -> u32 {
        self.x + self.width
    }

    fn bottom(&self) -> u32 {
        self.y + self.height
    }
}

/// The viewport in canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ViewRect {
    fn new(mut x: f32, mut y: f32, mut width: f32, mut height: f32) -> Self {
        if width < 0.0 {
            x += width;
            width = -width;
        }
        if height < 0.0 {
            y += height;
            height = -height;
        }
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn right(&self) -> f32 {
        self.x + self.width
    }

    fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn expanded(&self, by: f32) -> Self {
        Self {
            x: self.x - by,
            y: self.y - by,
            width: self.width + 2.0 * by,
            height: self.height + 2.0 * by,
        }
    }

    /// A strict overlap, so a tile that only shares an edge is outside.
    fn intersects(&self, tile: TileRect) -> bool {
        tile.width > 0
            && tile.height > 0
            && self.width > 0.0
            && self.height > 0.0
            && self.x < tile.right() as f32
            && self.right() > tile.x as f32
            && self.y < tile.bottom() as f32
            && self.bottom() > tile.y as f32
    }

    /// The shortest gap to the tile, zero when they overlap.
    fn gap(&self, tile: TileRect) -> f32 {
        let dx = (self.x - tile.right() as f32)
            .max(tile.x as f32 - self.right())
            .max(0.0);
        let dy = (self.y - tile.bottom() as f32)
            .max(tile.y as f32 - self.bottom())
            .max(0.0);
        dx.hypot(dy)
    }
}

/// The suggested split of a byte budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub struct BudgetSplit {
    /// Bytes the current residents may keep.
    pub keep: usize,
    /// Bytes left for neighbour preloading.
    pub preload: usize,
    /// Bytes the eviction order must free when the residents are already over budget.
    pub release: usize,
}

/// A residency ranking for one canvas, viewport and zoom.
pub struct TileResidency {
    canvas: (u32, u32),
    tile: u32,
    columns: u32,
    rows: u32,
    view: ViewRect,
    zoom: f32,
    margin: u32,
}

impl TileResidency {
    /// `canvas` and `tile` are canvas pixels, `view` is `(x, y, width, height)` in canvas pixels,
    /// and `zoom` is device pixels per canvas pixel. The margin starts at one tile.
    pub fn new(canvas: (u32, u32), tile: u32, view: (f32, f32, f32, f32), zoom: f32) -> Self {
        let tile = tile.max(1);
        let (x, y, width, height) = view;
        let zoom = if zoom.is_finite() && zoom > 0.0 {
            zoom
        } else {
            1.0
        };
        Self {
            canvas,
            tile,
            columns: canvas.0.div_ceil(tile),
            rows: canvas.1.div_ceil(tile),
            view: ViewRect::new(x, y, width, height),
            zoom,
            margin: 1,
        }
    }

    /// How many tiles around the viewport are preloaded. Zero disables preloading.
    #[cfg(test)]
    pub fn with_margin(mut self, tiles: u32) -> Self {
        self.margin = tiles;
        self
    }

    fn rect(&self, (column, row): (u32, u32)) -> Option<TileRect> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        let x = column * self.tile;
        let y = row * self.tile;
        Some(TileRect {
            x,
            y,
            width: self.tile.min(self.canvas.0 - x),
            height: self.tile.min(self.canvas.1 - y),
        })
    }

    /// Rank one tile. Coordinates outside the canvas are far.
    pub fn priority(&self, tile: (u32, u32), edited: &HashSet<(u32, u32)>) -> TilePriority {
        let Some(rect) = self.rect(tile) else {
            return TilePriority::Far;
        };
        if edited.contains(&tile) {
            return TilePriority::Active;
        }
        if self.view.intersects(rect) {
            return TilePriority::Visible;
        }
        let halo = self.view.expanded(self.margin as f32 * self.tile as f32);
        if self.margin > 0 && halo.intersects(rect) {
            return TilePriority::Neighbour;
        }
        TilePriority::Far
    }

    /// Every tile of the canvas, row by row.
    pub fn rank(&self, edited: &HashSet<(u32, u32)>) -> Vec<((u32, u32), TilePriority)> {
        (0..self.rows)
            .flat_map(|row| (0..self.columns).map(move |column| (column, row)))
            .map(|tile| (tile, self.priority(tile, edited)))
            .collect()
    }

    /// Tiles to drop, farthest first: every far tile, then the neighbours farthest from the
    /// viewport. Active and visible tiles are never in the order.
    pub fn eviction_order(&self, edited: &HashSet<(u32, u32)>) -> Vec<(u32, u32)> {
        let mut victims: Vec<((u32, u32), TilePriority, f32)> = self
            .rank(edited)
            .into_iter()
            .filter_map(|(tile, priority)| match priority {
                TilePriority::Active | TilePriority::Visible => None,
                _ => {
                    let distance =
                        self.rect(tile).map_or(f32::INFINITY, |r| self.view.gap(r)) * self.zoom;
                    Some((tile, priority, distance))
                }
            })
            .collect();
        victims.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.total_cmp(&a.2))
                .then_with(|| a.0.1.cmp(&b.0.1))
                .then_with(|| a.0.0.cmp(&b.0.0))
        });
        victims.into_iter().map(|(tile, _, _)| tile).collect()
    }

    /// Keep what is resident and spend the remainder on neighbours. When the residents are
    /// already over budget, `release` says how much the eviction order has to free.
    #[cfg(test)]
    pub fn budget_split(&self, resident_bytes: usize, budget_bytes: usize) -> BudgetSplit {
        let keep = resident_bytes.min(budget_bytes);
        BudgetSplit {
            keep,
            preload: budget_bytes - keep,
            release: resident_bytes - keep,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TilePriority::{Active, Far, Neighbour, Visible};

    fn policy(view: (f32, f32, f32, f32)) -> TileResidency {
        TileResidency::new((1024, 1024), 256, view, 1.0)
    }

    #[test]
    fn only_tiles_under_the_viewport_are_visible() {
        let p = policy((300.0, 300.0, 100.0, 100.0)).with_margin(0);
        assert_eq!(p.priority((1, 1), &HashSet::new()), Visible);
        assert_eq!(p.priority((0, 1), &HashSet::new()), Far);
        assert_eq!(p.priority((2, 1), &HashSet::new()), Far);

        // a tile that only shares an edge with the viewport stays outside
        let edge = policy((0.0, 0.0, 512.0, 512.0)).with_margin(0);
        assert_eq!(edge.priority((1, 1), &HashSet::new()), Visible);
        assert_eq!(edge.priority((2, 0), &HashSet::new()), Far);
        assert_eq!(edge.priority((0, 2), &HashSet::new()), Far);
    }

    #[test]
    fn the_margin_turns_the_ring_into_neighbours() {
        let p = policy((300.0, 300.0, 100.0, 100.0));
        assert_eq!(p.priority((1, 1), &HashSet::new()), Visible);
        assert_eq!(p.priority((0, 0), &HashSet::new()), Neighbour);
        assert_eq!(p.priority((2, 2), &HashSet::new()), Neighbour);
        assert_eq!(p.priority((3, 3), &HashSet::new()), Far);
        assert_eq!(p.with_margin(0).priority((0, 0), &HashSet::new()), Far);
    }

    #[test]
    fn edited_tiles_are_active_even_when_far() {
        let p = policy((0.0, 0.0, 100.0, 100.0));
        assert_eq!(p.priority((0, 0), &HashSet::new()), Visible);
        assert_eq!(p.priority((0, 0), &HashSet::from([(0, 0)])), Active);
        assert_eq!(p.priority((3, 3), &HashSet::new()), Far);
        assert_eq!(p.priority((3, 3), &HashSet::from([(3, 3)])), Active);
    }

    #[test]
    fn eviction_drops_far_tiles_then_the_farthest_neighbours() {
        let p = TileResidency::new((1024, 512), 256, (0.0, 0.0, 128.0, 128.0), 1.0);
        let order = p.eviction_order(&HashSet::from([(3, 1)]));
        assert_eq!(order, vec![(3, 0), (2, 1), (2, 0), (1, 1), (1, 0), (0, 1)]);
    }

    #[test]
    fn an_eight_k_canvas_leaves_most_tiles_far() {
        let p = TileResidency::new((7680, 4320), 256, (0.0, 0.0, 512.0, 512.0), 0.25);
        let ranked = p.rank(&HashSet::new());
        let count = |wanted: TilePriority| ranked.iter().filter(|(_, p)| *p == wanted).count();
        assert_eq!(ranked.len(), 30 * 17);
        assert_eq!(count(Visible), 4);
        assert_eq!(count(Neighbour), 5);
        assert_eq!(count(Far), 30 * 17 - 9);
    }

    #[test]
    fn the_budget_goes_to_residents_before_preloading() {
        let p = policy((0.0, 0.0, 256.0, 256.0));
        let split = p.budget_split(600, 1000);
        assert_eq!((split.keep, split.preload, split.release), (600, 400, 0));
        let over = p.budget_split(1400, 1000);
        assert_eq!((over.keep, over.preload, over.release), (1000, 0, 400));
    }
}
