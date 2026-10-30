//! The player's armies and the armies they can see. A move or a stop from a
//! remote client is the player's order (ADR 0015): the capability layer
//! takes the army back from an extension moving it.
use super::tag;
use crate::game::armies::{self as layer, RouteSource as Source};
use eu4_harmony_remote::{
    armies::{Army, ArmyAnswer, ArmyRequest, RouteSource},
    ArmyId, Error, ProvinceId,
};

layer! {
    fn army(id: u64) -> layer::Army = layer::army;
    fn own_armies() -> Vec<layer::Army> = layer::own_armies;
    fn armies_in(province: u32) -> Vec<layer::Army> = layer::in_province;
    fn route(id: u64) -> Vec<u32> = layer::route;
    fn can_enter(id: u64, province: u32) -> bool = layer::can_enter;
    fn strait_open(id: u64, from: u32, to: u32) -> bool = layer::strait_open;
    fn can_attack(id: u64, target: u64, province: u32) -> bool = layer::can_attack;
    fn find_route(id: u64, goal: u32) -> Vec<u32> = layer::find_route;
    fn move_to(id: u64, goal: u32) -> () = layer::move_to;
    fn stop(id: u64) -> () = layer::stop;
    fn selected() -> u64 = layer::selected;
}

pub fn run(request: ArmyRequest, game: &mut (impl Layer + ?Sized)) -> Result<ArmyAnswer, Error> {
    let provinces = |ids: Vec<u32>| ids.into_iter().map(ProvinceId).collect();
    Ok(match request {
        ArmyRequest::Get { army } => ArmyAnswer::Get(snapshot(&game.army(army.0)?)),
        ArmyRequest::List {} => ArmyAnswer::List(game.own_armies()?.iter().map(snapshot).collect()),
        ArmyRequest::InProvince { province } => ArmyAnswer::InProvince(game.armies_in(province.0)?.iter().map(snapshot).collect()),
        ArmyRequest::Route { army } => ArmyAnswer::Route(provinces(game.route(army.0)?)),
        ArmyRequest::CanEnter { army, province } => ArmyAnswer::CanEnter(game.can_enter(army.0, province.0)?),
        ArmyRequest::StraitOpen { army, from, to } => ArmyAnswer::StraitOpen(game.strait_open(army.0, from.0, to.0)?),
        ArmyRequest::CanAttack { army, target, province } => ArmyAnswer::CanAttack(game.can_attack(army.0, target.0, province.0)?),
        ArmyRequest::FindRoute { army, goal } => ArmyAnswer::FindRoute(provinces(game.find_route(army.0, goal.0)?)),
        ArmyRequest::Move { army, goal } => ArmyAnswer::Move(game.move_to(army.0, goal.0)?),
        ArmyRequest::Stop { army } => ArmyAnswer::Stop(game.stop(army.0)?),
        ArmyRequest::Selected {} => ArmyAnswer::Selected(ArmyId(game.selected()?)),
    })
}

fn snapshot(a: &layer::Army) -> Army {
    Army {
        id: ArmyId(a.id),
        owner: tag(a.owner),
        // A fleet's province is not read.
        province: (a.province != 0).then_some(ProvinceId(a.province)),
        land: a.land,
        in_battle: a.in_battle,
        sieging: a.sieging,
        retreating: a.retreating,
        exiled: a.exiled,
        movement_locked: a.movement_locked,
        attached: a.attached,
        has_followers: a.has_followers,
        busy: a.busy,
        regiments: a.regiments,
        route_len: a.route_len,
        destination: a.destination.map(ProvinceId),
        // Remote control's orders are the player's, so to it a route is
        // never its own.
        route_source: match a.route_source {
            Source::None => RouteSource::None,
            Source::External => RouteSource::External,
            Source::Own | Source::OtherExtension => RouteSource::Extension,
        },
    }
}
