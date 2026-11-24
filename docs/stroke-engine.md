# Resolution-independent stroke painting

Status: design, with the P8.0 slice implemented.

Implemented now: the recording stores a per-stroke seed and tilt/azimuth/roll; the tape can be
written as `.dkp` and read back; `replay::replay_tape` paints a tape at a uniform scale with either
canvas-following or screen-fixed brush radius; fills and clears replay; tests cover 1×/2× and the
two radius policies. `Length`, per-layer caches, editing and the full `.dkp` v1 manifest are still
ahead.

Nothing in this document is a promise that the current `.dkp` format or the current raster editor
will be replaced; it is the target the next slices move toward.

## The premise

A raster document stores the result of painting. Once a stroke is composited into pixels, its path,
pressure and brush are gone. A vector document stores notation instead of paint; it scales, but a
soft brush, grain and wet media become an approximation.

DrawKit should store **the painting behaviour and the pixels it produced**. The pixels keep the exact
look; the behaviour lets a stroke be re-rendered at another size, restyled, moved, or erased without
repainting by hand. This is the same direction as Solstice's Brush Stroke Layer and vector brush
strokes, extended to the whole document instead of one layer type.

The narrow, testable definition:

> A stroke is an immutable object with geometry, dynamics, a brush and a seed. The document can
> rasterise those objects at any output scale, and can edit an object and re-rasterise only what
> depends on it.

It is not "save every brush as a PNG", and it is not "convert every stroke to Bézier curves". Raw
sample data stays; smoothing is an edit or a rendering detail, never the source of truth.

## Prior art, read not assumed

| Project | What it actually does | What DrawKit takes | What it does not take |
| --- | --- | --- | --- |
| [MyPaint] | A `Stroke` records brush settings, brush state, and a stream of `dtime, x, y, pressure, xtilt, ytilt, viewzoom, viewrotation, barrel_rotation`; replay runs the brush engine. Strokes are immutable; `Brushwork` renders atomically. | The event stream, immutable stroke, brush state and seed; atomic rendering. | Per-event `viewzoom` as a brush input (it makes the document view-dependent); GPL code. |
| [Paintlang] | Every operation is an object with a bounding box and transform. A cache composites appended operations onto a snapshot; a drag invalidates and rebuilds from the previous snapshot. Brush stamps use a per-stroke `mulberry32` seed. | Deterministic per-stroke seed; append-only cache; bounding box for hit testing and invalidation. | Its in-memory draw closures as the file format; JS code. |
| [Solstice] `Brush Stroke Layer` | A paint layer remembers its brush strokes and redraws them at the new size when the image/layer is scaled; brush size grows with the scale. It detects when pixels no longer match the strokes and then scales those parts as pixels. Brush tips, textures and gradients used by strokes are stored in the file so it redraws on a machine without the presets. | The layer model (raster cache + remembered strokes); replay on scale; the "pixels no longer match" warning; embedding brush assets. | Re-rendering every dab on one thread; GPL code. |
| [Solstice] vector strokes | Each stroke on a vector layer is an object: move points, change width/density, erase the touched part/up to an intersection, simplify, restyle, free transform. Limits: smudge has nothing to mix with, non-normal blend applies inside the stroke, a texture with a random per-stroke offset stops matching after a move. | Editing vocabulary and the honest limits; a stroke is one undo step; width is pressure data, not a separate number. | Making every paint layer a vector layer. |
| [Krita] / PSD | Mature tiled raster storage and ordinary vector layers. They preserve pixels for other applications. | Interop target: a DrawKit file must be exportable as pixels without losing what other tools understand. | Rebuilding Krita. |

The three open-source references use different licenses; this document borrows behaviour and data
shapes, not code.

## The model

```text
Creation space                 Stroke space                  Output space
what the painter did           how it is painted             where it is shown
─────────────────              ─────────────────             ────────────────
samples, timing,       ──▶     brush, material,      ──▶     width × height,
pressure, tilt, tilt,          source-over, cache           pixels per unit,
speed, seed                    per layer/scale              device profile
```

### Stroke

```rust
pub struct StrokeId(u64);

pub struct Stroke {
    pub id: StrokeId,
    pub layer: LayerId,
    pub seed: u64,                 // stable random stream for this stroke
    pub z: u64,                    // order inside the layer
    pub transform: Affine2,        // move/scale/rotate after the fact
    pub path: StrokePath,          // document-space samples
    pub brush: BrushRef,           // preset id + explicit overrides
    pub color: Paint,
    pub opacity: f32,              // stroke opacity, applied once
    pub blend: BlendMode,
    pub material: MaterialParams,  // pigment, wetness, thickness, seed
    pub flags: StrokeFlags,        // context-dependent, mirrored, ...
    pub engine: EngineVersion,     // the renderer that made the cached pixels
}

pub struct StrokePath {
    pub samples: Vec<Sample>,      // quantised on disk, not in memory
    pub bounds: Rect,              // document units; hit test and invalidation
}

pub struct Sample {
    pub time: f32,                 // seconds from stroke start
    pub x: f32, pub y: f32,        // document units
    pub pressure: f32,             // 0..1
    pub tilt: [f32; 2],            // -1..1
    pub azimuth: f32,              // radians
    pub roll: f32,                 // radians
}
```

Speed and direction are not stored per sample; they are derived from the path and timing. They are
inputs to the brush at replay time, so a different brush can use them without changing the source
data.

The path is in **document units**. A document has a fixed size in units (typically 1 unit = 1 pixel
at the reference scale) and an optional physical DPI. Brush sizes are resolved per output (below).
The stroke itself never stores "20 pixels at the current zoom".

### Brush and material

```rust
pub struct Brush {
    pub id: AssetId,               // content hash of the preset
    pub size: Length,              // how its diameter resolves
    pub spacing: f32,              // fraction of diameter, in document units
    pub hardness: ScalarInput,     // pressure/tilt/speed-driven
    pub flow: ScalarInput,
    pub opacity: ScalarInput,
    pub tip: TipRef,               // round, image, or procedural
    pub texture: Option<TextureRef>, // grain, paper, bristle
    pub mixing: Mixing,            // none, smear, wet
    pub engine: BrushEngineId,
}
```

`ScalarInput` is a small curve: constant, pressure, tilt, speed, direction, random, or a combination.
This keeps the brush a data structure the renderer can replay, rather than a closure that cannot be
saved.

### Length and scale: the part that must be explicit

The original problem was "canvas 4000 → 1000, line stays 20 px". That is not a raster/vector
question; it is a question of which size the number refers to. DrawKit stores a length with its
reference:

```rust
pub enum Length {
    Canvas(f32),       // document units; 20 becomes 5 after a 0.25× canvas
    Screen(f32),       // output pixels; 20 stays 20 at any output
    Physical(f32),     // millimetres; 20 px at 254 dpi is about 2 mm
}

pub struct OutputSpec {
    pub width: u32,
    pub height: u32,
    pub pixels_per_unit: f32,      // output px / document unit
    pub brush_scale: BrushScale,   // follows Canvas, Screen or Physical
    pub texture_scale: f32,        // sampled in document or output space
    pub detail: DetailLevel,       // spacing / LOD / quality
}
```

Resolution rules:

```text
canvas_width_units = 4000, output_width_px = 1000  =>  pixels_per_unit = 0.25

Canvas(20)   -> 20 * 0.25 = 5 px
Screen(20)   -> 20 px
Physical(2)  -> 2 mm * 1000 px / (4000 units * 25.4 / 96 dpi ...)  // resolved from DPI
```

On the canvas, the user does not choose "pixels"; the tool stores `Canvas` by default. The tool
options expose a **thickness policy** (`Follow canvas`, `Fixed on screen`, `Physical`) and the
export dialog exposes the same choice. This is the setting that makes the user's example possible
without silently changing every other brush.

There are three different operations, and they must not share a command:

| Operation | Geometry | Brush size | Use |
| --- | --- | --- | --- |
| Change output | unchanged | resolved per policy | export/print/another device |
| Scale document | multiplied | policy decides | enlarge line art, make a poster |
| Resize canvas | cropped/padded | unchanged | more space around the painting |

Mixing them is what makes canvas resizing surprising in raster editors. DrawKit names them
separately.

## Rendering

A renderer is a pure function of `(strokes, output, engine version)`. It has two consumers:

1. **Live paint**: the existing raster path paints each new segment at screen resolution, then the
   committed stroke is appended to the layer's cache. The user sees ink immediately; no whole-canvas
   replay on every sample.
2. **Export/re-render**: the stroke list is rasterised into tiles at the requested output, in
   parallel, and composited into the layer.

### Sampling and determinism

- Stamps are placed by **arc length in document units**, derived from the brush's resolved diameter.
  The dab count therefore does not change when the output resolution changes; only each dab's pixel
  footprint does.
- `spacing`, `scatter`, `jitter` and texture offsets use the stroke's `seed`; no global random state.
- Velocity is document units per second, never screen pixels per second.
- The brush engine has an explicit version. A cached image is valid only for the engine version that
  produced it. A newer engine can keep the cache and offer "re-render", or re-render on export.
- A stroke is rendered to an **alpha scratch mask**, then composited once with its stroke opacity.
  Rendering each dab directly with the stroke opacity darkens overlaps. Build-up brushes opt into
  per-dab compositing through their brush mode, and say so in the preset.
- Smudge and wet brushes read the pixels beneath the stroke. They are **context-dependent**: they
  cannot be rendered from the stroke alone without the backdrop. They replay in layer order against
  the layer/cache below; they are not cached as independent strokes with the same guarantees.

### Tiles and caches

```text
Layer
├── strokes (source of truth, ordered)
└── cache
    ├── scale bucket (1/4, 1/2, 1, 2, 4 relative to reference)
    ├── tile key (bucket, x, y, prefix_z)
    └── validity: prefix of the stroke list + engine version + layer transform
```

Appending a stroke extends a cache without touching earlier tiles. Editing stroke `z` invalidates
tiles overlapped by its bounds and every later stroke in that tile; the cache becomes a prefix up to
`z - 1`, and the remainder is replayed. Moving a layer transform does not transform the cache
(that would blur); it marks the cache stale and re-renders when the drag settles, while the drag
shows the old pixels as a preview.

A layer has a consistency state:

- `Exact`: every visible pixel was produced by its strokes at the cache scale.
- `Mixed`: part of the layer is raster (an imported PSD, a filter, a cut stroke); the stroke list is
  not sufficient for that region. Scaling must fall back to pixels there.
- `Stale`: the cache does not match the current stroke list or engine; show it, but mark it.

Solstice's warning ("this layer's pixels no longer match its strokes") is the right user-facing
version of this state. We keep the state in the document, not in a toast.

### Layers

```rust
pub enum LayerContent {
    Paint { strokes: Vec<Stroke>, cache: TileCache, consistency: Consistency },
    Raster { surface: Surface },            // PSD/KRA/PNG import
    Hybrid { base: Surface, strokes: Vec<Stroke>, cache: TileCache },
}
```

A new painting is a `Paint` layer. Importing a PSD opens `Raster` layers; painting on one makes it
`Hybrid`, so undo of the stroke can remove only the stroke and the imported base remains untouched.
This is how raster/vector coexist without pretending a photo is a stack of strokes.

Masks and selections are layer/raster data, not strokes. A selection can clip a stroke at render
time (a reference), but the stroke keeps its whole path; if the user moves half a stroke, the render
and the model diverge, so the layer becomes `Mixed` and the stroke is split into two new strokes
when possible.

## Editing

The first editable operations, in order of value:

1. **Select and move** a whole stroke with its brush and pressure (bounds hit test, then per-pixel
   alpha hit test at the screen scale).
2. **Restyle**: replace colour, brush preset or size; the path, pressure and seed stay.
3. **Delete** a stroke; undo restores it.
4. **Reorder** (z) within a layer.
5. **Split/erase**: a stroke eraser cuts at path distance; the remaining pieces become new strokes
   with the same brush, seed and transform. Solstice's "touched part / up to intersection" is the
   interaction target.
6. **Width, density and pressure edits** along the path: width is a change to the recorded pressure
   (or an explicit width channel), not a separate vector width, so brushes that respond to pressure
   keep working.
7. **Simplify**: Douglas–Peucker in document units with an error tolerance that includes pressure;
   the raw stroke is replaced by a new one, undo is one step.

Every edit produces a new immutable stroke. History stores operations (add/remove/replace stroke,
change layer, transform), not raster snapshots. Raster imports and non-stroke paint still use the
existing tile history; a `Paint` layer with strokes only needs the stroke command log.

## File format

Working name: **`.dkp`** (DrawKit Painting), a ZIP container. It is deliberately boring:

```text
manifest.json                 version, document units, DPI, layers, brushes, outputs, engine versions
strokes/<layer>/<chunk>.bin   chunked stroke records, delta + quantised
assets/brushes/<hash>.png     tips, grain, paper, gradients
assets/icc/<hash>.icc         colour profiles
cache/<layer>/<bucket>/...    optional PNG/WebP preview tiles (never authoritative)
preview.png                   flattened 8-bit preview
merged.png                    flattened fallback for other tools
```

Stroke chunk:

```text
magic "DKS1" | u16 version | u32 layer | u32 count
per stroke:
  id u64, seed u64, z u64, flags u16, brush u32, color [u8;4], opacity u8
  transform [f32;6], sample_count u32
  samples: delta x,y (i32, 1/16 unit), pressure u16, tilt [i16;2], azimuth u16, roll u16,
           dt u16 (milliseconds)
```

Coordinates are quantised to 1/16 document unit and pressures to 16 bits; this is below the pen's
noise floor and keeps a sample near 12–16 bytes. Timestamps are deltas. Paths can be split into
chunks so a crash loses at most one chunk and random access is possible.

Compression: the ZIP entry is deflated in v0. A later format version can add zstd per chunk with a
flag; readers must reject unknown compression rather than guess. Limits on sample count, chunk
count, decompressed size and nesting are mandatory before any allocation.

The manifest records a digest of each layer's stroke list and cache. On open, the cache is used only
if the digest and engine version match; otherwise it is a preview and the strokes are the truth.
Saving is atomic: write a temporary file, fsync, rename.

### Interop

- `.dkp` is the project: the stroke tape carries the painting, the layer tree carries the palette,
  and imported or baked layers carry a raster cache. Pixels stay a cache, not the source.
- KRA/PSD export rasterises at the chosen `OutputSpec`; the stroke list is not portable through
  those formats. The export dialog offers to save the `.dkp` alongside, and the app says that the PSD
  is a print of the painting, not the painting.
- KRA/PSD/PNG import becomes a `Raster` base layer; painting on it creates a `Hybrid` layer.
- Solstice inserts stroke data into `.kra` and Krita drops it on re-save. We do not copy that
  extension until the format is stable; the `.dkp` is the source of truth and the interop files are
  derivatives.
- SVG export can write the centerline and pressure as metadata, but the default SVG is a rendered
  image, because a soft brush is not a path.

## Performance and data budget

| Quantity | Target |
| --- | --- |
| Input to displayed ink | existing raster segment path, no replay (≤ 16 ms at 1080p) |
| Append a committed stroke to the cache | one dirty rectangle, no full layer |
| Re-render one edited stroke tile | ≤ 4K tile in 16 ms on the GPU; CPU fallback may be slower |
| Open a 10k-stroke painting | preview first; strokes loaded lazily by layer/chunk |
| Source data, 10k strokes × 80 samples | ~10 MB quantised before deflate |
| 100k-stroke document | ~100 MB source; cache is the problem, so capped by scale bucket and layer |
| Export 4K | tiled parallel; exact output, no cache dependency |

Cache memory is bounded by scale bucket and visible tiles; a full-document cache at every scale is
not allowed. A stroke that has not been edited and lies outside the viewport needs no cache at all.

## Migration plan

The current core already has a command tape (`record.rs`), dirty rectangles, tiled surfaces and a
history. The plan grows that instead of replacing it.

**P8.0 — richer recording (small, shippable)**
- Extend the recording event with tilt, azimuth, roll, speed/time, seed and a brush preset reference.
- Store brush presets and their images in the project; assign each stroke a seed.
- Add a "thickness policy" to the tool options and the export dialog (screen vs canvas) while the
  current raster engine still paints.
- Round-trip strokes into a `.dkp` sidecar; replay only for a test/export at 1× and 2×.

**P8.1 — stroke layers and replay**
- `Paint` layers hold `Vec<Stroke>` plus a tile cache. A stroke does not store a PNG.
- Append-only cache; invalidate from edited `z`; re-render on scale.
- `OutputSpec` and `Length`; render a document at 0.5× and 2×; compare downsampled output.
- Keep imported raster layers as `Raster`; mark `Hybrid` when painted.

**P8.2 — editing**
- Hit test, select, move, delete, restyle, reorder.
- Stroke eraser with split; simplify; width/density edits.
- Consistency state and the "pixels no longer match" warning; convert affected regions to `Mixed`.

**P8.3 — brushes, materials, GPU**
- Deterministic brush engine versioning; grain/texture assets; context-dependent brushes.
- GPU stamp renderer for tiles and live ink; CPU parity tests.
- Wet-media simulation grid in document units, or an explicit "not resolution independent" badge.

**P8.4 — format and API**
- Freeze `.dkp` v1 and publish a reader; optional embed in KRA if Krita's behaviour is stable.
- Expose the engine as a library: `commit_stroke`, `render(OutputSpec)`, `edit(StrokeEdit)`.

## Decisions

1. The source of truth is the stroke list plus a raster base; pixels are a cache, not the document.
2. A stroke is immutable; an edit creates a new stroke and a history command.
3. All geometry is in document units; brush size is a `Length` resolved by an explicit output
   policy, never by the current view zoom.
4. The canvas is resized, scaled and re-output by three named operations, not one.
5. Caches are append-only prefixes keyed by scale and engine version; edits invalidate a suffix.
6. A brush engine version is part of the file; a cache produced by another version is a preview.
7. Imported PSD/KRA layers stay raster; painting on them makes a hybrid layer, never a fake stroke
   list.
8. Context-dependent brushes (smudge, wet) are replayed against the backdrop and have weaker cache
   guarantees.
9. The file is a ZIP with a manifest, chunked binary strokes and embedded brush assets; KRA/PSD are
   pixel exports, not the source.
10. Start with normal blending and the current brushes. Blend modes and materials come after the
    geometry and cache are proven.

## Open questions

- **Physical units**: map `Length::Physical` through the document DPI, and what the DPI means when
  the canvas is scaled. Is DPI only an output property, or part of the document?
- **Brush state**: MyPaint records the brush engine's internal state (smudge carried colour, etc.).
  Do we record it per stroke, or recompute it from the backdrop? Recomputing is cleaner but slower
  and depends on the cache.
- **Random textures**: a per-stroke random texture offset is deterministic to replay, but a move
  after painting may stop matching the cached pixels unless the texture coordinates stay in
  document space. Solstice hit this limit; we should make the offset document-relative.
- **Partial strokes**: a selection that cuts a stroke should split it. Defining the split point by
  path distance is straightforward; by pixel mask is not.
- **GPU determinism**: GPU stamping may differ in the last bit. Do we accept `≤ 1/255` and keep the
  CPU as the reference, or require bit-exact and restrict the GPU path?
- **Incremental save**: a 100k-stroke document should not rewrite every chunk on save. An index of
  dirty chunks is needed before the format is frozen.
- **Collaboration**: stroke ids and ordering are enough for a basic merge, but layer transforms and
  brush assets need conflict rules.
- **When not to use strokes**: photo import, painted-over filters and texture stamps may be better
  as raster. The consistency state exists to say so honestly.

## Tests

- **Encoding**: quantise/dequantise a sample stream and assert bounds; fuzz truncated chunks and
  reject without allocating.
- **Replay determinism**: same stroke and seed rendered twice is byte-identical; two processes agree.
- **Resolution**: render a stroke/document at 1×, 2×, 4×; downsample the larger images and compare
  within a tolerance; the path/dab count must be identical.
- **Cache**: append equals from-scratch; editing stroke `k` invalidates exactly tiles overlapping
  `k` and later; the prefix is reused.
- **Policy**: `Canvas(20)`, `Screen(20)` and `Physical` across the 4000→1000 example produce 5 px,
  20 px and the expected millimetres.
- **Editing**: move/restyle/delete/undo restores the exact previous render; splitting a stroke and
  rejoining preserves the sampled geometry within tolerance.
- **Interop**: render → PSD/KRA → reopen as a raster base → export; pixels match within one step.
- **Budget**: 10k strokes, append, edit one, export 4K; memory and time recorded in the benchmark
  suite.

## References

- MyPaint: <https://github.com/mypaint/mypaint> — `lib/stroke.py`, `lib/command.py`.
- Paintlang: <https://github.com/MallikShashank/paintlang> — `js/02-scene.js`, `js/04-brush-engine.js`.
- Solstice Brush Stroke Layer: <https://github.com/udtren/Solstice/blob/main/docs/brush-stroke-layer.md>
- Solstice Vector Brush Strokes: <https://github.com/udtren/Solstice/blob/main/docs/vector-brush-strokes.md>
- Krita file format: <https://docs.krita.org/en/general_concepts/file_formats/file_kra.html>
- Krita tile data: <https://community.kde.org/Krita/Tile_Data_Format>
