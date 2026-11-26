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

What it measured on an Apple M6, 16 GB, release bundle:

| run | ops | per op | memory peak |
|---|---|---|---|
| 2048, ink only, no per-sample present | 400 in 1.6 s | 3.6 ms p50 | 998 MB |
| 4096, ink only, no per-sample present | 400 in 8.5 s | 22.6 ms p50 (commit present ~11 ms) | 1915 MB |
| 4096, mix, no per-sample present | 207 in 12.3 s | see below | 2840 MB |
| 4096, ink, a layer every 10 | 171 in 15 s | 17.7 ms p50, 672 ms p95 | 4884 MB (18 layers) |

Mix, 4096: every brush except ink is 12–26 ms p50. Fill is 461 ms p95 478, undo+redo 11 ms, and
the ink stroke that follows undo is 109 ms against 23 ms for the same brush elsewhere.

Three costs stand out, and none of them is the GPU blend:

1. **Fill is the CPU mask.** The flood plan alone is 66 ms at 2048 and 230 ms at 4096 for a
   full-canvas fill (`cargo test --release -p drawkit-core --test runtime a_fill_plan_at_document_size`).
   The GPU blend is 6 / 22 ms. The plan allocates and scans one f32 per document pixel.
2. **Undo rebuilds the upload.** `undoTiles` swaps the journal textures, then `gpuStale` makes the
   next present upload the layer again: the stroke after undo costs about 85 ms more at 4096.
3. **A layer add re-uploads the stack.** `gpuLayerCount` changes and the shell resets the stack,
   so with 18 layers one present is 1.35 s. Memory grows by about 268 MB per 4K layer because the
   CPU cache is dense f32 per layer.

The synthetic loop is display-paced when presents are on: one present per pointer sample costs
8–12 ms, so the run reports about 5 operations per second regardless of paint cost.
