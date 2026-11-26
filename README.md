# DrawKit

A practical painting app with one Rust core and native shells. Its assist is a **tool**, not a
generator: a fill that reaches the line, layers that do not carry the old background into their
edges, and a stroke that does not lag behind the pen.

## Platforms

Only macOS is close to usable, and it is still unstable. The other shells are early, narrower ports.

| platform | shell | state |
|---|---|---|
| macOS | SwiftUI chrome, shared wgpu canvas and Rust colour picker | the most complete shell; still unstable |
| iPad | SwiftUI/UIKit, Pencil force and altitude, PencilKit import | painting, layers, mirror and settings; Metal canvas, HDR switch and EXR |
| Android | Jetpack Compose chrome over JNI | painting, layers and mirror |
| Linux | GTK4/libadwaita shell | canvas, brush/eraser/fill, colour, size, layers, undo/redo; GPU compositing with CPU fallback |
| Windows | WinUI 3, Fluent controls | preview; resident GPU layer compositing, CPU presentation |

The shells use native platform chrome around the same Rust canvas; `docs/ui.md` records the two
contracts: desktop (macOS, Windows) uses a docked inspector, left rail and keyboard; tablet
(iPadOS, Android) uses a floating top capsule, tool card, sheets and pen-sized targets.

## What makes it different

| problem | the fix |
|---|---|
| a fill leaves a white fringe | the fill goes **under** the line: the mask walks the ink band from the region up to the stroke's opaque core, so a soft four-pixel anti-aliased ramp is covered too and the outer edge is left alone. Filled inside opaque paint, it instead takes the missing coverage from the pixels (`1 - ink`). Gap closing, growth and softening stay exposed for leaks and for the fill's own outer edge |
| a detail should become its own layer | **rectangular selection**: drag a frame, extract it to a new layer, clear it, or clip a fill to it; desktop and iPad |
| a cut-out layer shows other colours at its edges | **decontamination**: `foreground = (pixel - (1 - a) * background) / a`, with `a` from the selection |
| ink trails the pen | **stroke prediction**: a least-squares line through the last few samples, evaluated one frame ahead |
| a quick line comes out wobbly | **Auto Shape**: an 11-16-4 tanh classifier over the finished stroke picks line, ellipse or rectangle; freehand is left alone |

The first learned piece is the shape classifier: about 300 weights trained on synthetic gestures by
`scripts/train_shape_model.py` (run it with uv). It runs in the core without a runtime or a network,
is off by default, and its replacement is one undo step. On a deliberately hard set it beats the
traditional geometry baseline 89.5% to 62.0% macro accuracy, replaces about half of the strokes at
0.8 confidence, and is wrong about one in fifty; the full comparison is in `docs/auto-shape.md`.

## Painting

- Pressure-sensitive brush with soft round, pencil, ink, marker, airbrush, bristle, spray, chalk
  and watercolour presets; a smudge tool that pulls existing paint along the pointer; an eraser
  through the same coverage mask with its **own size and opacity** (the strength is recorded in
  the tape); an eyedropper that keeps HDR stops; and a fill with tolerance, gap closing, growth
  and feathering that sits under the line when the paper shows through and over the paint when
  it does not.
- Layer groups with inherited visibility and group opacity, solid fill layers, ordinary paint
  layers, a colour tag and in-place rename per layer, reference layers that stay on the canvas
  but are skipped by every export, per-rectangle pixel undo, and an undo timeline that also covers adding, deleting,
  moving and changing a layer.
- A free mirror axis at any angle, with an optional perpendicular axis. Mirrored segments share
  the undo step and the tape.
- A pull-string stabiliser that records the line it actually drew, and a short extrapolated segment
  that hides input latency without ever reaching the undo stack or the tape.
- A paper-removal assist for imported ink: coverage becomes alpha and the colour is decontaminated,
  leaving transparent line art instead of a white page.
- sRGB, Display P3 or Adobe RGB documents; an opened PNG or JPEG keeps the space its ICC profile
  names instead of being treated as sRGB; HDR documents paint above paper white with a **Glow**
  control and a display exposure, and present through an extended-range swapchain; the eyedropper
  keeps the stops above white.
- A command tape (`.dkp`) that can be scrubbed: samples, pressure, tilt, seed and geometry are kept,
  so a painting can be replayed at another scale.
- Timelapse export: the tape is replayed into an H.264 MP4, not a screen recording. Thirty frames a
  second, automatically sped up to about a minute, rendered off the main thread.
- The macOS canvas keeps the layer textures resident on the GPU, SDR and HDR alike: an edit uploads
  only the dirty rectangle of the layer that changed, the blend modes run in a shader, and no
  pixels are read back for the screen. PNG, Copy Merged, the navigator preview and EXR all read
  that resident frame once; the core's CPU composite remains only as a no-surface fallback.
- The brush presets are a tray of raised tokens, not a system menu; the welcome screen's colour
  space choice is a row of cards with a one-line purpose (web, wide screen, print). Sliders use
  the native `Slider` at `.large`; steppers, switches and checkboxes stay `.regular` so they do
  not tower over the 12 pt labels. All are tinted with the app's vermilion, and they
  keep the system's animations and Liquid Glass instead of being redrawn. Custom drawing remains
  only where a selector needs a shape the system does not have (the brush tray, the colour-space
  cards, the radial menu); floating control layers use glass, content cards stay opaque. The layer
  palette is a native `List`: a swipe from the trailing edge reveals Delete through `swipeActions`,
  a press-and-drag lift reorders the row (SwiftUI has no pressure event, so a short long-press
  starts it), Return or F2 renames the focused layer, and clipping, transparency and lock stay in
  the three-dot menu.
- macOS gestures: two-finger pan, pinch zoom, swipe undo/redo, Option-swipe to step layers (with
  haptic feedback), and a Force Touch deep click opens an animated quick menu.
- A navigator in the canvas corner shows the whole painting on a carbon plate, refreshed with the
  layer thumbnails rather than on every segment.
- Tablet buttons: a pen's right and middle clicks can be mapped to a radial quick menu, eyedropper,
  undo/redo, brush/eraser, layer stepping or panning; the ring grows out of the nib over the canvas
  and switches brushes and recent colours with a spring reveal, and the first tablet event offers
  the Force Proportions mapping reminder.

## Files

- **DrawKit project** (`.dkp`) — the native format: the stroke tape plus the layer tree, with a
  per-layer pixel cache so a long painting opens immediately.
- **Krita** (`.kra`) and **Photoshop** (`.psd`) — read and write raster layers.
- **PNG / JPEG / WebP** — open and export; the raster exports flatten over paper, with a quality
  setting for JPEG and lossless WebP.
- **OpenEXR** — linear 32-bit export that keeps HDR highlights.
- **Layer PNGs** — each paint or fill layer as its own canvas-sized file, named in palette order.
- **MP4** — a timelapse rendered from the stroke tape, with paper behind the paint.

Limits and the exact Krita tile layout are in [`docs/formats.md`](docs/formats.md).

## Architecture

```text
crates/core     painting, layers, fill, prediction, history, recording, project file
                (tools.rs keeps the Editor and its state; tools/{paint,erase,fill,
                 io,tape,history,replay} hold the impl blocks and their tests;
                 document/{layers,composite} keep the tree and the pixel walk)
crates/formats  Krita, Photoshop, PNG/JPEG, EXR and the stroke tape
crates/gpu      wgpu renderer: compositor, brush stamps, morphology, selection,
                colour conversion, thumbnails, canvas surface and export. Resident
                layers are tiled and keep no CPU backup; an evicted tile is re-filled
                from the core, and the coldest GPU tile goes first. Fill, layer
                thumbnails and the export encode also run here.
                CanvasSurface now takes Win32, Xlib, XCB and Wayland windows;
                GpuCompositor serves bitmap shells with no swapchain.
crates/core/src/ffi/  editor, layers, rendering, picker and file C entry points
crates/jni      the Android bridge
app-macos       the macOS shell
  Sources/DrawKit/App      window, session, settings, onboarding
  Sources/DrawKit/Canvas   Metal canvas, navigator, radial overlay
  Sources/DrawKit/Model    PaintModel extensions, files, tape
  Sources/DrawKit/Panels   inspector, tool tray, colour
  Sources/DrawKit/Design   brand and motion tokens
app-ios         the iPad shell
app-android     the Android shell
app-windows     the WinUI 3 shell
app-linux       the GTK4/libadwaita shell
shared/strings.json   UI copy shared by every shell
```

Everything the app does lives in `core`, so a new shell is a window over the same behaviour, not a
second implementation. The canvas and colour wheel are drawn by Rust/wgpu and hosted in a
platform view.

## Build

```sh
cargo test                       # core, formats, figure and GPU tests
make app                         # dist/DrawKit.app
./scripts/build-linux.sh         # GTK4/libadwaita shell (Linux)
open dist/DrawKit.app
```

Requirements, per-platform instructions, packaging and CI are in
[`docs/build.md`](docs/build.md).

## Documentation

- [`docs/stroke-engine.md`](docs/stroke-engine.md) — the resolution-independent stroke model.
- [`docs/formats.md`](docs/formats.md) — what is read and written, and what is deliberately not.
- [`docs/hdr.md`](docs/hdr.md) — extended range, colour spaces and the GPU export.
- [`docs/perf.md`](docs/perf.md) — the measured brush, fill, layer and 8K numbers.
- [`docs/painting-runtime.md`](docs/painting-runtime.md) — the strategy and its benchmarks.
- [`docs/wet-media.md`](docs/wet-media.md) — the simulation field contract.
- [`docs/ui.md`](docs/ui.md) — the macOS, iPad and Android interfaces.

## Prior art

- [`sakihet/petit-paint`](https://github.com/sakihet/petit-paint) — a paint app in Rust.
- MyPaint, Paintlang, Solstice and Krita for brush and stroke models, documented in
  [`docs/stroke-engine.md`](docs/stroke-engine.md).

No paint app was found that hosts native controls per platform over one Rust core; that part is
ours to write, and it is why the core/shell split is the first decision in the repository.

## License

MIT. See [`LICENSE`](LICENSE).
