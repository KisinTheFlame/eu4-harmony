//! The capability catalogue (ADR one-capability-catalogue), as the core runs
//! its requests: through the capability layer (`crate::game`), for both of
//! its callers, remote control's engine (`crate::remote`), which runs them as
//! the player, and the Lua host (`crate::lua`), which runs them as the
//! extension calling. Both reach it through one seam, [`Catalogue`]: a
//! request runs at once or answers at a later safe point ([`Outcome`],
//! [`Wait`]).
//!
//! One file per group of capabilities, as in the capability layer: each has
//! the group's slice of the layer as a trait ([`Game`] is all of them), the
//! layer itself behind it ([`Live`]), and turns the group's requests into
//! calls and the layer's types into the answers. Every [`Game`] is a
//! [`Catalogue`]. The tests of a group that does more than call the layer put
//! a fake game behind the traits ([`fake`]); a capability a test does not
//! script fails there.

/// Declares a group's slice of the capability layer and its requests once:
/// its `Layer` trait, [`Live`] calling each capability there for its caller,
/// and, given `run Request => Answer;` first, the group's `run`, which
/// answers each request. Each line is one of
///
/// ```text
/// Variant => fn method -> Result = capability;
/// Variant => fn method -> Result = caller capability;
/// fn method(argument: Type, ...) -> Result = session capability;
/// Variant { field, ... } => game.method(expression, ...);
/// Variant { field, ... } => function(game, expression, ...);
/// ```
///
/// A method and the request it answers, whose capability takes the
/// request's arguments by their names, as the group's `args` module in
/// remote control declares them (`eu4_harmony_remote::<group>::args`, which
/// the group's file imports as `args`); a method alone; a request answered
/// by a method of another line; a request answered by a function of the
/// group's own, which takes the game first and returns a `Result`. Answers,
/// and the arguments a line passes itself, pass through [`Convert`].
///
/// What a capability takes first is what [`Live`] opens for it before it
/// runs ([`Opens`]): for a method and its request, what the capability's
/// place in remote control says, a session with a country to act as
/// (`crate::game::Session`), one that acts as no country
/// (`crate::game::WorldSession`), or the caller alone (`crate::game::Caller`)
/// for one that runs at the main menu too; or the caller alone when the
/// line says `caller`, for a capability that checks something of its own
/// first. A method alone says it before its capability: `session`, `world`
/// or `caller`. `run Request => Answer + other::Layer;` lets `run` and the
/// group's functions use another group's slice too. A group whose slice
/// does more than call the layer writes both itself.
macro_rules! layer {
    (@munch $head:tt [$($methods:tt)*] [$($arms:tt)*]
        $(#[$doc:meta])* $Variant:ident => fn $name:ident -> $ret:ty = caller $capability:path; $($rest:tt)*) => {
        layer!(@munch $head
            [$($methods)* { $(#[$doc])* [$Variant] fn $name -> $ret = caller $capability; }]
            [$($arms)* { $Variant request $name () () }]
            $($rest)*);
    };
    (@munch $head:tt [$($methods:tt)*] [$($arms:tt)*]
        $(#[$doc:meta])* $Variant:ident => fn $name:ident -> $ret:ty = $capability:path; $($rest:tt)*) => {
        layer!(@munch $head
            [$($methods)* { $(#[$doc])* [$Variant] fn $name -> $ret = place $capability; }]
            [$($arms)* { $Variant request $name () () }]
            $($rest)*);
    };
    (@munch $head:tt [$($methods:tt)*] $arms:tt
        $(#[$doc:meta])* fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty = $takes:ident $capability:path; $($rest:tt)*) => {
        layer!(@munch $head [$($methods)* { $(#[$doc])* [] fn $name($($arg: $ty),*) -> $ret = $takes $capability; }] $arms $($rest)*);
    };
    (@munch $head:tt $methods:tt [$($arms:tt)*]
        $Variant:ident { $($field:ident),* $(,)? } => game.$name:ident($($e:expr),* $(,)?); $($rest:tt)*) => {
        layer!(@munch $head $methods [$($arms)* { $Variant method $name ($($field),*) ($($e),*) }] $($rest)*);
    };
    (@munch $head:tt $methods:tt [$($arms:tt)*]
        $Variant:ident { $($field:ident),* $(,)? } => $function:ident(game $(, $e:expr)* $(,)?); $($rest:tt)*) => {
        layer!(@munch $head $methods [$($arms)* { $Variant function $function ($($field),*) ($($e),*) }] $($rest)*);
    };
    (@munch [] [$($methods:tt)*] []) => {
        layer!(@layer $($methods)*);
        layer!(@bindings None [$($methods)*] []);
    };
    (@munch [$Request:ident $Answer:ident [$($bound:tt)*]] [$($methods:tt)*]
        [$({ $Variant:ident $how:ident $call:ident ($($field:ident),*) ($($e:expr),*) })*]) => {
        layer!(@layer $($methods)*);
        layer!(@bindings $Request [$($methods)*] [$({ $Variant $how $call })*]);

        /// Answers one of the group's requests.
        pub fn run(request: $Request, game: &mut (impl Layer $($bound)* + ?Sized)) -> Result<$Answer, eu4_harmony_remote::Error> {
            Ok(match request.into_args() {
                $( args::Args::$Variant(arguments) => $Answer::$Variant($crate::catalogue::Convert::convert(
                    layer!(@call game arguments $Variant $how $call ($($field),*) ($($e),*))?
                )), )*
            })
        }
    };
    (@munch $($rest:tt)*) => {
        compile_error!(concat!("layer!: a line that is none of its forms: ", stringify!($($rest)*)));
    };
    (@call $game:ident $arguments:ident $Variant:ident request $name:ident () ()) => {
        $game.$name($arguments)
    };
    (@call $game:ident $arguments:ident $Variant:ident method $name:ident ($($field:ident),*) ($($e:expr),*)) => {{
        let args::$Variant { $($field),* } = $arguments;
        $game.$name($($crate::catalogue::Convert::convert($e)),*)
    }};
    (@call $game:ident $arguments:ident $Variant:ident function $function:ident ($($field:ident),*) ($($e:expr),*)) => {{
        let args::$Variant { $($field),* } = $arguments;
        $function($game, $($e),*)
    }};
    (@live $caller:ident session $capability:path, [] [$($arg:ident),*]) => {
        $capability(crate::game::Session::open($caller)?, $($arg),*)
    };
    (@live $caller:ident world $capability:path, [] [$($arg:ident),*]) => {
        $capability(crate::game::WorldSession::open($caller)?, $($arg),*)
    };
    (@live $caller:ident caller $capability:path, [] [$($arg:ident),*]) => {
        $capability($caller, $($arg),*)
    };
    (@live $caller:ident caller $capability:path, [$Variant:ident $arguments:ident] []) => {
        $crate::catalogue::Capability::call($capability, $caller, $arguments)
    };
    (@live $caller:ident place $capability:path, [$Variant:ident $arguments:ident] []) => {
        $crate::catalogue::Capability::call(
            $capability,
            <<args::$Variant as eu4_harmony_remote::Arguments>::Place as $crate::catalogue::Opens>::open($caller)?,
            $arguments,
        )
    };
    (@live $caller:ident $takes:ident $capability:path, $($rest:tt)*) => {
        compile_error!(concat!("layer!: ", stringify!($capability), " takes a session, a world or a caller, not ", stringify!($takes)))
    };
    (@layer $({ $(#[$doc:meta])* [$($Variant:ident)?] fn $name:ident $(($($arg:ident: $ty:ty),*))? -> $ret:ty = $takes:ident $capability:path; })*) => {
        /// The group's slice of the capability layer.
        #[cfg(not(test))]
        pub trait Layer {
            $( $(#[$doc])* fn $name(&mut self, $(arguments: args::$Variant)? $($($arg: $ty),*)?) -> crate::game::GResult<$ret>; )*
        }

        /// The group's slice of the capability layer; in a test, what a fake
        /// game does not script fails.
        #[cfg(test)]
        pub trait Layer {
            $(
                $(#[$doc])*
                fn $name(&mut self, $(arguments: args::$Variant)? $($($arg: $ty),*)?) -> crate::game::GResult<$ret> {
                    $( let _: args::$Variant = arguments; )?
                    $( let _ = ($($arg,)*); )?
                    Err($crate::catalogue::fake::unscripted(stringify!($name)))
                }
            )*
        }

        impl Layer for super::Live {
            $(
                fn $name(&mut self, $(arguments: args::$Variant)? $($($arg: $ty),*)?) -> crate::game::GResult<$ret> {
                    let caller = self.0;
                    layer!(@live caller $takes $capability, [$($Variant arguments)?] [$($($arg),*)?])
                }
            )*
        }
    };
    (@bindings $Request:ident [$({ $(#[$doc:meta])* [$($Variant:ident)?] fn $name:ident $(($($arg:ident: $ty:ty),*))? -> $ret:ty = $takes:ident $capability:path; })*]
        [$({ $RVariant:ident $how:ident $call:ident })*]) => {
        /// What the group's lines name, for the test that a request a method
        /// alone answers acts as a country exactly when the method opens a
        /// session with one.
        #[cfg(test)]
        pub fn bindings() -> $crate::catalogue::Bindings {
            $crate::catalogue::Bindings {
                methods: vec![$( $crate::catalogue::Method { name: stringify!($name), takes: stringify!($takes) } ),*],
                requests: vec![$(
                    $crate::catalogue::Answered {
                        request: stringify!($RVariant),
                        function: stringify!($how) == "function",
                        call: stringify!($call),
                        country: $Request::country_of(stringify!($RVariant)),
                    }
                ),*],
            }
        }
    };
    (run $Request:ident => $Answer:ident $(+ $other:ident::Layer)*; $($lines:tt)*) => {
        layer!(@munch [$Request $Answer [$(+ $other::Layer)*]] [] [] $($lines)*);
    };
    ($($lines:tt)*) => {
        layer!(@munch [] [] [] $($lines)*);
    };
}

pub mod describe;
pub mod events;
#[cfg(test)]
pub(crate) mod fake;

use crate::game::{self, input::Taken};
use eu4_harmony_remote::{Answer, ArmyId, ArmyList, Code, CountryTag, Error, ProvinceId, Request, Response};
use std::{
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Declares the groups once, in one list: [`Game`], every group's slice of
/// the capability layer, and `dispatch`, which runs a request in its group.
/// After the names `dispatch` gives its request, game, events (remote
/// control's, when its caller keeps them) and time, each line is one of
///
/// ```text
/// Group(module),
/// Group(module) => outcome,
/// Group => outcome,
/// ```
///
/// A group whose `run(request, game)` answers at once; a group with a slice
/// of the layer, whose [`Outcome`] the expression gives; a group without a
/// slice. The module of a group with a slice is declared here, as remote
/// control's groups declare theirs; a group without one declares its own.
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
        $( pub mod $module; )*

        /// The capability layer, as the catalogue's requests reach it: every
        /// group's slice.
        pub trait Game: $($module::Layer +)* {
            fn as_game(&mut self) -> &mut dyn Game;
        }
        impl<T: $($module::Layer +)*> Game for T {
            fn as_game(&mut self) -> &mut dyn Game {
                self
            }
        }

        /// Every group's lines, by its module.
        #[cfg(test)]
        fn bindings() -> Vec<(&'static str, Bindings)> {
            vec![$( (stringify!($module), $module::bindings()) ),*]
        }

        /// Runs one request at `now`, in its group.
        fn dispatch($request: Request, $game: &mut dyn Game, $events: Option<&events::Buffer>, $now: Instant) -> Outcome {
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
    Console(console) => console::run_and_answer_next(request, game),
    Question(questions),
    Popup(popups),
    Offer(offers),
    Events => match events {
        Some(events) => events::run(request, events, now),
        None => Outcome::Done(Err(events::unkept())),
    },
    Describe => Outcome::Done(Ok(Answer::Describe(describe::run(request)))),
    Camera(camera),
    Recruit(recruiting),
    Menu(menu) => menu::run(request, game),
    Diplomacy(diplomacy) => diplomacy::run(request, game, now),
    Extension(extensions),
    #[cfg(feature = "dev-memory")]
    Memory => Outcome::Done(crate::memory_account::answer(request).map(Answer::Memory)),
    Input(input) => match input::run(request, game) {
        (response, Some(taken)) => Outcome::Input(response.map(Answer::Input), taken),
        (response, None) => Outcome::Done(response.map(Answer::Input)),
    },
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

/// The catalogue as its two callers reach it, remote control's engine and the
/// Lua host: the one seam between them and the capability layer. Every
/// [`Game`] is one, [`Live`] in the game.
pub trait Catalogue {
    /// Runs one request at `now`. `events` are remote control's, which only
    /// its engine keeps: a request that reads them, run without, is answered
    /// that only remote clients read them.
    fn run(&mut self, request: Request, events: Option<&events::Buffer>, now: Instant) -> Outcome;

    /// At a safe point at `place`: `polls` asks the requests that wait, each
    /// with the same [`Poll`].
    fn at(&mut self, events: Option<&events::Buffer>, now: Instant, place: &Place, polls: &mut dyn FnMut(&mut Poll<'_>));

    /// Whether the game has had every input up to `taken`
    /// ([`Outcome::Input`]).
    fn handled(&mut self, taken: Taken) -> bool;
}

impl<T: Game + ?Sized> Catalogue for T {
    fn run(&mut self, request: Request, events: Option<&events::Buffer>, now: Instant) -> Outcome {
        dispatch(request, self.as_game(), events, now)
    }

    fn at(&mut self, events: Option<&events::Buffer>, now: Instant, place: &Place, polls: &mut dyn FnMut(&mut Poll<'_>)) {
        polls(&mut Poll::new(events, now, place, Some(self.as_game())));
    }

    fn handled(&mut self, taken: Taken) -> bool {
        input::Layer::handled(self, taken)
    }
}

/// What a group's `layer!` lines name: each method of its slice and what
/// it opens, and each request and what answers it.
#[cfg(test)]
#[derive(Default)]
pub struct Bindings {
    pub methods: Vec<Method>,
    pub requests: Vec<Answered>,
}

/// A method of a group's slice, and what it takes first: `session`,
/// `world` or `caller`, or `place` for one whose capability's place says.
#[cfg(test)]
pub struct Method {
    pub name: &'static str,
    pub takes: &'static str,
}

/// A request and the method or the group's function that answers it.
#[cfg(test)]
pub struct Answered {
    pub request: &'static str,
    pub function: bool,
    pub call: &'static str,
    /// Whether its capability acts as a country, as remote control
    /// describes it.
    pub country: Option<bool>,
}

/// What [`Live`] opens for a capability of a place
/// ([`eu4_harmony_remote::place`]) before it runs: a session with a country
/// to act as, one that acts as no country, or nothing but the caller.
pub trait Opens {
    type Taken;
    fn open(caller: game::Caller) -> game::GResult<Self::Taken>;
}

/// A capability as [`Live`] calls it, given what it takes first and its
/// arguments: one that has arguments takes them after it, by their names
/// ([`eu4_harmony_remote::Arguments`]), and one without takes nothing more.
pub trait Capability<Taken, Arguments, Form> {
    type Output;
    fn call(self, taken: Taken, arguments: Arguments) -> Self::Output;
}

/// A [`Capability`] that takes its arguments.
pub struct WithArguments;
/// A [`Capability`] of none, which takes only what it takes first.
pub struct WithNone;

impl<F, T, A, R> Capability<T, A, WithArguments> for F
where
    F: FnOnce(T, A) -> R,
{
    type Output = R;
    fn call(self, taken: T, arguments: A) -> R {
        self(taken, arguments)
    }
}

impl<F, T, A: eu4_harmony_remote::Arguments, R> Capability<T, A, WithNone> for F
where
    F: FnOnce(T) -> R,
{
    type Output = R;
    fn call(self, taken: T, _: A) -> R {
        const { assert!(A::COUNT == 0, "a capability with arguments takes them") };
        self(taken)
    }
}

impl Opens for eu4_harmony_remote::place::Game {
    type Taken = game::Session;
    fn open(caller: game::Caller) -> game::GResult<game::Session> {
        game::Session::open(caller)
    }
}

impl Opens for eu4_harmony_remote::place::World {
    type Taken = game::WorldSession;
    fn open(caller: game::Caller) -> game::GResult<game::WorldSession> {
        game::WorldSession::open(caller)
    }
}

impl Opens for eu4_harmony_remote::place::Anywhere {
    type Taken = game::Caller;
    fn open(caller: game::Caller) -> game::GResult<game::Caller> {
        Ok(caller)
    }
}

/// The capability layer itself, called as its caller: remote control, or an
/// extension through the Lua host, which runs the same groups
/// (ADR one-capability-catalogue).
pub struct Live(pub game::Caller);

/// A value as the other side takes it: a request's as the capability layer
/// takes it, and the layer's as the group's answer gives it (`layer!`).
/// Most are the same value; the IDs remote clients write are the game's
/// numbers.
pub trait Convert<T> {
    fn convert(self) -> T;
}

impl<T> Convert<T> for T {
    fn convert(self) -> T {
        self
    }
}

impl Convert<u32> for ProvinceId {
    fn convert(self) -> u32 {
        self.0
    }
}

impl Convert<Vec<ProvinceId>> for Vec<u32> {
    fn convert(self) -> Vec<ProvinceId> {
        self.into_iter().map(ProvinceId).collect()
    }
}

impl Convert<u64> for ArmyId {
    fn convert(self) -> u64 {
        self.0
    }
}

impl Convert<ArmyId> for u64 {
    fn convert(self) -> ArmyId {
        ArmyId(self)
    }
}

impl Convert<Vec<ArmyId>> for ArmyList {
    fn convert(self) -> Vec<ArmyId> {
        self.0
    }
}

impl Convert<String> for &str {
    fn convert(self) -> String {
        self.to_string()
    }
}

impl From<game::Fail> for Error {
    fn from(fail: game::Fail) -> Self {
        let code = match fail.status {
            game::Status::NotInGame => Code::NotInGame,
            game::Status::NoCountry => Code::NoCountry,
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
    /// A game's frame with no game the player can play, and why (the
    /// player's country not matching its tag, say): requests that need a
    /// game are answered with it. Observe mode is a game like any other.
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
    /// Answered at once, with an input taken that the game has yet to have:
    /// the requests after it wait until it has ([`Catalogue::handled`]).
    Input(Response, Taken),
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
    /// Remote control's events, which only its engine keeps.
    pub events: Option<&'a events::Buffer>,
    pub now: Instant,
    /// Where a request runs: remote control's engine runs at every safe point,
    /// extensions only at a game's safe frame.
    pub place: &'a Place,
    game: Option<&'a mut dyn Game>,
}

impl<'a> Poll<'a> {
    /// At `place`, with the capability layer if a waiting request may use
    /// it.
    pub fn new(events: Option<&'a events::Buffer>, now: Instant, place: &'a Place, game: Option<&'a mut dyn Game>) -> Self {
        Self { events, now, place, game }
    }

    /// The game, at a game's safe frame only.
    pub fn game(&mut self) -> Option<&mut dyn Game> {
        match self.game.as_mut() {
            Some(game) if matches!(self.place, Place::Game) => Some(&mut **game),
            _ => None,
        }
    }

    /// The game, at a game's frame, also one with no country the player
    /// plays: for a request that needs only the game going on.
    pub fn world(&mut self) -> Option<&mut dyn Game> {
        match self.game.as_mut() {
            Some(game) if matches!(self.place, Place::Game | Place::Unplayable(_)) => Some(&mut **game),
            _ => None,
        }
    }

    /// At a game's frame with no game the player can play, why.
    pub fn unplayable(&self) -> Option<&'a Error> {
        match self.place {
            Place::Unplayable(why) => Some(why),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_remote::{capabilities, result_shape};
    use serde_json::Value;

    #[test]
    fn the_layer_s_failures_keep_their_meaning() {
        let cases = [
            (game::Status::NotInGame, Code::NotInGame),
            (game::Status::NotReady, Code::NotReady),
            (game::Status::NotFound, Code::NotFound),
            (game::Status::Rejected, Code::Rejected),
            (game::Status::InvalidArgument, Code::InvalidArgument),
            (game::Status::Unavailable, Code::Unavailable),
            (game::Status::WrongContext, Code::Internal),
            (game::Status::Internal, Code::Internal),
        ];
        for (status, code) in cases {
            assert_eq!(Error::from(game::Fail { status, message: "why".into() }), Error::new(code, "why"));
        }
    }

    #[test]
    fn what_reads_remote_control_s_events_says_so_when_run_without() {
        let mut game = fake::Fake::default();
        let reads = [r#"{"capability": "events"}"#, r#"{"capability": "time.advance", "days": 1}"#];
        for json in reads {
            let request = eu4_harmony_remote::request_from_json(json).unwrap();
            let Outcome::Done(Err(error)) = game.run(request, None, Instant::now()) else { panic!("{json} ran without events") };
            assert_eq!(error, events::unkept());
        }
        assert!(game.calls.is_empty(), "{:?}", game.calls);
    }

    /// A request answered by a method alone acts as a country, in its
    /// description, exactly when the method opens a session with a country
    /// to act as; one that opens a session without is described as acting
    /// as none. A method of its own request opens what its place says, which
    /// the compiler holds it to.
    #[test]
    fn what_acts_as_a_country_opens_a_session_with_one() {
        let groups = bindings();
        let mut wrong = Vec::new();
        let mut checked = 0;
        for (group, bindings) in &groups {
            for request in bindings.requests.iter().filter(|r| !r.function) {
                let Some(method) = bindings.methods.iter().find(|m| m.name == request.call) else { continue };
                let expected = match method.takes {
                    "session" => true,
                    "world" => false,
                    _ => continue,
                };
                checked += 1;
                if request.country != Some(expected) {
                    wrong.push(format!("{group} {}: takes a {}, described as country {:?}", request.request, method.takes, request.country));
                }
            }
        }
        assert!(checked > 30, "only {checked} checked");
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Every answer the fake game gives, through the groups that make it,
    /// has the shape its capability describes, as Lua has it.
    #[test]
    fn answers_have_the_shapes_their_capabilities_describe() {
        let mut shaped = Vec::new();
        for capability in capabilities() {
            let mut fields = serde_json::Map::new();
            fields.insert("capability".into(), capability.name().into());
            for arg in capability.args.iter().filter(|a| !a.optional) {
                fields.insert(arg.name.into(), arg.kind.json(arg.kind.example()).unwrap());
            }
            let request = eu4_harmony_remote::request_from_json(&Value::Object(fields).to_string()).unwrap();
            let name = request.name();
            if let Outcome::Done(Ok(answer)) = fake::Fake::default().run(request, Some(&events::Buffer::new()), Instant::now()) {
                let lua = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(&answer)).unwrap();
                let shape = result_shape(&name).unwrap_or_else(|| panic!("{name} has no result shape"));
                shape.check(&lua).unwrap_or_else(|why| panic!("{name}'s answer is not its shape: {why}\n{lua}"));
                shaped.push(name);
            }
        }
        assert!(shaped.len() > 20, "only {} answers were checked against their shapes: {shaped:?}", shaped.len());
    }
}
