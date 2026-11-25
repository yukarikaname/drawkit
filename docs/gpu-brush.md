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

## A GPU-resident stroke

The pen-up path never reads pixels back:

1. `stroke_commit` composites the coverage into every resident tile the stroke touched, reading the
   old tile as a texture and writing a fresh one.
2. The replaced tile textures are kept as the undo record. `undo` and `redo` swap textures; no
   pixels move.
3. The core records a history step that only holds the touched rectangles; the pixels stay in the
   journal.
4. The CPU surface is a cache. It is marked stale and rebuilt only when something reads pixels:
   save, export, the eyedropper, a thumbnail, or a tool that still runs on the CPU. That rebuild
   (`materialize`) reads the stale rectangles and writes them into the surface without adding a
   history step.
5. `markSurfaceFull` materialises first, so no upload can overwrite committed tiles with stale
   pixels. A tile that is stale is never evicted before it has been read back.

4K, 240 samples, one stroke:

| step | time |
|---|---|
| end_stroke + begin | 2.4 ms |
| GPU composite | 6–7 ms |
| CPU catch-up at pen-up | none |
| materialize, only when a save or a pixel tool asks | 15–28 ms |

The CPU work still exists, but it is off the pen-up path and proportional to what is actually
dirty. The sliced replay remains the fallback for brushes whose preview is still approximate
(grain, paper, tip texture, dual tip, wet, pressure alpha, mirror, stabilisation), because those
must be rasterised on the CPU for now.

## Next GPU steps

- Port grain, paper/tip texture, dual tip, wet pickup and roll to the stamp shader.
- Port paper/tip texture, dual tip, wet, pressure alpha, roll and grain to the stamp shader so the
  CPU brush no longer needs to exist and every stroke can stay resident.
- GPU thumbnails and eyedropper so a save is the only remaining materialise.
- Store coverage at half or quarter resolution to cut bandwidth.
- Move coverage into a tile texture so preview order is correct for painting below the top layer.
