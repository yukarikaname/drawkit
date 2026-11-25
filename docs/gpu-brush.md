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
Because of that the preview is still approximate for those brushes. The GPU commit below is
therefore limited to brushes whose preview is exact; everything else keeps the CPU replay until the
shader is complete.

## Committing into the resident tiles

`DRAWKIT_BENCH_TILES=1` runs the whole commit on the GPU path:

1. `stroke_commit` composites the coverage into every resident tile the stroke touched. Each tile
   is read as a texture and written to a fresh texture, because a storage texture cannot be sampled
   in the same pass. The fragment shader repeats the core's `over()` on straight alpha.
2. The touched rectangles of those tiles are copied to staging buffers and mapped asynchronously.
   The coverage overlay comes down as soon as the new tiles exist, so the screen already shows the
   committed stroke.
3. When the copies land, the core writes the pixels into the layer surface and pushes one history
   step. The tape still holds the events.

4K, 240 samples, one stroke:

| step | time |
|---|---|
| end_stroke + begin | 2.5 ms |
| GPU composite and copies mapped | 5.0 ms |
| apply 4 tiles and close the step | 18.6 ms total, 5.4 ms worst tile |

The GPU part is what the CPU replay used to do; the application is memory-bound:
`commit_tile` snapshots the before image, converts RGBA8 to f32 and writes it back, and the history
step rebuilds the stroke's bounding box as before/after images. On this synthetic stroke the
bounding box is 1.6 M pixels while the ink covers a thin curve, so most of that traffic is empty
space.

History is stored per tile now, so the commit never rebuilds a bounding-box image. The app applies
one tile per run-loop tick: the largest piece stays at 5.4 ms, under a 60 Hz frame, and the CPU is
caught up in about four frames. The sliced replay remains the fallback for brushes whose preview is
still approximate.

## Next GPU steps

- Port grain, paper/tip texture, dual tip, wet pickup and roll to the stamp shader.
- Store history per tile and skip the union image, then switch the app to the tile commit.
- Port paper/tip texture, dual tip, wet, pressure alpha, roll and grain to the stamp shader so the
  CPU brush no longer needs to exist.
- Store coverage at half or quarter resolution to cut bandwidth.
- Move coverage into a tile texture so preview order is correct for painting below the top layer.
