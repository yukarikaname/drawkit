# DrawKit for Linux

The Linux shell is a GTK4/libadwaita application written in Rust. It links `drawkit-core` and
`drawkit-gpu` directly; it does not go through the C ABI that the Apple and Windows shells use.

## Build

Ubuntu 24.04 needs the GTK and Vulkan development packages:

```sh
sudo apt-get install -y libgtk-4-dev libadwaita-1-dev \
    libvulkan-dev mesa-vulkan-drivers
./scripts/build-linux.sh
./build/linux/drawkit-linux
```

The shell lives in its own cargo workspace (`app-linux/Cargo.toml`), so a macOS or iOS checkout
does not need GTK installed. CI builds it in the Linux job.

## Current shell

- 1024 × 1024 document, brush / eraser / fill, colour, size and opacity
- undo and redo
- layer list with selection, visibility, add, add group, remove and sibling move
- GPU layer compositing through `GpuCompositor`, with the core CPU composite as fallback for a
  missing adapter, a group with non-normal blend/opacity, or a clipping base that is a group
- the result is presented as a `GdkMemoryTexture` in a `GtkPicture`

## Next

GTK owns the window's Wayland/X11 surface, so the zero-copy `CanvasSurface` path needs either a
custom `GdkSurface`/`GtkGLArea` or a child surface. Until then the headless compositor is the
presentation path. File open/save, selection, zoom/pan and the reference layer UI are also still
to be wired.
