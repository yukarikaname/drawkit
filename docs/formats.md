# File formats: Krita and Photoshop

DrawKit reads and writes layered raster documents so a painting can leave the app. The format code
lives in `crates/formats`, below the shells and above the painting core; every shell calls the same
two functions and gets the same layers.

| Format | Read | Write | What survives |
| --- | --- | --- | --- |
| DrawKit project (`.dkp`) | yes | yes | stroke tape, layer tree, groups, fill layers, bakes and a per-layer pixel cache |
| Krita (`.kra`) | yes | yes | paint layers, names, visibility, opacity, 8-bit RGBA |
| Photoshop (`.psd`) | yes | yes | raster layers, names, visibility, opacity, offsets |
| PNG | yes (flattened) | yes | the composite over white |
| JPEG | yes (flattened) | yes | the composite over paper, quality 1–100 |
| WebP | yes (flattened) | yes | the composite over paper, lossless |


## Krita

A `.kra` is a ZIP:

- `mimetype` — `application/x-krita`, stored first;
- `maindoc.xml` — the layer tree, topmost layer first;
- `<image name>/layers/<filename>` — one tiled pixel file per paint layer;
- `mergedimage.png` — Krita's own flattened render, used as a fallback.

A layer file starts with five text lines (`VERSION 2`, `TILEWIDTH 64`, `TILEHEIGHT 64`, `PIXELSIZE
4`, `DATA <n>`). Each of the `n` tiles is a line `x,y,LZF,size`, then `size` bytes: one flag byte
and the tile. Flag 1 means the rest is LZF-compressed, 0 means stored. Pixels are planar B, G, R, A.
The reader supports 64×64 tiles at any origin, layers offset by `x`/`y`, and the transparent
`.defaultpixel` fallback.

The writer emits 64×64 stored or LZF tiles, RGBA8, no masks, no groups, no layer styles. A document
whose image colour space is not 8-bit RGBA opens as its `mergedimage.png` layer instead of failing.

## Photoshop

Reading and writing use [`ag-psd`](https://crates.io/crates/ag-psd), which reads PSD and PSB,
decodes RLE and ZIP channels, and returns straight RGBA per layer. The reader keeps raster layers
with their names, visibility, opacity and offset. Groups are flattened and their visibility/opacity
are inherited. The writer emits a layered PSD with a flattened composite, so other editors see the
right image even if they do not read DrawKit's layers.

Ignored in both directions: blend modes other than normal, clipping masks, layer effects, text and
vector layers, smart objects, adjustment layers, 16/32-bit channels, CMYK and masks. These are
reported in the roadmap rather than approximated silently.

## Import semantics today

The native project format is `.dkp`. It keeps the working colour space and the dynamic range, the
layer tree with groups and fill layers, the selected layer, and a PNG pixel cache for every paint
layer. An SDR cache is 8-bit RGBA; an HDR cache is 16-bit RGBA so gradients keep their steps. The
cache is cropped to the tiles the layer actually uses and carries its rectangle, so a sparse 4K
sketch opens in under a millisecond instead of decoding thirty-three million empty pixels.
An explicit import (File > Import) fits the incoming document into the current canvas. Opening a
document from the welcome screen or File > Open does not fit: `dk_open_file` sizes the new editor to
the file. PNG and JPEG open as one raster layer through the same path. History and the recording tape
start over; import never merges into the current painting.

## Stroke tape

`.dkp` is a ZIP with a deflated JSON manifest, a deflated binary event stream and stored PNG
layer caches. The event stream is mostly repeated floats and small integers, so deflate cuts it by
roughly eight times; a PNG is already compressed, so it is stored rather than deflated twice. Version 2 adds the brush preset byte to stroke and erase events; version 1
tapes still open and replay with the soft round brush. Version 5 adds the brush radius policy (canvas
or screen) and version 6 the smart-fill flag. Version 7 adds the eraser strength, written after the
radius policy and read as full strength from older tapes. One event is a stroke segment (or a fill/clear) with its layer, pressure,
tilt, azimuth, roll and seed; a reader can replay it at another scale. The first slice supports a
uniform output scale and a canvas-following or screen-fixed brush radius. It does not yet store
brush presets or texture assets; those are required before the format is frozen.

Animation frames reuse this container rather than adding a new file type. Each frame is a complete
nested `.dkp` entry named `frame-<i>.dkp`; the manifest records `frames` and `activeFrame`. The
top-level tape remains the active frame, so older readers still open the selected picture, while a
new reader restores every frame with its own layer tree, events and undo-free pixel cache.
The manifest also carries the playback `fps`, per-frame `frameDurations`, the `loopEnabled` /
`loopStart` / `loopEnd` range, and one editable definition per brush slot in `brushes`. A stroke
event still names only its brush kind; opening the project restores that slot's hardness, flow,
spacing, jitter, angle, roundness, grain, paper/tip texture, wet, dual tip and pressure response
before replaying. The optional `textures` array stores one small grayscale paper/tip map. `loopEnd` is one past the last frame; zero means the end of the
timeline, which keeps old projects valid.

## Tests

- `read_kra_bytes` assembles a hand-built ZIP with a stored tile and checks planes, opacity and
  names.
- `write_kra` → `read_kra_bytes` round-trips two layers.
- `read_psd_bytes` uses two fixtures from [`chinedufn/psd`](https://github.com/chinedufn/psd)
  (MIT/Apache-2.0) to check visibility, offsets and layer pixels.
- `write_psd` → `read_psd_bytes` round-trips two layers.
- `write_dkp` → `read_dkp_bytes` round-trips every event kind, including layers and a seed, and
  rejects a truncated stream.

## References

- Krita tile data format: <https://community.kde.org/Krita/Tile_Data_Format>
- Krita file format notes: <https://docs.krita.org/en/general_concepts/file_formats/file_kra.html>
- Observed KRA details and limits: <https://github.com/SethRobinson/Patchy/blob/main/docs/kra.md>
- PSD fixtures and reader: <https://github.com/chinedufn/psd>
- PSD reader/writer: <https://github.com/Vasyanator/ag-psd-rs>

## Animation interchange

The native `.dkp` keeps every frame as a nested tape. For other software, DrawKit also writes:

- **PNG sequence**: a directory with `frame-0001.png`, `frame-0002.png`, … and an `animation.json`
  manifest carrying size, FPS, loop flag and frame durations. The same directory imports back as
  one paint layer per frame. This is the universal path for CSP, TVPaint, OpenToonz and Aseprite
  sequence import.
- **Aseprite** (`.aseprite`): one cel per frame, written with the open `aseprite-io` crate. The
  DrawKit layer tree is flattened for this export; the Aseprite timeline is the target, not a
  replacement for `.dkp`.

CSP `.clip` is proprietary. A future optional reader can use the MIT `clipfile` crate for import,
but `.clip` cannot carry the DrawKit stroke tape or its replayable brush parameters.
