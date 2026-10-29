//! Time: the date, whether the game is paused and by what, and its speed;
//! pausing, resuming and setting the speed as the player would; stopping time
//! as a date is reached, for remote control's advancing; and whether a
//! question waits for the player's answer. The evidence is in
//! docs/research/time.md.
use super::{context, context_any, events, fail, game, map_manager, paused, questions, Caller, Ctx, GResult, Status};
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::campaign::{
        globals::{END_DATE, WORLD},
        map_manager as manager, pause as pause_state, world,
    },
    points::Point,
};
use std::sync::Mutex;

/// `(world, index)`: sets the speed, clamped to 0..=4, as both speed commands
/// do.
const SET_SPEED: usize = 0x7579f0;
/// The speed-up command's execution, which reads the speed.
const SPEED_UP: usize = 0xb79970;
/// `(pause state)`: the toggle the game console's `gamespeed` uses. When
/// nothing holds the game paused, it sets the player's pause; paused for any
/// reason, it resumes and lets go of every window holding the game paused, as
/// the player's resuming does.
const TOGGLE_PAUSE: usize = 0x7e0250;
/// `(app)`: starts `eu4.exe` again, at the end of the game's main loop, when
/// the game returns to the main menu.
const RESTART: usize = 0x14c3a10;

/// The game speeds, slowest first.
const SPEEDS: std::ops::RangeInclusive<u32> = 1..=5;

/// Refuses a speed the game does not have.
pub fn check_speed(speed: u32) -> GResult<()> {
    if !SPEEDS.contains(&speed) {
        return fail(Status::InvalidArgument, format!("the speed is from 1 to 5, not {speed}"));
    }
    Ok(())
}

/// What holds the game paused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pause {
    /// Nothing: time runs.
    #[default]
    Running,
    /// The player paused it, or remote control or an extension did.
    Player,
    /// A window that pauses the game while it is open, such as an event window
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
    /// The end date, once the game is over: past it, unless the player
    /// chose to play on, time does not run (see [`resume`]).
    pub over: Option<i32>,
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
    Ok(Time { session: events::session(), date: ctx.date, speed: index as u32 + 1, pause, over: over(m, &ctx)? })
}

/// Pauses the game with the player's own pause, as the player would, also
/// when windows hold it paused already: it then stays paused once they close,
/// until it is resumed. Nothing when the player's pause holds it already.
pub fn pause(_: Caller) -> GResult<()> {
    let m = game()?;
    pause_game(m, &context(m)?)
}

/// Resumes the game, as the player would: also when windows hold it paused,
/// which stay open. Nothing when it runs. Refused, as the player's resuming
/// is, once the game is over ([`over`]).
pub fn resume(_: Caller) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    if paused(m, &ctx)? {
        if let Some(end) = over(m, &ctx)? {
            return Err(game_over(end));
        }
        toggle(m, pause_state_of(m, &ctx)?)?;
        if paused(m, &ctx)? {
            return fail(Status::Rejected, "the game stays paused");
        }
    }
    Ok(())
}

/// The end date, once the game is over: past it, unless the player chose to
/// play on. The player's resuming is refused then (`0x810570`, the pause
/// key's function), and the map manager's frame pauses the game again at
/// every frame, so time would not run anyway. The other check there, that
/// the player has a country unless the game lets an observer run time, is
/// [`context`]'s: remote control refuses an observer whatever the game lets
/// the player do.
fn over(m: &Memory, ctx: &Ctx) -> GResult<Option<i32>> {
    let end = m.i32(m.at(END_DATE))?;
    Ok((ctx.date > end && m.u8(ctx.world + world::PLAY_ON)? == 0).then_some(end))
}

/// Why time does not run once the game is over, at its end date `end`.
pub fn game_over(end: i32) -> super::Fail {
    let end = eu4_harmony_remote::Date(end);
    super::Fail {
        status: Status::Rejected,
        message: format!("the game is over: it is past its end date, {end}, and stays paused, as for the player, until the end-of-game summary is closed to play on"),
    }
}

/// Sets the game speed, 1 (slowest) to 5 (fastest), as the player would.
pub fn set_speed(_: Caller, speed: u32) -> GResult<()> {
    check_speed(speed)?;
    let m = game()?;
    let ctx = context(m)?;
    m.call(m.at(SET_SPEED), &[ctx.world, (speed - 1) as usize])?;
    Ok(())
}

/// How many questions wait for the player's answer, those `question.list`
/// lists, as [`questions::count`] counts them.
///
/// That count is the one place remote control's advancing time learns that
/// one needs an answer: an advance asks it at the safe point, through this,
/// and the end of the daily update during an advance asks it directly, so
/// the two never disagree with each other or with the questions listed.
pub fn questions_waiting(_: Caller) -> GResult<usize> {
    let m = game()?;
    questions::count(m, &context(m)?)
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

/// After the daily update (`events`' observer): pauses the game with the
/// player's pause when a stop's date is reached, or when a question came up
/// on the day while an advance runs, so that the next frame begins no other
/// day, and a window that holds the game paused too (an event window, a
/// message set to pause) does not let it run on once it closes.
pub(super) fn day_passed(m: &Memory) {
    if crate::saves::load_in_progress() {
        return;
    }
    let Ok(ctx) = context_any(m) else { return };
    let session = events::session();
    let due = stops(|s| s.reach(session, ctx.date)) || (stops(|s| s.advancing(session)) && questions::count(m, &ctx).is_ok_and(|waiting| waiting > 0));
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

fn toggle(m: &Memory, state: usize) -> GResult<()> {
    m.call(m.at(TOGGLE_PAUSE), &[state])?;
    Ok(())
}

/// Sets the player's pause, whatever else holds the game paused, so that the
/// game stays paused once a window holding it closes. The toggle sets it only
/// when nothing holds the game, and resumes a game that windows hold; so then
/// it toggles twice: the windows are let go (they stay open, as when the
/// player resumes), and the player's pause holds the game.
fn pause_game(m: &Memory, ctx: &Ctx) -> GResult<()> {
    let state = pause_state_of(m, ctx)?;
    if m.u8(state + pause_state::PLAYER_PAUSED)? != 0 {
        return Ok(());
    }
    if paused(m, ctx)? {
        toggle(m, state)?;
    }
    toggle(m, state)?;
    if m.u8(state + pause_state::PLAYER_PAUSED)? == 0 || !paused(m, ctx)? {
        return fail(Status::Rejected, "the game does not pause");
    }
    Ok(())
}

/// Before the game starts `eu4.exe` again, whoever asked for it: remote
/// control answers every request, waiting, queued or sent from now on, while
/// this process can still write the answers, and waits a while for its
/// clients to read them (`remote::Engine::end`).
extern "C" fn restart_before(_: *mut HookFrame) {
    crate::guarded("restart", || {
        log!("the game starts eu4.exe again");
        crate::remote::restarting();
    });
}

/// The speed, the pause toggle and the restart.
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
