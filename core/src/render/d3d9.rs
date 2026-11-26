//! Direct3D 9 device primitives owned by the `render` node: viewport query,
//! explicit `Present`, and the last presented device handle for still notices.
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D9::{IDirect3DDevice9, D3DVIEWPORT9};

/// The device the game presented its last frame with, for still notices.
static DEVICE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Records the live `IDirect3DDevice9*` observed during a frame present.
pub(crate) fn record_device(device: *mut c_void) {
    if !device.is_null() {
        DEVICE.store(device, Ordering::Relaxed);
    }
}

/// Clears the cached device pointer before a device reset.
pub(crate) fn clear_device() {
    DEVICE.store(std::ptr::null_mut(), Ordering::Relaxed);
}

/// The last device the game presented with, or null before the first frame or
/// after a device reset.
pub(crate) fn last_device() -> *mut c_void {
    DEVICE.load(Ordering::Relaxed)
}

/// The device's viewport size in pixels.
///
/// # Safety
/// `device` is null or a live `IDirect3DDevice9*`.
pub(crate) unsafe fn screen(device: *mut c_void) -> (f32, f32) {
    let Some(device) = IDirect3DDevice9::from_raw_borrowed(&device) else { return (0.0, 0.0) };
    let mut viewport = D3DVIEWPORT9::default();
    if device.GetViewport(&mut viewport).is_err() {
        return (0.0, 0.0);
    }
    (viewport.Width as f32, viewport.Height as f32)
}

/// Presents what was drawn, as the game's Present wrapper does, outside
/// the game's frame (for still notices and hidden-start occlusion handling).
///
/// # Safety
/// `device` is null or the game's live `IDirect3DDevice9*`.
pub(crate) unsafe fn present(device: *mut c_void) -> windows::core::Result<()> {
    let Some(device) = IDirect3DDevice9::from_raw_borrowed(&device) else { return Ok(()) };
    device.Present(std::ptr::null(), std::ptr::null(), windows::Win32::Foundation::HWND::default(), std::ptr::null())
}
