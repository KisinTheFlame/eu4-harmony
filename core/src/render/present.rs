//! Presentation and frame-boundary lifecycle around Direct3D 9 `Present`,
//! device reset (`0x16d4db0` and `0x16d4bb0`), and the drawing thread's
//! prepared-frame handoff.
//!
//! Owns frame holding (`hold` / `let_go` / `holding`), unattended-run
//! `S_PRESENT_OCCLUDED` handling, device tracking (`super::d3d9`), and the
//! single frame-lifecycle contract shared by main-thread presentation and
//! drawing-thread presentation:
//! - main-thread-only work (font page growth per ADR font-pages-grow, safe
//!   point, periodic text costs, remote input playback) runs on every
//!   main-thread frame boundary, whether presented inline or handed off to the
//!   drawing thread;
//! - presenting-thread work (framework UI overlay, late native tooltips,
//!   screenshot readback) runs before every `Present` on whichever thread
//!   presents, without ever running main-thread-only work on the drawing
//!   thread.
use super::{d3d9, drawing};
use crate::base::called_by_game;
use crate::native::hooks::Call;
use crate::native::memory::Memory;
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

/// External callbacks wired once by `app` into the `render` frame lifecycle.
pub(crate) struct FrameHooks {
    /// Main-thread-only font page growth (ADR font-pages-grow).
    pub(crate) grow_font_pages: fn(),
    /// Main-thread frame-boundary maintenance (safe point and text costs).
    pub(crate) main_frame_done: fn(),
    /// Framework UI overlay and late native tooltips drawn on the presenting thread.
    pub(crate) overlay_before_present: unsafe fn(usize, Option<*mut c_void>),
    /// Discards late native tooltips when a held main-thread frame is skipped.
    pub(crate) discard_late: fn(),
    /// Prepares GUI window ordering on the main thread before drawing-thread staging.
    pub(crate) prepare_scene: fn(&Memory) -> crate::base::Result<()>,
    /// Clears tooltips and framework UI resources before D3D9 device reset.
    pub(crate) before_reset: fn(),
}

static HOOKS: OnceLock<FrameHooks> = OnceLock::new();

pub(crate) fn set_hooks(hooks: FrameHooks) {
    let _ = HOOKS.set(hooks);
}

/// Frame-hold state shared between `hold` / `let_go` / `holding` and main-thread
/// presentation.
struct HoldState {
    holding: AtomicBool,
    held: AtomicU64,
}

impl HoldState {
    const fn new() -> Self {
        Self { holding: AtomicBool::new(false), held: AtomicU64::new(0) }
    }

    fn hold(&self) {
        self.held.store(0, Ordering::Relaxed);
        self.holding.store(true, Ordering::Relaxed);
    }

    fn let_go(&self) -> u64 {
        self.holding.store(false, Ordering::Relaxed);
        self.held.swap(0, Ordering::Relaxed)
    }

    fn holding(&self) -> bool {
        self.holding.load(Ordering::Relaxed)
    }

    fn take_held(&self) -> bool {
        let holding = self.holding.load(Ordering::Relaxed);
        if holding {
            self.held.fetch_add(1, Ordering::Relaxed);
        }
        holding
    }
}

static HOLD: HoldState = HoldState::new();

/// Holds back the frames the main thread presents from now on.
pub(crate) fn hold() {
    HOLD.hold();
}

/// Lets the frames the main thread presents through again; answers how many
/// were held back.
pub(crate) fn let_go() -> u64 {
    HOLD.let_go()
}

/// Whether frames are held back now, without counting one.
pub(crate) fn holding() -> bool {
    HOLD.holding()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PresentThread {
    Main,
    Drawing,
    Other,
}

/// Narrow native/Direct3D adapter underneath the frame-lifecycle coordinator.
trait PresentBackend {
    type Token;
    fn device_of(&mut self, renderer: usize) -> Option<*mut c_void>;
    fn record_device(&mut self, device: *mut c_void);
    fn screen_size(&mut self, device: *mut c_void) -> (f32, f32);
    fn screenshot(&mut self, device: *mut c_void);
    fn input(&mut self, screen: (u32, u32));
    fn drawing_presented(&mut self, width: u32, height: u32);
    fn drawing_present_begins(&mut self);
    fn drawing_present_ends(&mut self);
    fn submission_begins(&mut self) -> Self::Token;
    fn submission_ends(&mut self, token: Self::Token, success: bool, main_thread: bool);
    fn unattended(&self) -> bool;
    fn d3d9_present(&mut self, device: *mut c_void) -> bool;
    fn write_presented_flag(&mut self, renderer: usize, ok: u8);
    fn original_present(&mut self, renderer: usize) -> u8;
}

/// Ensures `drawing_present_ends` always runs when the drawing thread leaves
/// `Present`, including on panic unwind.
struct DrawingPresentGuard<'a, B: PresentBackend> {
    backend: &'a mut B,
}

impl<'a, B: PresentBackend> DrawingPresentGuard<'a, B> {
    fn enter(backend: &'a mut B) -> Self {
        backend.drawing_present_begins();
        Self { backend }
    }
}

impl<B: PresentBackend> Drop for DrawingPresentGuard<'_, B> {
    fn drop(&mut self) {
        self.backend.drawing_present_ends();
    }
}

fn call_present<B: PresentBackend>(backend: &mut B, renderer: usize, device: Option<*mut c_void>) -> u8 {
    if !backend.unattended() {
        return backend.original_present(renderer);
    }
    let Some(device) = device.filter(|p| !p.is_null()) else {
        return backend.original_present(renderer);
    };
    let ok = u8::from(backend.d3d9_present(device));
    backend.write_presented_flag(renderer, ok);
    ok
}

fn run_overlay_and_screenshot<B: PresentBackend>(hooks: Option<&FrameHooks>, backend: &mut B, renderer: usize, device: Option<*mut c_void>) {
    if let Some(h) = hooks {
        unsafe { (h.overlay_before_present)(renderer, device) };
    }
    if let Some(device) = device {
        backend.screenshot(device);
    }
}

fn complete_main_frame(hooks: Option<&FrameHooks>, screen: Option<(u32, u32)>, on_input: impl FnOnce((u32, u32))) {
    if let Some(h) = hooks {
        (h.main_frame_done)();
    }
    if let Some(screen) = screen {
        on_input(screen);
    }
}

fn run_present<B: PresentBackend>(thread: PresentThread, hold: &HoldState, hooks: Option<&FrameHooks>, backend: &mut B, renderer: usize) -> u8 {
    match thread {
        PresentThread::Main => {
            if hold.take_held() {
                if let Some(h) = hooks {
                    (h.discard_late)();
                }
                return 1;
            }
            let device = backend.device_of(renderer);
            if let Some(d) = device {
                backend.record_device(d);
            }
            if let Some(h) = hooks {
                (h.grow_font_pages)();
            }
            run_overlay_and_screenshot(hooks, backend, renderer, device);
            let screen = device.map(|d| {
                let (w, h) = backend.screen_size(d);
                (w as u32, h as u32)
            });
            complete_main_frame(hooks, screen, |size| backend.input(size));
            let submission = backend.submission_begins();
            let presented = call_present(backend, renderer, device);
            backend.submission_ends(submission, presented != 0, true);
            presented
        }
        PresentThread::Drawing => {
            let device = backend.device_of(renderer);
            if let Some(d) = device {
                backend.record_device(d);
            }
            run_overlay_and_screenshot(hooks, backend, renderer, device);
            if let Some(d) = device {
                let (width, height) = backend.screen_size(d);
                backend.drawing_presented(width as u32, height as u32);
            }
            let guard = DrawingPresentGuard::enter(backend);
            let submission = guard.backend.submission_begins();
            let presented = call_present(guard.backend, renderer, device);
            guard.backend.submission_ends(submission, presented != 0, false);
            presented
        }
        PresentThread::Other => {
            let device = backend.device_of(renderer);
            call_present(backend, renderer, device)
        }
    }
}

/// Main-thread entry used by `drawing::prepare` before staging a GUI scene:
/// grows font pages on the main thread, then prepares native window ordering.
pub(super) fn prepare_scene(m: &Memory) -> crate::base::Result<()> {
    prepare_scene_with(HOOKS.get(), m)
}

fn prepare_scene_with(hooks: Option<&FrameHooks>, m: &Memory) -> crate::base::Result<()> {
    let Some(h) = hooks else {
        return Ok(());
    };
    (h.grow_font_pages)();
    (h.prepare_scene)(m)
}

/// Main-thread entry used by `drawing` when a prepared frame is handed off to
/// the drawing thread (so the main thread will not enter `Present` for it):
/// runs the same main-thread-only font page growth, safe point, text costs,
/// and remote input playback as an inline main-thread frame.
pub(super) fn main_frame_prepared(screen: Option<(u32, u32)>) {
    main_frame_prepared_with(HOOKS.get(), screen, |size| {
        crate::base::guarded("input", || super::input::presenting(size));
    });
}

fn main_frame_prepared_with(hooks: Option<&FrameHooks>, screen: Option<(u32, u32)>, on_input: impl FnOnce((u32, u32))) {
    if let Some(h) = hooks {
        (h.grow_font_pages)();
    }
    complete_main_frame(hooks, screen, on_input);
}

fn current_thread() -> PresentThread {
    if crate::base::thread_id() == crate::base::main_thread() {
        PresentThread::Main
    } else if crate::base::is_drawing_thread() {
        PresentThread::Drawing
    } else {
        PresentThread::Other
    }
}

struct LiveBackend;

impl PresentBackend for LiveBackend {
    type Token = probes::Submission;

    fn device_of(&mut self, renderer: usize) -> Option<*mut c_void> {
        crate::base::guarded("frame present", || crate::native::memory().and_then(|m| m.ptr(renderer + DEVICE).ok()).map(|device| device as *mut c_void))
    }

    fn record_device(&mut self, device: *mut c_void) {
        d3d9::record_device(device);
    }

    fn screen_size(&mut self, device: *mut c_void) -> (f32, f32) {
        crate::base::guarded("screen size", || unsafe { d3d9::screen(device) })
    }

    fn screenshot(&mut self, device: *mut c_void) {
        crate::base::guarded("screenshot", || super::screenshot::presenting(device));
    }

    fn input(&mut self, screen: (u32, u32)) {
        crate::base::guarded("input", || super::input::presenting(screen));
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

    fn unattended(&self) -> bool {
        crate::base::unattended()
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
    run_present(current_thread(), &HOLD, HOOKS.get(), &mut LiveBackend, renderer)
}

fn before_reset(_: &mut Call) {
    probes::device_reset();
    drawing::device_reset();
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
    use std::cell::RefCell;

    thread_local! {
        static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    fn record(tag: &'static str) {
        EVENTS.with(|e| e.borrow_mut().push(tag));
    }

    fn take_events() -> Vec<&'static str> {
        EVENTS.with(|e| std::mem::take(&mut *e.borrow_mut()))
    }

    fn test_hooks() -> FrameHooks {
        FrameHooks {
            grow_font_pages: || record("grow_font_pages"),
            main_frame_done: || record("main_frame_done"),
            overlay_before_present: |_, _| record("overlay"),
            discard_late: || record("discard_late"),
            prepare_scene: |_| {
                record("prepare_scene");
                Ok(())
            },
            before_reset: || record("before_reset"),
        }
    }

    struct FakeBackend {
        hidden: bool,
        device: Option<*mut c_void>,
        screen: (f32, f32),
        d3d9_ok: bool,
        orig_ret: u8,
        panic_on_present: bool,
        presented_flag: Option<u8>,
        drawing_size: Option<(u32, u32)>,
        input_size: Option<(u32, u32)>,
        submissions: Vec<(bool, bool)>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                hidden: false,
                device: Some(0x1000 as *mut c_void),
                screen: (1920.0, 1080.0),
                d3d9_ok: true,
                orig_ret: 1,
                panic_on_present: false,
                presented_flag: None,
                drawing_size: None,
                input_size: None,
                submissions: Vec::new(),
            }
        }
    }

    impl PresentBackend for FakeBackend {
        type Token = u32;
        fn device_of(&mut self, _renderer: usize) -> Option<*mut c_void> {
            self.device
        }
        fn record_device(&mut self, _device: *mut c_void) {
            record("record_device");
        }
        fn screen_size(&mut self, _device: *mut c_void) -> (f32, f32) {
            self.screen
        }
        fn screenshot(&mut self, _device: *mut c_void) {
            record("screenshot");
        }
        fn input(&mut self, screen: (u32, u32)) {
            self.input_size = Some(screen);
            record("input");
        }
        fn drawing_presented(&mut self, width: u32, height: u32) {
            self.drawing_size = Some((width, height));
            record("drawing_presented");
        }
        fn drawing_present_begins(&mut self) {
            record("drawing_begins");
        }
        fn drawing_present_ends(&mut self) {
            record("drawing_ends");
        }
        fn submission_begins(&mut self) -> u32 {
            1
        }
        fn submission_ends(&mut self, _token: u32, success: bool, main_thread: bool) {
            self.submissions.push((success, main_thread));
        }
        fn unattended(&self) -> bool {
            self.hidden
        }
        fn d3d9_present(&mut self, _device: *mut c_void) -> bool {
            record("d3d9_present");
            if self.panic_on_present {
                panic!("simulated Present fault");
            }
            self.d3d9_ok
        }
        fn write_presented_flag(&mut self, _renderer: usize, ok: u8) {
            self.presented_flag = Some(ok);
        }
        fn original_present(&mut self, _renderer: usize) -> u8 {
            record("original_present");
            if self.panic_on_present {
                panic!("simulated Present fault");
            }
            self.orig_ret
        }
    }

    #[test]
    fn held_main_frame_discards_late_tooltips_counts_held_and_skips_present() {
        let _ = take_events();
        let hold = HoldState::new();
        let hooks = test_hooks();
        let mut backend = FakeBackend::new();
        hold.hold();
        assert_eq!(run_present(PresentThread::Main, &hold, Some(&hooks), &mut backend, 0x2000), 1);
        assert_eq!(take_events(), ["discard_late"]);
        assert!(backend.submissions.is_empty());
        assert_eq!(hold.let_go(), 1);
    }

    #[test]
    fn main_thread_and_drawing_thread_paths_preserve_main_only_parity() {
        let _ = take_events();
        let hold = HoldState::new();
        let hooks = test_hooks();

        // Main-thread inline present runs font growth, overlay, screenshot, main maintenance, input, Present.
        let mut main = FakeBackend::new();
        assert_eq!(run_present(PresentThread::Main, &hold, Some(&hooks), &mut main, 0x2000), 1);
        assert_eq!(take_events(), ["record_device", "grow_font_pages", "overlay", "screenshot", "main_frame_done", "input", "original_present"]);
        assert_eq!(main.input_size, Some((1920, 1080)));
        assert_eq!(main.submissions, [(true, true)]);

        // Drawing-thread present runs overlay and screenshot, never font growth or main_frame_done.
        let mut draw = FakeBackend::new();
        assert_eq!(run_present(PresentThread::Drawing, &hold, Some(&hooks), &mut draw, 0x2000), 1);
        assert_eq!(take_events(), ["record_device", "overlay", "screenshot", "drawing_presented", "drawing_begins", "original_present", "drawing_ends"]);
        assert_eq!(draw.drawing_size, Some((1920, 1080)));
        assert_eq!(draw.submissions, [(true, false)]);

        // Main-thread prepared handoff runs the exact same main-thread maintenance (font growth, safe point, input).
        let mut prepared_input = None;
        main_frame_prepared_with(Some(&hooks), draw.drawing_size, |size| {
            prepared_input = Some(size);
            record("input");
        });
        assert_eq!(take_events(), ["grow_font_pages", "main_frame_done", "input"]);
        assert_eq!(prepared_input, Some((1920, 1080)));

        // Scene preparation grows font pages on the main thread before preparing window ordering.
        prepare_scene_with(Some(&hooks), &Memory { base: 0 }).unwrap();
        assert_eq!(take_events(), ["grow_font_pages", "prepare_scene"]);
    }

    #[test]
    fn hidden_start_calls_d3d9_present_directly_and_records_flag() {
        let _ = take_events();
        let hold = HoldState::new();
        let hooks = test_hooks();
        let mut backend = FakeBackend::new();
        backend.hidden = true;
        backend.d3d9_ok = true;
        assert_eq!(run_present(PresentThread::Main, &hold, Some(&hooks), &mut backend, 0x2000), 1);
        let events = take_events();
        assert_eq!(backend.presented_flag, Some(1));
        assert!(events.contains(&"d3d9_present"));
        assert!(!events.contains(&"original_present"));
    }

    #[test]
    fn drawing_present_ends_even_when_present_panics() {
        let _ = take_events();
        let hold = HoldState::new();
        let hooks = test_hooks();
        let mut draw = FakeBackend::new();
        draw.panic_on_present = true;
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_present(PresentThread::Drawing, &hold, Some(&hooks), &mut draw, 0x2000);
        }));
        assert!(panicked.is_err());
        assert_eq!(take_events().last(), Some(&"drawing_ends"));
    }
}
