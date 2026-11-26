# Brush latency

`cargo run --release -p drawkit-core --example bench_brush` paints a run of samples and composites
the dirty rectangle for each one, which is the CPU work a shell must finish before it can upload.
It excludes the GPU upload and the display present.

Measured on an Apple silicon Mac, release build, radius 18 px, 960 samples per size:

| Canvas | segment avg | p95 | p99 | full-canvas composite |
|---|---|---|---|---|
| 1920×1080 | 0.04 ms | 0.08 ms | 0.11 ms | 5.5 ms |
| 3840×2160 | 0.03 ms | 0.04 ms | 0.07 ms | 10.8 ms |

Each sample dirtied about 1,584 pixels, so the dirty-rectangle path is what keeps the budget: the
full canvas is measured only as a ceiling. Both ceilings stay under the 16 ms / 33 ms targets.
The shell should keep uploading only `take_dirty()`, not the whole surface.

## macOS end-to-end path

`DRAWKIT_BENCH=1 ./DrawKit` runs the real CanvasView path without synthetic mouse events: reset to
a 3840×3840 canvas, paint 240 samples, upload the dirty layer rectangle and present the Metal
surface, then print percentiles and quit.

On an Apple silicon Mac with a 1568×1346 drawable:

| Part | avg | p50 | p95 | p99 | max |
|---|---|---|---|---|---|
| stroke (core + refresh) | 0.23 ms | 0.17 ms | 0.48 ms | 0.98 ms | 1.49 ms |
| present (upload + composite + swapchain) | 0.90 ms | 0.21 ms | 1.53 ms | 3.12 ms | 123.2 ms |

The maximum is the first present, which allocates the surface and tiles; steady-state frames are
around 1.5 ms at p95. The benchmark intentionally bypasses the welcome sheet and the recovery
dialog, so it measures drawing rather than UI automation.

## Painting runtime benchmark

Two ignored tests print the table used to compare changes; they cover the CPU raster path and the
offscreen GPU compositor separately, and include the readbacks those paths do.

```sh
cargo test --release -p drawkit-core --test runtime -- --ignored --nocapture
cargo test --release -p drawkit-gpu  --test runtime -- --ignored --nocapture
```

Baseline on an Apple silicon Mac (the machine is shared, treat single runs as noisy):

| layer | canvas | operation | time |
|---|---|---|---|
| CPU | 2K | 200 strokes × 30 segments | 49.3 ms (0.25 ms/stroke) |
| CPU | 2K | undo all / redo all | 9.6 ms / 11.1 ms |
| CPU | 2K / 4K | one fill, tolerance 0.1, expand 1 | 101 ms / 386 ms |
| CPU | 2K / 4K | surface after 48 strokes, one layer | 62 MiB / 154 MiB |
| CPU | 2K | tape for the same strokes | 15 KiB (1440 events) |
| CPU | 1K | composite 100 layers | 54.5 ms/frame |
| GPU | 2K | composite 100 layers + readback | 29.7 ms/frame |
| GPU | 2K / 4K | stroke commit (30 segments, batch) + readback | 3.6 ms / 8.5 ms |
| GPU | 2K / 4K | fill apply, synthetic disk mask | 17 ms / 58 ms |

The fill numbers are the reason fill mask generation is the next CPU-to-GPU target; the stroke
commit numbers include the tile readback the benchmark asks for and are not the live path.

## Stress runs

`DRAWKIT_BENCH_STRESS=1 ./DrawKit` drives the real mouse handlers with random strokes and, with
`DRAWKIT_BENCH_STRESS_MIX=1`, cycles every brush, eraser, smudge, fill and undo/redo. It samples
`phys_footprint` and Metal `currentAllocatedSize` after each operation.

| variable | meaning |
|---|---|
| `DRAWKIT_BENCH_SIZE` | canvas side (default 3840) |
| `DRAWKIT_BENCH_STRESS_STROKES` / `_SECONDS` | budget; whichever comes first |
| `DRAWKIT_BENCH_STRESS_MIX` | cycle brushes and tools instead of ink only |
| `DRAWKIT_BENCH_STRESS_LAYERS` | add a layer every N strokes |
| `DRAWKIT_BENCH_STRESS_NOPRESENT` | skip the per-sample present, keep one per stroke |
| `DRAWKIT_BENCH_STRESS_SETTLE_MS` | run-loop settle after each operation (default 1 ms) |

What it measured on an Apple M6, 16 GB, release bundle (after the layer and fill work below):

| run | ops | per op | memory peak |
|---|---|---|---|
| 2048, ink only, no per-sample present | 400 in 1.6 s | 3.6 ms p50 | 998 MB |
| 4096, ink only, no per-sample present | 400 in 8.5 s | 22.6 ms p50 (commit present ~11 ms) | 1915 MB |
| 4096, ink, a layer every 10 | 400 in 6.5 s | 23.4 ms p50, 33 ms p95 | 1410 MB (6 resident) |
| 2048, mix, no per-sample present | 222 in 10 s | see below | ~1400 MB |
| 4096, mix, no per-sample present | 162 in 12 s | see below | 3520 MB |

Mix, 4096: brushes 12–27 ms p50, fill 208 ms, layer add 54 ms, undo+redo 8.6 ms. Mix, 2048:
layer add 34 ms, remove 16.5 ms, move 14.5 ms, paste 58 ms, canvas resize 169 ms p50.

The synthetic loop is display-paced when presents are on: one present per pointer sample costs
8–12 ms, so the run reports about 5 operations per second regardless of paint cost.

## Many layers

A full frame with 32 visible fill layers, composited into a texture with no readback
(`cargo test --release -p drawkit-gpu --test runtime many_visible_layers -- --ignored --nocapture`):

| canvas | full frame | nothing dirty |
|---|---|---|
| 2048×2048 | 23.7 ms | 0 ms |
| 4096×4096 | 75.9 ms | 0 ms |

That is the all-tiles-dirty cost (open, reorder, canvas resize). A stroke dirties one or two
tiles, so a live frame composites 32 layers for those tiles only. There is no below/above region
cache yet; the number above is why the composite model is on the architecture list.

## Thirty-two painted layers

`DRAWKIT_BENCH_STRESS_BASE_LAYERS=32` paints one stroke on each of 32 layers, then draws on the
top one. Real content, not fill specs. After the backup, frame-preview and CPU-cache work:

| document | footprint peak | Metal peak |
|---|---|---|
| 4K, empty | 707 MB | 211 MB |
| 4K, 32 painted layers | 3179 MB | 941 MB |
| 8K, empty | 1605 MB | 803 MB |
| 8K, 8 painted layers | 4948 MB | 1435 MB |
| 8K, 32 painted layers | 8604 MB | 2416 MB |

Compute is not the problem: one present composites every visible layer for the stroke's dirty
tiles and costs 6.3 ms at 4K, 5.6 ms at 24 MP, with 32 layers.

Memory is still the gap. An empty 8K document used to cost 2.83 GB and now costs 1.60 GB: the
biggest single item was the CPU composite, allocated at document size for a GPU-only canvas, and
a full-canvas CGImage per animation frame for a 44 px strip. What remains:

- the GPU stack's 1024 px tiles allocate 4 MB for a thin stroke; a 256 px tile would need a
  batched compositor first, because one pass per layer per tile already makes a 4K/32-layer full
  frame 8192 passes;
- the tile journal keeps old textures for undo up to 1.5 GB, and cannot be trimmed without
  trimming the core history in step;
- a full-width stroke at 8K still adds about 250 MB of live and transient tile storage (core
  tiles, committed readback tiles, preview scratch), which macOS does not return to the system
  between strokes.

## What the stress runs changed

1. **The fill plan is no longer the cost.** The flood and morphology now run on byte masks with a
   separable square morphology and a monotonic-deque grow: 66 → 7 ms at 2048 and 230 → 28 ms at
   4096. The GPU blend stays 6 / 22 ms. An end-to-end 4K fill fell from 460 to 208 ms; the rest is
   the full-canvas composite frame (268 MB) the plan reads plus the mask and the tile upload.
2. **Layer structure edits are incremental.** Add/remove/move keep the other layers' textures;
   the shell no longer resets the stack. Layer add fell from 445 to 36 ms at 4096, the longest
   stall from 1.36 s to 60 ms, and a group or clip reference that cannot be renumbered still
   falls back to a rebuild.
3. **Undo reports its rectangles.** `undo_tiles_rects` / `redo_tiles_rects` return the swapped
   tiles, so the shell can update those and not the layer. Undo itself is a texture swap.
4. **Memory is still duplicated.** A painted 4K layer costs about 235 MB in the process because
   the GPU stack keeps an f32 tile backup next to the core's own sparse surface. 18 painted
   layers peaked at 4.9 GB. Removing that duplication is on the list.
