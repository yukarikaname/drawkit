# GPU stroke preview

The macOS canvas now has a live GPU stroke path:

1. Pointer down switches the core to `defer_raster`: the brush records its tape events but does not
   paint the CPU surface.
2. `CanvasSurface` owns a coverage buffer and a compute pipeline. Each pointer sample accumulates
   stamp coverage in that buffer on the GPU; the buffer is never read back during the drag.
3. The paper shader overlays the coverage colour while presenting the resident layer stack.
4. On pointer up the core replays the recorded group into the CPU surface once, records one history
   step, and the shell rebuilds the resident layer texture from the final pixels.

Measured by `DRAWKIT_BENCH=1` on a 3840×3840 canvas with 240 samples:

| Part | avg | p50 | p95 |
|---|---|---|---|
| path | stroke avg | stroke p95 | present avg | present p95 | pen-up commit |
|---|---|---|---|---|---|
| GPU preview | 0.10 ms | 0.22 ms | 0.56 ms | 0.42 ms | 30.6 ms |
| CPU | 0.18 ms | 0.33 ms | 0.84 ms | 1.29 ms | 12.2 ms |

Per sample including present, GPU is about 0.66 ms average / 0.64 ms p95, CPU about 1.02 ms
average / 1.62 ms p95. The GPU path is roughly 1.5× faster on average and 2.5× faster at p95, but it
moves the full CPU rasterisation to pen-up, so a long 4K stroke pays a one-time ~30 ms commit.

The commit is the cost of rebuilding the authoritative CPU pixels once; the drag itself no longer
rasterises. The preview shader carries the spacing phase across segments and uses the brush hardness,
roundness and angle. It does not yet apply paper, tip texture, wet or the dual tip; those are exact
only after the pen-up commit.

## Next GPU steps

- Store coverage at half or quarter resolution to cut the present bandwidth.
- Move coverage into a tile texture and composite it into the resident layer tiles so preview order
  is correct for painting below the top layer.
- Rasterise the deferred group on a worker snapshot in chunks, keeping the GPU preview visible until
  the commit finishes.
