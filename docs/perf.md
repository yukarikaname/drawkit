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
