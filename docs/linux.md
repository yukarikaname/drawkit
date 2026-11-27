# DrawKit for Linux

The Linux shell is a GTK4/libadwaita application written in Rust. It links `drawkit-core` and
`drawkit-gpu`; the canvas goes through the same `dk_canvas_surface_*` C entry points the Apple,
Windows and Android shells use. It targets **Wayland only** and presents through a dma-buf.

## Build

Ubuntu 24.04 needs the GTK, libadwaita, Wayland, GBM and Vulkan development packages:

```sh
sudo apt-get install -y libgtk-4-dev libadwaita-1-dev libwayland-dev \
    libgbm-dev libvulkan-dev mesa-vulkan-drivers
./scripts/build-linux.sh
./build/linux/drawkit-linux
```

The shell lives in its own cargo workspace (`app-linux/Cargo.toml`), so a macOS or iOS checkout
does not need GTK installed. CI builds it in the Linux job. To install it as an application rather
than run it from the build directory, use the Flatpak manifest in `flatpak/` or
`packaging/install.sh`; what a person gets is in [linux-native.md](linux-native.md).

## Current shell

- 1024 × 1024 document, brush / eraser / fill, colour, size and opacity
- undo and redo, with the GPU tile journal as the record for GPU strokes and fills
- layer list with selection, visibility, add, add group, remove and sibling move
- a GBM buffer is imported into wgpu as a texture, the resident layers and the live stroke are
  rendered into it, and GTK scans the same buffer out as a `GdkDmabufTexture` inside a
  `GtkGraphicsOffload`: no readback, no swapchain on the window's surface
- there is no fallback picture; a missing render node, Vulkan feature or importable modifier is
  one sentence in the status bar and an empty canvas
- File → Open / Save / Save As through `GtkFileDialog` (the desktop portal under Flatpak), `.dkp`
  only for saving, and double-click on a `.dkp` opens it

## Why Wayland only

The presentation path is `zwp_linux_dmabuf_v1`, which only exists on Wayland, and the alternative
(X11) would be a second renderer sharing one window with GTK — a path nobody would test. On X11 the
canvas stays empty and the status bar says so. The route, the ownership rules and the failure
codes are in [linux-wayland.md](linux-wayland.md).

## Next

Selection, zoom/pan, export, HDR and the reference layer UI are still to be wired.
