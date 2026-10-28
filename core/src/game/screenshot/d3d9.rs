//! Reading the frame back from the game's Direct3D 9 device, in steps that
//! keep the main thread from waiting for the GPU. Reading a render target
//! into memory waits until the GPU has drawn everything it was given; at the
//! frame's present, the GPU is still drawing it. So:
//!
//! 1. [`FrameCopy::start`], at the frame's present: the back buffer is
//!    copied on the GPU into a render target of the framework's own (which
//!    also resolves multisampling), and an event query marks the copy's end.
//! 2. [`FrameCopy::read`], at a later present, once the query says the GPU
//!    is past the copy: the copy is read into a surface in system memory.
//! 3. [`Reading::lock`], at that present or a later one, once the transfer
//!    is done: the surface is locked, and its pixels can be read from any
//!    thread until the [`Locked`] is dropped, on the main thread.
//!
//! The main thread waits for the GPU only when it has not caught up after
//! [`PATIENCE`] frames. Everything here runs on the thread that presents.
use crate::game::{Fail, GResult, Status};
use std::ffi::c_void;
use windows::core::{Error, Interface, HRESULT};
use windows::Win32::Foundation::S_FALSE;
use windows::Win32::Graphics::Direct3D9::*;

/// Frames presented before the main thread waits for the GPU after all.
pub const PATIENCE: u32 = 4;

const D3DERR_WASSTILLDRAWING: HRESULT = HRESULT(0x8876_021c_u32 as i32);
const D3DERR_DEVICELOST: HRESULT = HRESULT(0x8876_0868_u32 as i32);
const D3DERR_DEVICENOTRESET: HRESULT = HRESULT(0x8876_0869_u32 as i32);

/// A failed step, `what` saying which: not ready while the device is lost,
/// as when another program has the screen, unavailable otherwise.
fn failed(what: &'static str) -> impl Fn(Error) -> Fail {
    move |e| {
        let lost = matches!(e.code(), D3DERR_DEVICELOST | D3DERR_DEVICENOTRESET);
        let status = if lost { Status::NotReady } else { Status::Unavailable };
        Fail { status, message: format!("{what}: {e}") }
    }
}

fn missing(what: &'static str) -> Fail {
    Fail { status: Status::Unavailable, message: format!("{what}: none made") }
}

/// The device the game presents with.
pub unsafe fn device<'a>(device: &'a *mut c_void) -> Option<&'a IDirect3DDevice9> {
    IDirect3DDevice9::from_raw_borrowed(device)
}

/// What a frame is, as its back buffer says.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub width: u32,
    pub height: u32,
    pub format: D3DFORMAT,
    pub multisample: D3DMULTISAMPLE_TYPE,
}

impl Shape {
    /// Whether its pixels are 4 bytes each: blue, green, red and alpha or
    /// nothing, which is what a screenshot reads.
    pub fn readable(&self) -> bool {
        matches!(self.format, D3DFMT_X8R8G8B8 | D3DFMT_A8R8G8B8)
    }
}

/// The frame copied on the GPU.
pub struct FrameCopy {
    target: IDirect3DSurface9,
    done: Option<IDirect3DQuery9>,
    pub shape: Shape,
    /// Frames presented since.
    frames: u32,
}

impl FrameCopy {
    /// Copies the back buffer, as it is about to be presented, on the GPU.
    pub unsafe fn start(device: &IDirect3DDevice9) -> GResult<Self> {
        let back = device.GetBackBuffer(0, 0, D3DBACKBUFFER_TYPE_MONO).map_err(failed("the back buffer"))?;
        let mut desc = D3DSURFACE_DESC::default();
        back.GetDesc(&mut desc).map_err(failed("the back buffer's description"))?;
        let shape = Shape { width: desc.Width, height: desc.Height, format: desc.Format, multisample: desc.MultiSampleType };
        if !shape.readable() {
            let message = format!("the frame's pixel format is D3DFORMAT {}, which a screenshot cannot read", desc.Format.0);
            return Err(Fail { status: Status::Unavailable, message });
        }
        let mut target = None;
        device
            .CreateRenderTarget(desc.Width, desc.Height, desc.Format, D3DMULTISAMPLE_NONE, 0, false, &mut target, std::ptr::null_mut())
            .map_err(failed("a render target to copy the frame to"))?;
        let target = target.ok_or_else(|| missing("a render target to copy the frame to"))?;
        device.StretchRect(&back, std::ptr::null(), &target, std::ptr::null(), D3DTEXF_NONE).map_err(failed("copying the frame"))?;
        // Without a query the copy is given PATIENCE frames.
        let done = device.CreateQuery(D3DQUERYTYPE_EVENT).ok().filter(|q| q.Issue(D3DISSUE_END).is_ok());
        Ok(Self { target, done, shape, frames: 0 })
    }

    /// At a later present: whether the GPU has made the copy, or has had
    /// [`PATIENCE`] frames to.
    pub unsafe fn ready(&mut self) -> GResult<bool> {
        self.frames += 1;
        let Some(done) = &self.done else { return Ok(self.frames >= PATIENCE) };
        // Through the vtable: the wrapper takes S_FALSE, not yet, for done.
        let result = (Interface::vtable(done).GetData)(done.as_raw(), std::ptr::null_mut(), 0, D3DGETDATA_FLUSH);
        if result == S_FALSE {
            return Ok(self.frames >= PATIENCE);
        }
        result.ok().map(|()| true).map_err(failed("waiting for the frame's copy"))
    }

    /// Starts reading the copy into system memory.
    pub unsafe fn read(&self, device: &IDirect3DDevice9) -> GResult<Reading> {
        let mut memory = None;
        let (width, height, format) = (self.shape.width, self.shape.height, self.shape.format);
        device
            .CreateOffscreenPlainSurface(width, height, format, D3DPOOL_SYSTEMMEM, &mut memory, std::ptr::null_mut())
            .map_err(failed("a surface in memory for the frame"))?;
        let memory = memory.ok_or_else(|| missing("a surface in memory for the frame"))?;
        device.GetRenderTargetData(&self.target, &memory).map_err(failed("reading the frame back"))?;
        Ok(Reading { memory, tries: 0 })
    }
}

/// The frame being read into system memory.
pub struct Reading {
    memory: IDirect3DSurface9,
    tries: u32,
}

impl Reading {
    /// The surface locked, once the transfer is done; None while it is not,
    /// for [`PATIENCE`] tries, after which it waits.
    pub unsafe fn lock(&mut self) -> GResult<Option<Locked>> {
        self.tries += 1;
        let mut locked = D3DLOCKED_RECT::default();
        match self.memory.LockRect(&mut locked, std::ptr::null(), (D3DLOCK_READONLY | D3DLOCK_DONOTWAIT) as u32) {
            Ok(()) => {}
            Err(e) if e.code() == D3DERR_WASSTILLDRAWING && self.tries < PATIENCE => return Ok(None),
            Err(e) if e.code() == D3DERR_WASSTILLDRAWING => {
                self.memory.LockRect(&mut locked, std::ptr::null(), D3DLOCK_READONLY as u32).map_err(failed("locking the frame in memory"))?
            }
            Err(e) => return Err(failed("locking the frame in memory")(e)),
        }
        Ok(Some(Locked { memory: self.memory.clone(), bits: locked.pBits as *const u8, pitch: locked.Pitch as usize }))
    }
}

/// The frame in system memory, locked until dropped.
pub struct Locked {
    memory: IDirect3DSurface9,
    pub bits: *const u8,
    /// Bytes from one row to the next.
    pub pitch: usize,
}

impl Drop for Locked {
    fn drop(&mut self) {
        unsafe {
            let _ = self.memory.UnlockRect();
        }
    }
}
