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
use crate::called_by_game;
use crate::points::{Function, Point};
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

#[called_by_game]
unsafe extern "C" fn cursor_at(out: *mut [i32; 2], full_screen: u8, borderless: u8) -> *mut [i32; 2] {
    let held = || {
        // No player's mouse moves a hidden game's cursor: remote control
        // holds it from the start.
        if crate::hidden::hidden() && state().cursor.is_none() {
            hold(None);
        }
        state().cursor
    };
    match crate::guarded("virtual cursor", held) {
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
    /// What the game had been handed as held before its first frame was,
    /// once it was.
    before: Option<Held>,
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

impl State {
    const fn new() -> Self {
        State {
            frame: None,
            presents: 0,
            queue: VecDeque::new(),
            planned: Held { buttons: Vec::new(), keys: Vec::new() },
            held: Held { buttons: Vec::new(), keys: Vec::new() },
            next: 0,
            finished: None,
            cursor: None,
            real_at: None,
        }
    }

    /// A frame `size` pixels large is about to be presented: the steps of
    /// the input first in line to hand over now, each with what is held
    /// before and after it.
    ///
    /// An input taken for a frame of another size is dropped, as its
    /// positions no longer mean what they meant. What it was handed over
    /// already and would have let go of later, a modifier or a button held
    /// down, is let go of in its place, one frame each, as at a session's
    /// beginning: else it would stay held, and its client was told it
    /// was taken.
    fn present(&mut self, size: (u32, u32)) -> Vec<(Step, Held, Held)> {
        self.presents += 1;
        self.frame = Some(size);
        let Some(q) = self.queue.front_mut() else { return Vec::new() };
        let number = q.number;
        if q.frame != size && q.frame != ANY_FRAME {
            let before = q.before.clone().unwrap_or_else(|| self.held.clone());
            let left = Held {
                buttons: self.held.buttons.iter().copied().filter(|b| !before.buttons.contains(b)).collect(),
                keys: self.held.keys.iter().copied().filter(|k| !before.keys.contains(k)).collect(),
            };
            log!(
                "input #{number}: dropped, the frame changed from {}x{} to {size:?} before it was handed over; letting go of {} it held",
                q.frame.0,
                q.frame.1,
                left.buttons.len() + left.keys.len()
            );
            q.frames = left.release().into();
            q.frame = ANY_FRAME;
            // What it would have held is not held: plan again from what the game has.
            self.planned = self.held.clone();
            for step in self.queue.iter().flat_map(|q| q.frames.iter().flatten()) {
                self.planned.apply(step);
            }
        }
        let Some(q) = self.queue.front_mut() else { return Vec::new() };
        q.before.get_or_insert_with(|| self.held.clone());
        let steps = q.frames.pop_front().unwrap_or_default();
        if q.frames.is_empty() {
            self.queue.pop_front();
            self.finished = Some((number, self.presents));
        }
        let mut given = Vec::new();
        for step in steps {
            let before = self.held.clone();
            self.held.apply(&step);
            given.push((step, before, self.held.clone()));
        }
        given
    }
}

static STATE: Mutex<State> = Mutex::new(State::new());

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
    s.queue.push_back(Queued { number: s.next, frames, frame, before: None });
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
    let (given, cursor) = {
        let mut s = state();
        let given = s.present(size);
        (given, s.cursor)
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

/// Remote control holds the cursor at `at`, in pixels of the window's client
/// area; with none, where it holds it already, or else in the middle of the
/// window, where it does not scroll the map. For a
/// hidden game, whose cursor no player's mouse moves (`crate::hidden`).
pub fn hold(at: Option<[i32; 2]>) {
    let mut s = state();
    let Some(at) = at.or(s.cursor).or_else(|| window::middle(s.frame)) else { return };
    s.cursor = Some(at);
    s.real_at = window::real_cursor();
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
    s.queue.push_back(Queued { number, frames: release.into(), frame: ANY_FRAME, before: None });
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

/// The refusals the framework words itself of input at screen positions.
pub mod checks {
    use crate::game::{Fail, GResult, Status};

    /// Refuses a position off an image `(width, height)` pixels large: the frame,
    /// or a screenshot of it at `scale` percent.
    pub fn position(x: u32, y: u32, (width, height): (u32, u32), scale: u32) -> GResult<()> {
        if x >= width || y >= height {
            let image = if scale == 100 { "the frame".to_string() } else { format!("a screenshot at scale {scale}") };
            return Err(Fail::new(Status::InvalidArgument, format!("{x}, {y} is off {image}, which is {width} by {height} pixels")));
        }
        Ok(())
    }

    /// Refuses a number of clicks that is not 1 to 3.
    pub fn count(count: u32) -> GResult<()> {
        within("count", count, 3, "clicks")
    }

    /// Refuses a number of notches that is not 1 to 100.
    pub fn amount(amount: u32) -> GResult<()> {
        within("amount", amount, 100, "notches")
    }

    /// Refuses a number of presses that is not 1 to 100.
    pub fn repeat(repeat: u32) -> GResult<()> {
        within("repeat", repeat, 100, "presses")
    }

    fn within(name: &str, value: u32, most: u32, what: &str) -> GResult<()> {
        if !(1..=most).contains(&value) {
            return Err(Fail::new(Status::InvalidArgument, format!("{name} is 1 to {most} {what}; {value} is not")));
        }
        Ok(())
    }

    /// Refuses empty text.
    pub fn text(text: &str) -> GResult<()> {
        if text.is_empty() {
            return Err(Fail::new(Status::InvalidArgument, "the text is empty"));
        }
        Ok(())
    }

    /// Refuses more than one key where one is held or let go.
    pub fn one_key(names: &[&str]) -> GResult<()> {
        if names.len() != 1 {
            return Err(Fail::new(Status::InvalidArgument, format!("one key, not {}", names.join("+"))));
        }
        Ok(())
    }

    /// Refuses a modifier that is not ctrl, shift, alt or win (`modifier` says
    /// whether the key named is one).
    pub fn modifiers<'a>(keys: impl IntoIterator<Item = (&'a str, bool)>) -> GResult<()> {
        for (name, modifier) in keys {
            if !modifier {
                return Err(Fail::new(Status::InvalidArgument, format!("{name} is no modifier; the modifiers are ctrl, shift, alt and win")));
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn positions_are_on_the_image_and_numbers_in_their_ranges() {
            assert!(position(0, 0, (1920, 1080), 100).is_ok() && position(1919, 1079, (1920, 1080), 100).is_ok());
            assert_eq!(position(1920, 5, (1920, 1080), 100).unwrap_err().message, "1920, 5 is off the frame, which is 1920 by 1080 pixels");
            assert_eq!(position(5, 540, (960, 540), 50).unwrap_err().message, "5, 540 is off a screenshot at scale 50, which is 960 by 540 pixels");
            assert!(count(1).is_ok() && count(3).is_ok());
            assert_eq!(count(4).unwrap_err().message, "count is 1 to 3 clicks; 4 is not");
            assert_eq!(amount(0).unwrap_err().status, Status::InvalidArgument);
            assert!(repeat(100).is_ok() && repeat(101).is_err());
            assert_eq!(text("").unwrap_err().message, "the text is empty");
            assert_eq!(one_key(&["ctrl", "s"]).unwrap_err().message, "one key, not ctrl+s");
            assert!(one_key(&["shift"]).is_ok());
            assert!(modifiers([("ctrl", true), ("shift", true)]).is_ok());
            assert_eq!(modifiers([("ctrl", true), ("s", false)]).unwrap_err().message, "s is no modifier; the modifiers are ctrl, shift, alt and win");
        }
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

    #[test]
    fn an_input_dropped_for_another_frame_size_lets_go_of_what_it_held() {
        let shift = Key::named("shift").unwrap();
        let mut s = State::new();
        let frame = (1920, 1080);
        let drag = Action::Drag { from: (10, 10), to: (500, 10), button: Button::Left, modifiers: vec![shift] };
        let frames: VecDeque<Vec<Step>> = plan::frames(&drag, &s.planned).into();
        frames.iter().flatten().for_each(|step| s.planned.apply(step));
        s.queue.push_back(Queued { number: 1, frames, frame, before: None });
        // Shift down, the move to the start, the button down.
        for _ in 0..3 {
            s.present(frame);
        }
        assert_eq!(s.held, Held { buttons: vec![Button::Left], keys: vec![shift] });
        // The window changes size: the rest of the drag goes, and what it
        // held is let go of, a frame each.
        let given: Vec<Step> = (0..2).flat_map(|_| s.present((1280, 720))).map(|(step, _, _)| step).collect();
        assert_eq!(given, [Step::Button(Button::Left, false), Step::Key(shift, false)]);
        assert_eq!(s.held, Held::default());
        assert_eq!(s.planned, Held::default());
        assert!(s.queue.is_empty());
        assert_eq!(s.finished, Some((1, s.presents)));
        // An input dropped before any of it was handed over gives nothing.
        let click = Action::Click { at: (10, 10), button: Button::Left, count: 1, modifiers: vec![shift] };
        s.queue.push_back(Queued { number: 2, frames: plan::frames(&click, &s.planned).into(), frame, before: None });
        assert!(s.present((1280, 720)).is_empty());
        assert!(s.queue.is_empty() && s.held == Held::default());
    }
}
