//! Presentation lifecycle around the game's Direct3D 9 `Present` and device
//! reset wrappers (`0x16d4db0` and `0x16d4bb0`).
//!
//! Owns frame holding (`hold` / `let_go` / `holding`), hidden-start
//! `S_PRESENT_OCCLUDED` handling, device tracking (`super::d3d9`), and the
//! explicit split between main-thread and drawing-thread present phases so
//! main-thread-only work (such as font page growth, ADR font-pages-grow)
//! never runs on the drawing thread.
use super::{d3d9, drawing};
use crate::base::called_by_game;
use crate::native::hooks::Call;
use crate::native::objects::text::renderer::DEVICE;
use crate::native::points::{Function, Point};
use crate::native::probes;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

const PRESENT: usize = 0x16d4db0;
const PRESENT_OCCLUDED: usize = 0x16d4ddb;
const RENDERER_PRESENTED: usize = 0x132;
const BEFORE_RESET: usize = 0x16d4bb0;

type Present = unsafe extern "C" fn(usize) -> u8;

static ORIGINAL_PRESENT: Function<Present> = Function::new();

/// While set, frames presented on the main thread are held back so the game's
/// window keeps showing what it showed last (such as a still notice).
static HOLDING: AtomicBool = AtomicBool::new(false);
static HELD: AtomicU64 = AtomicU64::new(0);

/// Holds back the frames the main thread presents from now on.
pub(crate) fn hold() {
    HELD.store(0, Ordering::Relaxed);
    HOLDING.store(true, Ordering::Relaxed);
}

/// Lets the frames the main thread presents through again; answers how many
/// were held back.
pub(crate) fn let_go() -> u64 {
    HOLDING.store(false, Ordering::Relaxed);
    HELD.swap(0, Ordering::Relaxed)
}

/// Whether frames are held back now, without counting one.
pub(crate) fn holding() -> bool {
    HOLDING.load(Ordering::Relaxed)
}

/// Whether the frame the main thread presents now is held back, counting it.
fn take_held() -> bool {
    let holding = HOLDING.load(Ordering::Relaxed);
    if holding {
        HELD.fetch_add(1, Ordering::Relaxed);
    }
    holding
}

pub(crate) struct PresentHooks {
    pub(crate) discard_late: fn(),
    /// Runs on the main thread before Present: main-thread-only work (font
    /// page growth) plus framework UI, late tooltips, and screenshots.
    pub(crate) before_main_present: unsafe fn(usize, Option<*mut c_void>),
    /// Runs on the drawing thread before Present: framework UI, late tooltips,
    /// and screenshots (never main-thread-only font page growth).
    pub(crate) before_drawing_present: unsafe fn(usize, Option<*mut c_void>),
    pub(crate) after_main_present: unsafe fn(Option<*mut c_void>),
    pub(crate) before_reset: fn(),
}

static HOOKS: OnceLock<PresentHooks> = OnceLock::new();

pub(crate) fn set_hooks(hooks: PresentHooks) {
    let _ = HOOKS.set(hooks);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PresentThread {
    Main,
    Drawing,
    Other,
}

/// Internal seam separating presentation orchestration from Direct3D 9 and
/// native hooks so the full `present` state machine is testable in-process.
trait PresentHost {
    type Token;
    fn thread(&self) -> PresentThread;
    fn take_held(&mut self) -> bool;
    fn discard_late(&mut self);
    fn device_of(&mut self, renderer: usize) -> Option<*mut c_void>;
    fn record_device(&mut self, device: *mut c_void);
    fn before_main_present(&mut self, renderer: usize, device: Option<*mut c_void>);
    fn before_drawing_present(&mut self, renderer: usize, device: Option<*mut c_void>);
    fn after_main_present(&mut self, device: Option<*mut c_void>);
    fn screen_size(&mut self, device: *mut c_void) -> (f32, f32);
    fn drawing_presented(&mut self, width: u32, height: u32);
    fn drawing_present_begins(&mut self);
    fn drawing_present_ends(&mut self);
    fn submission_begins(&mut self) -> Self::Token;
    fn submission_ends(&mut self, token: Self::Token, success: bool, main_thread: bool);
    fn hidden_start(&self) -> bool;
    fn d3d9_present(&mut self, device: *mut c_void) -> bool;
    fn write_presented_flag(&mut self, renderer: usize, ok: u8);
    fn original_present(&mut self, renderer: usize) -> u8;
}

fn call_present_with<H: PresentHost>(host: &mut H, renderer: usize) -> u8 {
    if !host.hidden_start() {
        return host.original_present(renderer);
    }
    let Some(device) = host.device_of(renderer).filter(|p| !p.is_null()) else {
        return host.original_present(renderer);
    };
    let ok = u8::from(host.d3d9_present(device));
    host.write_presented_flag(renderer, ok);
    ok
}

fn run_present<H: PresentHost>(host: &mut H, renderer: usize) -> u8 {
    match host.thread() {
        PresentThread::Main => {
            if host.take_held() {
                host.discard_late();
                return 1;
            }
            let device = host.device_of(renderer);
            if let Some(d) = device {
                host.record_device(d);
            }
            host.before_main_present(renderer, device);
            host.after_main_present(device);
            let submission = host.submission_begins();
            let presented = call_present_with(host, renderer);
            host.submission_ends(submission, presented != 0, true);
            presented
        }
        PresentThread::Drawing => {
            let device = host.device_of(renderer);
            if let Some(d) = device {
                host.record_device(d);
            }
            host.before_drawing_present(renderer, device);
            if let Some(d) = device {
                let (width, height) = host.screen_size(d);
                host.drawing_presented(width as u32, height as u32);
            }
            host.drawing_present_begins();
            // Drop guard ensures drawing_present_ends runs even if Present unwinds.
            let mut ended = false;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let submission = host.submission_begins();
                let presented = call_present_with(host, renderer);
                host.submission_ends(submission, presented != 0, false);
                presented
            }));
            if !ended {
                ended = true;
                host.drawing_present_ends();
            }
            let _ = ended;
            match result {
                Ok(presented) => presented,
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }
        PresentThread::Other => call_present_with(host, renderer),
    }
}

struct LiveHost;

impl PresentHost for LiveHost {
    type Token = probes::Submission;

    fn thread(&self) -> PresentThread {
        if crate::base::thread_id() == crate::base::main_thread() {
            PresentThread::Main
        } else if crate::base::is_drawing_thread() {
            PresentThread::Drawing
        } else {
            PresentThread::Other
        }
    }

    fn take_held(&mut self) -> bool {
        take_held()
    }

    fn discard_late(&mut self) {
        if let Some(h) = HOOKS.get() {
            (h.discard_late)();
        }
    }

    fn device_of(&mut self, renderer: usize) -> Option<*mut c_void> {
        crate::base::guarded("frame present", || crate::native::memory().and_then(|m| m.ptr(renderer + DEVICE).ok()).map(|device| device as *mut c_void))
    }

    fn record_device(&mut self, device: *mut c_void) {
        d3d9::record_device(device);
    }

    fn before_main_present(&mut self, renderer: usize, device: Option<*mut c_void>) {
        if let Some(h) = HOOKS.get() {
            unsafe { (h.before_main_present)(renderer, device) };
        }
        crate::base::guarded("screenshot", || {
            if let Some(device) = device {
                super::screenshot::presenting(device);
            }
        });
    }

    fn before_drawing_present(&mut self, renderer: usize, device: Option<*mut c_void>) {
        if let Some(h) = HOOKS.get() {
            unsafe { (h.before_drawing_present)(renderer, device) };
        }
        crate::base::guarded("screenshot", || {
            if let Some(device) = device {
                super::screenshot::presenting(device);
            }
        });
    }

    fn after_main_present(&mut self, device: Option<*mut c_void>) {
        if let Some(h) = HOOKS.get() {
            unsafe { (h.after_main_present)(device) };
        }
        crate::base::guarded("input", || {
            if let Some(device) = device {
                let (width, height) = unsafe { d3d9::screen(device) };
                super::input::presenting((width as u32, height as u32));
            }
        });
    }

    fn screen_size(&mut self, device: *mut c_void) -> (f32, f32) {
        crate::base::guarded("screen size", || unsafe { d3d9::screen(device) })
    }

    fn drawing_presented(&mut self, width: u32, height: u32) {
        drawing::presented(width, height);
    }

    fn drawing_present_begins(&mut self) {
        drawing::present_begins();
    }

    fn drawing_present_ends(&mut self) {
        drawing::present_ends();
    }

    fn submission_begins(&mut self) -> Self::Token {
        probes::submission_begins()
    }

    fn submission_ends(&mut self, token: Self::Token, success: bool, main_thread: bool) {
        probes::submission_ends(token, success, main_thread);
    }

    fn hidden_start(&self) -> bool {
        crate::base::hidden_start()
    }

    fn d3d9_present(&mut self, device: *mut c_void) -> bool {
        unsafe { d3d9::present(device).is_ok() }
    }

    fn write_presented_flag(&mut self, renderer: usize, ok: u8) {
        if let Some(m) = crate::native::memory() {
            let _ = m.write(renderer + RENDERER_PRESENTED, &[ok]);
        }
    }

    fn original_present(&mut self, renderer: usize) -> u8 {
        unsafe { ORIGINAL_PRESENT.get()(renderer) }
    }
}

#[called_by_game]
unsafe extern "C" fn present(renderer: usize) -> u8 {
    run_present(&mut LiveHost, renderer)
}

fn before_reset(_: &mut Call) {
    probes::device_reset();
    d3d9::clear_device();
    super::screenshot::device_reset();
    if let Some(h) = HOOKS.get() {
        (h.before_reset)();
    }
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(PRESENT, "40 53 48 83 ec 30 48 8b d9 48 c7 44 24 20 00 00", "frame present", present as Present, &ORIGINAL_PRESENT),
        Point::keep(PRESENT_OCCLUDED, "3d 78 08 76 08 74 0f c6 83 32 01 00 00 01", "present occlusion check and presented flag").uses(&[RENDERER_PRESENTED]),
        Point::observe(BEFORE_RESET, "40 53 48 83 ec 20 48 8b d9 48 8b 49 10 48 85 c9", "before device reset", 1, Some(before_reset), None),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeHost {
        thread: PresentThread,
        held: bool,
        hidden: bool,
        device: Option<*mut c_void>,
        screen: (f32, f32),
        d3d9_ok: bool,
        orig_ret: u8,
        panic_on_present: bool,
        events: Vec<&'static str>,
        presented_flag: Option<u8>,
        drawing_size: Option<(u32, u32)>,
        submissions: Vec<(bool, bool)>,
    }

    impl FakeHost {
        fn new(thread: PresentThread) -> Self {
            Self {
                thread,
                held: false,
                hidden: false,
                device: Some(0x1000 as *mut c_void),
                screen: (1920.0, 1080.0),
                d3d9_ok: true,
                orig_ret: 1,
                panic_on_present: false,
                events: Vec::new(),
                presented_flag: None,
                drawing_size: None,
                submissions: Vec::new(),
            }
        }
    }

    impl PresentHost for FakeHost {
        type Token = u32;
        fn thread(&self) -> PresentThread {
            self.thread
        }
        fn take_held(&mut self) -> bool {
            self.held
        }
        fn discard_late(&mut self) {
            self.events.push("discard_late");
        }
        fn device_of(&mut self, _renderer: usize) -> Option<*mut c_void> {
            self.device
        }
        fn record_device(&mut self, _device: *mut c_void) {
            self.events.push("record_device");
        }
        fn before_main_present(&mut self, _renderer: usize, _device: Option<*mut c_void>) {
            self.events.push("before_main");
        }
        fn before_drawing_present(&mut self, _renderer: usize, _device: Option<*mut c_void>) {
            self.events.push("before_drawing");
        }
        fn after_main_present(&mut self, _device: Option<*mut c_void>) {
            self.events.push("after_main");
        }
        fn screen_size(&mut self, _device: *mut c_void) -> (f32, f32) {
            self.screen
        }
        fn drawing_presented(&mut self, width: u32, height: u32) {
            self.drawing_size = Some((width, height));
            self.events.push("drawing_presented");
        }
        fn drawing_present_begins(&mut self) {
            self.events.push("drawing_begins");
        }
        fn drawing_present_ends(&mut self) {
            self.events.push("drawing_ends");
        }
        fn submission_begins(&mut self) -> u32 {
            1
        }
        fn submission_ends(&mut self, _token: u32, success: bool, main_thread: bool) {
            self.submissions.push((success, main_thread));
        }
        fn hidden_start(&self) -> bool {
            self.hidden
        }
        fn d3d9_present(&mut self, _device: *mut c_void) -> bool {
            self.events.push("d3d9_present");
            if self.panic_on_present {
                panic!("simulated Present fault");
            }
            self.d3d9_ok
        }
        fn write_presented_flag(&mut self, _renderer: usize, ok: u8) {
            self.presented_flag = Some(ok);
        }
        fn original_present(&mut self, _renderer: usize) -> u8 {
            self.events.push("original_present");
            if self.panic_on_present {
                panic!("simulated Present fault");
            }
            self.orig_ret
        }
    }

    #[test]
    fn held_main_frame_discards_late_tooltips_and_skips_present() {
        let mut host = FakeHost::new(PresentThread::Main);
        host.held = true;
        assert_eq!(run_present(&mut host, 0x2000), 1);
        assert_eq!(host.events, ["discard_late"]);
        assert!(host.submissions.is_empty());
    }

    #[test]
    fn main_and_drawing_threads_invoke_distinct_before_present_phases() {
        let mut main = FakeHost::new(PresentThread::Main);
        assert_eq!(run_present(&mut main, 0x2000), 1);
        assert_eq!(main.events, ["record_device", "before_main", "after_main", "original_present"]);
        assert_eq!(main.submissions, [(true, true)]);

        let mut draw = FakeHost::new(PresentThread::Drawing);
        assert_eq!(run_present(&mut draw, 0x2000), 1);
        assert_eq!(draw.events, ["record_device", "before_drawing", "drawing_presented", "drawing_begins", "original_present", "drawing_ends"]);
        assert_eq!(draw.drawing_size, Some((1920, 1080)));
        assert_eq!(draw.submissions, [(true, false)]);
    }

    #[test]
    fn hidden_start_calls_d3d9_present_directly_and_records_flag() {
        let mut host = FakeHost::new(PresentThread::Main);
        host.hidden = true;
        host.d3d9_ok = true;
        assert_eq!(run_present(&mut host, 0x2000), 1);
        assert_eq!(host.presented_flag, Some(1));
        assert!(host.events.contains(&"d3d9_present"));
        assert!(!host.events.contains(&"original_present"));
    }

    #[test]
    fn drawing_present_ends_even_when_present_panics() {
        let mut draw = FakeHost::new(PresentThread::Drawing);
        draw.panic_on_present = true;
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_present(&mut draw, 0x2000);
        }));
        assert!(panicked.is_err());
        assert_eq!(draw.events.last(), Some(&"drawing_ends"));
    }
}
