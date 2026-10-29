//! Screenshots, for remote control only (ADR 0015): the frame the player
//! sees, the framework UI included, as a PNG.
//!
//! A screenshot takes the next frame the main thread presents after it is
//! asked for, or, while another one's frame is being read back, the first
//! one presented after that frame is let go. In the frame present hook (`framework_ui::hook`), after the
//! framework UI is drawn and just before the game presents, [`presenting`]
//! reads that frame back from the game's Direct3D 9 device, over this and
//! the next few presents, in steps that keep the main thread from waiting
//! for the GPU ([`d3d9`]). A thread of the framework's own then copies the
//! pixels out and scales and encodes them ([`png`]); the main thread lets
//! the frame go at the present after that. Both parts are logged, with how
//! long each took.
pub mod d3d9;
pub mod png;

use super::{fail, Caller, Fail, GResult, Status};
use std::ffi::c_void;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

/// A screenshot taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// The size of the frame it was taken from.
    pub frame_width: u32,
    pub frame_height: u32,
}

/// A screenshot asked for, until it is taken. Whoever asked keeps it and
/// looks at it later; dropping it gives it up.
pub struct Shot {
    percent: u32,
    taken: Mutex<Option<GResult<Image>>>,
}

impl Shot {
    pub fn new(percent: u32) -> Arc<Self> {
        Arc::new(Self { percent, taken: Mutex::new(None) })
    }

    /// The image's size in percent of the frame's.
    pub fn percent(&self) -> u32 {
        self.percent
    }

    pub fn finish(&self, result: GResult<Image>) {
        *self.taken.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
    }

    /// The image, or why there is none, once taken (handed out once).
    pub fn taken(&self) -> Option<GResult<Image>> {
        self.taken.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    /// Nobody waits for it any more: the capture holds the only reference.
    fn given_up(shot: &Arc<Self>) -> bool {
        Arc::strong_count(shot) == 1
    }
}

/// Refuses an image size that is not 1 to 100 percent of the frame's.
pub fn check_scale(percent: u32) -> GResult<()> {
    if !(1..=100).contains(&percent) {
        return fail(Status::InvalidArgument, format!("scale is a percent from 1 to 100; {percent} is not one"));
    }
    Ok(())
}

/// Asks for a screenshot of the next frame the main thread presents, at
/// `percent` of its size (1 to 100).
pub fn screenshot(caller: Caller, percent: u32) -> GResult<Arc<Shot>> {
    if let Caller::Extension(_) = caller {
        return fail(Status::Unavailable, "only remote control takes screenshots");
    }
    check_scale(percent)?;
    let shot = Shot::new(percent);
    capture().asked.push(shot.clone());
    Ok(shot)
}

/// How long the main thread spent on one frame.
#[derive(Clone, Copy, Default)]
struct Spent {
    /// At the frame's present, copying it on the GPU.
    copy: Duration,
    /// At the presents after, until the frame was locked in memory.
    read: Duration,
    /// Presents from the frame's to the one it was locked at.
    frames: u32,
    /// Of `read`: making the surface in memory (None when it was kept from
    /// the frame before), `GetRenderTargetData`, and locking, over how many
    /// tries.
    made: Option<Duration>,
    transfer: Duration,
    locking: Duration,
    tries: u32,
}

enum Stage {
    Idle,
    /// The frame copied on the GPU, for these shots.
    Copied {
        copy: d3d9::FrameCopy,
        shots: Vec<Arc<Shot>>,
        spent: Spent,
    },
    /// The copy being read into memory.
    Reading {
        copy: d3d9::FrameCopy,
        reading: d3d9::Reading,
        shots: Vec<Arc<Shot>>,
        spent: Spent,
    },
    /// The frame locked in memory while the screenshot thread copies the
    /// pixels out; it sets `out` once it has.
    CopyingOut {
        locked: d3d9::Locked,
        memory: d3d9::Memory,
        out: Arc<AtomicBool>,
        shape: d3d9::Shape,
        spent: Spent,
    },
}

struct Capture {
    /// Shots that take the next frame copied.
    asked: Vec<Arc<Shot>>,
    stage: Stage,
    /// The surface in memory the last frame was read into, for the next.
    memory: Option<d3d9::Memory>,
}

// The device's objects are used only on the thread that presents.
unsafe impl Send for Capture {}

static CAPTURE: Mutex<Capture> = Mutex::new(Capture { asked: Vec::new(), stage: Stage::Idle, memory: None });

fn capture() -> std::sync::MutexGuard<'static, Capture> {
    CAPTURE.lock().unwrap_or_else(|e| e.into_inner())
}

fn finish_all(shots: Vec<Arc<Shot>>, fail: &Fail) {
    log!("screenshot failed: {}", fail.message);
    for shot in shots {
        shot.finish(Err(fail.clone()));
    }
}

/// At every frame the main thread presents, after the framework UI is drawn,
/// on `device` (an `IDirect3DDevice9*`).
pub fn presenting(device: *mut c_void) {
    let mut capture = capture();
    if capture.asked.is_empty() && matches!(capture.stage, Stage::Idle) {
        return;
    }
    let Some(device) = (unsafe { d3d9::device(&device) }) else { return };
    unsafe { capture.step(device) };
}

impl Capture {
    unsafe fn step(&mut self, device: &windows::Win32::Graphics::Direct3D9::IDirect3DDevice9) {
        let started = Instant::now();
        let stage = std::mem::replace(&mut self.stage, Stage::Idle);
        self.stage = match stage {
            Stage::Idle => Stage::Idle,
            // The frame before was copied out: let it go.
            Stage::CopyingOut { locked, memory, out, shape, spent } if out.load(Ordering::Acquire) => {
                drop(locked);
                self.memory = Some(memory);
                let total = spent.copy + spent.read + started.elapsed();
                let made = spent.made.map_or("kept from before".to_string(), |made| format!("made in {:.2} ms", ms(made)));
                log!(
                    "screenshot of a {}x{} frame (D3DFORMAT {}, multisampling {}), on the main thread: {:.2} ms copying it on the GPU at its present; {:.2} ms reading it back over the {} presents after (the surface in memory {made}, GetRenderTargetData {:.2} ms, locking {:.2} ms in {} tries); {:.2} ms letting it go; {:.2} ms in all",
                    shape.width,
                    shape.height,
                    shape.format.0,
                    shape.multisample.0,
                    ms(spent.copy),
                    ms(spent.read),
                    spent.frames,
                    ms(spent.transfer),
                    ms(spent.locking),
                    spent.tries,
                    ms(started.elapsed()),
                    ms(total)
                );
                Stage::Idle
            }
            stage @ Stage::CopyingOut { .. } => stage,
            Stage::Copied { mut copy, shots, mut spent } => {
                spent.frames += 1;
                let memory = &mut self.memory;
                match copy.ready().and_then(|ready| ready.then(|| copy.read(device, memory.take())).transpose()) {
                    Ok(None) => Stage::Copied { copy, shots, spent: spent.add(started) },
                    Ok(Some(reading)) => {
                        spent.made = reading.made;
                        spent.transfer = reading.transfer;
                        lock(copy, reading, shots, spent, started)
                    }
                    Err(fail) => {
                        finish_all(shots, &fail);
                        Stage::Idle
                    }
                }
            }
            Stage::Reading { copy, reading, shots, mut spent } => {
                spent.frames += 1;
                lock(copy, reading, shots, spent, started)
            }
        };
        if matches!(self.stage, Stage::Idle) {
            self.asked.retain(|shot| !Shot::given_up(shot));
            if !self.asked.is_empty() {
                let started = Instant::now();
                let shots = std::mem::take(&mut self.asked);
                self.stage = match d3d9::FrameCopy::start(device) {
                    Ok(copy) => Stage::Copied { copy, shots, spent: Spent { copy: started.elapsed(), ..Spent::default() } },
                    Err(fail) => {
                        finish_all(shots, &fail);
                        Stage::Idle
                    }
                };
            }
        }
    }
}

/// Locks the frame being read once it is in memory, and hands it to a
/// thread of its own to copy out.
unsafe fn lock(copy: d3d9::FrameCopy, reading: d3d9::Reading, shots: Vec<Arc<Shot>>, spent: Spent, started: Instant) -> Stage {
    match reading.lock() {
        Ok(d3d9::Lock::NotYet(reading)) => Stage::Reading { copy, reading, shots, spent: spent.add(started) },
        Ok(d3d9::Lock::Done(reading, locked)) => {
            let spent = Spent { locking: reading.locking, tries: reading.tries(), ..spent };
            let memory = reading.into_memory();
            let out = Arc::new(AtomicBool::new(false));
            let frame = Frame { bits: locked.bits as usize, pitch: locked.pitch, shape: copy.shape };
            let kept = shots.clone();
            let thread = {
                let out = out.clone();
                std::thread::Builder::new().name("eu4-harmony screenshot".into()).spawn(move || crate::guarded("screenshot", || frame.take(shots, &out)))
            };
            match thread {
                Ok(_) => {
                    drop(kept);
                    Stage::CopyingOut { locked, memory, out, shape: copy.shape, spent: spent.add(started) }
                }
                Err(e) => {
                    finish_all(kept, &Fail { status: Status::Unavailable, message: format!("no thread to encode the frame: {e}") });
                    Stage::Idle
                }
            }
        }
        Err(fail) => {
            finish_all(shots, &fail);
            Stage::Idle
        }
    }
}

impl Spent {
    /// With the time spent at this present.
    fn add(mut self, started: Instant) -> Self {
        self.read += started.elapsed();
        self
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// The game is about to reset the device: a frame copied on the GPU goes
/// with its render target, and its shots take the next frame. A frame
/// already in memory stays.
pub fn device_reset() {
    let mut capture = capture();
    let stage = std::mem::replace(&mut capture.stage, Stage::Idle);
    match stage {
        Stage::Copied { shots, .. } | Stage::Reading { shots, .. } => {
            log!("screenshot: the device is reset before the frame was read back; the next frame is taken");
            let asked = std::mem::take(&mut capture.asked);
            capture.asked = shots.into_iter().chain(asked).collect();
        }
        other => capture.stage = other,
    }
}

/// A frame locked in memory, as the screenshot thread reads it.
struct Frame {
    bits: usize,
    pitch: usize,
    shape: d3d9::Shape,
}

impl Frame {
    /// On the screenshot thread: copies the pixels out, says so through
    /// `out`, then makes each shot's PNG.
    fn take(self, shots: Vec<Arc<Shot>>, out: &AtomicBool) {
        let _com = png::Com::new();
        let started = Instant::now();
        let (width, height) = (self.shape.width, self.shape.height);
        // The main thread keeps the frame locked until `out` is set.
        let picture = unsafe { png::Picture::copy(self.bits as *const u8, self.pitch, width, height) };
        out.store(true, Ordering::Release);
        let picture = match picture {
            Ok(picture) => picture,
            Err(e) => return finish_all(shots, &Fail { status: Status::Unavailable, message: format!("the frame's pixels cannot be held: {e}") }),
        };
        log!("screenshot: the {width}x{height} frame's pixels copied out in {:.1} ms", ms(started.elapsed()));
        for shot in shots {
            if Shot::given_up(&shot) {
                log!("screenshot at {}%: given up before it was encoded", shot.percent);
                continue;
            }
            let started = Instant::now();
            let result = match picture.png(shot.percent) {
                Ok((png, w, h)) => {
                    log!("screenshot at {}%: {w}x{h}, a PNG of {} bytes, encoded in {:.1} ms", shot.percent, png.len(), ms(started.elapsed()));
                    Ok(Image { png, width: w, height: h, frame_width: width, frame_height: height })
                }
                Err(e) => {
                    log!("screenshot at {}%: cannot be encoded: {e}", shot.percent);
                    Err(Fail { status: Status::Unavailable, message: format!("the frame cannot be encoded as PNG: {e}") })
                }
            };
            shot.finish(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_remote_control_takes_screenshots_from_1_to_100_percent() {
        assert_eq!(screenshot(Caller::Extension(0), 50).err().map(|f| f.status), Some(Status::Unavailable));
        for wrong in [0, 101] {
            let fail = screenshot(Caller::Remote, wrong).err().unwrap();
            assert_eq!(fail.status, Status::InvalidArgument);
            assert!(fail.message.contains(&wrong.to_string()), "{}", fail.message);
        }
        let shot = screenshot(Caller::Remote, 50).unwrap();
        assert_eq!(shot.percent(), 50);
        assert!(shot.taken().is_none());
        assert!(capture().asked.iter().any(|asked| Arc::ptr_eq(asked, &shot)));
        drop(shot);
        capture().asked.retain(|shot| !Shot::given_up(shot));
    }

    #[test]
    fn a_shot_is_handed_out_once() {
        let shot = Shot::new(100);
        let image = Image { png: vec![1], width: 1, height: 1, frame_width: 1, frame_height: 1 };
        shot.finish(Ok(image.clone()));
        assert_eq!(shot.taken().unwrap().unwrap(), image);
        assert!(shot.taken().is_none());
    }

    #[test]
    fn the_screenshot_thread_makes_each_shot_and_lets_the_frame_go_first() {
        // Two pixels across, one down, as a locked surface holds them.
        let pixels = [0u8, 0, 255, 0, 255, 0, 0, 0];
        let frame = Frame {
            bits: pixels.as_ptr() as usize,
            pitch: 8,
            shape: d3d9::Shape {
                width: 2,
                height: 1,
                format: windows::Win32::Graphics::Direct3D9::D3DFMT_X8R8G8B8,
                multisample: windows::Win32::Graphics::Direct3D9::D3DMULTISAMPLE_NONE,
            },
        };
        let (full, half, given_up) = (Shot::new(100), Shot::new(50), Shot::new(100));
        let out = AtomicBool::new(false);
        let shots = vec![full.clone(), half.clone(), given_up];
        frame.take(shots, &out);
        assert!(out.load(Ordering::Acquire));
        let full = full.taken().unwrap().unwrap();
        assert_eq!((full.width, full.height, full.frame_width, full.frame_height), (2, 1, 2, 1));
        assert_eq!(&full.png[..4], b"\x89PNG");
        let half = half.taken().unwrap().unwrap();
        assert_eq!((half.width, half.height, half.frame_width, half.frame_height), (1, 1, 2, 1));
    }
}
