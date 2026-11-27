//! The zero-copy present entry point for Linux.
//!
//! The shell allocates the buffer (GBM), so it owns the memory and can hand the same buffer to
//! GTK. This entry only borrows the rendering device: the fd is a dup of that buffer and is owned
//! by the call, as `wgpu-hal`'s import takes ownership of it. Return codes are the
//! `surface::dmabuf::DMABUF_*` constants; `-4` usually means the driver refused the modifier.

use std::os::fd::FromRawFd;
use std::os::raw::c_void;

use super::canvas::canvas;

/// Render the resident canvas into `fd` and submit. `1` drawn, `0` no layers, negative is a
/// failure the shell reports verbatim (see `surface::dmabuf`).
///
/// # Safety
/// `fd` must be a live, writable, single-plane `DRM_FORMAT_XRGB8888` dmabuf of `width` by
/// `height` pixels with the given `stride`, `offset` and DRM `modifier`. The callee owns the fd.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn dk_canvas_surface_present_dmabuf(
    handle: *mut c_void,
    fd: i32,
    width: u32,
    height: u32,
    stride: u32,
    offset: u64,
    modifier: u64,
    paper_x: f32,
    paper_y: f32,
    paper_width: f32,
    paper_height: f32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return -1;
    };
    if fd < 0 {
        return -1;
    }
    // SAFETY: the caller promised a live fd for this call; the import takes it from here.
    let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
    surface.present_dmabuf(
        fd,
        width,
        height,
        stride,
        offset,
        modifier,
        [paper_x, paper_y, paper_width, paper_height],
    )
}
