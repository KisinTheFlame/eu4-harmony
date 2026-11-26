//! Where the framework meets the game's Direct3D 9 `Present` and device reset
//! wrappers (`0x16d4db0` and `0x16d4bb0`). Pure hook shell: upper nodes (the
//! framework UI, tooltips, font page growth, screenshots, input, safe points)
//! plug in through [`PresentHooks`] registered once at startup.
use super::drawing;
use crate::base::called_by_game;
use crate::native::hooks::Call;
use crate::native::objects::text::renderer::DEVICE;
use crate::native::points::{Function, Point};
use crate::native::probes;
use std::ffi::c_void;
use std::sync::OnceLock;

const PRESENT: usize = 0x16d4db0;
const PRESENT_OCCLUDED: usize = 0x16d4ddb;
const RENDERER_PRESENTED: usize = 0x132;
const BEFORE_RESET: usize = 0x16d4bb0;

type Present = unsafe extern "C" fn(usize) -> u8;

static ORIGINAL_PRESENT: Function<Present> = Function::new();

pub(crate) struct PresentHooks {
    pub(crate) is_held: fn() -> bool,
    pub(crate) discard_late: fn(),
    pub(crate) before_present: unsafe fn(usize, Option<*mut c_void>),
    pub(crate) after_main_present: unsafe fn(Option<*mut c_void>),
    pub(crate) before_reset: fn(),
    pub(crate) d3d9_present: unsafe fn(*mut c_void) -> bool,
    pub(crate) d3d9_screen: unsafe fn(*mut c_void) -> (f32, f32),
}

static HOOKS: OnceLock<PresentHooks> = OnceLock::new();

pub(crate) fn set_hooks(hooks: PresentHooks) {
    let _ = HOOKS.set(hooks);
}

struct PresentingGuard<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> PresentingGuard<F> {
    fn begin(begin: impl FnOnce(), end: F) -> Self {
        begin();
        Self(Some(end))
    }
}

impl<F: FnOnce()> Drop for PresentingGuard<F> {
    fn drop(&mut self) {
        if let Some(end) = self.0.take() {
            end();
        }
    }
}

unsafe fn before_present(renderer: usize) -> Option<*mut c_void> {
    let device =
        crate::base::guarded("frame present", || crate::native::memory().and_then(|m| m.ptr(renderer + DEVICE).ok()).map(|device| device as *mut c_void));
    if let Some(hooks) = HOOKS.get() {
        (hooks.before_present)(renderer, device);
    }
    device
}

/// Presents through the game's wrapper, except in a hidden start where
/// `S_PRESENT_OCCLUDED` (display asleep or desktop locked) is treated as
/// presented while negative Direct3D errors still fail.
unsafe fn call_present(renderer: usize) -> u8 {
    if !crate::base::hidden_start() {
        return ORIGINAL_PRESENT.get()(renderer);
    }
    let Some(hooks) = HOOKS.get() else {
        return ORIGINAL_PRESENT.get()(renderer);
    };
    let Some(device) = crate::native::memory().and_then(|m| m.ptr(renderer + DEVICE).ok()).filter(|&p| p != 0) else {
        return ORIGINAL_PRESENT.get()(renderer);
    };
    let ok = u8::from((hooks.d3d9_present)(device as *mut c_void));
    if let Some(m) = crate::native::memory() {
        let _ = m.write(renderer + RENDERER_PRESENTED, &[ok]);
    }
    ok
}

#[called_by_game]
unsafe extern "C" fn present(renderer: usize) -> u8 {
    if crate::base::thread_id() == crate::base::main_thread() {
        if HOOKS.get().is_some_and(|h| (h.is_held)()) {
            if let Some(h) = HOOKS.get() {
                (h.discard_late)();
            }
            return 1;
        }
        let device = before_present(renderer);
        if let Some(h) = HOOKS.get() {
            (h.after_main_present)(device);
        }
    } else if drawing::is_drawing_thread() {
        let device = before_present(renderer);
        crate::base::guarded("screen size", || {
            if let (Some(device), Some(h)) = (device, HOOKS.get()) {
                let (width, height) = unsafe { (h.d3d9_screen)(device) };
                drawing::presented(width as u32, height as u32);
            }
        });
        let _presenting = PresentingGuard::begin(drawing::present_begins, drawing::present_ends);
        let submission = probes::submission_begins();
        let presented = call_present(renderer);
        probes::submission_ends(submission, presented != 0, false);
        return presented;
    }
    let submission = (crate::base::thread_id() == crate::base::main_thread()).then(probes::submission_begins);
    let presented = call_present(renderer);
    if let Some(submission) = submission {
        probes::submission_ends(submission, presented != 0, true);
    }
    presented
}

fn before_reset(_: &mut Call) {
    probes::device_reset();
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
    use super::PresentingGuard;
    use std::cell::Cell;

    #[test]
    fn presenting_guard_pairs_begins_and_ends_including_on_unwind() {
        let log = Cell::new((0u32, 0u32));
        {
            let _guard = PresentingGuard::begin(|| log.set((log.get().0 + 1, log.get().1)), || log.set((log.get().0, log.get().1 + 1)));
            assert_eq!(log.get(), (1, 0));
        }
        assert_eq!(log.get(), (1, 1));

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = PresentingGuard::begin(|| log.set((log.get().0 + 1, log.get().1)), || log.set((log.get().0, log.get().1 + 1)));
            assert_eq!(log.get(), (2, 1));
            panic!("simulated Present fault");
        }));
        assert!(panicked.is_err());
        assert_eq!(log.get(), (2, 2));
    }
}
