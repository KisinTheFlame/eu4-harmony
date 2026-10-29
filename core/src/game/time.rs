//! Time: the date, whether the game is paused and by what, and its speed;
//! pausing, resuming and setting the speed as the player would; stopping time
//! as a date is reached, for remote control's advancing; and whether a game
//! event waits for the player's answer. The evidence is in
//! docs/research/time.md.
use super::{context, context_any, events, fail, game, map_manager, paused, Caller, Ctx, GResult, Status};
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::{
        campaign::{classes, globals::WORLD, map_manager as manager, pause as pause_state, world},
        list,
    },
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
/// The map manager's method that shows an event to the player
/// ([`manager::SHOW_EVENT`]).
const SHOW_EVENT: usize = 0x828c00;
/// `(app)`: starts `eu4.exe` again, at the end of the game's main loop, when
/// the game returns to the main menu.
const RESTART: usize = 0x14c3a10;

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
    let index = m.i32(ctx.world + world::SPEED)?;
    if !(0..=4).contains(&index) {
        return Err(format!("unexpected game speed {index}").into());
    }
    let pause = if !paused(m, &ctx)? {
        Pause::Running
    } else if m.u8(pause_state_of(m, &ctx)? + pause_state::PLAYER_PAUSED)? != 0 {
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

/// How many game events wait for the player's answer, as
/// [`game_events_waiting_in`] counts them.
pub fn game_events_waiting(_: Caller) -> GResult<usize> {
    let m = game()?;
    game_events_waiting_in(m, &context(m)?)
}

/// How many game events wait for the player's answer in the campaign `ctx`
/// was read from, also during the daily update: the event windows open among
/// the map manager's popups. Answering one closes its window.
///
/// The one place remote control's advancing time learns that a game event
/// needs an answer: an advance asks it at the safe point, through
/// [`game_events_waiting`], and the end of the daily update during an
/// advance asks it directly, so the two never disagree. What the events are
/// and answering them belongs to the events and offers capabilities.
pub fn game_events_waiting_in(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    let manager = map_manager(m, ctx)?;
    let mut node = m.ptr(manager + manager::POPUPS + list::HEAD)?;
    let mut waiting = 0;
    for _ in 0..MAX_POPUPS {
        if node == 0 {
            return Ok(waiting);
        }
        let window = m.ptr(node + list::ITEM)?;
        if m.u8(node + list::REMOVED)? == 0 && window != 0 && m.ptr(window)? == m.at(classes::EVENT_WINDOW) {
            waiting += 1;
        }
        node = m.ptr(node + list::NEXT)?;
    }
    Err(format!("more than {MAX_POPUPS} popups").into())
}

/// A date time stops at once it is reached, held for an advance. Released
/// when the advance answers; dropped unreleased, when its client gave it up,
/// it calls the advance off: time stops at the end of the day under way. It
/// belongs to the session it was held in: once another game is loaded or
/// started, it goes, called off or not, and never pauses the new game.
#[derive(Debug)]
pub struct Stop {
    number: u64,
    released: bool,
}

/// A stop held.
struct Held {
    number: u64,
    /// The session it was held in ([`events::session`]).
    session: u64,
    date: i32,
    reached: bool,
    /// Its advance was given up: it goes once reached.
    called_off: bool,
}

/// The stops held, by the session the game is in when they are asked about.
#[derive(Default)]
struct Stops {
    last: u64,
    held: Vec<Held>,
}

impl Stops {
    /// Holds a stop at `date` in `session`, and gives its number.
    fn hold(&mut self, session: u64, date: i32) -> u64 {
        self.forget_other_sessions(session);
        self.last += 1;
        self.held.push(Held { number: self.last, session, date, reached: false, called_off: false });
        self.last
    }

    /// Forgets a stop: its advance answered.
    fn release(&mut self, number: u64) {
        self.held.retain(|held| held.number != number);
    }

    /// Calls a stop off in `session`: one not reached stops time at the end
    /// of the day under way; one reached, or held in another session, goes.
    fn call_off(&mut self, number: u64, session: u64) {
        self.forget_other_sessions(session);
        self.held.retain(|held| held.number != number || !held.reached);
        for held in self.held.iter_mut().filter(|held| held.number == number) {
            held.date = i32::MIN;
            held.called_off = true;
        }
    }

    /// Whether an advance in `session` holds a stop it has not reached.
    fn advancing(&self, session: u64) -> bool {
        self.held.iter().any(|held| held.session == session && !held.reached)
    }

    /// Whether `date`, in `session`, reaches a stop not reached before; each
    /// is reached once, and one called off goes then. Stops held in another
    /// session go without being reached.
    fn reach(&mut self, session: u64, date: i32) -> bool {
        self.forget_other_sessions(session);
        let mut due = false;
        for held in self.held.iter_mut().filter(|held| !held.reached && date >= held.date) {
            held.reached = true;
            due = true;
        }
        self.held.retain(|held| !(held.reached && held.called_off));
        due
    }

    fn forget_other_sessions(&mut self, session: u64) {
        self.held.retain(|held| held.session == session);
    }
}

static STOPS: Mutex<Stops> = Mutex::new(Stops { last: 0, held: Vec::new() });

fn stops<T>(f: impl FnOnce(&mut Stops) -> T) -> T {
    f(&mut STOPS.lock().unwrap_or_else(|e| e.into_inner()))
}

impl Stop {
    /// Forgets the date: the advance answered.
    pub fn release(mut self) {
        self.released = true;
        let number = self.number;
        stops(|s| s.release(number));
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        if !self.released {
            let (number, session) = (self.number, events::session());
            stops(|s| s.call_off(number, session));
        }
    }
}

/// Pauses the game as soon as the daily update reaches `date`, before
/// another day can begin; for remote control's advancing only. The stop
/// pauses the game once, in this session only, and holds until the returned
/// [`Stop`] goes.
pub fn stop_at(caller: Caller, date: i32) -> GResult<Stop> {
    if let Caller::Extension(_) = caller {
        return fail(Status::Unavailable, "only remote control advances time");
    }
    let session = events::session();
    let number = stops(|s| s.hold(session, date));
    Ok(Stop { number, released: false })
}

/// After the daily update (`events`' observer): pauses the game when a stop's
/// date is reached, or when a game event came up on the day while an advance
/// runs, so that the next frame begins no other day.
pub(super) fn day_passed(m: &Memory) {
    if crate::saves::load_in_progress() {
        return;
    }
    let Ok(ctx) = context_any(m) else { return };
    let session = events::session();
    let due = stops(|s| s.reach(session, ctx.date)) || (stops(|s| s.advancing(session)) && game_events_waiting_in(m, &ctx).is_ok_and(|waiting| waiting > 0));
    if due {
        if let Err(e) = pause_game(m, &ctx) {
            log!("time: cannot pause the advance: {}", e.message);
        }
    }
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
        .uses(&[world::SPEED, 4]),
        Point::keep(SPEED_UP, "48 8b 0d 01 65 7c 01 8b 91 08 1e 00 00 ff c2 e9 6c e0 bd ff", "speed up command").uses(&[WORLD, world::SPEED]),
        Point::keep(TOGGLE_PAUSE, "40 53 48 83 ec 20 80 79 48 00 48 8b d9 75 5c", "pause toggle").uses(&[pause_state::PLAYER_PAUSED]),
        Point::method(classes::MAP_MANAGER, manager::SHOW_EVENT, SHOW_EVENT, "map manager shows an event"),
        // It makes the event window (0x10b9a50) and adds it to its own
        // popups (0x6ecae0, this in rcx).
        Point::keep(0x828c43usize, "4c 8b ce 4c 8b c5 48 8b d3 48 8b c8 e8 fc 0d 89 00 90 48 8b d0 48 8b cb e8 80 3e ec ff", "event window shown"),
        // Adding a popup walks the list for one of its kind.
        Point::keep(
            0x6ecb14usize,
            "48 8b 5e 78 48 85 db 74 1c 0f 1f 00 48 8b 0b 48 8b 01 ff 50 08 84 c0 75 60 48 8b 43 10 48 8b d8 48 85 c0 75 e7",
            "map manager popups",
        )
        .uses(&[manager::POPUPS + list::HEAD, list::ITEM, list::NEXT]),
        // A popup that closes while the list is being walked is marked to go
        // later.
        Point::keep(0x6ea583usize, "41 80 bc 24 8c 00 00 00 00 74 06 c6 41 18 01", "popup gone").uses(&[manager::POPUPS + list::CHANGING, list::REMOVED]),
        Point::keep(0x10b9b3busize, "48 8d 05 b6 f5 c9 00 48 89 03", "event window class").uses(&[classes::EVENT_WINDOW]),
        Point::observe(RESTART, "48 89 5c 24 08 48 89 74 24 18 55 57 41 56", "restart", 1, Some(restart_before), None),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_is_reached_once_and_forgotten_when_released() {
        let mut s = Stops::default();
        let first = s.hold(1, 100);
        let second = s.hold(1, 124);
        assert!(!s.reach(1, 99));
        assert!(s.reach(1, 100));
        assert!(!s.reach(1, 100), "a stop pauses the game once");
        // A day past both, as when one was missed: one pause for both.
        s.release(first);
        let third = s.hold(1, 148);
        assert!(s.reach(1, 150));
        assert!(!s.reach(1, 172));
        s.release(second);
        s.release(third);
        assert!(s.held.is_empty());

        // Only a stop not reached yet is an advance running.
        assert!(!s.advancing(1));
        let running = s.hold(1, 500);
        assert!(s.advancing(1));
        s.release(running);
        assert!(!s.advancing(1));

        // Given up before its date, an advance calls the stop off: the game
        // pauses at the end of the next day, and the stop goes.
        let given_up = s.hold(1, 1000);
        s.call_off(given_up, 1);
        assert!(s.reach(1, 200));
        assert!(!s.reach(1, 224));
        assert!(s.held.is_empty());
        // Given up once reached, it just goes.
        let reached = s.hold(1, 300);
        assert!(s.reach(1, 300));
        s.call_off(reached, 1);
        assert!(s.held.is_empty());
    }

    #[test]
    fn a_stop_never_outlives_its_session() {
        let mut s = Stops::default();
        // Given up while a save loads, the advance goes at the new session's
        // first safe point: its stop goes with it, and the new game's first
        // day does not pause.
        let given_up = s.hold(1, 1000);
        s.call_off(given_up, 2);
        assert!(s.held.is_empty());
        assert!(!s.reach(2, 50));

        // Called off just before another game is loaded: the new game's
        // first day does not reach it.
        let called_off = s.hold(2, 1000);
        s.call_off(called_off, 2);
        assert!(!s.reach(3, 50));
        assert!(s.held.is_empty());

        // Still held when another game is loaded: the new game's days
        // neither reach it nor make an advance of it, and releasing it
        // afterwards changes nothing.
        let held = s.hold(3, 100);
        assert!(!s.advancing(4));
        assert!(!s.reach(4, 200));
        assert!(s.held.is_empty());
        s.release(held);

        // An advance in the new session holds its own stop alone.
        s.hold(4, 100);
        let new = s.hold(5, 100);
        assert_eq!(s.held.iter().map(|held| held.number).collect::<Vec<_>>(), [new]);
        assert!(s.reach(5, 100));
    }

    #[test]
    fn only_remote_control_holds_a_stop_and_one_dropped_is_called_off() {
        assert_eq!(stop_at(Caller::Extension(0), 100).unwrap_err().status, Status::Unavailable);
        let session = events::session();
        let answered = stop_at(Caller::Remote, 100).unwrap();
        let given_up = stop_at(Caller::Remote, 1000).unwrap();
        answered.release();
        drop(given_up);
        assert!(stops(|s| s.held.iter().all(|held| held.called_off && held.date == i32::MIN) && s.held.len() == 1));
        assert!(stops(|s| s.reach(session, 0)));
        assert!(stops(|s| s.held.is_empty()));
    }
}
