//! Apply the pixels a GPU stroke committed to the resident layer tiles. The CPU surface keeps the
//! same pixels for history, export and replay, and the stroke never stamps the brush again.

use super::*;

/// One undo step assembled from the non-overlapping tile rectangles a commit returned.
#[derive(Clone)]
pub(super) struct TileCommit {
    layer: usize,
    /// The stroke group the commit belongs to, and its tape position. `None` for a commit with no
    /// recorded stroke, which cannot be recoloured or deleted later.
    group: Option<u64>,
    tape_index: Option<u64>,
    /// The deferred raster the commit replaced. An empty commit hands it back so the shell can
    /// still fall back to the CPU replay.
    pending: Option<PendingStroke>,
}

/// Split a rectangle into blocks the size of a core tile, in surface coordinates. The conversion
/// buffers then stay one tile large instead of one GPU rect large.
fn blocks(rect: Rect) -> impl Iterator<Item = (usize, usize, usize, usize)> {
    let step = crate::surface::TILE;
    (0..rect.height).step_by(step).flat_map(move |y| {
        (0..rect.width).step_by(step).map(move |x| {
            (
                rect.x + x,
                rect.y + y,
                step.min(rect.width - x),
                step.min(rect.height - y),
            )
        })
    })
}

impl Editor {
    /// Prepare to collect committed tiles. Returns false for a brush whose GPU preview is not the
    /// stroke the CPU would have painted, so the shell keeps the sliced replay.
    pub fn begin_tile_commit(&mut self) -> bool {
        self.cancel_prediction();
        self.predictor.clear();
        self.smoothed = None;
        // The GPU tiles already hold the stroke. Rasterising the recorded segments here would
        // paint the same pixels a second time with the CPU brush and push a second history step.
        let pending = self.pending_stroke.take();
        let stroke = self.stroke.take();
        let (layer, group, tape_index) = match stroke {
            Some(session) => (
                session.layer,
                Some(session.group),
                self.group_tape_index(session.group),
            ),
            None => pending
                .as_ref()
                .map_or(
                    (self.document.active, None, None),
                    |pending| match pending.writer {
                        Some((group, tape_index)) => (pending.layer, Some(group), Some(tape_index)),
                        None => (pending.layer, None, None),
                    },
                ),
        };
        if let Some(target) = self.document.layers.get_mut(layer) {
            target.surface.end_stroke_accumulation();
        }
        self.tile_commit = Some(TileCommit {
            layer,
            group,
            tape_index,
            pending,
        });
        true
    }

    /// Apply pixels materialised from the GPU into the CPU cache. The GPU journal already holds
    /// the history step, so this never pushes a new one.
    pub fn apply_tiles(&mut self, layer: usize, rect: Rect, pixels: &[u8]) -> bool {
        let expected = rect.width * rect.height * 4;
        if rect.is_empty() || pixels.len() < expected {
            return false;
        }
        let Some(target) = self.document.layers.get_mut(layer) else {
            return false;
        };
        // Convert one core tile at a time. A whole 1024 px GPU tile is 16 MB of f32 at 4K, and a
        // stroke row touches several of them; the old whole-rect copy left that much behind in
        // the allocator for every stroke.
        for (x, y, width, height) in blocks(rect) {
            let mut colors = Vec::with_capacity(width * height);
            for row in y..y + height {
                for column in x..x + width {
                    let at = ((row - rect.y) * rect.width + (column - rect.x)) * 4;
                    colors.push([
                        f32::from(pixels[at]) / 255.0,
                        f32::from(pixels[at + 1]) / 255.0,
                        f32::from(pixels[at + 2]) / 255.0,
                        f32::from(pixels[at + 3]) / 255.0,
                    ]);
                }
            }
            target
                .surface
                .write_pixels_from_gpu(Rect::new(x, y, width, height), &colors);
        }
        true
    }

    /// Apply linear float tiles from an extended-range GPU commit. The surfaces store encoded
    /// working RGB, so each pixel goes back through the inverse matrix and the transfer curve.
    pub fn apply_tiles_linear(&mut self, layer: usize, rect: Rect, pixels: &[f32]) -> bool {
        let expected = rect.width * rect.height * 4;
        if rect.is_empty() || pixels.len() < expected {
            return false;
        }
        let Some(target) = self.document.layers.get_mut(layer) else {
            return false;
        };
        let space = self.document.color_space;
        let inverse = space.from_display_p3();
        for (x, y, width, height) in blocks(rect) {
            let mut colors = Vec::with_capacity(width * height);
            for row in y..y + height {
                for column in x..x + width {
                    let at = ((row - rect.y) * rect.width + (column - rect.x)) * 4;
                    let encoded = |channel: usize| {
                        let linear = inverse[channel][0] * pixels[at]
                            + inverse[channel][1] * pixels[at + 1]
                            + inverse[channel][2] * pixels[at + 2];
                        space.encode(linear.max(0.0))
                    };
                    colors.push([
                        encoded(0),
                        encoded(1),
                        encoded(2),
                        pixels[at + 3].clamp(0.0, 1.0),
                    ]);
                }
            }
            target
                .surface
                .write_pixels_from_gpu(Rect::new(x, y, width, height), &colors);
        }
        true
    }

    /// Close a GPU commit: the pixels stay in the resident tiles and the journal is their undo
    /// record, so the history only keeps the touched rectangles.
    pub fn end_gpu_commit(&mut self, rects: &[Rect]) -> bool {
        let Some(commit) = self.tile_commit.take() else {
            return false;
        };
        if let Some(layer) = self.document.layers.get_mut(commit.layer) {
            layer.surface.end_stroke_accumulation();
            // The GPU reports the tiles it actually wrote as rectangles. They are the stroke's
            // area: a partial tile at the edge of a rect is included, because the commit wrote
            // into it.
            if let (Some(group), Some(tape_index)) = (commit.group, commit.tape_index) {
                for &rect in rects {
                    layer.surface.record_writer(group, tape_index, rect);
                }
            }
        }
        if rects.is_empty() {
            self.pending_stroke = commit.pending;
            return false;
        }
        self.record_gpu_step(commit.layer, rects);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INK: Rgba = [0.0, 0.0, 0.0, 1.0];

    fn paint(editor: &mut Editor, from: (f32, f32), to: (f32, f32), radius: f32) -> u64 {
        editor.begin_stroke(0);
        editor.stroke_at(from, to, radius, 1.0, INK, 0);
        editor.end_stroke();
        editor
            .recording
            .events()
            .last()
            .and_then(|event| event.group)
            .expect("the stroke was recorded")
    }

    #[test]
    fn overlapping_strokes_share_the_tile_and_the_newest_writer_wins() {
        let mut editor = Editor::new(1024, 1024, 1 << 20);
        let first = paint(&mut editor, (100.0, 100.0), (400.0, 100.0), 20.0);
        let second = paint(&mut editor, (300.0, 60.0), (300.0, 160.0), 20.0);

        let surface = &editor.document.layers[0].surface;
        let shared = (1, 0);
        assert!(surface.tiles_for_group(first).unwrap().contains(&shared));
        assert!(surface.tiles_for_group(second).unwrap().contains(&shared));
        assert_eq!(surface.newest_writer(shared), Some((second, 1)));
    }

    #[test]
    fn a_gpu_commit_and_the_cpu_stroke_describe_the_same_tiles() {
        // One short segment straddling the tile corner at (256, 256) so the shared tiles are
        // unambiguous, and neither path may pad the list with neighbours.
        let (from, to, radius) = ((254.0, 254.0), (258.0, 258.0), 4.0);
        let mut cpu = Editor::new(3840, 2160, 1 << 22);
        let cpu_group = paint(&mut cpu, from, to, radius);
        let mut cpu_tiles = cpu.document.layers[0]
            .surface
            .tiles_for_group(cpu_group)
            .expect("the CPU stroke")
            .to_vec();

        let mut gpu = Editor::new(3840, 2160, 1 << 22);
        gpu.defer_raster = true;
        gpu.begin_stroke(0);
        gpu.stroke_at(from, to, radius, 1.0, INK, 0);
        gpu.end_stroke();
        let gpu_group = gpu
            .recording
            .events()
            .last()
            .and_then(|event| event.group)
            .expect("the GPU stroke was recorded");
        let bounds = gpu.stroke_bounds(gpu_group).expect("bounds");
        let rects: Vec<Rect> = blocks(bounds)
            .map(|(x, y, width, height)| Rect::new(x, y, width, height))
            .collect();
        assert!(gpu.begin_tile_commit());
        assert!(gpu.end_gpu_commit(&rects));
        let mut gpu_tiles = gpu.document.layers[0]
            .surface
            .tiles_for_group(gpu_group)
            .expect("the GPU commit")
            .to_vec();

        cpu_tiles.sort_unstable();
        gpu_tiles.sort_unstable();
        assert_eq!(
            cpu_tiles.len(),
            4,
            "the CPU stroke should straddle a corner"
        );
        assert_eq!(gpu_tiles, cpu_tiles);
        assert_eq!(
            gpu.document.layers[0].surface.newest_writer((1, 0)),
            Some((gpu_group, 0))
        );
    }

    #[test]
    fn a_deferred_stroke_records_its_group_when_the_replay_finishes() {
        let mut editor = Editor::new(1024, 1024, 1 << 20);
        editor.defer_raster = true;
        editor.begin_stroke(0);
        editor.stroke_at((100.0, 100.0), (400.0, 100.0), 10.0, 1.0, INK, 0);
        editor.end_stroke();
        let group = editor
            .recording
            .events()
            .last()
            .and_then(|event| event.group)
            .expect("the deferred stroke was recorded");
        assert!(
            editor.document.layers[0]
                .surface
                .tiles_for_group(group)
                .is_none(),
            "a writer was recorded before the replay ran"
        );
        while !editor.advance_deferred(8) {}
        assert!(
            editor.document.layers[0]
                .surface
                .tiles_for_group(group)
                .is_some(),
            "the finished replay left no writer entry"
        );
    }

    #[test]
    fn painting_another_layer_does_not_add_writers_to_this_one() {
        let mut editor = Editor::new(512, 512, 1 << 20);
        let first = paint(&mut editor, (10.0, 10.0), (100.0, 100.0), 5.0);
        let before = editor.document.layers[0]
            .surface
            .tiles_for_group(first)
            .expect("the first stroke")
            .to_vec();

        let second_layer = editor.document.add_layer("second");
        let second = paint(&mut editor, (200.0, 200.0), (400.0, 400.0), 8.0);

        assert_eq!(
            editor.document.layers[0]
                .surface
                .tiles_for_group(first)
                .unwrap(),
            before.as_slice(),
            "the first layer's writer map changed"
        );
        assert!(
            editor.document.layers[0]
                .surface
                .tiles_for_group(second)
                .is_none(),
            "the second layer's stroke leaked into the first"
        );
        assert!(
            editor.document.layers[second_layer]
                .surface
                .tiles_for_group(second)
                .is_some()
        );
    }
}
