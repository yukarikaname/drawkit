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
| stroke (core + refresh) | 0.13 ms | 0.10 ms | 0.25 ms | 0.32 ms | 0.70 ms |
| present (upload + composite + swapchain) | 0.75 ms | 0.31 ms | 0.76 ms | 1.40 ms | 93.8 ms |

The maximum is the first present, which allocates the surface and tiles; steady-state frames stay
under a millisecond. The benchmark intentionally bypasses the welcome sheet and the recovery
dialog, so it measures drawing rather than UI automation.
