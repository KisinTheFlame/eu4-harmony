//! The capability catalogue (ADR one-capability-catalogue), as the core runs
//! its requests: through the capability layer (`crate::game`), for both of
//! its callers, remote control's engine (`crate::remote`), which runs them as
//! the player, and the Lua host (`crate::lua`), which runs them as the
//! extension calling. Both reach it through one seam, [`Catalogue`]: a
//! request runs at once or answers at a later safe point ([`Outcome`],
//! [`Wait`]).
//!
//! A group whose requests only call the capability layer binds them in the
//! layer's own module, beside the functions they call ([`bind`]). A group
//! whose tests give the fake game answers of its own has a file here, with
//! its slice of the layer as a trait ([`Game`] is all of them, and [`Live`]
//! the layer itself behind it), and turns the group's requests into calls of
//! the layer, which answers in the catalogue's own types. Every [`Game`] is
//! a [`Catalogue`]. The fake game of the tests ([`fake`]) fails a capability
//! a test does not script, and every request of a bound group.

/// Binds a group's requests to the capability layer's functions once, in
/// the layer's module: `run_request` answers each request as its caller
/// asks it. After the group's module in remote control (whose `args` module
/// declares the requests' arguments), its request and its answer, each line
/// is one of
///
/// ```text
/// Variant => capability;
/// Variant => caller capability;
/// Variant { field, ... } => session capability(expression, ...);
/// ```
///
/// A request whose capability takes its arguments by their names, after
/// what [`Live`] opens for it as its place in remote control says
/// ([`Opens`]): a session with a country to act as (`crate::game::Session`),
/// one that acts as no country (`crate::game::WorldSession`), or the caller
/// alone (`crate::game::Caller`); one whose capability takes the caller
/// instead, to check something of its own first; and one whose capability
/// takes what the line passes it, after a session, a world session (`world`)
/// or the caller (`caller`). Answers, and what a line passes, pass through
/// [`Convert`].
macro_rules! bind {
    (@munch $head:tt [$($arms:tt)*] $Variant:ident => caller $capability:path; $($rest:tt)*) => {
        $crate::catalogue::bind!(@munch $head [$($arms)* { $Variant caller [$capability] () () }] $($rest)*);
    };
    (@munch $head:tt [$($arms:tt)*] $Variant:ident => $capability:path; $($rest:tt)*) => {
        $crate::catalogue::bind!(@munch $head [$($arms)* { $Variant place [$capability] () () }] $($rest)*);
    };
    (@munch $head:tt [$($arms:tt)*]
        $Variant:ident { $($field:ident),* $(,)? } => $takes:ident $($capability:ident)::+ ($($e:expr),* $(,)?); $($rest:tt)*) => {
        $crate::catalogue::bind!(@munch $head [$($arms)* { $Variant $takes [$($capability)::+] ($($field),*) ($($e),*) }] $($rest)*);
    };
    (@munch [($($group:ident)::+) $Request:ident $Answer:ident]
        [$({ $Variant:ident $takes:ident [$capability:path] ($($field:ident),*) ($($e:expr),*) })*]) => {
        /// Answers one of the group's requests, as `caller` asks it.
        pub fn run_request(
            request: $($group)::+::$Request,
            caller: $crate::game::Caller,
        ) -> ::std::result::Result<$($group)::+::$Answer, eu4_harmony_remote::Error> {
            use $($group)::+ as group;
            Ok(match request.into_args() {
                $( group::args::Args::$Variant(arguments) => group::$Answer::$Variant($crate::catalogue::Convert::convert(
                    $crate::catalogue::bind!(@call caller arguments $Variant $takes [$capability] ($($field),*) ($($e),*))?
                )), )*
            })
        }

        /// What the group's lines name, for the test that a request acts as
        /// a country exactly when it opens a session with one.
        #[cfg(test)]
        pub fn bindings() -> $crate::catalogue::Bindings {
            use $($group)::+ as group;
            $crate::catalogue::Bindings {
                methods: vec![$( $crate::catalogue::Method { name: stringify!($Variant), takes: stringify!($takes) } ),*],
                requests: vec![$(
                    $crate::catalogue::Answered {
                        request: stringify!($Variant),
                        function: false,
                        call: stringify!($Variant),
                        country: group::$Request::country_of(stringify!($Variant)),
                    }
                ),*],
            }
        }
    };
    (@munch $($rest:tt)*) => {
        compile_error!(concat!("bind!: a line that is none of its forms: ", stringify!($($rest)*)));
    };
    // `group` is the group's module in remote control, as `run_request`
    // names it.
    (@call $caller:ident $arguments:ident $Variant:ident place [$capability:path] () ()) => {
        $crate::catalogue::Capability::call(
            $capability,
            <<group::args::$Variant as eu4_harmony_remote::Arguments>::Place as $crate::catalogue::Opens>::open($caller)?,
            $arguments,
        )
    };
    (@call $caller:ident $arguments:ident $Variant:ident caller [$capability:path] () ()) => {
        $crate::catalogue::Capability::call($capability, $caller, $arguments)
    };
    (@call $caller:ident $arguments:ident $Variant:ident session [$capability:path] ($($field:ident),*) ($($e:expr),*)) => {{
        let group::args::$Variant { $($field),* } = $arguments;
        $capability($crate::game::Session::open($caller)?, $($crate::catalogue::Convert::convert($e)),*)
    }};
    (@call $caller:ident $arguments:ident $Variant:ident world [$capability:path] ($($field:ident),*) ($($e:expr),*)) => {{
        let group::args::$Variant { $($field),* } = $arguments;
        $capability($crate::game::WorldSession::open($caller)?, $($crate::catalogue::Convert::convert($e)),*)
    }};
    (@call $caller:ident $arguments:ident $Variant:ident caller [$capability:path] ($($field:ident),*) ($($e:expr),*)) => {{
        let group::args::$Variant { $($field),* } = $arguments;
        $capability($caller, $($crate::catalogue::Convert::convert($e)),*)
    }};
    (@call $caller:ident $arguments:ident $Variant:ident $takes:ident [$capability:path] $($rest:tt)*) => {
        compile_error!(concat!("bind!: ", stringify!($capability), " takes a session, a world or a caller, not ", stringify!($takes)))
    };
    ($($group:ident)::+ : $Request:ident => $Answer:ident; $($lines:tt)*) => {
        $crate::catalogue::bind!(@munch [($($group)::+) $Request $Answer] [] $($lines)*);
    };
}
pub(crate) use bind;

/// Declares the slice of the capability layer of a group that has one, and
/// its requests, once: its `Layer` trait, [`Live`] calling each capability
/// there for its caller, and, given `run Request => Answer;` first, the
/// group's `run`, which answers each request. Each line is one of
///
/// ```text
/// Variant => fn method -> Result = capability;
/// Variant => fn method -> Result = caller capability;
/// fn method(argument: Type, ...) -> Result = session capability;
/// Variant { field, ... } => game.method(expression, ...);
/// ```
///
/// A method and the request it answers, whose capability takes the
/// request's arguments by their names, as the group's `args` module in
/// remote control declares them (`eu4_harmony_remote::<group>::args`, which
/// the group's file imports as `args`); a method alone; a request answered
/// by a method of another line. Answers, and the arguments a line passes
/// itself, pass through [`Convert`].
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
#[cfg(test)]
pub(crate) mod fake;
pub mod signals;

use crate::game;
use eu4_harmony_remote::{Answer, ArmyId, ArmyList, Code, CountryTag, Error, ProvinceId, Request, Response};
use std::{
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Declares, from the list of the groups ([`capability_groups`]), [`Game`],
/// the slices of the capability layer of the groups that have one, and
/// `dispatch`, which runs a request in its group. After the names
/// `dispatch` gives its request, game, signals (remote control's, when its
/// caller keeps them) and time, each line is one of
///
/// ```text
/// Group(module),
/// Group(module) => outcome,
/// Group(module: Layer),
/// Group(module: Layer) => outcome,
/// Group => outcome,
/// ```
///
/// A group bound to the layer in its layer module ([`bind`]), answered at
/// once, or as the expression gives its [`Outcome`] (its module here is
/// declared beside the list); a group with a slice of the layer, whose
/// `run(request, game)` answers at once, or whose [`Outcome`] the
/// expression gives; a group of neither. The module here of a group with a
/// slice is declared here, as remote control's groups declare theirs; the
/// capability layer declares each group's module of its own.
macro_rules! groups {
    (@munch $names:tt [$($slices:tt)*] [$($bound:tt)*] [$($arms:tt)*]
        $(#[$attr:meta])* $Group:ident($module:ident: Layer) => $outcome:expr, $($rest:tt)*) => {
        groups!(@munch $names [$($slices)* $module] [$($bound)*] [$($arms)* { $(#[$attr])* $Group => $outcome }] $($rest)*);
    };
    (@munch [$request:ident $game:ident $signals:ident $now:ident] [$($slices:tt)*] [$($bound:tt)*] [$($arms:tt)*]
        $(#[$attr:meta])* $Group:ident($module:ident: Layer), $($rest:tt)*) => {
        groups!(@munch [$request $game $signals $now] [$($slices)* $module] [$($bound)*]
            [$($arms)* { $(#[$attr])* $Group => Outcome::Done($module::run($request, $game).map(Answer::$Group)) }]
            $($rest)*);
    };
    (@munch $names:tt [$($slices:tt)*] [$($bound:tt)*] [$($arms:tt)*]
        $(#[$attr:meta])* $Group:ident($module:ident) => $outcome:expr, $($rest:tt)*) => {
        groups!(@munch $names [$($slices)*] [$($bound)* $module] [$($arms)* { $(#[$attr])* $Group => $outcome }] $($rest)*);
    };
    (@munch [$request:ident $game:ident $signals:ident $now:ident] [$($slices:tt)*] [$($bound:tt)*] [$($arms:tt)*]
        $(#[$attr:meta])* $Group:ident($module:ident), $($rest:tt)*) => {
        groups!(@munch [$request $game $signals $now] [$($slices)*] [$($bound)* $module]
            [$($arms)* { $(#[$attr])* $Group => Outcome::Done(bound($game, stringify!($module), |caller| game::$module::run_request($request, caller)).map(Answer::$Group)) }]
            $($rest)*);
    };
    (@munch $names:tt $slices:tt $bound:tt [$($arms:tt)*] $(#[$attr:meta])* $Group:ident => $outcome:expr, $($rest:tt)*) => {
        groups!(@munch $names $slices $bound [$($arms)* { $(#[$attr])* $Group => $outcome }] $($rest)*);
    };
    (@munch [$request:ident $game:ident $signals:ident $now:ident] [$($slice:ident)*] [$($bound:ident)*]
        [$({ $(#[$attr:meta])* $Group:ident => $outcome:expr })*]) => {
        $( pub mod $slice; )*

        /// The capability layer, as the catalogue's requests reach it: the
        /// slices of the groups that have one, and the caller the bound
        /// groups' requests call the layer as.
        pub trait Game: $($slice::Layer +)* {
            fn as_game(&mut self) -> &mut dyn Game;

            /// Whom the bound groups' requests call the layer as; a fake game
            /// calls it as no one, and they fail there.
            fn caller(&self) -> Option<game::Caller>;

            /// The requests from now on act `acting`, when they are a remote
            /// client's (ADR requests-act-as-the-player-or-as-the-game).
            fn act_as(&mut self, _acting: game::As) {}
        }

        /// Every group's lines, by its module.
        #[cfg(test)]
        fn bindings() -> Vec<(&'static str, Bindings)> {
            vec![
                $( (stringify!($slice), $slice::bindings()), )*
                $( (stringify!($bound), game::$bound::bindings()), )*
            ]
        }

        /// Runs one request at `now`, in its group.
        fn dispatch($request: Request, $game: &mut dyn Game, $signals: Option<&signals::Buffer>, $now: Instant) -> Outcome {
            match $request {
                $( $(#[$attr])* Request::$Group($request) => $outcome, )*
            }
        }
    };
    (@munch $($rest:tt)*) => {
        compile_error!(concat!("groups!: a line that is none of its forms: ", stringify!($($rest)*)));
    };
    (fn dispatch($request:ident, $game:ident, $signals:ident, $now:ident); $($lines:tt)*) => {
        groups!(@munch [$request $game $signals $now] [] [] [] $($lines)*);
    };
}

/// The modules of the bound groups that do more than answer at once.
pub mod console;

/// The capability groups, once, as [`groups!`] takes them: each group's
/// line, read here, where [`groups!`] declares [`Game`] and `dispatch`,
/// and by the capability layer (`crate::game`), which declares the layer's
/// module of each group named with a module and collects its dependency
/// points. A new group is one line here.
macro_rules! capability_groups {
    ($then:ident) => {
        $then! {
            fn dispatch(request, game, signals, now);
            Session(session: Layer),
            Time(time: Layer) => { let view = view(game); time::run(request, game, signals, view) },
            Country(countries: Layer),
            Province(provinces: Layer),
            Army(armies: Layer),
            Text(texts: Layer),
            Screenshot(screenshot: Layer) => screenshot::run(request, game),
            Console(console) => console::run_and_answer_next(request, game),
            Question(questions),
            Popup(popups),
            Offer(offers),
            Signals => match signals {
                Some(signals) => signals::run(request, signals, view(game), now),
                None => Outcome::Done(Err(signals::unkept())),
            },
            Describe => Outcome::Done(Ok(Answer::Describe(describe::run(request)))),
            Camera(camera),
            Recruit(recruiting: Layer),
            Menu(menu: Layer) => menu::run(request, game),
            Diplomacy(diplomacy: Layer) => diplomacy::run(request, game, now),
            Extension(extensions),
            #[cfg(feature = "dev-memory")]
            Memory => Outcome::Done(crate::memory_account::answer(request).map(Answer::Memory)),
            Input(input: Layer) => input::run(request, game),
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
            Advisor(advisors),
            Empire(empire),
            CelestialEmpire(celestial_empire),
            Religion(religion),
            Mercenary(mercenaries),
            Fort(forts),
            Siege(sieges),
            Peace(peace: Layer) => peace::run(request, game, now),
            Occupation(occupation),
            Missionary(missionaries),
            Colonist(colonists),
            TradeCompany(trade_companies),
            CenterOfTrade(centers_of_trade),
            Merchant(merchants),
            Trade(trade),
            Script(script),
        }
    };
}
pub(crate) use capability_groups;

capability_groups!(groups);

/// The view `game`'s caller reads; the full view in a fake game, which
/// calls the layer as no one.
fn view(game: &dyn Game) -> game::View {
    game.caller().map_or(game::View::Full, game::view)
}

/// Whether `caller` may ask for a capability that does what no player could
/// (`as_game`): an extension, a request as the game and a fake game may, a
/// request as the player may not (ADR
/// requests-act-as-the-player-or-as-the-game).
fn may_ask(as_game: bool, caller: Option<game::Caller>) -> bool {
    !as_game || caller != Some(game::Caller::Remote(game::As::Player))
}

/// The refusal of a capability that does what no player could to a request
/// as the player.
fn as_the_game(request: &Request) -> Error {
    Error::new(Code::AsTheGame, format!("{} does what no player could; a request as the player cannot ask for it: ask as the game", request.name()))
}

/// Runs a request of a bound group, as `game` calls the layer: `run` with
/// its caller, or, in a fake game, which calls it as no one, not at all.
fn bound<A>(game: &mut dyn Game, group: &str, run: impl FnOnce(game::Caller) -> Result<A, Error>) -> Result<A, Error> {
    match game.caller() {
        Some(caller) => run(caller),
        None => Err(Error::new(Code::Internal, format!("{group} is not scripted in this test"))),
    }
}

impl Game for Live {
    fn as_game(&mut self) -> &mut dyn Game {
        self
    }

    fn caller(&self) -> Option<game::Caller> {
        Some(self.0)
    }

    fn act_as(&mut self, acting: game::As) {
        if let game::Caller::Remote(_) = self.0 {
            self.0 = game::Caller::Remote(acting);
        }
    }
}
/// The catalogue as its two callers reach it, remote control's engine and the
/// Lua host: the one seam between them and the capability layer. Every
/// [`Game`] is one, [`Live`] in the game.
pub trait Catalogue {
    /// Runs one request at `now`. `signals` are remote control's, which only
    /// its engine keeps: a request that reads them, run without, is answered
    /// that only remote clients read them.
    fn run(&mut self, request: Request, signals: Option<&signals::Buffer>, now: Instant) -> Outcome;

    /// At a safe point at `place`: `polls` asks the requests that wait, each
    /// with the same [`Poll`].
    fn at(&mut self, signals: Option<&signals::Buffer>, now: Instant, place: &Place, polls: &mut dyn FnMut(&mut Poll<'_>));

    /// The requests run from now on act `acting`, when they are a remote
    /// client's: remote control's engine says so before each it runs.
    fn act_as(&mut self, _acting: game::As) {}
}

impl<T: Game + ?Sized> Catalogue for T {
    /// A capability that does what no player could is refused to a request
    /// as the player.
    fn run(&mut self, request: Request, signals: Option<&signals::Buffer>, now: Instant) -> Outcome {
        if !may_ask(request.as_game(), self.caller()) {
            return Outcome::Done(Err(as_the_game(&request)));
        }
        dispatch(request, self.as_game(), signals, now)
    }

    fn act_as(&mut self, acting: game::As) {
        Game::act_as(self, acting);
    }

    fn at(&mut self, signals: Option<&signals::Buffer>, now: Instant, place: &Place, polls: &mut dyn FnMut(&mut Poll<'_>)) {
        polls(&mut Poll::new(signals, now, place, Some(self.as_game())));
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
/// process, in which sessions and signals are numbered from 1 again; a
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
    /// Answered at once, and the requests after it that the [`Hold`] holds
    /// back wait, in order, until it lets go.
    Held(Response, Box<dyn Hold>),
}

/// What an answer holds back of the requests queued after it, such as the
/// requests after an input, until the game has had the input.
pub trait Hold: Send {
    /// Whether it holds back `request`; one it lets pass runs as if it were
    /// not there.
    fn holds(&self, request: &Request) -> bool;

    /// Whether it has let go, as seen at this safe point.
    fn let_go(&mut self, at: &mut Poll<'_>) -> bool;
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
    fn ending(&mut self, _signals: &signals::Buffer) -> Option<Response> {
        None
    }
}

/// What a waiting request may look at.
pub struct Poll<'a> {
    /// Remote control's signals, which only its engine keeps.
    pub signals: Option<&'a signals::Buffer>,
    pub now: Instant,
    /// Where a request runs: remote control's engine runs at every safe point,
    /// extensions only at a game's safe frame.
    pub place: &'a Place,
    game: Option<&'a mut dyn Game>,
}

impl<'a> Poll<'a> {
    /// At `place`, with the capability layer if a waiting request may use
    /// it.
    pub fn new(signals: Option<&'a signals::Buffer>, now: Instant, place: &'a Place, game: Option<&'a mut dyn Game>) -> Self {
        Self { signals, now, place, game }
    }

    /// The requests polled from now on act `acting`, when they are a remote
    /// client's: as they did when they ran.
    pub fn act_as(&mut self, acting: game::As) {
        if let Some(game) = self.game.as_mut() {
            game.act_as(acting);
        }
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

    /// The capability layer at any safe point, also outside a game: only
    /// for what reads nothing of a game, such as whether the game has had
    /// an input.
    fn layer(&mut self) -> Option<&mut dyn Game> {
        self.game.as_mut().map(|game| &mut **game as &mut dyn Game)
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
    fn what_no_player_could_do_is_asked_for_only_as_the_game() {
        use game::{As, Caller};
        assert!(!may_ask(true, Some(Caller::Remote(As::Player))));
        for caller in [Some(Caller::Remote(As::Game)), Some(Caller::Extension(0)), None] {
            assert!(may_ask(true, caller));
        }
        for caller in [Some(Caller::Remote(As::Player)), Some(Caller::Remote(As::Game)), Some(Caller::Extension(0)), None] {
            assert!(may_ask(false, caller));
        }
        // A request as the game reads the full view; the next one, as the
        // player, the player's view again.
        let mut live = Live(Caller::Remote(As::Player));
        Game::act_as(&mut live, As::Game);
        assert_eq!((live.0, view(&live)), (Caller::Remote(As::Game), game::View::Full));
        Game::act_as(&mut live, As::Player);
        assert_eq!(view(&live), game::View::Player);
        // An extension stays itself.
        let mut extension = Live(Caller::Extension(2));
        Game::act_as(&mut extension, As::Player);
        assert_eq!(extension.0, Caller::Extension(2));
        assert_eq!(as_the_game(&Request::Session(eu4_harmony_remote::session::SessionRequest::Get {})).code, Code::AsTheGame);
    }

    #[test]
    fn what_reads_remote_control_s_signals_says_so_when_run_without() {
        let mut game = fake::Fake::default();
        let reads = [r#"{"capability": "signals"}"#, r#"{"capability": "time.advance", "days": 1}"#];
        for json in reads {
            let request = eu4_harmony_remote::request_from_json(json).unwrap();
            let Outcome::Done(Err(error)) = game.run(request, None, Instant::now()) else { panic!("{json} ran without signals") };
            assert_eq!(error, signals::unkept());
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
        assert!(checked > 20, "only {checked} checked");
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
            if let Outcome::Done(Ok(answer)) = fake::Fake::default().run(request, Some(&signals::Buffer::new()), Instant::now()) {
                let lua = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(&answer)).unwrap();
                let shape = result_shape(&name).unwrap_or_else(|| panic!("{name} has no result shape"));
                shape.check(&lua).unwrap_or_else(|why| panic!("{name}'s answer is not its shape: {why}\n{lua}"));
                shaped.push(name);
            }
        }
        assert!(shaped.len() > 20, "only {} answers were checked against their shapes: {shaped:?}", shaped.len());
    }
}
