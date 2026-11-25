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
| stroke (record only) | 0.09 ms | 0.08 ms | 0.17 ms |
| present with preview | 6.7 ms | 6.0 ms | 11.5 ms |
| CPU commit at pen up | 24.6 ms | | |

The commit is the cost of rebuilding the authoritative CPU pixels once; the drag itself no longer
rasterises. The preview shader currently uses a round coverage stamp and does not yet apply paper,
tip texture, wet or the dual tip; those are exact only after the pen-up commit.

## Next GPU steps

- Store coverage at half or quarter resolution to cut the present bandwidth.
- Move coverage into a tile texture and composite it into the resident layer tiles so preview order
  is correct for painting below the top layer.
- Rasterise the deferred group on a worker snapshot in chunks, keeping the GPU preview visible until
  the commit finishes.
