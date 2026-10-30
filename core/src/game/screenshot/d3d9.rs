//! Reading the frame back from the game's Direct3D 9 device, in steps that
//! keep the main thread from waiting for the GPU. At the frame's present the
//! GPU is still drawing it, and reading a render target into memory waits
//! until the GPU has done everything it was given before: in the game,
//! locking a surface read back at a present took the main thread 12 to 68
//! ms (docs/research/screenshots.md). So each step is queued on the GPU with
//! an event query after it, and the next one waits for the query. The
//! queries are only looked at, never flushed: the game's own Present hands
//! the GPU what was queued, while it waits for the display anyway, where a
//! flush right after the transfer was queued took the main thread 8 to 31
//! ms.
//!
//! 1. [`FrameCopy::start`], at the frame's present: the back buffer is
//!    copied on the GPU into a render target of the framework's own (which
//!    also resolves multisampling).
//! 2. [`FrameCopy::read`], at a later present, once the GPU is past the
//!    copy: the copy is read into a surface in system memory.
//! 3. [`Reading::lock`], at a later present, once the GPU is past that
//!    transfer: the surface is locked, and its pixels can be read from any
//!    thread until the [`Locked`] is dropped, on the main thread.
//!
//! The main thread waits for the GPU only when a step is not done after
//! [`PATIENCE`] presents. Everything here runs on the thread that
//! presents.
use crate::game::{Fail, GResult, Status};
use std::ffi::c_void;
use std::time::{Duration, Instant};
use windows::core::{Error, Interface, HRESULT};
use windows::Win32::Foundation::S_FALSE;
use windows::Win32::Graphics::Direct3D9::*;

/// Presents after which the main thread waits for the GPU after all.
pub const PATIENCE: u32 = 8;

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
///
/// # Safety
/// `device` is null or the game's live device.
pub unsafe fn device(device: &*mut c_void) -> Option<&IDirect3DDevice9> {
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
    /// Presents since.
    frames: u32,
}

impl FrameCopy {
    /// Copies the back buffer, as it is about to be presented, on the GPU.
    ///
    /// # Safety
    /// On the main thread, at a present of `device`.
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
        // Without a query the copy is given PATIENCE presents.
        let done = device.CreateQuery(D3DQUERYTYPE_EVENT).ok().filter(|q| q.Issue(D3DISSUE_END).is_ok());
        Ok(Self { target, done, shape, frames: 0 })
    }

    /// At a later present: whether the GPU has made the copy, or has had
    /// [`PATIENCE`] presents to.
    ///
    /// # Safety
    /// On the main thread, at a present.
    pub unsafe fn ready(&mut self) -> GResult<bool> {
        self.frames += 1;
        Ok(passed(&self.done).map_err(failed("waiting for the frame's copy"))? || self.frames >= PATIENCE)
    }

    /// Starts reading the copy into system memory: into `kept`, the surface
    /// the frame before was read into, when the frame is the same shape.
    ///
    /// # Safety
    /// On the main thread, at a present of `device`.
    pub unsafe fn read(&mut self, device: &IDirect3DDevice9, kept: Option<Memory>) -> GResult<Reading> {
        let started = Instant::now();
        let (memory, made) = match kept.filter(|kept| kept.fits(&self.shape)) {
            Some(kept) => (kept, None),
            None => {
                let mut surface = None;
                let (width, height, format) = (self.shape.width, self.shape.height, self.shape.format);
                device
                    .CreateOffscreenPlainSurface(width, height, format, D3DPOOL_SYSTEMMEM, &mut surface, std::ptr::null_mut())
                    .map_err(failed("a surface in memory for the frame"))?;
                let surface = surface.ok_or_else(|| missing("a surface in memory for the frame"))?;
                (Memory { surface, shape: self.shape }, Some(started.elapsed()))
            }
        };
        let started = Instant::now();
        device.GetRenderTargetData(&self.target, &memory.surface).map_err(failed("reading the frame back"))?;
        let transfer = started.elapsed();
        // The same query, issued again, marks the transfer's end.
        let done = self.done.take().filter(|q| q.Issue(D3DISSUE_END).is_ok());
        Ok(Reading { memory, done, tries: 0, made, transfer, locking: Duration::ZERO })
    }
}

/// Whether the GPU is past where the event query `done` was issued; false
/// with no query, which leaves the waiting to [`PATIENCE`]. Never flushes.
unsafe fn passed(done: &Option<IDirect3DQuery9>) -> windows::core::Result<bool> {
    let Some(done) = done else { return Ok(false) };
    // Through the vtable: the wrapper takes S_FALSE, not yet, for done.
    let result = (Interface::vtable(done).GetData)(done.as_raw(), std::ptr::null_mut(), 0, 0);
    if result == S_FALSE {
        return Ok(false);
    }
    result.ok().map(|()| true)
}

/// A surface in system memory that frames are read into. It survives a
/// device reset, and is kept from one screenshot to the next: letting one
/// go took the main thread 2.4 ms for a 3840x2160 frame.
pub struct Memory {
    surface: IDirect3DSurface9,
    shape: Shape,
}

impl Memory {
    fn fits(&self, shape: &Shape) -> bool {
        (self.shape.width, self.shape.height, self.shape.format) == (shape.width, shape.height, shape.format)
    }
}

/// The frame being read into system memory.
pub struct Reading {
    memory: Memory,
    done: Option<IDirect3DQuery9>,
    tries: u32,
    /// How long making the surface took; None when it was kept from before.
    pub made: Option<Duration>,
    /// How long `GetRenderTargetData` took.
    pub transfer: Duration,
    /// How long locking took, over the tries.
    pub locking: Duration,
}

/// What locking the surface came to.
pub enum Lock {
    /// The transfer is not done yet.
    NotYet(Reading),
    Done(Reading, Locked),
}

impl Reading {
    /// Locks the surface once the GPU is past the transfer, looking
    /// without waiting [`PATIENCE`] times, then waiting.
    ///
    /// # Safety
    /// On the main thread, at a present.
    pub unsafe fn lock(mut self) -> GResult<Lock> {
        self.tries += 1;
        let started = Instant::now();
        let patient = self.tries < PATIENCE;
        if patient && !passed(&self.done).map_err(failed("waiting for the frame's transfer"))? {
            self.locking += started.elapsed();
            return Ok(Lock::NotYet(self));
        }
        let mut locked = D3DLOCKED_RECT::default();
        let surface = &self.memory.surface;
        let result = match surface.LockRect(&mut locked, std::ptr::null(), (D3DLOCK_READONLY | D3DLOCK_DONOTWAIT) as u32) {
            Err(e) if e.code() == D3DERR_WASSTILLDRAWING && !patient => surface.LockRect(&mut locked, std::ptr::null(), D3DLOCK_READONLY as u32),
            other => other,
        };
        self.locking += started.elapsed();
        match result {
            Ok(()) => {
                let locked = Locked { surface: self.memory.surface.clone(), bits: locked.pBits as *const u8, pitch: locked.Pitch as usize };
                Ok(Lock::Done(self, locked))
            }
            Err(e) if e.code() == D3DERR_WASSTILLDRAWING => Ok(Lock::NotYet(self)),
            Err(e) => Err(failed("locking the frame in memory")(e)),
        }
    }

    /// How many times locking was tried.
    pub fn tries(&self) -> u32 {
        self.tries
    }

    /// The surface, to keep for the next frame once the frame is unlocked.
    pub fn into_memory(self) -> Memory {
        self.memory
    }
}

/// The frame in system memory, locked until dropped.
pub struct Locked {
    surface: IDirect3DSurface9,
    pub bits: *const u8,
    /// Bytes from one row to the next.
    pub pitch: usize,
}

impl Drop for Locked {
    fn drop(&mut self) {
        unsafe {
            let _ = self.surface.UnlockRect();
        }
    }
}
