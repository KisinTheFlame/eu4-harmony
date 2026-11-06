//! The player's armies and the armies they can see. A move or a stop from a
//! remote client is the player's order (ADR remote-control-uses-capabilities):
//! the capability layer takes the army back from an extension moving it.
use super::tag;
use crate::game::{
    armies::{self as layer, RouteSource as Source},
    units,
};
use eu4_harmony_remote::{
    armies::{Army, ArmyAction, ArmyAnswer, ArmyKind, ArmyRequest, Blocked, RouteBy},
    ArmyId, Error, ProvinceId,
};

layer! {
    run ArmyRequest => ArmyAnswer;
    fn army(army: u64) -> layer::Army = session layer::army;
    fn own_armies() -> Vec<layer::Army> = session layer::own_armies;
    fn armies_in(province: u32) -> Vec<layer::Army> = session layer::in_province;
    fn route(army: u64) -> Vec<u32> = session layer::route;
    Get { army } => get(game, army.0);
    List {} => list(game);
    At { province } => at(game, province.0);
    RouteTo => fn find_route(army: u64, province: u32) -> Vec<u32> = session layer::find_route;
    CanStep => fn can_step(army: u64, from: u32, to: u32) -> bool = session layer::can_step;
    CanAttack => fn can_attack(army: u64, target: u64) -> bool = session layer::can_attack_seen;
    fn move_to(army: u64, province: u32, transports: bool) -> () = session layer::move_to;
    Move { army, province, transports } => game.move_to(army, province, transports.unwrap_or(false));
    Stop => fn stop(army: u64) -> () = session layer::stop;
    Selected => fn selected() -> u64 = session layer::selected;
    Options => fn army_options(army: u64) -> Vec<ArmyAction> = session units::options;
    Split => fn split(army: u64) -> u64 = session units::split;
    Merge => fn merge(armies: Vec<ArmyId>) -> () = session units::merge;
    Disband => fn disband(army: u64) -> () = session units::disband;
    Consolidate => fn consolidate(army: u64) -> () = session units::consolidate;
    Drill => fn drill(army: u64) -> () = session units::drill;
    StopDrill => fn stop_drill(army: u64) -> () = session units::stop_drill;
    Mission => fn mission(army: u64, mission: String, target: Option<u32>) -> () = session units::mission;
    StopMission => fn stop_mission(army: u64) -> () = session units::stop_mission;
}

fn get(game: &mut (impl Layer + ?Sized), army: u64) -> Result<Army, Error> {
    let a = game.army(army)?;
    own(game, &a)
}

fn list(game: &mut (impl Layer + ?Sized)) -> Result<Vec<Army>, Error> {
    game.own_armies()?.iter().map(|a| own(game, a)).collect()
}

/// The armies the player sees in the province.
fn at(game: &mut (impl Layer + ?Sized), province: u32) -> Result<Vec<Army>, Error> {
    Ok(game.armies_in(province)?.iter().map(|a| snapshot(a, Vec::new())).collect())
}

/// One of the player's own armies, with its route.
fn own(game: &mut (impl Layer + ?Sized), a: &layer::Army) -> Result<Army, Error> {
    let route = if a.route_len > 0 { game.route(a.id)?.into_iter().map(ProvinceId).collect() } else { Vec::new() };
    Ok(snapshot(a, route))
}

fn snapshot(a: &layer::Army, route: Vec<ProvinceId>) -> Army {
    let blocked = [
        (a.in_battle, Blocked::InBattle),
        (a.sieging, Blocked::Sieging),
        (a.retreating, Blocked::Retreating),
        (a.exiled, Blocked::Exiled),
        (a.movement_locked, Blocked::MovementLocked),
        (a.attached, Blocked::Attached),
        (a.has_followers, Blocked::HasFollowers),
        (a.busy, Blocked::Busy),
    ];
    Army {
        id: ArmyId(a.id),
        owner: tag(a.owner),
        kind: if a.land { ArmyKind::Army } else { ArmyKind::Navy },
        province: (a.province != 0).then_some(ProvinceId(a.province)),
        regiments: a.regiments,
        blocked: blocked.into_iter().filter_map(|(set, why)| set.then_some(why)).collect(),
        route,
        // Remote control's orders are the player's, so to it a route is
        // never its own: only an extension's is.
        route_by: match a.route_source {
            Source::None => None,
            Source::External => Some(RouteBy::PlayerOrGame),
            Source::Own => Some(RouteBy::ThisMod),
            Source::OtherExtension => Some(RouteBy::OtherMod),
        },
        aboard: a.aboard.map(ArmyId),
        carrying: a.carrying,
    }
}
