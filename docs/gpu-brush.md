# GPU stroke preview

The macOS canvas now has a live GPU stroke path:

1. Pointer down switches the core to `defer_raster`: the brush records its tape events but does not
   paint the CPU surface.
2. `CanvasSurface` owns a coverage buffer and a compute pipeline. Each pointer sample accumulates
   stamp coverage in that buffer on the GPU; the buffer is never read back during the drag.
3. The paper shader overlays the coverage colour while presenting the resident layer stack.
4. On pointer up the core replays the recorded group into the CPU surface once, records one history
   step, and the shell rebuilds the resident layer texture from the final pixels.

Measured by `DRAWKIT_BENCH=1` on a 3840×3840 canvas with 240 samples (`DRAWKIT_BENCH_CPU=1`
forces the CPU path):

| path | stroke avg | stroke p95 | present avg | present p95 | pen-up commit |
|---|---|---|---|---|---|
| GPU preview | 0.10 ms | 0.27 ms | 0.53 ms | 0.42 ms | ~25 ms in 30 slices, max 3.6 ms |
| CPU | 0.18 ms | 0.33 ms | 0.84 ms | 1.29 ms | 12.2 ms |

The present average includes the first-frame outlier (up to ~100 ms when the window is occluded);
its p50 is 0.11 ms. Run-to-run noise moves the stroke figures by a few hundredths, so read them as
"about a tenth of a millisecond per sample".

Per sample including present, GPU is about 0.63 ms average / 0.69 ms p95, CPU about 1.02 ms
average / 1.62 ms p95. The GPU path is roughly 1.6× faster on average and 2.3× faster at p95. The
deferred commit is applied in 8-segment slices on the main run loop; the total work is still ~25 ms
for 240 samples, but the largest single slice stays under a 60 Hz frame.

The commit is the cost of rebuilding the authoritative CPU pixels once; the drag itself no longer
rasterises.

## Coverage buffer

The coverage buffer is kept between strokes and cleared with a command buffer. A 4K buffer is
64 MB, so allocating it and clearing it from the host on every pen-down would cost more than the
first sample; `stroke begin` is now about 0.14 ms.

## Segment uniforms

The stroke keeps its parameters in a 128-slot uniform ring. One bind group is created at
`stroke_begin` and each segment writes its slot and rebinds with a dynamic offset, so a sample does
not allocate a buffer or a bind group.

## Parity with the CPU brush

The falloff, spacing and phase logic mirror `crates/core/src/brush`:

- `coverage()` is the same `smoothstep` of `t / edge`, with `edge = max(1 - hardness, 0.04)`.
- The tip angle is stored in degrees and the core measures the major axis as `(cos, sin)`, while
  the shader rotates the other way; the shader negates the angle after converting to radians.
- Stamp spacing is `max(radius * max(spacing, 0.02), 0.5)`, and the fractional phase carries
  across samples.

Not yet ported: grain jitter, paper and tip textures, the dual tip, wet pickup, and pen roll.
Because of that the preview is still approximate for those brushes, and the pen-up commit must stay
on the CPU. Writing preview coverage straight into the layer would change the pixels at pen-up.

## The coverage-readback experiment

`DRAWKIT_BENCH=1 DRAWKIT_BENCH_GPU_COMMIT=1` finishes the stroke from the GPU coverage instead of
replaying it: read the dirty rows back, apply `over(colour × coverage)` tile by tile, and record one
history step. It is correct, and the tape still holds the events, but it is slower:

| step | time |
|---|---|
| coverage copy + blocking map, 6.7 MB / 1.67 M floats, 4K | 23–26 ms |
| the same at 1080p, 3.7 MB | 2.5 ms |
| whole GPU commit, including two snapshots | 31–47 ms |
| sliced CPU replay, the same 4K stroke | ~25 ms total, max 3.6 ms per slice |

The copy is small, so the cost is not bandwidth: the blocking `map` waits behind a queue that holds
one submit per sample plus the presents, and it scales worse than the bytes do. Applying the pixels
is a few milliseconds once the tiles are walked instead of looked up per pixel.

So the readback path is kept behind the benchmark flag and the app still uses the sliced replay. A
GPU commit only wins once one of these holds:

- the pixels stay on the GPU (commit into resident layer tiles, read back for history only), or
- the copy is submitted at pen-up and collected a frame later, with the preview still on screen.

Until then the honest statement is that the drag is GPU and the commit is CPU.

## Next GPU steps

- Port grain, paper/tip texture, dual tip, wet pickup and roll to the stamp shader.
- Commit into resident layer tiles instead of reading the coverage back: copy the tile to a
  temporary, blend coverage into the tile, then read the tile back for history.
- If the readback stays, submit it at pen-up and collect it a frame later.
- Store coverage at half or quarter resolution to cut bandwidth.
- Move coverage into a tile texture so preview order is correct for painting below the top layer.
