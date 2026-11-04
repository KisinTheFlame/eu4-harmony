//! Input, for remote control only (ADR remote-clients-give-the-players-input):
//! mouse and keyboard input at screen positions, given to the game as the
//! player's own.
//!
//! An input taken ([`take`]) is checked and queued, and its frames planned
//! ([`plan`]); at each frame the main thread presents ([`presenting`]), the
//! next frame's steps are handed to the game's window procedure as the
//! messages Windows sends for a real mouse and keyboard ([`window`]), so
//! they go through the framework UI first and then to SDL, which the game
//! reads at the start of its next frame. Inputs are handed over one after
//! another, in the order they were taken; [`handled`] says when an input's
//! last frame has run.
//!
//! The game reads where the cursor is once a frame (`CURSOR_AT`): from
//! Windows' own cursor while its window has the focus, from SDL otherwise
//! (docs/research/input.md). The framework answers it with the cursor input
//! left, the virtual cursor, until the player's own mouse moves over the
//! game; while the virtual cursor holds, the messages Windows sends for a
//! cursor that did not move, and for leaving the window, do not reach the
//! game.
pub mod plan;
mod window;

use super::{fail, Caller, GResult, Status};
use crate::points::{Function, Point};
use eu4_harmony_checks::input as checks;
use plan::{Held, Step};
use std::collections::VecDeque;
use std::sync::Mutex;

pub use eu4_harmony_remote::input::{Button, Direction, Key};

/// The game's cursor position, as its input device reads it once a frame
/// (rcx the point it writes, in pixels of the window's client area; dl and
/// r8b whether the window is full screen or borderless): Windows' cursor while
/// the window has the focus (`[0x1fd10c5]`), SDL's otherwise.
const CURSOR_AT: usize = 0x14e5e50;

type CursorAt = unsafe extern "C" fn(*mut [i32; 2], u8, u8) -> *mut [i32; 2];

static ORIGINAL_CURSOR_AT: Function<CursorAt> = Function::new();

pub fn points() -> Vec<Point> {
    vec![Point::replace(CURSOR_AT, "48 89 5c 24 10 55 56 57 48 83 ec 40", "cursor position", cursor_at as CursorAt, &ORIGINAL_CURSOR_AT)]
}

unsafe extern "C" fn cursor_at(out: *mut [i32; 2], full_screen: u8, borderless: u8) -> *mut [i32; 2] {
    match crate::guarded("virtual cursor", || state().cursor) {
        Some(at) => {
            *out = at;
            out
        }
        None => ORIGINAL_CURSOR_AT.get()(out, full_screen, borderless),
    }
}

/// A screen position: pixels of the frame from its top left corner.
pub type Position = (u32, u32);

/// One input, its positions in pixels of the frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Click {
        at: Position,
        button: Button,
        count: u32,
        modifiers: Vec<Key>,
    },
    Move {
        at: Position,
        modifiers: Vec<Key>,
    },
    Drag {
        from: Position,
        to: Position,
        button: Button,
        modifiers: Vec<Key>,
    },
    Scroll {
        at: Position,
        direction: Direction,
        amount: u32,
        modifiers: Vec<Key>,
    },
    /// Keys pressed together, `repeat` times.
    Keys {
        keys: Vec<Key>,
        repeat: u32,
    },
    Type(String),
    MouseDown {
        at: Position,
        button: Button,
    },
    MouseUp {
        at: Position,
        button: Button,
    },
    KeyDown(Key),
    KeyUp(Key),
}

impl Action {
    fn positions(&mut self) -> Vec<&mut Position> {
        match self {
            Action::Click { at, .. } | Action::Move { at, .. } | Action::Scroll { at, .. } | Action::MouseDown { at, .. } | Action::MouseUp { at, .. } => {
                vec![at]
            }
            Action::Drag { from, to, .. } => vec![from, to],
            Action::Keys { .. } | Action::Type(_) | Action::KeyDown(_) | Action::KeyUp(_) => vec![],
        }
    }

    fn modifiers(&self) -> &[Key] {
        match self {
            Action::Click { modifiers, .. } | Action::Move { modifiers, .. } | Action::Drag { modifiers, .. } | Action::Scroll { modifiers, .. } => modifiers,
            _ => &[],
        }
    }
}

/// An input taken, by its number: inputs are numbered in the order they are
/// taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Taken(pub u64);

/// Where the cursor is and what remote control holds down.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    /// The cursor's screen position; None off the frame.
    pub at: Option<Position>,
    pub buttons: Vec<Button>,
    pub keys: Vec<Key>,
}

/// An input queued: its frames still to hand over.
struct Queued {
    number: u64,
    frames: VecDeque<Vec<Step>>,
    /// The frame's size when it was taken.
    frame: (u32, u32),
}

struct State {
    /// The size of the frame presented last; None before the first.
    frame: Option<(u32, u32)>,
    /// Frames presented.
    presents: u64,
    queue: VecDeque<Queued>,
    /// What is held once everything queued is handed over.
    planned: Held,
    /// What the game has been handed as held.
    held: Held,
    next: u64,
    /// The last input whose frames are all handed over, and the present that
    /// handed over its last.
    finished: Option<(u64, u64)>,
    /// The virtual cursor, in pixels of the window's client area.
    cursor: Option<[i32; 2]>,
    /// Where Windows' cursor was, on the screen, when the virtual cursor last
    /// moved: a mouse message that finds it elsewhere is the player's.
    real_at: Option<[i32; 2]>,
}

static STATE: Mutex<State> = Mutex::new(State {
    frame: None,
    presents: 0,
    queue: VecDeque::new(),
    planned: Held { buttons: Vec::new(), keys: Vec::new() },
    held: Held { buttons: Vec::new(), keys: Vec::new() },
    next: 0,
    finished: None,
    cursor: None,
    real_at: None,
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A frame size that drops no input when the frame changes: for an input
/// without positions.
const ANY_FRAME: (u32, u32) = (0, 0);

/// A position in a screenshot at `scale` percent as one in the frame, `frame`
/// pixels large: the middle of the pixels the screenshot's pixel stands for.
fn scale_up((x, y): Position, frame: (u32, u32), scale: u32) -> Position {
    use super::screenshot::png::scaled;
    let up = |v: u32, side: u32| ((2 * v as u64 + 1) * side as u64 / (2 * scaled(side, scale) as u64)).min(side as u64 - 1) as u32;
    (up(x, frame.0), up(y, frame.1))
}

/// Takes an input: checks it against the frame presented last and queues it.
/// `action`'s positions are in a screenshot at `scale` percent of the frame.
pub fn take(caller: Caller, mut action: Action, scale: u32) -> GResult<Taken> {
    if let Caller::Extension(_) = caller {
        return fail(Status::Unavailable, "only remote control gives input");
    }
    super::screenshot::check_scale(scale)?;
    match &action {
        Action::Click { count, .. } => checks::count(*count)?,
        Action::Scroll { amount, .. } => checks::amount(*amount)?,
        Action::Keys { repeat, .. } => checks::repeat(*repeat)?,
        Action::Type(text) => checks::text(text)?,
        _ => {}
    }
    checks::modifiers(action.modifiers().iter().map(|k| (k.name, k.modifier())))?;
    let mut guard = state();
    let s = &mut *guard;
    let positions = action.positions();
    let frame = match (s.frame, positions.is_empty()) {
        (_, true) => ANY_FRAME,
        (Some(frame), false) => frame,
        (None, false) => return fail(Status::NotReady, "the game has not presented a frame yet"),
    };
    let image = (super::screenshot::png::scaled(frame.0, scale), super::screenshot::png::scaled(frame.1, scale));
    for at in positions {
        checks::position(at.0, at.1, image, scale)?;
        *at = scale_up(*at, frame, scale);
    }
    let frames: VecDeque<Vec<Step>> = plan::frames(&action, &s.planned).into();
    frames.iter().flatten().for_each(|step| s.planned.apply(step));
    s.next += 1;
    s.queue.push_back(Queued { number: s.next, frames, frame });
    Ok(Taken(s.next))
}

/// Whether every input up to `taken` has been handed to the game and a frame
/// has run since the last of it.
pub fn handled(taken: Taken) -> bool {
    let s = state();
    s.finished.is_some_and(|(number, at)| number >= taken.0 && s.presents > at)
}

/// Where the game takes the cursor to be, and what remote control holds.
pub fn cursor(caller: Caller) -> GResult<Cursor> {
    if let Caller::Extension(_) = caller {
        return fail(Status::Unavailable, "only remote control gives input");
    }
    let s = state();
    let at = s.frame.and_then(|frame| window::cursor(s.cursor, frame));
    Ok(Cursor { at, buttons: s.held.buttons.clone(), keys: s.held.keys.clone() })
}

/// Just before the main thread presents a frame `size` pixels large: hands
/// the game the next frame's steps of the input first in line.
pub fn presenting(size: (u32, u32)) {
    let mut given = Vec::new();
    let cursor = {
        let mut guard = state();
        let s = &mut *guard;
        s.presents += 1;
        s.frame = Some(size);
        let Some(q) = s.queue.front_mut() else { return };
        let number = q.number;
        if q.frame != size && q.frame != ANY_FRAME {
            log!("input #{number}: dropped, the frame changed from {}x{} to {size:?} before it was handed over", q.frame.0, q.frame.1);
            s.queue.pop_front();
            s.finished = Some((number, s.presents));
            // What it would have held is not held: plan again from what the game has.
            s.planned = s.held.clone();
            for step in s.queue.iter().flat_map(|q| q.frames.iter().flatten()) {
                s.planned.apply(step);
            }
            return;
        }
        let steps = q.frames.pop_front().unwrap_or_default();
        if q.frames.is_empty() {
            s.queue.pop_front();
            s.finished = Some((number, s.presents));
        }
        for step in steps {
            let before = s.held.clone();
            s.held.apply(&step);
            given.push((step, before, s.held.clone()));
        }
        s.cursor
    };
    // Outside the lock: the window procedure the messages go through must not
    // find it held.
    let mut moved = None;
    for (step, before, after) in &given {
        if let Some(m) = window::give(step, before, after, size, moved.map_or(cursor, |(c, _)| Some(c))) {
            moved = Some(m);
        }
    }
    if let Some((at, real_at)) = moved {
        let mut s = state();
        s.cursor = Some(at);
        s.real_at = real_at;
    }
}

/// A session begins: remote control lets go of everything it holds.
pub fn session_began() {
    let mut s = state();
    if s.planned == Held::default() {
        return;
    }
    let release = s.planned.release();
    s.planned = Held::default();
    s.next += 1;
    let number = s.next;
    log!("input: a session began; letting go of {} held", release.len());
    s.queue.push_back(Queued { number, frames: release.into(), frame: ANY_FRAME });
}

/// A message Windows sends the game's window, not one input gave: whether
/// the game is to be kept from it. While the virtual cursor holds, a
/// mouse message with Windows' cursor where it was, and leaving the window,
/// are kept from the game; the player's mouse moving, or a button or the
/// wheel, hands the cursor back to the player.
pub fn real(message: u32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    const WM_MOUSELEAVE: u32 = 0x02a3;
    let mut s = state();
    if s.cursor.is_none() {
        return false;
    }
    match message {
        WM_MOUSELEAVE => true,
        WM_MOUSEMOVE if window::real_cursor() == s.real_at => true,
        WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_MOUSEWHEEL | 0x020e => {
            log!("input: the player's mouse moved over the game; the cursor is the player's again");
            s.cursor = None;
            s.real_at = None;
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screenshot_s_pixel_is_the_middle_of_the_frame_s_pixels_it_stands_for() {
        assert_eq!(scale_up((0, 0), (3840, 2160), 50), (1, 1));
        assert_eq!(scale_up((1919, 1079), (3840, 2160), 50), (3839, 2159));
        assert_eq!(scale_up((100, 200), (3840, 2160), 100), (100, 200));
        assert_eq!(scale_up((0, 0), (1920, 1080), 1), (50, 49));
    }
}
