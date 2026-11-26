//! Which stroke group wrote which tile, and in which tape order.
//!
//! A later slice recolours or deletes one recorded stroke by re-rendering the tiles it touched
//! instead of the whole layer. That needs three facts, all recorded once when a stroke finishes:
//!
//! - the tile coordinates each group wrote, so an edit knows where to look;
//! - the tape index the group starts at, so two writers can be ordered even when their tile lists
//!   say nothing about who came later;
//! - the newest writer of every tile, so a tile can name the stroke that currently decides its
//!   pixels.
//!
//! Nothing is recorded per sample: a pointer stroke calls `record_writer` once from `end_stroke`
//! (or from the deferred/GPU commit that stands in for it), with the rectangle the stroke ended up
//! covering. A predicted preview segment can widen that rectangle by the tiles it briefly touched;
//! prediction is off by default, and the map over-approximating is safe (a later edit re-renders a
//! few more tiles, never too few).
//!
//! An eraser records through `extend_writer` instead, once per segment, because it is the one
//! command whose tiles come from a surface it modified without a finish step of its own. Extending
//! a group that is already listed unions the new tiles into the entry, so the map still holds one
//! entry and one newest-writer slot per stroke.
//!
//! `forget` is the other direction: a deleted stroke has no events left to replay, so its entry
//! goes and the tiles it held fall back to the newest remaining group, if any.
//!
//! A stroke with no tape group is skipped. `next_group` still numbers it, but a group with no
//! events cannot be re-rendered, so remembering its tiles would only tell a future edit where it
//! cannot go. That is the case when recording is disabled, or when `Surface` is written directly
//! rather than through `Editor`.
//!
//! The map is bounded by [`MAX_GROUPS`]. Each group owns exactly one entry, so the only growth is
//! the number of finished strokes; when the cap is reached the oldest group is dropped, its tiles
//! are cleared from the per-tile table, and its slot is reused. Strokes that touch many tiles are
//! the point, not a runaway: their area is what a later re-render has to cover.

use std::collections::{HashMap, VecDeque};

use super::{Rect, Surface, TILE};

/// A tile coordinate, column then row, in `TILE`-pixel units.
pub type TileCoord = (u32, u32);

/// Most groups one surface can remember. A document would need more than 65 535 finished strokes
/// on one layer before the oldest tile lists start to fall off.
pub const MAX_GROUPS: usize = u16::MAX as usize;

#[derive(Clone)]
struct WriterEntry {
    group: u64,
    tape_index: u64,
    tiles: Vec<TileCoord>,
}

/// The per-surface writer tables. Cloning clones the tile lists, which is what makes a history
/// snapshot and a layer copy independent.
#[derive(Clone)]
pub(crate) struct StrokeWriters {
    tiles_x: usize,
    /// group -> live slot in `entries`.
    index: HashMap<u64, u32>,
    /// Stable slots so the per-tile table can hold a slot number instead of a group id.
    entries: Vec<Option<WriterEntry>>,
    /// Slots freed by eviction, reused by the next group.
    free: Vec<u32>,
    /// Live slots in paint order, oldest first.
    order: VecDeque<u32>,
    /// The newest writer of each tile, as a slot. Empty until something is recorded.
    newest: Vec<Option<u32>>,
}

impl StrokeWriters {
    pub(crate) fn new(tiles: usize, tiles_x: usize) -> Self {
        Self {
            tiles_x,
            index: HashMap::new(),
            entries: Vec::new(),
            free: Vec::new(),
            order: VecDeque::new(),
            newest: vec![None; tiles],
        }
    }

    /// Remember one finished stroke. A group is recorded once; a second call for the same group is
    /// ignored so a duplicated finish cannot grow the table.
    fn record(&mut self, group: u64, tape_index: u64, tiles: &[TileCoord]) {
        if tiles.is_empty() || self.index.contains_key(&group) {
            return;
        }
        if self.order.len() >= MAX_GROUPS {
            self.evict_oldest();
        }
        let slot = self.allocate();
        for &(tx, ty) in tiles {
            let at = ty as usize * self.tiles_x + tx as usize;
            if let Some(cell) = self.newest.get_mut(at) {
                *cell = Some(slot);
            }
        }
        self.index.insert(group, slot);
        self.entries[slot as usize] = Some(WriterEntry {
            group,
            tape_index,
            tiles: tiles.to_vec(),
        });
        self.order.push_back(slot);
    }

    /// Add tiles to a group that is already listed, or record it like a finished stroke. The new
    /// tiles become the group's own and get it as their newest writer; a tile the group already
    /// holds is not added twice. The entry keeps the tape index it was first recorded with.
    fn extend(&mut self, group: u64, tape_index: u64, tiles: &[TileCoord]) {
        if tiles.is_empty() {
            return;
        }
        let Some(&slot) = self.index.get(&group) else {
            self.record(group, tape_index, tiles);
            return;
        };
        let tiles_x = self.tiles_x;
        let newest = &mut self.newest;
        let Some(entry) = self.entries.get_mut(slot as usize).and_then(Option::as_mut) else {
            return;
        };
        for &(tx, ty) in tiles {
            if entry.tiles.contains(&(tx, ty)) {
                continue;
            }
            entry.tiles.push((tx, ty));
            if let Some(cell) = newest.get_mut(ty as usize * tiles_x + tx as usize) {
                *cell = Some(slot);
            }
        }
    }

    /// Drop a group whose events are gone, and hand the tiles it held to the newest group still
    /// listed on them. A tile with no older writer falls back to `None`.
    fn forget(&mut self, group: u64) {
        let Some(slot) = self.index.remove(&group) else {
            return;
        };
        let Some(entry) = self.entries.get_mut(slot as usize).and_then(Option::take) else {
            return;
        };
        self.order.retain(|&queued| queued != slot);
        self.free.push(slot);
        for &tile in &entry.tiles {
            let newest = self.newest_for(tile);
            let at = tile.1 as usize * self.tiles_x + tile.0 as usize;
            if let Some(cell) = self.newest.get_mut(at) {
                *cell = newest;
            }
        }
    }

    /// The slot of the remaining group with the highest tape index on `tile`, if any.
    fn newest_for(&self, tile: TileCoord) -> Option<u32> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| {
                let entry = entry.as_ref()?;
                entry
                    .tiles
                    .contains(&tile)
                    .then_some((slot as u32, entry.tape_index))
            })
            .max_by_key(|&(_, tape_index)| tape_index)
            .map(|(slot, _)| slot)
    }

    fn allocate(&mut self) -> u32 {
        if let Some(slot) = self.free.pop() {
            return slot;
        }
        let slot = self.entries.len() as u32;
        self.entries.push(None);
        slot
    }

    /// Drop the oldest group and unlink it from the tiles it held. A tile whose newest writer is
    /// the dropped group falls back to "no writer", which is honest: the one still-listed group
    /// below it is not the newest writer of that tile any more.
    fn evict_oldest(&mut self) {
        let Some(slot) = self.order.pop_front() else {
            return;
        };
        let Some(entry) = self.entries[slot as usize].take() else {
            return;
        };
        self.index.remove(&entry.group);
        for &(tx, ty) in &entry.tiles {
            let at = ty as usize * self.tiles_x + tx as usize;
            if let Some(cell) = self.newest.get_mut(at)
                && *cell == Some(slot)
            {
                *cell = None;
            }
        }
        self.free.push(slot);
    }

    fn tiles(&self, group: u64) -> Option<&[TileCoord]> {
        let slot = *self.index.get(&group)?;
        self.entries
            .get(slot as usize)?
            .as_ref()
            .map(|entry| entry.tiles.as_slice())
    }

    fn newest_at(&self, at: usize) -> Option<(u64, u64)> {
        let slot = (*self.newest.get(at)?)?;
        let entry = self.entries.get(slot as usize)?.as_ref()?;
        Some((entry.group, entry.tape_index))
    }

    /// Approximate heap bytes the tables hold, for the 4K stroke measurement. The tile lists
    /// dominate; the hash map estimate counts a bucket for its key, value and control byte.
    fn bytes(&self) -> usize {
        let newest = self.newest.capacity() * std::mem::size_of::<Option<u32>>();
        let entries = self.entries.capacity() * std::mem::size_of::<Option<WriterEntry>>();
        let tiles: usize = self
            .entries
            .iter()
            .flatten()
            .map(|entry| entry.tiles.capacity() * std::mem::size_of::<TileCoord>())
            .sum();
        let index = self
            .index
            .capacity()
            .saturating_mul(std::mem::size_of::<u64>() + std::mem::size_of::<u32>() + 8);
        let order = self.order.capacity() * std::mem::size_of::<u32>();
        let free = self.free.capacity() * std::mem::size_of::<u32>();
        newest + entries + tiles + index + order + free
    }
}

impl Surface {
    /// Record that `group` painted every tile under `rect`, at tape position `tape_index`. Called
    /// once when a stroke finishes; the rectangle is the one the stroke dirtied, not one sample.
    pub(crate) fn record_writer(&mut self, group: u64, tape_index: u64, rect: Rect) {
        let tiles = self.rect_tiles(rect);
        if tiles.is_empty() {
            return;
        }
        self.writers.record(group, tape_index, &tiles);
    }

    /// Add the tiles under `rect` to `group`, or record them when it has no entry yet. An eraser
    /// calls this per segment: its segments are the same group, so the entry grows instead of
    /// being replaced.
    pub(crate) fn extend_writer(&mut self, group: u64, tape_index: u64, rect: Rect) {
        let tiles = self.rect_tiles(rect);
        if tiles.is_empty() {
            return;
        }
        self.writers.extend(group, tape_index, &tiles);
    }

    /// Forget a group whose tape events were removed. Its tiles fall back to the newest remaining
    /// writer, so a later edit's closure no longer expands through it.
    pub(crate) fn forget_writer(&mut self, group: u64) {
        self.writers.forget(group);
    }

    /// The tiles one recorded group wrote, in row-major order, or `None` when the surface has no
    /// entry for it. This is the region a recolour or delete has to re-render.
    pub fn tiles_for_group(&self, group: u64) -> Option<&[TileCoord]> {
        self.writers.tiles(group)
    }

    /// Every group this surface remembers and the tiles it wrote, oldest first. A stroke edit
    /// walks this once to find the groups that share a tile with the group being changed.
    pub fn writer_groups(&self) -> impl Iterator<Item = (u64, &[TileCoord])> + '_ {
        self.writers
            .entries
            .iter()
            .flatten()
            .map(|entry| (entry.group, entry.tiles.as_slice()))
    }

    /// The group and tape index that most recently wrote `tile`, where `tile` is
    /// `(column, row)` in `TILE`-pixel units. `None` for an untouched or dropped tile.
    pub fn newest_writer(&self, tile: TileCoord) -> Option<(u64, u64)> {
        if self.tiles_x == 0 {
            return None;
        }
        let (tx, ty) = tile;
        if tx as usize >= self.tiles_x || ty as usize >= self.tiles.len() / self.tiles_x {
            return None;
        }
        self.writers
            .newest_at(ty as usize * self.tiles_x + tx as usize)
    }

    /// Heap bytes the writer map holds. Kept separate from `allocated_bytes`: the map is bookkeeping
    /// a dormant history branch also carries, not pixel storage.
    pub fn writer_bytes(&self) -> usize {
        self.writers.bytes()
    }

    fn rect_tiles(&self, rect: Rect) -> Vec<TileCoord> {
        if rect.is_empty() || rect.x >= self.width || rect.y >= self.height {
            return Vec::new();
        }
        let x_end = (rect.x + rect.width).min(self.width);
        let y_end = (rect.y + rect.height).min(self.height);
        let first_x = rect.x / TILE;
        let last_x = (x_end - 1) / TILE;
        let first_y = rect.y / TILE;
        let last_y = (y_end - 1) / TILE;
        let columns = last_x - first_x + 1;
        let rows = last_y - first_y + 1;
        let mut tiles = Vec::with_capacity(columns * rows);
        for ty in first_y..=last_y {
            for tx in first_x..=last_x {
                tiles.push((tx as u32, ty as u32));
            }
        }
        tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    fn red() -> Rgba {
        [1.0, 0.0, 0.0, 1.0]
    }

    /// Paint through the public surface path so the extent the recording uses is exercised.
    fn paint(surface: &mut Surface, rect: Rect) -> Rect {
        surface.begin_stroke_accumulation();
        for (x, y) in [
            (rect.x, rect.y),
            (rect.x + rect.width - 1, rect.y + rect.height - 1),
        ] {
            surface.set(x, y, red());
        }
        surface
            .end_stroke_accumulation()
            .expect("the stroke dirtied")
    }

    #[test]
    fn two_strokes_share_a_tile_and_the_later_one_wins() {
        let mut surface = Surface::new(1024, 1024);
        let first = paint(&mut surface, Rect::new(100, 100, 60, 60));
        surface.record_writer(7, 0, first);
        let second = paint(&mut surface, Rect::new(140, 140, 60, 60));
        surface.record_writer(9, 1, second);

        assert!(surface.tiles_for_group(9).unwrap().contains(&(0, 0)));
        assert_eq!(surface.newest_writer((0, 0)), Some((9, 1)));
    }

    #[test]
    fn a_stroke_across_a_tile_corner_records_only_those_four_tiles() {
        let mut surface = Surface::new(3840, 2160);
        // One segment straddling the corner at (256, 256) writes all four neighbouring tiles.
        surface.begin_stroke_accumulation();
        surface.mark(Rect::new(250, 250, 12, 12));
        let extent = surface.end_stroke_accumulation().expect("an extent");
        surface.record_writer(3, 0, extent);

        let mut tiles = surface.tiles_for_group(3).unwrap().to_vec();
        tiles.sort_unstable();
        assert_eq!(tiles, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
        for tile in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            assert_eq!(surface.newest_writer(tile), Some((3, 0)));
        }
    }

    #[test]
    fn a_rect_away_from_a_boundary_does_not_touch_its_neighbours() {
        let mut surface = Surface::new(3840, 2160);
        surface.record_writer(5, 0, Rect::new(300, 300, 40, 40));
        assert_eq!(surface.tiles_for_group(5), Some(&[(1, 1)][..]));
        assert_eq!(surface.newest_writer((0, 1)), None);
        assert_eq!(surface.newest_writer((2, 1)), None);
    }

    #[test]
    fn an_untouched_tile_has_no_writer() {
        let surface = Surface::new(3840, 2160);
        assert_eq!(surface.newest_writer((0, 0)), None);
        assert_eq!(surface.tiles_for_group(1), None);
        assert_eq!(surface.newest_writer((15, 0)), None, "past the last column");
    }

    #[test]
    fn extending_a_group_unions_its_tiles_without_duplicates() {
        let mut surface = Surface::new(1024, 1024);
        let own = Rect::new(100, 100, 60, 60);
        surface.record_writer(7, 0, own);
        // A later group owns the tile the extension will claim next.
        surface.record_writer(9, 1, Rect::new(300, 300, 60, 60));
        assert_eq!(surface.newest_writer((1, 1)), Some((9, 1)));

        // The same segment twice must not add the tile a second time.
        surface.extend_writer(7, 0, own);
        surface.extend_writer(7, 0, Rect::new(300, 300, 60, 60));

        assert_eq!(
            surface.tiles_for_group(7),
            Some(&[(0, 0), (1, 1)][..]),
            "the extension repeated a tile or lost one"
        );
        assert_eq!(
            surface.newest_writer((1, 1)),
            Some((7, 0)),
            "the extended group is not the newest writer of the tile it added"
        );
        assert_eq!(surface.newest_writer((0, 0)), Some((7, 0)));
    }

    #[test]
    fn extending_an_unrecorded_group_records_it() {
        let mut surface = Surface::new(1024, 1024);
        surface.extend_writer(4, 5, Rect::new(300, 300, 60, 60));
        assert_eq!(surface.tiles_for_group(4), Some(&[(1, 1)][..]));
        assert_eq!(surface.newest_writer((1, 1)), Some((4, 5)));
    }

    #[test]
    fn forgetting_a_group_hands_its_tiles_to_an_older_overlapping_group() {
        let mut surface = Surface::new(1024, 1024);
        let shared = Rect::new(100, 100, 60, 60);
        surface.record_writer(3, 0, shared);
        surface.record_writer(8, 2, shared);
        assert_eq!(surface.newest_writer((0, 0)), Some((8, 2)));

        surface.forget_writer(8);
        assert_eq!(surface.tiles_for_group(8), None);
        assert_eq!(
            surface.newest_writer((0, 0)),
            Some((3, 0)),
            "the tile did not fall back to the older writer"
        );

        surface.forget_writer(3);
        assert_eq!(surface.tiles_for_group(3), None);
        assert_eq!(
            surface.newest_writer((0, 0)),
            None,
            "a tile only the forgotten groups held keeps a writer"
        );

        // Both slots went back to the free list and the order table is empty.
        assert_eq!(surface.writers.order.len(), 0);
        surface.record_writer(11, 4, shared);
        assert_eq!(surface.writers.order.len(), 1);
        assert_eq!(
            surface.writers.entries.len(),
            2,
            "a new group grew the table"
        );
        assert_eq!(surface.newest_writer((0, 0)), Some((11, 4)));
    }

    #[test]
    fn the_oldest_group_is_dropped_at_the_cap() {
        let mut surface = Surface::new(512, 512);
        for group in 0..(MAX_GROUPS as u64 + 2) {
            surface.record_writer(group, group, Rect::new(0, 0, 4, 4));
        }
        assert!(surface.tiles_for_group(0).is_none());
        assert!(surface.tiles_for_group(1).is_none());
        assert!(surface.tiles_for_group(2).is_some());
        assert_eq!(
            surface.newest_writer((0, 0)),
            Some((MAX_GROUPS as u64 + 1, MAX_GROUPS as u64 + 1)),
            "the last stroke still owns the tile"
        );
        // The cap holds and slots are reused, not appended forever.
        assert_eq!(surface.writers.order.len(), MAX_GROUPS);
        assert!(surface.writers.entries.len() <= MAX_GROUPS + 2);
    }

    #[test]
    fn two_hundred_strokes_of_twenty_tiles_stay_small_at_4k() {
        let mut surface = Surface::new(3840, 2160);
        // Five by four tiles: the area of a broad but ordinary stroke.
        let rect = Rect::new(256, 256, 5 * TILE, 4 * TILE);
        for group in 0..200 {
            surface.record_writer(group, group, rect);
        }
        let bytes = surface.writer_bytes();
        assert_eq!(surface.tiles_for_group(199).unwrap().len(), 20);
        assert!(
            bytes < 4 * 1024 * 1024,
            "200 strokes hold {bytes} bytes, more than a few MiB"
        );
        println!("writer map: {bytes} bytes for 200 strokes of 20 tiles at 4K");
    }
}
