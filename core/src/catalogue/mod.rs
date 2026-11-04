//! The capability catalogue (ADR one-capability-catalogue), as the core runs
//! its requests: through the capability layer (`crate::game`), for both of
//! its callers, remote control's engine (`crate::remote`), which runs them as
//! the player, and the Lua host (`crate::lua`), which runs them as the
//! extension calling ([`run_for`], [`poll_for`]). A request runs at once or
//! answers at a later safe point ([`Outcome`], [`Wait`]).
//!
//! One file per group of capabilities, as in the capability layer: each has
//! the group's slice of the layer as a trait ([`Game`] is all of them), the
//! layer itself behind it ([`Live`]), and turns the group's requests into
//! calls and the layer's types into the answers. Tests put a fake game behind
//! the traits ([`fake`]).

/// Declares a group's slice of the capability layer once: its `Layer`
/// trait, and [`Live`] calling each capability there as its caller.
/// A group whose slice does more than call them writes both itself.
macro_rules! layer {
    ($( $(#[$doc:meta])* fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty = $capability:path; )*) => {
        /// The group's slice of the capability layer.
        pub trait Layer {
            $( $(#[$doc])* fn $name(&mut self, $($arg: $ty),*) -> crate::game::GResult<$ret>; )*
        }

        impl Layer for super::Live {
            $( fn $name(&mut self, $($arg: $ty),*) -> crate::game::GResult<$ret> { $capability(self.0, $($arg),*) } )*
        }
    };
}

pub mod armies;
pub mod camera;
pub mod console;
pub mod countries;
pub mod describe;
pub mod desktop;
pub mod diplomacy;
pub mod events;
pub mod extensions;
#[cfg(test)]
pub(crate) mod fake;
pub mod input;
pub mod menu;
pub mod offers;
pub mod provinces;
pub mod questions;
pub mod recruiting;
pub mod screenshot;
pub mod session;
pub mod texts;
pub mod time;

use crate::game;
use eu4_harmony_remote::{Answer, Code, CountryTag, Error, Request, Response};
use std::{
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// The capability layer, as the catalogue's requests reach it: every
/// group's slice.
pub trait Game:
    session::Layer
    + countries::Layer
    + provinces::Layer
    + armies::Layer
    + texts::Layer
    + camera::Layer
    + screenshot::Layer
    + console::Layer
    + time::Layer
    + recruiting::Layer
    + questions::Layer
    + offers::Layer
    + menu::Layer
    + diplomacy::Layer
    + extensions::Layer
    + input::Layer
    + desktop::Layer
{
}
impl<
        T: session::Layer
            + countries::Layer
            + provinces::Layer
            + armies::Layer
            + texts::Layer
            + camera::Layer
            + screenshot::Layer
            + console::Layer
            + time::Layer
            + recruiting::Layer
            + questions::Layer
            + offers::Layer
            + menu::Layer
            + diplomacy::Layer
            + extensions::Layer
            + input::Layer
            + desktop::Layer
            + ?Sized,
    > Game for T
{
}

/// The capability layer itself, called as its caller: remote control, or an
/// extension through the Lua host, which runs the same groups
/// (ADR one-capability-catalogue).
pub struct Live(pub game::Caller);

impl From<game::Fail> for Error {
    fn from(fail: game::Fail) -> Self {
        let code = match fail.status {
            game::Status::NotInGame => Code::NotInGame,
            game::Status::NotReady => Code::NotReady,
            game::Status::NotFound => Code::NotFound,
            game::Status::Rejected => Code::Rejected,
            game::Status::InvalidArgument => Code::InvalidArgument,
            game::Status::Unavailable => Code::Unavailable,
            game::Status::WrongContext | game::Status::Internal => Code::Internal,
        };
        Error::new(code, fail.message)
    }
}

/// The game process remote control answers for: when it started, in
/// milliseconds since 1970. A restart starts the game again, as a new
/// process, in which sessions and events are numbered from 1 again; a
/// client tells the two apart by this.
pub fn process() -> u64 {
    static PROCESS: OnceLock<u64> = OnceLock::new();
    *PROCESS.get_or_init(|| started().unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)))
}

/// When this process started, in milliseconds since 1970.
fn started() -> Option<u64> {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let mut times = [FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }; 4];
    let [created, exited, kernel, user] = &mut times;
    if unsafe { GetProcessTimes(GetCurrentProcess(), created, exited, kernel, user) } == 0 {
        return None;
    }
    // 100-nanosecond intervals since 1601.
    let since_1601 = (u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime)) / 10_000;
    since_1601.checked_sub(11_644_473_600_000)
}

/// A country's tag as remote clients write it.
pub fn tag(tag: game::Tag) -> CountryTag {
    CountryTag(tag.letters())
}

/// Where a request runs: remote control's engine runs at every safe point,
/// extensions only at a game's safe frame.
#[derive(Clone, Debug)]
pub enum Place {
    /// A game's safe frame: every request runs.
    Game,
    /// A game's frame with no game the player can play, and why (the player
    /// only observes, say): requests that need a game are answered with it.
    Unplayable(Error),
    /// A safe point with no game to read. Requests that need one wait for a
    /// game frame, or, given an error, are answered with it.
    Outside(Option<Error>),
}

/// What running a request comes to.
pub enum Outcome {
    Done(Response),
    /// The answer comes at a later safe point.
    Later(Box<dyn Wait>),
}

/// A request that waits: asked at every safe point until it answers.
pub trait Wait: Send {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response>;

    /// Its client gave it up, as seen at this safe point: what it does before
    /// it goes, unanswered. Nothing by default.
    fn given_up(&mut self, _at: &mut Poll<'_>) {}

    /// The process is about to end, as the game starts `eu4.exe` again: the
    /// answer to write while it can, if it has one of its own. Without one,
    /// it is answered as every other request then
    /// (`crate::remote::Engine::end`).
    fn ending(&mut self, _events: &events::Buffer) -> Option<Response> {
        None
    }
}

/// What a waiting request may look at.
pub struct Poll<'a> {
    pub events: &'a events::Buffer,
    pub now: Instant,
    /// Where a request runs: remote control's engine runs at every safe point,
    /// extensions only at a game's safe frame.
    pub place: &'a Place,
    game: &'a mut dyn Game,
}

impl<'a> Poll<'a> {
    pub fn new(events: &'a events::Buffer, now: Instant, place: &'a Place, game: &'a mut dyn Game) -> Self {
        Self { events, now, place, game }
    }

    /// The game, at a game's safe frame only.
    pub fn game(&mut self) -> Option<&mut dyn Game> {
        matches!(self.place, Place::Game).then_some(&mut *self.game)
    }

    /// At a game's frame with no game the player can play, why.
    pub fn unplayable(&self) -> Option<&'a Error> {
        match self.place {
            Place::Unplayable(why) => Some(why),
            _ => None,
        }
    }
}

/// Runs one request for an extension, as the Lua host has it: a capability
/// of the catalogue both sides have, called as `caller`. A request that
/// waits (a diplomatic action waiting for its answer) is polled with
/// [`poll_for`] at later safe points.
pub fn run_for(caller: game::Caller, request: Request) -> Outcome {
    dispatch(request, &mut Live(caller), &events::Buffer::new(), Instant::now())
}

/// Polls a request [`run_for`] left waiting, at a game's safe frame.
pub fn poll_for(caller: game::Caller, wait: &mut dyn Wait) -> Option<Response> {
    let events = events::Buffer::new();
    let mut live = Live(caller);
    wait.poll(&mut Poll::new(&events, Instant::now(), &Place::Game, &mut live))
}

/// Runs one request at `now`, group by group.
pub(crate) fn dispatch(request: Request, game: &mut dyn Game, events: &events::Buffer, now: Instant) -> Outcome {
    let done = |response: Response| Outcome::Done(response);
    match request {
        Request::Session(r) => done(session::run(r, game).map(Answer::Session)),
        Request::Time(r) => time::run(r, game, events),
        Request::Country(r) => done(countries::run(r, game).map(Answer::Country)),
        Request::Province(r) => done(provinces::run(r, game).map(Answer::Province)),
        Request::Army(r) => done(armies::run(r, game).map(Answer::Army)),
        Request::Text(r) => done(texts::run(r, game).map(Answer::Text)),
        Request::Screenshot(r) => screenshot::run(r, game),
        Request::Console(r) => done(console::run(r, game).map(Answer::Console)),
        Request::Question(r) => done(questions::run(r, game).map(Answer::Question)),
        Request::Offer(r) => done(offers::run(r, game).map(Answer::Offer)),
        Request::Events(r) => events::run(r, events, now),
        Request::Describe(r) => done(Ok(Answer::Describe(describe::run(r)))),
        Request::Camera(r) => done(camera::run(r, game).map(Answer::Camera)),
        Request::Recruit(r) => done(recruiting::run(r, game).map(Answer::Recruit)),
        Request::Menu(r) => menu::run(r, game),
        Request::Diplomacy(r) => diplomacy::run(r, game, now),
        Request::Extension(r) => done(extensions::run(r, game).map(Answer::Extension)),
        #[cfg(feature = "dev-memory")]
        Request::Memory(r) => done(crate::memory_account::answer(r).map(Answer::Memory)),
        Request::Input(r) => done(input::run(r, game).0.map(Answer::Input)),
        Request::Desktop(r) => done(desktop::run(r, game).map(Answer::Desktop)),
    }
}
