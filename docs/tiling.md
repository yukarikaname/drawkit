# 8K residency: from layer budget to tiles

The first step is in `crates/gpu/src/layers.rs`: a `LayerStack` has a byte budget (1.5 GB from the
macOS surface), keeps the least-recently-used order, drops whole layer textures when an upload
pushes it over, and the shell re-uploads a missing layer before present. That removes the
unbounded "one full texture per layer" allocation, but a single opaque 8K layer is still 256 MB of
RGBA8 and a cold layer is all-or-nothing.

The next step is a tile stack.

## Target

- Tile size 1024 (8K is 64 tiles, 512 would be 256 and too many draws).
- One texture per non-empty tile, straight RGBA8, plus a CPU backup of the same tile.
- LRU per tile, not per layer. Dropping a GPU tile keeps its CPU bytes; compositing can rehydrate
  it from the backup without the shell.
- Full canvas output texture remains, because the surface samples one painting texture.
- Composite one output tile at a time: collect each layer's tile at that coordinate (or a fill
  source), call `composite_sources_to_texture` at the tile size, copy the result into the output
  at the tile origin.
- Layers with groups or clipping continue on the existing full-layer path; the canvas already
  turns the GPU path off for those.
- Evict a tile only after the present that used it; never evict the tile the current stroke is
  writing.

## Costs

For 8K, a tile is 1024×1024×4 = 4 MB. A 1.5 GB budget is ~375 resident tiles, enough for about six
fully painted layers. Sparse line art uses far fewer. The per-tile `composite_sources_to_texture`
currently allocates its two targets per call; the first implementation should reuse a tile target
pair so 64 tiles do not allocate 128 full-size resources per frame.

## API sketch

```text
TiledLayerStack
  set(index, pixels)                  // split into tiles, keep backups
  update_rect(index, rect, pixels)    // write only intersecting tiles
  composite_texture(specs) -> Texture // per-tile passes into one output
  thumbnail(index, w, h)              // materialize a full scratch layer, then downsample
  set_budget(bytes)                   // LRU over GPU tiles
```

The shell keeps the same `CanvasSurface` calls; only the store changes. The fallback is the
current full-layer stack, selected when the canvas is small or the tile path is disabled.
