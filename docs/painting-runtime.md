# Painting runtime strategy

This is the direction the engine is measured against, not a description of everything that exists.

## Position

**Apple-first, Rust core, Metal through wgpu.**

- Rust owns the document, the recording, the history, the brush and fill algorithms, and the
  scheduling of CPU work.
- Swift owns the Apple layer: windows, menus, gestures, Apple Pencil input, files, lifecycle.
- The GPU layer is wgpu today. Native Metal is a later, targeted move for the paths profiling
  names, not a rewrite.
- macOS first for large documents, many layers, history and residency. The iPad stays an
  architectural requirement from day one (input path, memory budget, thermals), not a port that
  starts later.

Not doing: a node editor, a general procedural-graphics toolkit, five equally deep ports, every
brush type, or a Swift rewrite of the core.

## The five investments

1. **Pencil to screen latency.** The whole chain, not just the dab shader: event intake,
   coalesced and predicted samples, stroke runtime, GPU stamping, compositing, present timing.
   The number that matters is the distance between the nib and the ink.
2. **Spatial state runtime.** Tile-level revision, region dependency, temporal dependency,
   checkpoints, incremental invalidation, and local history edits. The interesting part is the
   time-dependent brush (smudge, wet), where a stroke reads what earlier strokes wrote.
3. **Unified memory residency.** One tile residency manager for drawing, viewport, neighbours,
   far tiles, checkpoints and the CPU cache. Unified memory is not free transfer; it is a cache
   hierarchy with a bandwidth and a pressure budget.
4. **Multi-resolution, demand-driven rendering.** Viewport and zoom first, edited region next,
   far regions later; high resolution filled in on the background; smooth switching while
   zooming. Memory and heat make this mandatory on iPad.
5. **Painting semantics.** Pencil brushes, wet brush, smudge, watercolor, textured brushes,
   custom brush engines, colour management. Wet brush that stays low-latency, undoable,
   recoverable and cheap in memory is more distinctive than another filter.

## Language boundaries

Never put a Swift/Rust call per segment on the hot path. Batch input samples and commands across
the boundary; the FFI is a per-frame or per-stroke boundary, not a per-dab one.

| layer | owns |
|---|---|
| Swift / SwiftUI / AppKit / UIKit | UI, gestures, Pencil, windows, files, lifecycle |
| Rust | document, recording, spatial state, dependencies, undo, checkpoints, brush and filter algorithms |
| wgpu (Metal backend) | GPU brush, tiles, compositing, filters, residency |

## Benchmarks

Frame rate is not the measurement. The suite reports, on the same documents and operations:

| test | what it answers |
|---|---|
| pencil input to display | end-to-end latency |
| local stroke update | incremental cost of a dab or a segment |
| historical stroke edit | recolour/delete/transform cost |
| 100-layer composite | layer-tree cost |
| 20K × 20K canvas | large-document behaviour |
| wet brush simulation | time-dependent brush cost |
| undo / redo | history cost |
| memory pressure | stability under the budget |
| cold recovery | open-after-crash time |

`crates/core/tests/runtime.rs` and `crates/gpu/tests/runtime.rs` hold the CPU and GPU halves;
both are ignored by default and run with `--ignored --nocapture`.

## Resource split (small team)

| area | share |
|---|---|
| Pencil and interaction latency | 25% |
| spatial state runtime | 25% |
| Metal, GPU and memory | 20% |
| brush and wet brush | 20% |
| multi-resolution and recovery | 10% |

## Relationship to the other documents

- `docs/perf.md` — current numbers.
- `docs/tiling.md` — tile storage and dirty rectangles.
- `docs/stroke-engine.md` — the recording and replay model.
