//! Window-system surfaces for Windows and Linux.
//!
//! Apple shells pass a `CAMetalLayer`; the other shells own a native window and pass its raw
//! handles instead. Keeping this in its own file leaves the shared surface, present path and paper
//! shader in `surface.rs`.

use super::*;

#[allow(unused_variables)]
unsafe fn native_target(
    display: *mut c_void,
    window: u64,
    platform: i32,
) -> Option<wgpu::SurfaceTargetUnsafe> {
    #[cfg(target_os = "windows")]
    {
        if platform == 0 {
            return Some(wgpu::SurfaceTargetUnsafe::SwapChainPanel(
                window as *mut c_void,
            ));
        }
        if platform == 1 {
            use raw_window_handle::{
                RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
            };
            let handle = Win32WindowHandle::new(std::num::NonZeroIsize::new(window as isize)?);
            return Some(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
                raw_window_handle: RawWindowHandle::Win32(handle),
            });
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        use raw_window_handle::{
            RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
            XcbDisplayHandle, XcbWindowHandle, XlibDisplayHandle, XlibWindowHandle,
        };
        let display = std::ptr::NonNull::new(display)?;
        let (display, window) = match platform {
            2 => (
                RawDisplayHandle::Xlib(XlibDisplayHandle::new(Some(display), 0)),
                RawWindowHandle::Xlib(XlibWindowHandle::new(window as _)),
            ),
            3 => (
                RawDisplayHandle::Xcb(XcbDisplayHandle::new(Some(display), 0)),
                RawWindowHandle::Xcb(XcbWindowHandle::new(std::num::NonZeroU32::new(
                    window as u32,
                )?)),
            ),
            4 => (
                RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display)),
                RawWindowHandle::Wayland(WaylandWindowHandle::new(std::ptr::NonNull::new(
                    window as *mut c_void,
                )?)),
            ),
            _ => return None,
        };
        Some(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(display),
            raw_window_handle: window,
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = (display, window, platform);
        None
    }
}

impl CanvasSurface {
    /// A surface on a native window: WinUI `SwapChainPanel`, Win32 `HWND`, Xlib, XCB or Wayland.
    ///
    /// `platform` is 0 for a WinUI `SwapChainPanel`, 1 for a Win32 `HWND`, 2 for Xlib, 3 for XCB
    /// and 4 for Wayland. `display` is the X11 `Display*` / `xcb_connection_t*` / `wl_display*`;
    /// `window` is the HWND, XID, xcb window id or `wl_surface*`. The window and display must
    /// outlive the returned surface.
    ///
    /// # Safety
    /// The raw handles must be valid and must remain valid for the surface's lifetime.
    pub unsafe fn new_native(
        display: *mut c_void,
        window: u64,
        platform: i32,
        width: u32,
        height: u32,
        hdr: bool,
        color_space: i32,
    ) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let target = unsafe { native_target(display, window, platform) }?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface: wgpu::Surface<'static> =
            unsafe { instance.create_surface_unsafe(target) }.ok()?;
        Self::from_surface(instance, surface, width, height, hdr, color_space)
    }
}
