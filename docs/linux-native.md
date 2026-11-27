# DrawKit on Linux, after installing

This is what a person gets when they install the Flatpak or run `packaging/install.sh`, and what
is still missing. The build itself is in [linux.md](linux.md); the graphics route and its failure
codes are in [linux-wayland.md](linux-wayland.md).

## What works

**Projects.** File → Open, Save and Save As are `GtkFileDialog`, which is the XDG desktop portal
under Flatpak. The picker shows the `DrawKit project` filter for `.dkp` and a second filter for
Krita, Photoshop and flat images. The shell never asks for `--filesystem=home`: the portal hands
it a readable path inside the sandbox, and the document portal keeps that path valid for the
session. A `.dkp` double-clicked in Files arrives through the same mechanism: the desktop entry's
`Exec=drawkit %f` and the `application/x-drawkit-strokes` MIME type install the association, and
`GtkApplication` opens it at startup (or in the running instance).

**The canvas.** On a Wayland session with a dmabuf-capable Vulkan driver the canvas is
zero-copy: a GBM buffer is imported into wgpu as a texture, the resident layers and the live
stroke are rendered into it, and the same buffer is shown as a `GdkDmabufTexture` inside a
`GtkGraphicsOffload`. GTK does not repaint the canvas area and the compositor scans the buffer out
directly, so a stroke costs no readback and no full-frame copy. The status bar says `GPU canvas
(dmabuf)`.

There is no fallback picture. If the render node is missing, the GPU has no
`VK_EXT_external_memory_dma_buf`, or the driver refuses the buffer's modifier, the canvas stays
empty and the status bar carries the concrete reason (for example `the driver refused the dmabuf
(modifier or stride)`). That is deliberate: a slower picture would hide a broken export.

**Scaling.** The buffer is allocated at the drawing area's size in device pixels, and the same
paper rectangle maps the pointer into the document, so a HiDPI or fractionally scaled session
lines up. The canvas direction is pinned left-to-right even in a right-to-left desktop: the
document, the pointer mapping and the brush ring are document coordinates, and mirroring them
would move ink.

**Right-to-left.** `DRAWKIT_LAYOUT_DIRECTION=rtl` (or `ltr`) forces the layout direction for a
review run; unset, GTK follows the locale. The tool bar, the layer list, the status bar and the
dialogs mirror; shortcuts (`Ctrl+O`, `Ctrl+S`, `Ctrl+Shift+S`) and their labels do not reorder.

## What is missing

- **NVIDIA proprietary** is not proven; its dma-buf export has historically not offered modifiers
  GTK accepts, and the status bar will say so rather than fall back.
- **HDR.** The import is eight-bit `Bgra8Unorm`; an extended-range canvas is rejected.
- **X11.** Not a supported session; the canvas says it needs Wayland.
- **Export.** The shell saves `.dkp`; PNG/JPEG/PSD/KRA writers exist in `crates/formats` but are not
  wired to a menu yet.
- **Zoom, pan, selection and the reference layer UI** are still macOS/iPad features.

## Checking it

```sh
flatpak-builder --user --install --force-clean build-flatpak flatpak/dev.drawkit.DrawKit.yml
flatpak run dev.drawkit.DrawKit
```

Without Flatpak, `packaging/install.sh` installs the same files under `$PREFIX` (`/usr` by
default) after building the shell. On either install, `update-mime-database` and
`update-desktop-database` need to have run once for the `.dkp` association to appear.

A headless proof of the buffer path (no window, no compositor) is the ignored test in
`crates/gpu/src/surface/dmabuf.rs`:

```sh
cargo test -p drawkit-gpu --lib -- --ignored a_gbm_buffer
```
