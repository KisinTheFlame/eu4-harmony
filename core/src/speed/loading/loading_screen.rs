//! Throttle redundant loading screen redraws (`0x11bb4f0`,
//! docs/game/program/loading.md).
//!
//! Each call to `0x11bb4f0(graphics, text, change_screen)` runs a full
//! Direct3D BeginScene..EndScene..Present sequence through `0x10b3600`, which
//! blocks on DWM/vsync. During startup, `0x10a8930` invokes it once before its
//! province loop and again every 100 provinces (~52 calls across ~4,940
//! provinces, taking ~15 ms of real work but ~1.5 s of Present waits), and
//! `0x12db850` invokes it for every map graphics sub-step.
//!
//! `0x10b3600` only renders the loading screen and has no message-pump or
//! simulation side effects. When `change_screen == 0`, the device is ready
//! (`[graphics + 0x530] != 0`), the loading screen GUI window exists
//! (`[0x242bef8] != 0`), and a redraw completed less than `MIN_INTERVAL` ago,
//! the redundant redraw is skipped. Calls that switch the background image,
//! run before the loading window is built, or observe a lost device always go
//! through the game's function unchanged.
use crate::base::called_by_game;
use crate::native::points::{Function, Point};
use crate::native::speed_up::SpeedUp;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// `0x11bb4f0(graphics, text, change_screen)`: updates the loading screen
/// status text and presents a frame.
const UPDATE_LOADING_SCREEN: usize = 0x11bb4f0;
/// `graphics + 0x530`: device-ready byte read at `0x11bb516`.
const DEVICE_READY_OFFSET: usize = 0x530;
/// `[0x242bef8]`: pointer to the loading screen GUI window checked at `0x11bb551`.
const LOADING_SCREEN_GUI: usize = 0x242bef8;
/// Minimum interval between consecutive throttled redraws (10 Hz max).
const MIN_INTERVAL: Duration = Duration::from_millis(100);

pub const SPEED_UP: SpeedUp = SpeedUp { name: "loading-screen", group: "loading", points, checkable: false };

type UpdateLoadingScreen = unsafe extern "C" fn(usize, usize, u8);
static ORIGINAL: Function<UpdateLoadingScreen> = Function::new();

/// Timestamp of the last completed redraw in microseconds since bootstrap,
/// stored as `micros + 1` so `0` means no redraw has happened yet.
static LAST_DRAW_US: AtomicU64 = AtomicU64::new(0);

fn points() -> Vec<Point> {
    vec![
        Point::replace(
            UPDATE_LOADING_SCREEN,
            "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 4c 89 74 24 20",
            "loading screen update",
            update_loading_screen as UpdateLoadingScreen,
            &ORIGINAL,
        ),
        Point::keep(0x11bb516usize, "80 b9 30 05 00 00 00", "loading screen device ready flag").uses(&[DEVICE_READY_OFFSET]),
        Point::keep(0x11bb551usize, "48 83 3d 9f 09 27 01 00", "loading screen gui window").uses(&[LOADING_SCREEN_GUI]),
    ]
}

fn should_draw(change_screen: u8, device_ready: bool, has_gui: bool, last_draw_stored: u64, now_us: u64) -> bool {
    if change_screen != 0 || !device_ready || !has_gui || last_draw_stored == 0 {
        return true;
    }
    let last_us = last_draw_stored - 1;
    now_us.saturating_sub(last_us) >= MIN_INTERVAL.as_micros() as u64
}

fn encode_us(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX - 1).saturating_add(1)
}

#[called_by_game]
unsafe extern "C" fn update_loading_screen(graphics: usize, text: usize, change_screen: u8) {
    let draw = crate::base::guarded("loading screen throttle", || {
        if graphics == 0 {
            return true;
        }
        let mem = crate::native::hooked_memory();
        let Ok(device_ready) = mem.u8(graphics + DEVICE_READY_OFFSET) else { return true };
        let Ok(gui_ptr) = mem.u64(mem.at(LOADING_SCREEN_GUI)) else { return true };
        let now_us = u64::try_from(crate::base::since_bootstrap().as_micros()).unwrap_or(u64::MAX);
        should_draw(change_screen, device_ready != 0, gui_ptr != 0, LAST_DRAW_US.load(Ordering::Acquire), now_us)
    });

    if !draw {
        return;
    }

    unsafe { ORIGINAL.get()(graphics, text, change_screen) };
    LAST_DRAW_US.store(encode_us(crate::base::since_bootstrap()), Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_draw_always_runs_even_at_zero_timestamp() {
        assert!(should_draw(0, true, true, 0, 0));
    }

    #[test]
    fn skips_rapid_redraws_within_min_interval() {
        let last = encode_us(Duration::from_millis(500));
        assert!(!should_draw(0, true, true, last, 500_000));
        assert!(!should_draw(0, true, true, last, 599_999));
        assert!(should_draw(0, true, true, last, 600_000));
    }

    #[test]
    fn never_throttles_screen_change_lost_device_or_missing_gui() {
        let last = encode_us(Duration::from_millis(500));
        assert!(should_draw(1, true, true, last, 501_000));
        assert!(should_draw(0, false, true, last, 501_000));
        assert!(should_draw(0, true, false, last, 501_000));
    }
}
