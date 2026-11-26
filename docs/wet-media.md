# Wet media

Watercolor that interacts across strokes is not a brush shader. A stroke leaves water and pigment
on the paper; the paper continues to move them, and the next stroke lands in a wet region rather
than next to a finished one. The engine therefore needs a **simulation field** next to the colour
tiles, and that field has its own state, residency, undo and replay rules.

This note is the contract; `crates/gpu/src/wet/` implements it.

## State

Per simulation cell, eight channels in two `Rgba16Float` textures:

| texture | channels |
|---|---|
| 0 | water, wetness, mobile pigment R, mobile pigment G |
| 1 | mobile pigment B, deposited R, deposited G, deposited B |

Paper is static: height and absorbency come from the document's paper texture and are sampled,
not stepped. Simulation resolution is a **document property**, suggested at half the reference
canvas (1920×1080 for a 4K document). Colour tiles stay at document resolution; the field is
upsampled only when it is composited.

## Passes

One step is one compute pass over the wet region, gather-only:

- water: `water + dt * (diffusion * laplacian(water) - evaporation)`, clamped to 0…1
- transport: the water gradient moves mobile pigment (upwind), plus diffusion while wet
- deposition: mobile pigment settles into the paper, faster where water recedes (the dark rim)
- drying: a cell below the dry threshold stops evolving and costs nothing

Injection is the same pass with a disc-shaped deposit of water and pigment. It is not a special
kernel: it reads the current pair and writes the next one like everything else.

## Determinism

Rules that make the layout replayable and tile-safe:

1. Fixed `dt` from the recorded simulation step, never frame time.
2. Double buffered: a pass reads one pair and writes the other; no in-place update.
3. Gather only: a cell writes itself. No scatter, no float atomics, no order-dependent splats.
4. Tiles write their interior and read a halo of the previous generation; tile order is
   irrelevant.
5. Paper noise is `hash(x, y, paper_seed)`, evaluated per cell; no global random stream.
6. Boundaries follow the wet mask, not the tile edge.

Cross-GPU bit-exactness is not promised. The contract is: same engine version and same device
class is reproducible; across versions or devices the result is visually equivalent. Cross
**resolution** re-simulation is not promised either: a PDE changes with `dx`, so the field is
pinned to the document's simulation resolution. A dry painting can still be re-rendered at any
output size; a wet interaction cannot be re-simulated at another one.

## Scheduling

Only wet regions step. A cell below the dry threshold with four dry neighbours stops evolving in
the shader, so a dry field is cheap to dispatch. The field is currently **one resident texture
pair per document at a pinned scale**, not tiles: at 1920x1080 a fully wet field steps in
0.13-0.30 ms and costs 63 MiB, so tiles would buy nothing at 4K. Tiling becomes worthwhile when an
8K field does not fit in the memory budget, not before. The step is gather-only, so moving to
tiles later is a storage change, not a kernel change.

The state is pinned to the document's simulation resolution. A 4K document runs at half
resolution (1920x1080, 63 MiB), an 8K document at a quarter (also 1920x1080), so the field stays
inside one budget. The scale is chosen once and stored; it is not a per-frame quality knob,
because changing it changes the PDE.

A stroke stores the simulation step index at which each segment was deposited. Replay advances
steps until that index, deposits, and continues; that makes stroke timing part of the field's
history without a wall clock.

A shell passes elapsed seconds to `advance_seconds`; the field accumulates them and runs fixed
1/60 s steps, at most eight per call, so a stalled frame does not spiral. `peak_water` is a GPU
reduction with one readback; a shell asks it to decide that a document has dried and to bake, not
every frame.

## Undo and recovery

The wet field is state, so undo cannot be "restore the colour pixels":

- a checkpoint is the field tiles a stroke touches, taken before depositing, keyed by step index;
- undo restores the checkpoint and replays the later overlapping strokes;
- a dry tile bakes through the existing tile commit, so the dry result is an ordinary
  `TileSwap` and the layer returns to the normal undo journal;
- a session that crashes while wet can either restore the last checkpoint or bake the field on
  recovery; both are explicit, not silent.

Checkpoints are budgeted (a rolling window, not one per stroke for the whole session). A 4K
half-resolution field is 63 MiB including both pairs, so a snapshot per stroke is not viable.

## File and replay contract

A project that contains wet media stores:

- the stroke tape with a `sim_step` per segment;
- simulation engine version, simulation resolution, tile size and halo;
- paper asset hash, pigment/LUT version and boundary mode;
- checkpoints, or a statement that the wet result was baked.

If exact cross-stroke replay is not required, the default is to **bake on dry** and keep the file
small. The tape still re-renders the dry result at any size; the wet interaction is a one-way
bake and is marked as such.

## Phases

1. **Stroke-local wet** (prototype): one stroke, four channels, inject + step + bake at pen-up.
   No cross-stroke state. Acceptance: edge darkening and a soft bloom, two runs identical on one
   GPU, undo restores only the touched tiles.
2. **Persistent wet layer**: eight channels, 512×512 tiles with a 2-cell halo, drying, pickup,
   Kubelka–Munk at composite time, black-run heuristic, checkpoints. Acceptance: wet blue plus
   wet red produces a pigmented mix, a dry tile bakes once, replay matches on the same GPU and
   stays within a small tolerance across GPUs.
3. **Layers and export**: independent wet layers, optional wet-on-wet across layers, export from
   the baked pixels.
4. **Optional LBM**: only if the field above is not convincing.

## What runs today

`crates/gpu/src/wet/` owns the kernel, the fixed clock, scale pinning, stroke injection and dry
detection. `crates/gpu/tests/wet_prototype.rs` covers four things:

- blue and red meet on wet paper, settle and dry without a NaN;
- a document-sized field maps document coordinates through its pinned scale;
- a stroke segment leaves a wet trail with pigment on it;
- a field reports dry after the water evaporates.

Release numbers on an Apple silicon Mac, full field wet:

| document | scale | field | memory | inject (event) | inject (row) | step |
|---|---|---|---|---|---|---|
| 1920x1080 | 1 | 1920x1080 | 63 MiB | 0.22 ms | 1.6 ms | 0.30 ms |
| 3840x2160 | 1/2 | 1920x1080 | 63 MiB | 0.20 ms | 1.5 ms | 0.14 ms |
| 7680x4320 | 1/4 | 1920x1080 | 63 MiB | 0.25 ms | 1.5 ms | 0.13 ms |

Injection is the cost of one pointer event, so a 60 Hz wet brush spends about 1.5% of the frame in
it and 1% stepping at 4K. It is deliberately not wired to the document, undo, or the tape yet;
those are the next phases.
