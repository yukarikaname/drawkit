// The zero-copy canvas buffer on Linux: one GBM buffer rendered by wgpu and scanned out by GTK.
//
// The buffer is ours. Each frame takes a dup of its fd for the renderer (the import owns that fd)
// and another dup for `GdkDmabufTextureBuilder` (GDK owns that one), so nothing is copied and no
// window surface is shared. `docs/linux-wayland.md` explains the route.
//
// Two buffers are kept so a frame GTK still holds is never overwritten. The builder's release
// callback marks a buffer free after the compositor is done with it.

use std::ffi::{c_int, c_uint, c_void};
use std::fs::{File, OpenOptions};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// `fourcc('X','R','2','4')`: little-endian XRGB, which GTK reads as opaque.
const GBM_FORMAT_XRGB8888: u32 = 0x3432_5258;
/// `fourcc('A','B','3','0')`: 10-bit ABGR, the HDR buffer a Wayland compositor takes as a
/// BT.2100 signal once the surface declares one.
const GBM_FORMAT_ABGR2101010: u32 = 0x3033_4241;
const GBM_BO_USE_RENDERING: u32 = 4;
const BUFFER_COUNT: usize = 2;

#[link(name = "gbm")]
unsafe extern "C" {
    fn gbm_create_device(fd: c_int) -> *mut c_void;
    fn gbm_device_destroy(device: *mut c_void);
    fn gbm_bo_create(
        device: *mut c_void,
        width: c_uint,
        height: c_uint,
        format: c_uint,
        flags: c_uint,
    ) -> *mut c_void;
    fn gbm_bo_destroy(bo: *mut c_void);
    fn gbm_bo_get_fd(bo: *mut c_void) -> c_int;
    fn gbm_bo_get_stride(bo: *mut c_void) -> c_uint;
    fn gbm_bo_get_offset(bo: *mut c_void, plane: c_int) -> c_uint;
    fn gbm_bo_get_modifier(bo: *mut c_void) -> u64;
}

/// One buffer in flight. The renderer and GTK each get a dup of the same GBM object.
pub(crate) struct Frame {
    pub(crate) buffer: usize,
    pub(crate) fd: i32,
    pub(crate) stride: u32,
    pub(crate) offset: u64,
    pub(crate) modifier: u64,
}

struct Buffer {
    bo: *mut c_void,
    /// True while a `GdkTexture` built from this buffer is still alive.
    busy: Arc<AtomicBool>,
}

pub(crate) struct BufferSet {
    device: *mut c_void,
    /// Keeps the DRM render node open for as long as the GBM device uses it.
    node: File,
    buffers: Vec<Buffer>,
    width: u32,
    height: u32,
    fourcc: u32,
    imported: Vec<bool>,
    next: usize,
}

impl BufferSet {
    pub(crate) fn new(width: u32, height: u32, hdr: bool) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("canvas has no size".to_string());
        }
        let format = if hdr { GBM_FORMAT_ABGR2101010 } else { GBM_FORMAT_XRGB8888 };
        let node = open_render_node()?;
        let device = unsafe { gbm_create_device(node.as_raw_fd()) };
        if device.is_null() {
            return Err("gbm_create_device failed".to_string());
        }
        let mut buffers: Vec<Buffer> = Vec::with_capacity(BUFFER_COUNT);
        for _ in 0..BUFFER_COUNT {
            let bo = unsafe {
                gbm_bo_create(device, width, height, format, GBM_BO_USE_RENDERING)
            };
            if bo.is_null() {
                for buffer in &buffers {
                    unsafe { gbm_bo_destroy(buffer.bo) };
                }
                unsafe { gbm_device_destroy(device) };
                return Err("gbm_bo_create refused the size or format".to_string());
            }
            buffers.push(Buffer {
                bo,
                busy: Arc::new(AtomicBool::new(false)),
            });
        }
        Ok(Self {
            device,
            node,
            buffers,
            fourcc: format,
            imported: vec![false; BUFFER_COUNT],
            width,
            height,
            next: 0,
        })
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn fourcc(&self) -> u32 {
        self.fourcc
    }

    /// The next buffer GTK is not holding, with an fd the renderer takes ownership of.
    /// Whether this buffer's memory has been imported into the renderer. A `BufferSet` is rebuilt
    /// on resize, so a new set re-imports everything.
    pub(crate) fn is_imported(&self, index: usize) -> bool {
        self.imported.get(index).copied().unwrap_or(false)
    }

    pub(crate) fn mark_imported(&mut self, index: usize) {
        if let Some(slot) = self.imported.get_mut(index) {
            *slot = true;
        }
    }

    pub(crate) fn acquire(&mut self) -> Option<Frame> {
        let start = self.next;
        for step in 0..self.buffers.len() {
            let index = (start + step) % self.buffers.len();
            if self.buffers[index].busy.load(Ordering::Acquire) {
                continue;
            }
            let bo = self.buffers[index].bo;
            let fd = unsafe { gbm_bo_get_fd(bo) };
            if fd < 0 {
                return None;
            }
            self.next = (index + 1) % self.buffers.len();
            return Some(Frame {
                buffer: index,
                fd,
                stride: unsafe { gbm_bo_get_stride(bo) },
                offset: u64::from(unsafe { gbm_bo_get_offset(bo, 0) }),
                modifier: unsafe { gbm_bo_get_modifier(bo) },
            });
        }
        None
    }

    /// Wrap the rendered buffer as a texture for GTK. The builder takes its own dup of the fd.
    pub(crate) fn texture(&self, frame: &Frame) -> Result<gtk::gdk::Texture, String> {
        let bo = self.buffers[frame.buffer].bo;
        let fd = unsafe { gbm_bo_get_fd(bo) };
        if fd < 0 {
            return Err("gbm_bo_get_fd failed for GTK".to_string());
        }
        let builder = gtk::gdk::DmabufTextureBuilder::new();
        builder.set_fd(0, fd);
        builder.set_n_planes(1);
        builder.set_fourcc(GBM_FORMAT_XRGB8888);
        builder.set_modifier(frame.modifier);
        builder.set_width(self.width);
        builder.set_height(self.height);
        builder.set_stride(0, frame.stride);
        builder.set_offset(0, frame.offset as u32);
        // The builder only stores the fd: GTK keeps it until the texture is released, so the
        // release callback closes it and frees the buffer for the next frame.
        // SAFETY: the fd is ours and is closed exactly once, in the callback.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let busy = self.buffers[frame.buffer].busy.clone();
        busy.store(true, Ordering::Release);
        // SAFETY: the descriptor stays open for the texture's lifetime and the bo outlives it.
        unsafe {
            builder.build_with_release_func(move || {
                busy.store(false, Ordering::Release);
                drop(fd);
            })
        }
        .map_err(|error| format!("dmabuf texture refused: {error}"))
    }
}

impl Drop for BufferSet {
    fn drop(&mut self) {
        for buffer in &self.buffers {
            unsafe { gbm_bo_destroy(buffer.bo) };
        }
        unsafe { gbm_device_destroy(self.device) };
        let _ = &self.node;
    }
}

/// The first DRM render node. `DRAWKIT_DRM_DEVICE` overrides it for a test rig.
fn open_render_node() -> Result<File, String> {
    if let Some(path) = std::env::var_os("DRAWKIT_DRM_DEVICE") {
        return OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| format!("cannot open {}: {error}", path.to_string_lossy()));
    }
    for index in 128..136 {
        let path = format!("/dev/dri/renderD{index}");
        if let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) {
            return Ok(file);
        }
    }
    Err("no DRM render node (/dev/dri/renderD*)".to_string())
}
