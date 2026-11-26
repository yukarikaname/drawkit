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
