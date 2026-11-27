//! The capability layer: every capability extensions and remote clients use,
//! implemented once (ADR remote-control-uses-capabilities). Shared game-world
//! read/command primitives live in `crate::world` and are re-exported here for
//! the capability group modules.
pub(crate) mod administration;
pub(crate) mod areas;
pub(crate) mod army;
pub(crate) mod frame;
pub(crate) mod geography;
pub(crate) mod map;
pub(crate) mod orders;
pub(crate) mod signal_costs;
pub(crate) mod signals;
pub(crate) mod spendings;
pub(crate) mod units;
pub(crate) mod wars;

#[cfg(test)]
pub(crate) use crate::fake_game::buffers;
pub(crate) use crate::world::{
    action, commands, conditions, content, context, context_any, country, envoys, fail, game, map_manager, modifiers, named, paused, player, power, reasons,
    special_countries, thousandths, unbulleted, view, world_context, world_context_any, As, Caller, Ctx, Fail, GResult, InWorld, Status, Tag, View, World,
    MAX_PROVINCES, MAX_UNITS,
};

/// Declares each capability group's module of the layer, and collects the
/// groups' dependency points ([`group_points`]), from the catalogue's list
/// of the groups (`super::catalogue::capability_groups`).
macro_rules! layer_modules {
    (@munch [$($modules:tt)*] $(#[$attr:meta])* $Group:ident($module:ident $(: Layer)?) $(=> $outcome:expr)?, $($rest:tt)*) => {
        layer_modules!(@munch [$($modules)* { $(#[$attr])* $module }] $($rest)*);
    };
    (@munch $modules:tt $(#[$attr:meta])* $Group:ident($module:ident: External) => $outcome:expr, $($rest:tt)*) => {
        layer_modules!(@munch $modules $($rest)*);
    };
    (@munch $modules:tt $(#[$attr:meta])* $Group:ident => $outcome:expr, $($rest:tt)*) => {
        layer_modules!(@munch $modules $($rest)*);
    };
    (@munch [$({ $(#[$attr:meta])* $module:ident })*]) => {
        $( $(#[$attr])* pub mod $module; )*

        /// The capability groups' dependency points.
        fn group_points() -> Vec<Point> {
            let mut points = Vec::new();
            $( $(#[$attr])* points.extend($module::points()); )*
            points
        }
    };
    (fn dispatch($($name:ident),*); $($lines:tt)*) => {
        layer_modules!(@munch [] $($lines)*);
    };
}

super::catalogue::capability_groups!(layer_modules);

use crate::native::{memory::Memory, points::Point};
use eu4_harmony_remote::signals::Kind;
use std::sync::Mutex;

pub use frame::{Frame, Heard};

/// The session a capability reads, as its caller reads it: the game, the
/// session at this instant ([`Ctx`]), how much of it the caller reads, the
/// caller, and where its commands go.
#[derive(Clone, Copy, Debug)]
pub struct Session {
    pub m: &'static Memory,
    pub ctx: Ctx,
    pub view: View,
    pub caller: Caller,
    pub commands: commands::Commands,
}

impl Session {
    pub fn open(caller: Caller) -> GResult<Self> {
        let m = game()?;
        let ctx = context(m)?;
        Ok(Self { m, ctx, view: view(caller), caller, commands: commands::Commands::Game })
    }

    pub fn with_view(self, view: View) -> Self {
        Self { view, ..self }
    }

    pub fn eyes(self) -> player::Eyes {
        player::Eyes::acting(self.view, self.ctx.player)
    }

    pub fn sees_all_of(self, country: Tag) -> bool {
        self.eyes().sees_all_of(country)
    }

    pub fn sees_inside(self, owner: Option<Tag>) -> bool {
        self.eyes().sees_inside(owner)
    }

    pub fn sees_full(self) -> bool {
        self.eyes() == player::Eyes::All
    }

    pub fn readable_province(self, id: u32) -> GResult<usize> {
        provinces::readable(self.m, &self.ctx, self.eyes(), id)
    }

    pub fn require_current_visibility(self) -> GResult<()> {
        if self.view == View::Player && !signals::visibility_current(self.m, &self.ctx) {
            return Err(eu4_harmony_remote::checks::armies::visibility_not_current().into());
        }
        Ok(())
    }

    pub fn province_hidden(self, province: usize) -> GResult<bool> {
        Ok(self.view == View::Player && geography::hidden(self.m, province)?)
    }
}

/// The session as a capability that acts as no country reads it.
#[derive(Clone, Copy, Debug)]
pub struct WorldSession {
    pub m: &'static Memory,
    pub world: World,
    pub view: View,
    pub caller: Caller,
    pub commands: commands::Commands,
}

impl WorldSession {
    pub fn open(caller: Caller) -> GResult<Self> {
        let m = game()?;
        Ok(Self { m, world: world_context(m)?, view: view(caller), caller, commands: commands::Commands::Game })
    }

    pub fn eyes(self) -> player::Eyes {
        player::Eyes::of(self.view, self.world.player)
    }

    pub fn sees_all_of(self, country: Tag) -> bool {
        self.eyes().sees_all_of(country)
    }

    pub fn sees_full(self) -> bool {
        self.eyes() == player::Eyes::All
    }

    pub fn readable_province(self, id: u32) -> GResult<usize> {
        provinces::readable(self.m, &self.world, self.eyes(), id)
    }
}

#[derive(Default)]
struct State {
    geography: geography::Geography,
    orders: orders::Orders,
    wars: wars::Watch,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

static RECALL: Mutex<frame::Recall> = Mutex::new(frame::Recall::new());

pub fn session_forgotten() {
    with_state(|s| {
        s.geography.clear();
        s.orders.clear();
        s.wars.clear();
    });
}

#[cfg(test)]
pub(crate) fn clear_geography() {
    with_state(|s| s.geography.clear());
}

pub fn begin_frame(m: &Memory) -> GResult<Frame> {
    let (world, fog) = signals::frame_campaign(m)?;
    let seen = crate::session::sessions::frame_read(world.world, world.player.own().0, || {
        let tag = |tag: Option<Tag>| tag.filter(|t| t.normal()).map(|t| String::from_utf8_lossy(&t.letters()).into_owned()).unwrap_or_else(|| "none".into());
        format!(
            "game_date={} own={} in_view={} observing={}",
            eu4_harmony_remote::Date(world.date),
            tag(Some(world.player.own())),
            tag(world.player.acting()),
            world.player.observing()
        )
    });
    let (visibility_current, changes) = signals::take_frame(&world, fog);
    with_state(|s| {
        for change in &changes {
            if let signals::Change::Ordered(army) = change {
                s.orders.revoke(*army);
            }
        }
    });
    let observed = frame::Observed { world, seen, visibility_current, changes };
    let mut recall = RECALL.lock().unwrap_or_else(|e| e.into_inner());
    Ok(recall.next(observed, signals::listened, |world| watch_wars(m, world)))
}

fn watch_wars(m: &Memory, ctx: &World) -> Vec<wars::Happened> {
    let war = signals::listened(Kind::WarDeclared) || signals::listened(Kind::PeaceMade);
    let owners = signals::listened(Kind::ProvinceOwnerChanged);
    let seen = with_state(|s| {
        let mut happened = Vec::new();
        match war {
            true => happened.extend(signal_costs::WARS.time(|| s.wars.wars(m, ctx))?),
            false => s.wars.forget_wars(),
        }
        match owners {
            true => happened.extend(signal_costs::OWNERS.time(|| Ok::<_, Fail>(s.wars.owners(s.geography.owners(m, ctx)?)))?),
            false => s.wars.forget_owners(),
        }
        Ok::<_, Fail>(happened)
    });
    seen.unwrap_or_else(|e| {
        log!("wars: {}", e.message);
        Vec::new()
    })
}

/// Dependency points owned by the capability groups and capability-specific helpers.
pub fn points() -> Vec<Point> {
    let mut points = group_points();
    points.extend(geography::points());
    points.extend(army::points());
    points.extend(orders::points());
    points.extend(signals::points());
    points.extend(administration::points());
    points.extend(areas::points());
    points.extend(units::points());
    points.extend(spendings::points());
    points.extend(map::points());
    points
}
