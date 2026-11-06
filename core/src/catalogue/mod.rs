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
/// fn method(argument: Type, ...) -> Result = session capability;
/// Variant => fn method(field: Type, ...) -> Result = session capability;
/// Variant { field, ... } => game.method(expression, ...);
/// Variant { field, ... } => function(game, expression, ...);
/// ```
///
/// A method alone; a method and the request it answers, whose fields are
/// its arguments; a request answered by a method of another line; a request
/// answered by a function of the group's own, which takes the game first
/// and returns a `Result`. Arguments and answers pass through [`Convert`].
/// What the capability takes first is said before it: `session`, the
/// session [`Live`] opens for the request (`crate::game::Session`), which
/// decides whether there is one to read before the capability runs; or
/// `caller`, the caller alone, for a capability that runs without a session
/// or checks something of its own first. `run Request => Answer +
/// other::Layer;` lets `run` and the group's functions use another group's
/// slice too. A group whose slice does more than call the layer writes both
/// itself.
macro_rules! layer {
    (@munch $head:tt [$($methods:tt)*] $arms:tt
        $(#[$doc:meta])* fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty = $takes:ident $capability:path; $($rest:tt)*) => {
        layer!(@munch $head [$($methods)* { $(#[$doc])* fn $name($($arg: $ty),*) -> $ret = $takes $capability; }] $arms $($rest)*);
    };
    (@munch $head:tt [$($methods:tt)*] [$($arms:tt)*]
        $(#[$doc:meta])* $Variant:ident => fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty = $takes:ident $capability:path; $($rest:tt)*) => {
        layer!(@munch $head
            [$($methods)* { $(#[$doc])* fn $name($($arg: $ty),*) -> $ret = $takes $capability; }]
            [$($arms)* { $Variant { $($arg),* } method $name($($arg),*) }]
            $($rest)*);
    };
    (@munch $head:tt $methods:tt [$($arms:tt)*]
        $Variant:ident { $($field:ident),* $(,)? } => game.$name:ident($($e:expr),* $(,)?); $($rest:tt)*) => {
        layer!(@munch $head $methods [$($arms)* { $Variant { $($field),* } method $name($($e),*) }] $($rest)*);
    };
    (@munch $head:tt $methods:tt [$($arms:tt)*]
        $Variant:ident { $($field:ident),* $(,)? } => $function:ident(game $(, $e:expr)* $(,)?); $($rest:tt)*) => {
        layer!(@munch $head $methods [$($arms)* { $Variant { $($field),* } function $function($($e),*) }] $($rest)*);
    };
    (@munch [] [$($methods:tt)*] []) => {
        layer!(@layer $($methods)*);
        layer!(@bindings [$($methods)*] []);
    };
    (@munch [$Request:ident $Answer:ident [$($bound:tt)*]] [$($methods:tt)*]
        [$({ $Variant:ident { $($field:ident),* } $how:ident $call:ident($($e:expr),*) })*]) => {
        layer!(@layer $($methods)*);
        layer!(@bindings [$($methods)*] [$({ $Variant { $($field),* } $how $call($($e),*) })*]);

        /// Answers one of the group's requests.
        pub fn run(request: $Request, game: &mut (impl Layer $($bound)* + ?Sized)) -> Result<$Answer, eu4_harmony_remote::Error> {
            Ok(match request {
                $( $Request::$Variant { $($field),* } => $Answer::$Variant($crate::catalogue::Convert::convert(layer!(@call game $how $call($($e),*))?)), )*
            })
        }
    };
    (@munch $($rest:tt)*) => {
        compile_error!(concat!("layer!: a line that is none of its forms: ", stringify!($($rest)*)));
    };
    (@call $game:ident method $name:ident($($e:expr),*)) => {
        $game.$name($($crate::catalogue::Convert::convert($e)),*)
    };
    (@call $game:ident function $function:ident($($e:expr),*)) => {
        $function($game, $($e),*)
    };
    (@live $caller:ident session $capability:path, $($arg:ident),*) => {
        $capability(crate::game::Session::open($caller)?, $($arg),*)
    };
    (@live $caller:ident caller $capability:path, $($arg:ident),*) => {
        $capability($caller, $($arg),*)
    };
    (@live $caller:ident $takes:ident $capability:path, $($arg:ident),*) => {
        compile_error!(concat!("layer!: ", stringify!($capability), " takes a session or a caller, not ", stringify!($takes)))
    };
    (@layer $({ $(#[$doc:meta])* fn $name:ident($($arg:ident: $ty:ty),*) -> $ret:ty = $takes:ident $capability:path; })*) => {
        /// The group's slice of the capability layer.
        #[cfg(not(test))]
        pub trait Layer {
            $( $(#[$doc])* fn $name(&mut self, $($arg: $ty),*) -> crate::game::GResult<$ret>; )*
        }

        /// The group's slice of the capability layer; in a test, what a fake
        /// game does not script fails.
        #[cfg(test)]
        pub trait Layer {
            $(
                $(#[$doc])*
                fn $name(&mut self, $($arg: $ty),*) -> crate::game::GResult<$ret> {
                    let _ = ($($arg,)*);
                    Err($crate::catalogue::fake::unscripted(stringify!($name)))
                }
            )*
        }

        impl Layer for super::Live {
            $(
                fn $name(&mut self, $($arg: $ty),*) -> crate::game::GResult<$ret> {
                    let caller = self.0;
                    layer!(@live caller $takes $capability, $($arg),*)
                }
            )*
        }
    };
    (@bindings [$({ $(#[$doc:meta])* fn $name:ident($($arg:ident: $ty:ty),*) -> $ret:ty = $takes:ident $capability:path; })*]
        [$({ $Variant:ident { $($field:ident),* } $how:ident $call:ident($($e:expr),*) })*]) => {
        /// What the group's lines name, for the test that every capability
        /// takes its arguments by their names.
        #[cfg(test)]
        pub fn bindings() -> $crate::catalogue::Bindings {
            $crate::catalogue::Bindings {
                methods: vec![$(
                    $crate::catalogue::Method {
                        name: stringify!($name),
                        arguments: vec![$(stringify!($arg)),*],
                        types: vec![$(stringify!($ty)),*],
                        capability: std::any::type_name_of_val(&$capability),
                    }
                ),*],
                requests: vec![$(
                    $crate::catalogue::Answered {
                        request: stringify!($Variant),
                        fields: vec![$(stringify!($field)),*],
                        function: stringify!($how) == "function",
                        call: stringify!($call),
                        arguments: vec![$(stringify!($e)),*],
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
    Console(console),
    Question(questions),
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

/// What a group's `layer!` lines name: each method of its slice and the
/// capability it calls, and each request and what answers it.
#[cfg(test)]
#[derive(Default)]
pub struct Bindings {
    pub methods: Vec<Method>,
    pub requests: Vec<Answered>,
}

/// A method of a group's slice: its arguments and their types, and the
/// capability it calls, by its path.
#[cfg(test)]
pub struct Method {
    pub name: &'static str,
    pub arguments: Vec<&'static str>,
    pub types: Vec<&'static str>,
    pub capability: &'static str,
}

/// A request, its fields, and the method or the group's function that
/// answers it, with the expressions it passes.
#[cfg(test)]
pub struct Answered {
    pub request: &'static str,
    pub fields: Vec<&'static str>,
    pub function: bool,
    pub call: &'static str,
    pub arguments: Vec<&'static str>,
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

    /// The parameters of the function at `path` (as `std::any::type_name`
    /// writes it), each its name and its type, read from its source.
    fn parameters(path: &str) -> Result<Vec<(String, String)>, String> {
        let mut segments: Vec<&str> = path.split("::").collect();
        let name = segments.pop().ok_or("no name")?;
        if segments.first() != Some(&"eu4_harmony_core") {
            return Err(format!("{path} is not the core's"));
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let module = segments[1..].join("/");
        let source = [format!("{module}.rs"), format!("{module}/mod.rs")]
            .iter()
            .find_map(|file| std::fs::read_to_string(root.join(file)).ok())
            .ok_or_else(|| format!("no source for {path}"))?;
        let header = ["pub fn ", "pub(crate) fn ", "pub(super) fn ", "fn "]
            .iter()
            .find_map(|start| source.lines().position(|line| line.strip_prefix(start).is_some_and(|rest| rest.starts_with(&format!("{name}(")))));
        let start = header.ok_or_else(|| format!("no fn {name} in the source of {path}"))?;
        let text: String = source.lines().skip(start).collect::<Vec<_>>().join(
            "
",
        );
        let open = text.find('(').unwrap();
        let (mut depth, mut current, mut parameters) = (0, String::new(), Vec::new());
        for c in text[open + 1..].chars() {
            match c {
                '(' | '[' | '{' | '<' => depth += 1,
                ')' if depth == 0 => break,
                ')' | ']' | '}' | '>' => depth -= 1,
                ',' if depth == 0 => {
                    parameters.push(std::mem::take(&mut current));
                    continue;
                }
                _ => {}
            }
            current.push(c);
        }
        parameters.push(current);
        Ok(parameters
            .iter()
            .filter_map(|p| p.rsplit_once(": "))
            .map(|(name, kind)| (name.trim().trim_start_matches("mut ").to_string(), kind.split_whitespace().collect()))
            .collect())
    }

    /// The field an argument passed is made of: its first word, without a
    /// reference.
    fn root(expression: &str) -> &str {
        let expression = expression.trim_start_matches(['&', ' ']);
        &expression[..expression.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(expression.len())]
    }

    /// Whether two of `types` are the same, so that the arguments of those
    /// could be passed the other way round and still compile.
    fn shared(types: &[String], i: usize) -> bool {
        types.iter().filter(|t| **t == types[i]).count() > 1
    }

    /// Two arguments of the same type cannot be passed the other way round:
    /// a capability's parameters of a type it takes more than once, after
    /// the session or the caller, are named as its binding's arguments; and
    /// a request's fields passed to a method or to the group's own function
    /// go to the parameters of their names, where it takes more than one of
    /// their type, and never to a parameter named after another field.
    #[test]
    fn arguments_of_the_same_type_reach_each_capability_by_their_names() {
        let groups = bindings();
        let methods: Vec<&Method> = groups.iter().flat_map(|(_, b)| &b.methods).collect();
        assert!(methods.len() > 140, "only {} methods", methods.len());
        let mut wrong = Vec::new();
        for method in &methods {
            let types: Vec<String> = method.types.iter().map(|t| t.split_whitespace().collect()).collect();
            let parameters = match parameters(method.capability) {
                Ok(parameters) if parameters.len() == method.arguments.len() + 1 => parameters,
                Ok(parameters) => {
                    wrong.push(format!("{} takes {:?}, its binding passes {:?}", method.capability, &parameters[1..], method.arguments));
                    continue;
                }
                Err(e) => {
                    wrong.push(e);
                    continue;
                }
            };
            for (i, argument) in method.arguments.iter().enumerate() {
                let parameter = &parameters[i + 1].0;
                if shared(&types, i) && parameter != argument {
                    wrong.push(format!("{} takes {argument}, of a type it takes twice, as {parameter}", method.capability));
                }
            }
        }
        for (group, bindings) in &groups {
            for request in &bindings.requests {
                let parameters = match request.function {
                    true => parameters(&format!("eu4_harmony_core::catalogue::{group}::{}", request.call)).map(|p| p[1..].to_vec()),
                    false => methods
                        .iter()
                        .find(|m| m.name == request.call)
                        .map(|m| m.arguments.iter().zip(&m.types).map(|(a, t)| (a.to_string(), t.split_whitespace().collect())).collect())
                        .ok_or_else(|| format!("no method {}", request.call)),
                };
                let parameters = match parameters {
                    Ok(parameters) => parameters,
                    Err(e) => {
                        wrong.push(e);
                        continue;
                    }
                };
                let types: Vec<String> = parameters.iter().map(|(_, t)| t.clone()).collect();
                for (i, argument) in request.arguments.iter().enumerate() {
                    let (field, parameter) = (root(argument), parameters.get(i).map_or("", |(p, _)| p.as_str()));
                    let other_field = parameter != field && request.fields.contains(&parameter);
                    if request.fields.contains(&field) && (other_field || shared(&types, i) && parameter != field) {
                        wrong.push(format!("{group} {}: {argument} goes to {}'s {parameter}", request.request, request.call));
                    }
                }
            }
        }
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
