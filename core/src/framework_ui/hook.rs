//! Where the framework UI meets the game's frame. The game reaches Direct3D 9
//! through a renderer object (its device at [`DEVICE`]) and
//! wrappers; every frame ends in the Present wrapper at 0x16d4db0 (renderer),
//! after the game's own EndScene. The framework draws its UI just before, in
//! a scene of its own when there is anything to draw, then reads the frame
//! back when a screenshot asks for it (`crate::game::screenshot`). Before a
//! device reset the game calls 0x16d4bb0 (renderer), where the framework
//! drops what lives in the default pool. A frame the main thread presents is
//! also a safe point (`crate::safe_point`).
//!
//! The device is created multithreaded (behaviour flags 0x44, seen in game):
//! each call takes the device's lock, a sequence of calls does not. While the
//! game loads, a thread of its own draws and presents the loading screen
//! (through 0x11bb450) as the main thread creates textures and draws into
//! them on the same device. Drawing the framework UI there interleaves its
//! state changes with the game's; in testing it went with graphics driver
//! hangs. So the framework draws only in frames the main thread presents: the
//! main menu and the game.
use crate::hooks::HookFrame;
use crate::objects::text::renderer::DEVICE;
use crate::points::{Function, Point};
use std::ffi::c_void;

const PRESENT: usize = 0x16d4db0;
const BEFORE_RESET: usize = 0x16d4bb0;

type Present = unsafe extern "C" fn(usize) -> u8;

static ORIGINAL_PRESENT: Function<Present> = Function::new();

unsafe extern "C" fn present(renderer: usize) -> u8 {
    if crate::thread_id() == crate::main_thread() {
        // Inside a guard, as all framework code here: a panic must not
        // unwind into the game.
        let device = crate::guarded("frame present", || crate::memory().and_then(|m| m.ptr(renderer + DEVICE).ok()).map(|device| device as *mut c_void));
        crate::guarded("framework UI frame", || {
            if let Some(device) = device {
                super::frame(device)
            }
        });
        // The frame as the player sees it, the framework UI included.
        crate::guarded("screenshot", || {
            if let Some(device) = device {
                crate::game::screenshot::presenting(device)
            }
        });
        crate::guarded("safe point", crate::safe_point::presented);
        crate::guarded("text costs", crate::unicode::costs::log_when_due);
    }
    ORIGINAL_PRESENT.get()(renderer)
}

extern "C" fn before_reset(_frame: *mut HookFrame) {
    crate::guarded("framework UI device reset", super::device_reset);
    crate::guarded("screenshot device reset", crate::game::screenshot::device_reset);
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(PRESENT, "40 53 48 83 ec 30 48 8b d9 48 c7 44 24 20 00 00", "frame present", present as Present, &ORIGINAL_PRESENT),
        Point::observe(BEFORE_RESET, "40 53 48 83 ec 20 48 8b d9 48 8b 49 10 48 85 c9", "before device reset", 1, Some(before_reset), None),
    ]
}
