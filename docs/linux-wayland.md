# The Wayland canvas

The GTK shell is Wayland-only and presents the canvas through a dma-buf. The shell allocates the
buffer outside wgpu, imports it as a texture, renders into it, and hands the same buffer to GTK;
nothing is read back and no `wl_surface` is shared with GTK's renderer.

X11 is not a supported session: the route below is a Wayland protocol
(`zwp_linux_dmabuf_v1`), and an X11 port would be a second presentation path nobody tests. On
X11 the canvas stays empty and the status bar says `needs a Wayland session`.

## the path

1. **Allocate.** `app-linux/src/dmabuf.rs` opens the first `/dev/dri/renderD*`
   (`DRAWKIT_DRM_DEVICE` overrides it), calls `gbm_create_device`, then `gbm_bo_create` at the
   drawing area's size in device pixels with `DRM_FORMAT_XRGB8888` and `GBM_BO_USE_RENDERING`.
   Two buffers are kept so one can be on screen while the next is rendered.
2. **Import and render.** `dk_canvas_surface_present_dmabuf(handle, fd, width, height, stride,
   offset, modifier, paper)` takes a dup of `gbm_bo_get_fd` and calls wgpu-hal 30's
   `vulkan::Device::texture_from_dmabuf_fd(fd, &TextureDescriptor, modifier, stride, offset)`,
   which uses `VK_EXT_external_memory_dma_buf` and `VK_EXT_image_drm_format_modifier`. wgpu wraps
   that texture with `Device::create_texture_from_hal::<hal::api::Vulkan>`. The resident layer
   stack is composited into the painting texture, then the paper pass draws into the imported
   `Bgra8Unorm` texture (`surface/dmabuf.rs`, `draw_frame(..., imported = true)`).
3. **Hand it to GTK.** A second `gbm_bo_get_fd` dup goes into `GdkDmabufTextureBuilder`
   (`set_fd`, `set_n_planes`, `set_fourcc`, `set_modifier`, `set_width`, `set_height`,
   `set_stride`, `set_offset`), and `build_with_release_func` returns a `GdkTexture`. The texture
   sits in a `GtkPicture` inside a `GtkGraphicsOffload` with `enabled = ALWAYS`, so GTK places it
   in a Wayland subsurface and the compositor scans it out. GTK's own renderer never paints the
   canvas area.
4. **Pace it.** The builder's release callback closes the GTK fd and marks the buffer free; the
   next present only uses a buffer that is not in flight. Presents are driven by the
   `GdkFrameClock` tick (`add_tick_callback`), one composite per tick, never per pointer sample.

## who owns what

- The **render-node fd** is held by `BufferSet` for the GBM device's lifetime.
- The **render fd** is produced by `gbm_bo_get_fd`, passed to `present_dmabuf`, and owned by the
  call: Vulkan takes it on success and closes it on failure.
- The **GTK fd** is another `gbm_bo_get_fd` dup, stored by `GdkDmabufTextureBuilder` (which does
  *not* take ownership) and closed by the release callback after the texture is gone.
- The **`gbm_bo`** stays alive in `BufferSet`; an already-built `GdkTexture` keeps an fd, so a
  resize that drops the set cannot free memory out from under a frame on screen.

## the device feature

`CanvasSurface` requests `wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF` at device creation when
the adapter advertises it (`surface.rs`). Without it the import fails with
`dmabuf: VK_EXT_external_memory_dma_buf was not enabled`. The entry returns a code, and the shell
puts one sentence in the status bar:

| code | meaning | status line |
|---|---|---|
| 1 | drawn | `GPU canvas (dmabuf)` |
| 0 | the stack has no layers | `the canvas has no layers` |
| -2 | not the Vulkan backend | `the device is not Vulkan` |
| -3 | feature not enabled | `VK_EXT_external_memory_dma_buf is missing` |
| -4 | import refused | `the driver refused the dmabuf (modifier or stride)` |
| -5 | paper pass failed | `the paper pass could not be encoded` |

There is no fallback picture. A person gets one honest sentence and an empty canvas, which is the
only way a broken export shows up before release.

## what is still open

- **NVIDIA proprietary** has historically not exported dma-bufs with the modifiers GTK accepts.
  The entry reports `-4` there; the fix is driver-side, not another renderer.
- **Frame pacing** is one buffer in flight with two buffers allocated; a frame the compositor
  still holds is skipped rather than waited on. A present fence (`VK_EXT_swapchain_maintenance1`
  is not applicable without a swapchain; `wl_buffer.release` via the release callback is what GTK
  gives us) would let the shell block for the right buffer instead of skipping.
- **HDR** is not wired: the import is `Bgra8Unorm`, so `present_dmabuf` rejects an extended-range
  canvas with `-1`.
- **GTK older than 4.14** has no `GdkDmabufTextureBuilder`/`GtkGraphicsOffload`; the Flatpak pins
  25.08 and the container/CI use Ubuntu 24.04, both new enough.

## why no wgpu fork

The earlier plan assumed wgpu could not export a dma-buf. It cannot, but wgpu-hal 30 can *import*
one (`texture_from_dmabuf_fd`) and wgpu can wrap a hal texture (`create_texture_from_hal`). Owning
the buffer on the shell side turns the missing export API into a missing convenience: GBM already
exports, so no `wgpu-hal` patch and no rebase burden. The remaining cost is the device feature and
the ownership rules above, both checked in `surface/dmabuf.rs`'s ignored round-trip test.
