//! The player's armies and the armies they can see. A move or a stop from a
//! remote client is the player's order (ADR 0015): the capability layer
//! takes the army back from an extension moving it.
use super::tag;
use crate::game::armies::{self as layer, RouteSource as Source};
use eu4_harmony_remote::{
    armies::{Army, ArmyAnswer, ArmyKind, ArmyRequest, Blocked, RouteBy},
    ArmyId, Error, ProvinceId,
};

layer! {
    fn army(id: u64) -> layer::Army = layer::army;
    fn own_armies() -> Vec<layer::Army> = layer::own_armies;
    fn armies_in(province: u32) -> Vec<layer::Army> = layer::in_province;
    fn route(id: u64) -> Vec<u32> = layer::route;
    fn can_step(id: u64, from: u32, to: u32) -> bool = layer::can_step;
    fn can_attack(id: u64, target: u64) -> bool = layer::can_attack_seen;
    fn find_route(id: u64, goal: u32) -> Vec<u32> = layer::find_route;
    fn move_to(id: u64, goal: u32) -> () = layer::move_to;
    fn stop(id: u64) -> () = layer::stop;
    fn selected() -> u64 = layer::selected;
}

pub fn run(request: ArmyRequest, game: &mut (impl Layer + ?Sized)) -> Result<ArmyAnswer, Error> {
    let provinces = |ids: Vec<u32>| ids.into_iter().map(ProvinceId).collect();
    Ok(match request {
        ArmyRequest::Get { army } => {
            let a = game.army(army.0)?;
            ArmyAnswer::Get(own(game, &a)?)
        }
        ArmyRequest::List {} => ArmyAnswer::List(game.own_armies()?.iter().map(|a| own(game, a)).collect::<Result<_, _>>()?),
        ArmyRequest::At { province } => ArmyAnswer::At(game.armies_in(province.0)?.iter().map(|a| snapshot(a, Vec::new())).collect()),
        ArmyRequest::RouteTo { army, province } => ArmyAnswer::RouteTo(provinces(game.find_route(army.0, province.0)?)),
        ArmyRequest::CanStep { army, from, to } => ArmyAnswer::CanStep(game.can_step(army.0, from.0, to.0)?),
        ArmyRequest::CanAttack { army, target } => ArmyAnswer::CanAttack(game.can_attack(army.0, target.0)?),
        ArmyRequest::Move { army, province } => ArmyAnswer::Move(game.move_to(army.0, province.0)?),
        ArmyRequest::Stop { army } => ArmyAnswer::Stop(game.stop(army.0)?),
        ArmyRequest::Selected {} => ArmyAnswer::Selected(ArmyId(game.selected()?)),
    })
}

/// One of the player's own armies, with its route.
fn own(game: &mut (impl Layer + ?Sized), a: &layer::Army) -> Result<Army, Error> {
    let route = if a.land && a.route_len > 0 { game.route(a.id)?.into_iter().map(ProvinceId).collect() } else { Vec::new() };
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
        // A navy's province is not read.
        province: (a.province != 0).then_some(ProvinceId(a.province)),
        regiments: a.regiments,
        blocked: blocked.into_iter().filter_map(|(set, why)| set.then_some(why)).collect(),
        route,
        // Remote control's orders are the player's, so to it a route is
        // never its own.
        route_by: match a.route_source {
            Source::None => None,
            Source::External => Some(RouteBy::PlayerOrGame),
            Source::Own | Source::OtherExtension => Some(RouteBy::OtherMod),
        },
    }
}
