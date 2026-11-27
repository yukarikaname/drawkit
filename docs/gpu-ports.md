# GPU compositing outside Apple

The painting core has two GPU entry points. macOS and iPad use the first; Windows uses the second
today, and a Linux shell can use either.

## 1. Window surface

`CanvasSurface` owns the swapchain, the resident layer stack and the paper shader. On Apple it is
created from a `CAMetalLayer` with `dk_canvas_surface_new`. Other systems use:

```c
dk_canvas_surface_new_native(
    void *display, uint64_t window, int platform,
    uint32_t width, uint32_t height, int hdr, int color_space);
```

| platform | window | display |
|---|---|---|
| 0 | WinUI `SwapChainPanel` (COM pointer) | unused |
| 1 | Win32 `HWND` | unused |
| 2 | Xlib `Window` (XID) | `Display *` |
| 3 | XCB `xcb_window_t` | `xcb_connection_t *` |
| 4 | Wayland `wl_surface *` | `wl_display *` |

The returned handle has the same upload, layer-spec, present and present-layers calls as the Apple
surface. `dk_canvas_surface_present_layers` composites the resident layers and draws them without a
readback. The window and display must stay alive until the surface is freed.

For a GTK4 Wayland shell:

```c
GdkDisplay *gdk = gtk_widget_get_display(widget);
struct wl_display *display = gdk_wayland_display_get_wl_display(gdk);
struct wl_surface *surface = gdk_wayland_window_get_wl_surface(
    gdk_wayland_display_get_default(), gtk_native_get_surface(native));
void *canvas = dk_canvas_surface_new_native(
    display, (uint64_t)surface, 4, width, height, 0, 0);
```

This only works for a shell whose window the toolkit does not draw into. GTK composites every
widget snapshot into that same `wl_surface`, so two renderers would attach buffers to one surface
and whichever commits last would win. `app-linux` therefore allocates a GBM buffer, imports it as
a wgpu texture and hands GTK the same buffer as a `GdkDmabufTexture`:

```c
/* Linux only. The fd is a dup of a gbm_bo; the call owns it. */
int code = dk_canvas_surface_present_dmabuf(
    canvas, fd, width, height, stride, offset, modifier,
    paper_x, paper_y, paper_width, paper_height);
```

The import is `wgpu-hal`'s Vulkan `texture_from_dmabuf_fd` plus
`wgpu::Device::create_texture_from_hal`, so no wgpu fork is involved. See
[linux-wayland.md](linux-wayland.md). Nothing in `crates/gpu` is Apple-specific any more.

## 2. Headless bitmap compositor

`GpuCompositor` is for shells that already own a bitmap, such as WinUI's `WriteableBitmap` or a
`GdkMemoryTexture`. It keeps the layers on the GPU, blends there, and returns one premultiplied
RGBA8 image:

```c
void *gpu = dk_gpu_compositor_new(width, height);
dk_gpu_compositor_reset(gpu, count, width, height);
dk_gpu_compositor_upload_layer(gpu, index, x, y, width, height, rgba);
dk_gpu_compositor_set_layer(gpu, index, opacity, blend, visible, fill,
                            r, g, b, a);
dk_gpu_compositor_composite(gpu, out_rgba, width, height);
```

`dk_gpu_compositor_new` returns null when the machine has no usable adapter. The shell should then
keep its existing CPU composite. Groups and clipping are not reproduced by this path; the Windows
shell falls back to the core composite while either is present. The byte budget defaults to 1 GiB
and can be changed with `dk_gpu_compositor_set_budget`.

## Build

The GPU library now produces an `rlib`, a static library and a `cdylib`:

```sh
cargo build --release --target x86_64-pc-windows-msvc -p drawkit-gpu
cargo build --release --target x86_64-unknown-linux-gnu -p drawkit-gpu
```

On Windows, `drawkit_gpu.dll` sits next to `drawkit_core.dll`. The WinUI shell loads it lazily, so
an older deployment without the DLL still paints through the core.

## Windows wiring

`PaintEngine` creates a compositor, uploads every layer on a full composite and uploads only the
dirty rectangle of the active layer after a segment. A full composite syncs layer opacity,
visibility, blend mode and fill colour. The result is copied into the existing `WriteableBitmap`;
the window still uses the Fluent controls and the same pointer input.

The zero-copy path is also live. `PaintEngine.AttachSwapChainPanel` takes the WinUI
`SwapChainPanel`, queries `ISwapChainPanelNative` (IID `63aad0b8-7c24-40ff-85a8-640d944cc325`),
and passes that COM pointer to `dk_canvas_surface_new_native` with platform 0. The Rust surface
draws the paper and the resident layers into the panel; no readback and no bitmap copy. Group
flattening and clipping follow the same rules as the bitmap compositor, and the `WriteableBitmap`
stays as the fallback for a missing DLL, no adapter, unsupported group or a failed surface.
