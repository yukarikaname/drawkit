# Wet media

Watercolor that interacts across strokes is not a brush shader. A stroke leaves water and pigment
on the paper; the paper continues to move them, and the next stroke lands in a wet region rather
than next to a finished one. The engine therefore needs a **simulation field** next to the colour
tiles, and that field has its own state, residency, undo and replay rules.

This note is the contract; `crates/gpu/src/wet.rs` is the first prototype of the kernel.

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

Only wet regions step. A tile is active when it was just injected into, when it is still above
the dry threshold, or when it borders an active tile (the wet front moves). Idle tiles are
skipped and eventually baked.

A stroke stores the simulation step index at which each segment was deposited. Replay advances
steps until that index, deposits, and continues; that makes stroke timing part of the field's
history without a wall clock.

## Undo and recovery

The wet field is state, so undo cannot be "restore the colour pixels":

- a checkpoint is the field tiles a stroke touches, taken before depositing, keyed by step index;
- undo restores the checkpoint and replays the later overlapping strokes;
- a dry tile bakes through the existing tile commit, so the dry result is an ordinary
  `TileSwap` and the layer returns to the normal undo journal;
- a session that crashes while wet can either restore the last checkpoint or bake the field on
  recovery; both are explicit, not silent.

Checkpoints are budgeted (a rolling window, not one per stroke for the whole session). A full 4K
half-resolution 8-channel field is about 33 MB, so a snapshot per stroke is not viable.

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

## Prototype

`crates/gpu/src/wet.rs` implements phase 1's kernel shape at 256×256, and
`crates/gpu/tests/wet_prototype.rs` injects blue and red, steps 900 times, and checks that they
meet, settle and dry without a NaN. On an Apple silicon Mac the whole 256×256 grid steps in about
0.19 ms, the midpoint picks up both pigments, and the field dries without a NaN. It is
deliberately not wired to the document, undo, or the tape yet.
