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
pub mod buildings;
pub mod camera;
pub mod centers_of_trade;
pub mod colonists;
pub mod console;
pub mod countries;
pub mod decisions;
pub mod describe;
pub mod desktop;
pub mod diplomacy;
pub mod economy;
pub mod estates;
pub mod events;
pub mod extensions;
#[cfg(test)]
pub(crate) mod fake;
pub mod focus;
pub mod forts;
pub mod government;
pub mod ideas;
pub mod input;
pub mod leaders;
pub mod loans;
pub mod menu;
pub mod mercenaries;
pub mod merchants;
pub mod missionaries;
pub mod missions;
pub mod occupation;
pub mod offers;
pub mod parliament;
pub mod peace;
pub mod policies;
pub mod provinces;
pub mod questions;
pub mod recruiting;
pub mod screenshot;
pub mod session;
pub mod sieges;
pub mod stability;
pub mod subjects;
pub mod technology;
pub mod texts;
pub mod time;
pub mod trade;
pub mod trade_companies;

use crate::game;
use eu4_harmony_remote::{Answer, Code, CountryTag, Error, Request, Response};
use std::{
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Declares the groups once, in one list: [`Game`], every group's slice of
/// the capability layer, and [`dispatch`], which runs a request in its group.
/// After the names `dispatch` gives its request, game, events and time, each
/// line is one of
///
/// ```text
/// Group(module),
/// Group(module) => outcome,
/// Group => outcome,
/// ```
///
/// A group whose `run(request, game)` answers at once; a group with a slice
/// of the layer, whose [`Outcome`] the expression gives; a group without a
/// slice.
macro_rules! groups {
    (@munch [$request:ident $game:ident $events:ident $now:ident] [$($modules:tt)*] [$($arms:tt)*]
        $(#[$attr:meta])* $Group:ident($module:ident) => $outcome:expr, $($rest:tt)*) => {
        groups!(@munch [$request $game $events $now] [$($modules)* $module] [$($arms)* { $(#[$attr])* $Group => $outcome }] $($rest)*);
    };
    (@munch [$request:ident $game:ident $events:ident $now:ident] [$($modules:tt)*] [$($arms:tt)*]
        $(#[$attr:meta])* $Group:ident($module:ident), $($rest:tt)*) => {
        groups!(@munch [$request $game $events $now] [$($modules)* $module]
            [$($arms)* { $(#[$attr])* $Group => Outcome::Done($module::run($request, $game).map(Answer::$Group)) }]
            $($rest)*);
    };
    (@munch $names:tt $modules:tt [$($arms:tt)*] $(#[$attr:meta])* $Group:ident => $outcome:expr, $($rest:tt)*) => {
        groups!(@munch $names $modules [$($arms)* { $(#[$attr])* $Group => $outcome }] $($rest)*);
    };
    (@munch [$request:ident $game:ident $events:ident $now:ident] [$($module:ident)*]
        [$({ $(#[$attr:meta])* $Group:ident => $outcome:expr })*]) => {
        /// The capability layer, as the catalogue's requests reach it: every
        /// group's slice.
        pub trait Game: $($module::Layer +)* {}
        impl<T: $($module::Layer +)* ?Sized> Game for T {}

        /// Runs one request at `now`, in its group.
        pub(crate) fn dispatch($request: Request, $game: &mut dyn Game, $events: &events::Buffer, $now: Instant) -> Outcome {
            match $request {
                $( $(#[$attr])* Request::$Group($request) => $outcome, )*
            }
        }
    };
    (@munch $($rest:tt)*) => {
        compile_error!(concat!("groups!: a line that is none of its forms: ", stringify!($($rest)*)));
    };
    (fn dispatch($request:ident, $game:ident, $events:ident, $now:ident); $($lines:tt)*) => {
        groups!(@munch [$request $game $events $now] [] [] $($lines)*);
    };
}

groups! {
    fn dispatch(request, game, events, now);
    Session(session),
    Time(time) => time::run(request, game, events),
    Country(countries),
    Province(provinces),
    Army(armies),
    Text(texts),
    Screenshot(screenshot) => screenshot::run(request, game),
    Console(console),
    Question(questions),
    Offer(offers),
    Events => events::run(request, events, now),
    Describe => Outcome::Done(Ok(Answer::Describe(describe::run(request)))),
    Camera(camera),
    Recruit(recruiting),
    Menu(menu) => menu::run(request, game),
    Diplomacy(diplomacy) => diplomacy::run(request, game, now),
    Extension(extensions),
    #[cfg(feature = "dev-memory")]
    Memory => Outcome::Done(crate::memory_account::answer(request).map(Answer::Memory)),
    Input(input) => Outcome::Done(input::run(request, game).0.map(Answer::Input)),
    Desktop(desktop),
    Decision(decisions),
    Subject(subjects),
    Idea(ideas),
    Policy(policies),
    Government(government),
    Estate(estates),
    Parliament(parliament),
    Mission(missions),
    Building(buildings),
    Technology(technology),
    Focus(focus),
    Stability(stability),
    Economy(economy),
    Loan(loans),
    Leader(leaders),
    Mercenary(mercenaries),
    Fort(forts),
    Siege(sieges),
    Peace(peace) => peace::run(request, game, now),
    Occupation(occupation),
    Missionary(missionaries),
    Colonist(colonists),
    TradeCompany(trade_companies),
    CenterOfTrade(centers_of_trade),
    Merchant(merchants),
    Trade(trade),
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
