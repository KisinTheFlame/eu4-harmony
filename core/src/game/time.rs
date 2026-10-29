//! Time: the date, whether the game is paused and by what, and its speed;
//! pausing, resuming and setting the speed as the player would; stopping time
//! as a date is reached, for remote control's advancing; and whether a game
//! event waits for the player's answer. The evidence is in
//! docs/research/time.md.
use super::{context, context_any, events, fail, game, map_manager, paused, Caller, Ctx, GResult, Status};
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::campaign::{classes, globals::WORLD, map_manager as manager, pause as pause_state},
    points::Point,
};
use std::sync::Mutex;

/// `(world, index)`: sets the speed, clamped to 0..=4, as both speed commands
/// do.
const SET_SPEED: usize = 0x7579f0;
/// The speed-up command's execution, which reads the speed.
const SPEED_UP: usize = 0xb79970;
/// `(pause state)`: the toggle the game console's `gamespeed` uses. Running,
/// it pauses as the player does; paused for any reason, it resumes and lets go
/// of every window holding the game paused, as the player's resuming does.
const TOGGLE_PAUSE: usize = 0x7e0250;
/// The map manager's method that shows an event to the player: it makes an
/// event window and adds it to the manager's popups.
const SHOW_EVENT: usize = 0x828c00;
const SHOW_EVENT_METHOD: usize = 0x108;
/// `(app)`: starts `eu4.exe` again, at the end of the game's main loop, when
/// the game returns to the main menu.
const RESTART: usize = 0x14c3a10;

/// The world's speed, i32: 0 for speed 1 to 4 for speed 5.
const SPEED: usize = 0x1e08;
/// The pause state's own flag (u8): nonzero once paused by the player (or by
/// remote control or an extension, through [`pause`]); when it is zero and
/// the game is paused, windows hold it paused.
const PLAYER_PAUSED: usize = 0x48;
/// The map manager's open popup windows: a list of nodes, the first here.
const POPUPS: usize = 0x78;
/// A node of the popups list.
mod popup {
    /// The window.
    pub const WINDOW: usize = 0;
    /// The next node.
    pub const NEXT: usize = 0x10;
    /// Nonzero (u8) once the window closed and the node waits to go.
    pub const GONE: usize = 0x18;
}
/// An event window, by vtable.
const EVENT_WINDOW: usize = 0x1d590f8;

/// The game speeds, slowest first.
pub const SPEEDS: std::ops::RangeInclusive<u32> = 1..=5;
/// The most popups read; the game shows a few at a time.
const MAX_POPUPS: usize = 1000;

/// What holds the game paused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pause {
    /// Nothing: time runs.
    #[default]
    Running,
    /// The player paused it, or remote control or an extension did.
    Player,
    /// A window that pauses the game while it is open, such as a game event's
    /// when the game pauses on events, or a message set to pause.
    Window,
}

/// The game's clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Time {
    /// The session, as [`super::session::Session`] has it.
    pub session: u64,
    /// The date (docs/research/dates.md), 24 a day.
    pub date: i32,
    /// 1 (slowest) to 5 (fastest).
    pub speed: u32,
    pub pause: Pause,
}

impl Time {
    pub fn paused(&self) -> bool {
        self.pause != Pause::Running
    }
}

/// The date, the speed, and what holds the game paused.
pub fn time(_: Caller) -> GResult<Time> {
    let m = game()?;
    let ctx = context(m)?;
    let index = m.i32(ctx.world + SPEED)?;
    if !(0..=4).contains(&index) {
        return Err(format!("unexpected game speed {index}").into());
    }
    let pause = if !paused(m, &ctx)? {
        Pause::Running
    } else if m.u8(pause_state_of(m, &ctx)? + PLAYER_PAUSED)? != 0 {
        Pause::Player
    } else {
        Pause::Window
    };
    Ok(Time { session: events::session(), date: ctx.date, speed: index as u32 + 1, pause })
}

/// Pauses the game, as the player would; nothing when it is paused already.
pub fn pause(_: Caller) -> GResult<()> {
    let m = game()?;
    pause_game(m, &context(m)?)
}

/// Resumes the game, as the player would: also when windows hold it paused,
/// which stay open. Nothing when it runs.
pub fn resume(_: Caller) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    if paused(m, &ctx)? {
        toggle(m, &ctx)?;
        if paused(m, &ctx)? {
            return fail(Status::Rejected, "the game stays paused");
        }
    }
    Ok(())
}

/// Sets the game speed, 1 (slowest) to 5 (fastest), as the player would.
pub fn set_speed(_: Caller, speed: u32) -> GResult<()> {
    if !SPEEDS.contains(&speed) {
        return fail(Status::InvalidArgument, format!("the speed is from 1 to 5, not {speed}"));
    }
    let m = game()?;
    let ctx = context(m)?;
    m.call(m.at(SET_SPEED), &[ctx.world, (speed - 1) as usize])?;
    Ok(())
}

/// How many game events wait for the player's answer: the event windows open
/// among the map manager's popups. Answering one closes its window.
///
/// This is where remote control's advancing time learns that a game event
/// needs an answer; what the events are and answering them belongs to the
/// events and offers capabilities.
pub fn game_events_waiting(_: Caller) -> GResult<usize> {
    let m = game()?;
    event_windows(m, &context(m)?)
}

/// The event windows open among the map manager's popups.
fn event_windows(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    let manager = map_manager(m, ctx)?;
    let mut node = m.ptr(manager + POPUPS)?;
    let mut waiting = 0;
    for _ in 0..MAX_POPUPS {
        if node == 0 {
            return Ok(waiting);
        }
        let window = m.ptr(node + popup::WINDOW)?;
        if m.u8(node + popup::GONE)? == 0 && window != 0 && m.ptr(window)? == m.at(EVENT_WINDOW) {
            waiting += 1;
        }
        node = m.ptr(node + popup::NEXT)?;
    }
    Err(format!("more than {MAX_POPUPS} popups").into())
}

/// A date time stops at once it is reached, held for an advance. Released
/// when the advance answers; dropped unreleased, when its client gave it up,
/// it calls the advance off: time stops at the end of the day under way.
#[derive(Debug)]
pub struct Stop {
    number: u64,
    released: bool,
}

/// A stop held: its number, its date, whether it was reached, and whether
/// it was called off (then it goes once reached).
type Held = (u64, i32, bool, bool);

#[derive(Default)]
struct Stops {
    last: u64,
    dates: Vec<Held>,
}

static STOPS: Mutex<Stops> = Mutex::new(Stops { last: 0, dates: Vec::new() });

fn stops<T>(f: impl FnOnce(&mut Stops) -> T) -> T {
    f(&mut STOPS.lock().unwrap_or_else(|e| e.into_inner()))
}

impl Stop {
    /// Forgets the date: the advance answered.
    pub fn release(mut self) {
        self.released = true;
        let number = self.number;
        stops(|s| s.dates.retain(|held| held.0 != number));
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let number = self.number;
        stops(|s| {
            s.dates.retain(|held| held.0 != number || !held.2);
            for held in s.dates.iter_mut().filter(|held| held.0 == number) {
                held.1 = i32::MIN;
                held.3 = true;
            }
        });
    }
}

/// Pauses the game as soon as the daily update reaches `date`, before
/// another day can begin; for remote control's advancing only. The stop
/// pauses the game once, and holds until the returned [`Stop`] goes.
pub fn stop_at(caller: Caller, date: i32) -> GResult<Stop> {
    if let Caller::Extension(_) = caller {
        return fail(Status::Unavailable, "only remote control advances time");
    }
    Ok(stops(|s| {
        s.last += 1;
        s.dates.push((s.last, date, false, false));
        Stop { number: s.last, released: false }
    }))
}

/// After the daily update (`events`' observer): pauses the game when a stop's
/// date is reached, or when a game event came up on the day while an advance
/// runs, so that the next frame begins no other day.
pub(super) fn day_passed(m: &Memory) {
    if crate::saves::load_in_progress() {
        return;
    }
    let Ok(ctx) = context_any(m) else { return };
    let due = reach(ctx.date) || (advancing() && event_windows(m, &ctx).is_ok_and(|open| open > 0));
    if due {
        if let Err(e) = pause_game(m, &ctx) {
            log!("time: cannot pause the advance: {}", e.message);
        }
    }
}

/// Whether an advance holds a stop it has not reached.
fn advancing() -> bool {
    stops(|s| s.dates.iter().any(|&(_, _, reached, _)| !reached))
}

/// Whether `date` reaches a stop not reached before; each is reached once,
/// and one called off goes then.
fn reach(date: i32) -> bool {
    stops(|s| {
        let mut due = false;
        for (_, at, reached, _) in &mut s.dates {
            if !*reached && date >= *at {
                *reached = true;
                due = true;
            }
        }
        s.dates.retain(|&(_, _, reached, called_off)| !(reached && called_off));
        due
    })
}

/// The map manager's pause state, checked by its owner.
fn pause_state_of(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    let manager = map_manager(m, ctx)?;
    let state = m.ptr(manager + manager::PAUSE)?;
    if m.ptr(state + pause_state::MANAGER)? != manager {
        return Err("unexpected pause owner".to_string().into());
    }
    Ok(state)
}

fn toggle(m: &Memory, ctx: &Ctx) -> GResult<()> {
    m.call(m.at(TOGGLE_PAUSE), &[pause_state_of(m, ctx)?])?;
    Ok(())
}

fn pause_game(m: &Memory, ctx: &Ctx) -> GResult<()> {
    if !paused(m, ctx)? {
        toggle(m, ctx)?;
        if !paused(m, ctx)? {
            return fail(Status::Rejected, "the game does not pause");
        }
    }
    Ok(())
}

/// Before the game starts `eu4.exe` again: remote control answers what waits
/// while this process can still write the answers.
extern "C" fn restart_before(_: *mut HookFrame) {
    crate::guarded("restart", || {
        log!("the game starts eu4.exe again");
        crate::remote::restarting();
    });
}

/// The speed, the pause toggle, the event windows among the popups, and the
/// restart.
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(
            SET_SPEED,
            "48 81 ec 88 00 00 00 41 b8 04 00 00 00 41 3b d0 44 0f 4e c2 33 c0 45 85 c0 41 0f 49 c0 89 81 08 1e 00 00",
            "set speed",
        )
        .uses(&[SPEED, 4]),
        Point::keep(SPEED_UP, "48 8b 0d 01 65 7c 01 8b 91 08 1e 00 00 ff c2 e9 6c e0 bd ff", "speed up command").uses(&[WORLD, SPEED]),
        Point::keep(TOGGLE_PAUSE, "40 53 48 83 ec 20 80 79 48 00 48 8b d9 75 5c", "pause toggle").uses(&[PLAYER_PAUSED]),
        Point::method(classes::MAP_MANAGER, SHOW_EVENT_METHOD, SHOW_EVENT, "map manager shows an event"),
        // It makes the event window (0x10b9a50) and adds it to its own
        // popups (0x6ecae0, this in rcx).
        Point::keep(0x828c43usize, "4c 8b ce 4c 8b c5 48 8b d3 48 8b c8 e8 fc 0d 89 00 90 48 8b d0 48 8b cb e8 80 3e ec ff", "event window shown"),
        // Adding a popup walks the list for one of its kind.
        Point::keep(
            0x6ecb14usize,
            "48 8b 5e 78 48 85 db 74 1c 0f 1f 00 48 8b 0b 48 8b 01 ff 50 08 84 c0 75 60 48 8b 43 10 48 8b d8 48 85 c0 75 e7",
            "map manager popups",
        )
        .uses(&[POPUPS, popup::NEXT]),
        // A popup that closes while the list is walked is marked to go later.
        Point::keep(0x6ea583usize, "41 80 bc 24 8c 00 00 00 00 74 06 c6 41 18 01", "popup gone").uses(&[popup::GONE]),
        Point::keep(0x10b9b3busize, "48 8d 05 b6 f5 c9 00 48 89 03", "event window class").uses(&[EVENT_WINDOW]),
        Point::observe(RESTART, "48 89 5c 24 08 48 89 74 24 18 55 57 41 56", "restart", 1, Some(restart_before), None),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_is_reached_once_and_forgotten_when_released() {
        let first = stop_at(Caller::Remote, 100).unwrap();
        let second = stop_at(Caller::Remote, 124).unwrap();
        assert_eq!(stop_at(Caller::Extension(0), 100).unwrap_err().status, Status::Unavailable);
        assert!(!reach(99));
        assert!(reach(100));
        assert!(!reach(100), "a stop pauses the game once");
        // A day past both, as when one was missed: one pause for both.
        first.release();
        let third = stop_at(Caller::Remote, 148).unwrap();
        assert!(reach(150));
        assert!(!reach(172));
        second.release();
        third.release();
        assert!(stops(|s| s.dates.is_empty()));

        // Only a stop not reached yet is an advance running.
        assert!(!advancing());
        let running = stop_at(Caller::Remote, 500).unwrap();
        assert!(advancing());
        running.release();
        assert!(!advancing());

        // Given up before its date, an advance calls the stop off: the game
        // pauses at the end of the next day, and the stop goes.
        drop(stop_at(Caller::Remote, 1000).unwrap());
        assert!(reach(200));
        assert!(!reach(224));
        assert!(stops(|s| s.dates.is_empty()));
        // Given up once reached, it just goes.
        let reached = stop_at(Caller::Remote, 300).unwrap();
        assert!(reach(300));
        drop(reached);
        assert!(stops(|s| s.dates.is_empty()));
    }
}
