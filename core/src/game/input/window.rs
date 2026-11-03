//! Steps handed to the game as the messages Windows sends its window for a
//! real mouse and keyboard, through the framework's window procedure
//! (`framework_ui::input`), so that the framework UI has them first and SDL,
//! which the game reads, after it (docs/research/input.md). A position in
//! the frame is one in the window's client area, scaled when the two differ.
use super::{plan::Held, plan::Step, Button, Direction, Position};
use crate::framework_ui::input::{inject, window};
use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, ScreenToClient};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_MOUSEHWHEEL: u32 = 0x020e;
const WM_UNICHAR: u32 = 0x0109;
const MK_LBUTTON: usize = 0x01;
const MK_RBUTTON: usize = 0x02;
const MK_SHIFT: usize = 0x04;
const MK_CONTROL: usize = 0x08;
const MK_MBUTTON: usize = 0x10;
const WHEEL_DELTA: i32 = 120;

/// The size of the window's client area.
fn client() -> Option<(i32, i32)> {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    (unsafe { GetClientRect(window()?, &mut r) } != 0 && r.right > 0 && r.bottom > 0).then_some((r.right, r.bottom))
}

/// A position in the frame, `frame` pixels large, in the client area.
fn to_client((x, y): Position, frame: (u32, u32)) -> [i32; 2] {
    match client() {
        Some((w, h)) if (w as u32, h as u32) != frame => [(x as i64 * w as i64 / frame.0 as i64) as i32, (y as i64 * h as i64 / frame.1 as i64) as i32],
        _ => [x as i32, y as i32],
    }
}

/// Where Windows' cursor is on the screen.
pub fn real_cursor() -> Option<[i32; 2]> {
    let mut p = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut p) } != 0).then_some([p.x, p.y])
}

/// The cursor's position in the frame: the virtual cursor, or else Windows'
/// own; None off the frame.
pub fn cursor(virtual_cursor: Option<[i32; 2]>, frame: (u32, u32)) -> Option<Position> {
    let [x, y] = match virtual_cursor {
        Some(at) => at,
        None => {
            let [x, y] = real_cursor()?;
            let mut p = POINT { x, y };
            if unsafe { ScreenToClient(window()?, &mut p) } == 0 {
                return None;
            }
            [p.x, p.y]
        }
    };
    let (w, h) = client().unwrap_or((frame.0 as i32, frame.1 as i32));
    if x < 0 || y < 0 || x >= w || y >= h {
        return None;
    }
    Some(((x as i64 * frame.0 as i64 / w as i64) as u32, (y as i64 * frame.1 as i64 / h as i64) as u32))
}

/// The mouse message flags for what is held.
fn flags(held: &Held) -> usize {
    let mut f = 0;
    for b in &held.buttons {
        f |= match b {
            Button::Left => MK_LBUTTON,
            Button::Right => MK_RBUTTON,
            Button::Middle => MK_MBUTTON,
        };
    }
    if held.key("shift") {
        f |= MK_SHIFT;
    }
    if held.key("ctrl") {
        f |= MK_CONTROL;
    }
    f
}

fn at(p: [i32; 2]) -> isize {
    ((p[1] as u16 as isize) << 16) | (p[0] as u16 as isize)
}

/// Hands the game one step: `before` and `after` what is held around it,
/// `cursor` the virtual cursor before it. Answers where a step that moved
/// the cursor left it, with Windows' own cursor then.
pub fn give(step: &Step, before: &Held, after: &Held, frame: (u32, u32), cursor: Option<[i32; 2]>) -> Option<([i32; 2], Option<[i32; 2]>)> {
    // Before the framework UI has the window, nothing reaches the game.
    window()?;
    let here = cursor.unwrap_or_else(|| {
        let (w, h) = client().unwrap_or((frame.0 as i32, frame.1 as i32));
        [w / 2, h / 2]
    });
    match *step {
        Step::Move(x, y) => {
            let to = to_client((x, y), frame);
            inject(WM_MOUSEMOVE, flags(after), at(to));
            return Some((to, real_cursor()));
        }
        Step::Button(button, down) => {
            let message = match (button, down) {
                (Button::Left, true) => WM_LBUTTONDOWN,
                (Button::Left, false) => WM_LBUTTONUP,
                (Button::Right, true) => WM_RBUTTONDOWN,
                (Button::Right, false) => WM_RBUTTONUP,
                (Button::Middle, true) => WM_MBUTTONDOWN,
                (Button::Middle, false) => WM_MBUTTONUP,
            };
            inject(message, flags(after), at(here));
            // Without a move first, the cursor is where the click was.
            if cursor.is_none() {
                return Some((here, real_cursor()));
            }
        }
        Step::Wheel(direction) => {
            let (message, delta) = match direction {
                Direction::Up => (WM_MOUSEWHEEL, WHEEL_DELTA),
                Direction::Down => (WM_MOUSEWHEEL, -WHEEL_DELTA),
                Direction::Right => (WM_MOUSEHWHEEL, WHEEL_DELTA),
                Direction::Left => (WM_MOUSEHWHEEL, -WHEEL_DELTA),
            };
            // The wheel's messages carry a position on the screen.
            let mut p = POINT { x: here[0], y: here[1] };
            unsafe { ClientToScreen(window()?, &mut p) };
            inject(message, ((delta as u16 as usize) << 16) | flags(after), at([p.x, p.y]));
        }
        Step::Key(key, down) => {
            // With alt held, or for alt itself, Windows sends the system key
            // messages.
            let system = before.key("alt") || after.key("alt") || key.name == "alt";
            let message = match (system, down) {
                (false, true) => WM_KEYDOWN,
                (false, false) => WM_KEYUP,
                (true, true) => WM_SYSKEYDOWN,
                (true, false) => WM_SYSKEYUP,
            };
            let mut l = 1 | (key.scan as isize) << 16;
            if key.extended {
                l |= 1 << 24;
            }
            if system && key.name != "alt" {
                l |= 1 << 29;
            }
            if !down {
                l |= (1 << 30) | (1 << 31);
            }
            inject(message, key.virtual_key as usize, l);
        }
        Step::Text(c) => {
            // A character of the Basic Multilingual Plane as a keyboard's
            // comes; one beyond it, which UTF-16 cannot carry in one message,
            // whole.
            let message = if (c as u32) < 0x10000 { WM_CHAR } else { WM_UNICHAR };
            inject(message, c as usize, 1);
        }
    }
    None
}
