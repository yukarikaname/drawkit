# 8K residency: tiled layers

`crates/gpu/src/layers.rs` keeps the public `LayerStack` API but stores layers as tiles
(`crates/gpu/src/layers/stack.rs`). A full-canvas RGBA8 texture per layer was 132 MB at 7680×4320;
the tiled stack only holds the tiles that contain pixels.

## What is live

- Tile edges are `min(1024, canvas edge)`, so a narrow document does not allocate 1024-row tiles.
  An 8K canvas is 8×5 tiles at the usual aspect, with a 1024×1024 tile costing 4 MB.
- `set` splits a full layer into tile backups. Fully transparent tiles are dropped, so sparse line
  art costs a handful of tiles rather than the whole canvas.
- `update_rect` merges a dirty rectangle into the intersecting backups. A resident GPU tile is
  written in place; a cold tile stays in its backup until the compositor needs it.
- `composite_texture` materialises missing GPU tiles, composites one canvas tile at a time into a
  pair of reused render targets, and copies the result into the output texture at the tile origin.
  The targets and bind groups are reused for the whole frame instead of allocating two textures per
  tile.
- `set_budget` applies a byte budget (1.5 GB from the macOS surface). The least-recently-used GPU
  tile is dropped first; its CPU backup rehydrates it on the next composite, so the shell no longer
  re-uploads on eviction. The tile being rendered is never evicted.
- `thumbnail` reads the backups on the CPU. It does not materialise a full-canvas texture just to
  make a 64-pixel preview.

## What is still open

- The output is one full-canvas texture per `present`. The surface still samples a single painting
  texture, so this is not per layer, but it is one 132 MB allocation at 8K per present.
- Every present composites every non-empty tile. A dirty-tile cache could composite only the tiles
  the stroke touched and reuse the rest of the previous frame.
- Cold tiles stay in process memory. Disk backing for an 8K document with many layers is not
  implemented.
- Layers with groups or clipping still turn the GPU path off in the shell, as before.

The shell keeps the same `CanvasSurface` calls; only the store beneath them changed.
