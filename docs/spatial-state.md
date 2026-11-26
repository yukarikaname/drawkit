# The spatial state runtime

This is the layer under the painting runtime that answers "what changed, and what depended on
it?" without comparing pixels. Right now only the counters exist. Nothing reads them yet, and
nothing about painting behaves differently.

## Tile revision

`Surface` (`crates/core/src/surface.rs`) is a tile grid, `TILE` pixels on a side. Every `Tile`
carries a `u64 revision`. A write operation that covers a tile bumps that tile's revision by one;
a read never does. Allocation is not a change either: a tile that was created but not written has
revision zero, and a missing tile reports zero. Revisions are compared and ordered, not treated as
timestamps; they wrap with `wrapping_add(1)` rather than saturating.

The bump lives in `bump_tiles` in `crates/core/src/surface/revision.rs` and runs once per
operation, after the pixels are written. `set` covers one tile. `blend_mask_at`,
`blend_mask_under_at`, `blend_mask_locked_at`, `erase_mask_at`, `blend_stroke_mask_at`,
`write_pixels`, `write_pixels_from_gpu`, `restore` and `resized` cover the whole rectangle they
touched and bump each tile in it once, not once per pixel. Import paths (`decode_layer` in
`crates/core/src/layer_png.rs`) and `Editor::clear_at` (`crates/core/src/tools/io.rs`) go through
`set` a pixel at a time, so there one call is one operation.

Two questions are available:

- `surface.tile_revision(x, y)` - the revision of the tile holding canvas pixel `(x, y)`, or zero
  when the tile is missing or outside the canvas.
- `surface.tile_rect_revision(rect)` - the maximum revision over the tiles the rect covers, or
  zero for an empty rect. This is the content version of a region.

## Region dependency

A temporal brush (smudge, wet) does not only depend on where it is drawn; it reads the composite
of a region as it was before the stroke. A region dependency is that pair: a rectangle plus the
revision it was read at. If the revision still matches, the cached backdrop is valid; if it moved,
the backdrop has to be rebuilt before the next segment or replay.

The foundation is `tile_rect_revision`: a dependency can be recorded as `(rect, revision)` with no
extra state in `Surface`, and checked later against the live surface with one call.

## Checkpoint

A checkpoint is the generation of each surface plus every tile revision at a moment in time. The
generation (`surface.generation()`) counts write operations on a surface, so comparing generations
is the cheap first test: if they are equal, no tile changed and the per-tile walk is unnecessary.
If they differ, only the tiles whose revisions differ have to be saved, restored or replayed.

This is what lets `Recording` (`crates/core/src/record.rs`) or the GPU tape replay
(`TapeReplay`, described in `docs/stroke-engine.md` and built over `GpuCompositor` in
`crates/gpu/src/compositor.rs`) skip work without walking the canvas.

## Incremental invalidation

The alternative to revisions is walking pixels to find out whether a region changed. Comparing a
stored revision with `tile_rect_revision` answers the same question in the number of tiles under
the rect. The intended shape is:

1. record `(rect, revision)` when a consumer reads or caches a region;
2. before reusing the cache, ask `tile_rect_revision(rect)` again;
3. equal - reuse; different - reprocess only that region and record the new revision.

An over-eager bump is harmless (a region is reprocessed). A missed bump is not (a stale cache is
reused), so the rule is to bump on every write operation, even one that rewrites the same colour.

## What is not built yet

- No dependency storage. Nothing records a `(rect, revision)` pair, and no brush consults one.
- No temporal or smudge brush that reads earlier content through a region dependency.
- No checkpoint type, no checkpoint storage, no tape splicing or local history edit built on it.
- No GPU-side revision counters. The GPU still learns what to upload from `gpu_dirty`.
- `Editor` (`crates/core/src/tools.rs`) does not expose revisions in its API or FFI, and history
  still restores rectangles rather than comparing revisions.
- Revisions are per surface, not per composited view: a change in a lower layer is not visible in
  the revision of a layer above it.

When a dependency or checkpoint is added, it should compare these counters instead of introducing
a second notion of dirtiness.
