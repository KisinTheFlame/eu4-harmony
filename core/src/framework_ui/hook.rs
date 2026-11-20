//! Where the framework UI meets the game's frame. The game reaches Direct3D 9
//! through a renderer object (its device at [`DEVICE`]) and
//! wrappers; every frame ends in the Present wrapper at 0x16d4db0 (renderer),
//! after the game's own EndScene. The framework draws its UI just before, in
//! a scene of its own when there is anything to draw, then reads the frame
//! back when a screenshot asks for it (`crate::game::screenshot`). Before a
//! device reset the game calls 0x16d4bb0 (renderer), where the framework
//! drops what lives in the default pool. A frame the main thread presents is
//! also a safe point (`crate::safe_point`). Frames held back
//! ([`super::hold`]) are neither presented nor any of that.
//!
//! The device is created multithreaded (behaviour flags 0x44, seen in game):
//! each call takes the device's lock, a sequence of calls does not. While the
//! game loads, a thread of its own draws and presents the loading screen
//! (through 0x11bb450) as the main thread creates textures and draws into
//! them on the same device. Drawing the framework UI there interleaves its
//! state changes with the game's; in testing it went with graphics driver
//! hangs. So the framework draws only in frames the main thread presents: the
//! main menu and the game.
use crate::called_by_game;
use crate::hooks::Call;
use crate::objects::text::renderer::DEVICE;
use crate::points::{Function, Point};
use std::ffi::c_void;

const PRESENT: usize = 0x16d4db0;
const BEFORE_RESET: usize = 0x16d4bb0;

type Present = unsafe extern "C" fn(usize) -> u8;

static ORIGINAL_PRESENT: Function<Present> = Function::new();

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

/// Runs the framework's guarded pre-Present work (font pages, framework UI,
/// native tooltips over it, and screenshot capture) and returns the Direct3D 9
/// device pointer when available.
unsafe fn before_present(renderer: usize) -> Option<*mut c_void> {
    let device = crate::guarded("frame present", || crate::memory().and_then(|m| m.ptr(renderer + DEVICE).ok()).map(|device| device as *mut c_void));
    crate::guarded("font pages", crate::unicode::fill::grow_pages);
    crate::guarded("framework UI frame", || {
        if let Some(device) = device {
            unsafe { super::frame(device) }
        }
    });
    crate::guarded("native tooltip over framework UI", || crate::tooltips::draw_late(renderer));
    // The frame as the player sees it, the framework UI included.
    crate::guarded("screenshot", || {
        if let Some(device) = device {
            crate::game::screenshot::presenting(device)
        }
    });
    device
}

#[called_by_game]
unsafe extern "C" fn present(renderer: usize) -> u8 {
    if crate::thread_id() == crate::main_thread() {
        if super::held() {
            crate::tooltips::discard_late();
            // Answered as a frame presented on a working device.
            return 1;
        }
        // Inside a guard, as all framework code here: a panic must not
        // unwind into the game.
        let device = before_present(renderer);
        crate::guarded("safe point", crate::safe_point::presented);
        // Input taken at this safe point or before reaches the game at its
        // next frame.
        crate::guarded("input", || {
            if let Some(device) = device {
                let (width, height) = unsafe { super::d3d9::Backend::screen(device) };
                crate::game::input::presenting((width as u32, height as u32));
            }
        });
        crate::guarded("text costs", crate::unicode::costs::log_when_due);
    } else if crate::drawing::is_drawing_thread() {
        // The drawing thread's frame, under the presentation lock: what the
        // main thread does at a present but its safe point and its input,
        // which it does where the frame was prepared (`crate::drawing`).
        // Present itself goes without the lock.
        let device = before_present(renderer);
        crate::guarded("screen size", || {
            if let Some(device) = device {
                let (width, height) = unsafe { super::d3d9::Backend::screen(device) };
                crate::drawing::presented(width as u32, height as u32);
            }
        });
        let _presenting = PresentingGuard::begin(crate::drawing::present_begins, crate::drawing::present_ends);
        let submission = crate::dev::submission_begins();
        let presented = ORIGINAL_PRESENT.get()(renderer);
        crate::dev::submission_ends(submission, presented != 0, false);
        return presented;
    }
    let submission = (crate::thread_id() == crate::main_thread()).then(crate::dev::submission_begins);
    let presented = ORIGINAL_PRESENT.get()(renderer);
    if let Some(submission) = submission {
        crate::dev::submission_ends(submission, presented != 0, true);
    }
    presented
}

fn before_reset(_: &mut Call) {
    crate::dev::device_reset();
    crate::tooltips::clear();
    super::device_reset();
    crate::game::screenshot::device_reset();
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(PRESENT, "40 53 48 83 ec 30 48 8b d9 48 c7 44 24 20 00 00", "frame present", present as Present, &ORIGINAL_PRESENT),
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
