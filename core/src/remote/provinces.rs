//! Provinces, their neighbors, and one province's snapshot.
use super::{tag, Live};
use crate::game::{self, provinces as layer, Caller, GResult};
use eu4_harmony_remote::{
    provinces::{Development, Fort, Neighbor, Province, ProvinceAnswer, ProvinceRequest, ProvinceSnapshot, Recruit, UnitKind},
    Error, ProvinceId,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn provinces(&mut self) -> GResult<Vec<game::provinces::Province>>;
    fn neighbors(&mut self, province: u32) -> GResult<Vec<game::provinces::Neighbor>>;
    fn province(&mut self, province: u32) -> GResult<layer::Snapshot>;
}

impl Layer for Live {
    fn provinces(&mut self) -> GResult<Vec<game::provinces::Province>> {
        game::provinces::provinces(Caller::Remote)
    }

    fn neighbors(&mut self, province: u32) -> GResult<Vec<game::provinces::Neighbor>> {
        game::provinces::neighbors(Caller::Remote, province)
    }

    fn province(&mut self, province: u32) -> GResult<layer::Snapshot> {
        layer::province(Caller::Remote, province)
    }
}

pub fn run(request: ProvinceRequest, game: &mut (impl Layer + ?Sized)) -> Result<ProvinceAnswer, Error> {
    match request {
        // The layer's list starts with a placeholder for number 0.
        ProvinceRequest::List {} => Ok(ProvinceAnswer::List(
            game.provinces()?
                .iter()
                .filter(|p| p.id != 0)
                .map(|p| Province {
                    id: ProvinceId(p.id),
                    land: p.land,
                    lake: p.lake,
                    impassable: p.impassable,
                    visible: p.visible,
                    owner: p.owner.map(tag),
                    controller: p.controller.map(tag),
                })
                .collect(),
        )),
        ProvinceRequest::Neighbors { province } => {
            Ok(ProvinceAnswer::Neighbors(game.neighbors(province.0)?.iter().map(|n| Neighbor { province: ProvinceId(n.to), strait: n.strait }).collect()))
        }
        ProvinceRequest::Get { province } => Ok(ProvinceAnswer::Get(snapshot(game.province(province.0)?))),
    }
}

fn snapshot(s: layer::Snapshot) -> ProvinceSnapshot {
    ProvinceSnapshot {
        id: ProvinceId(s.id),
        name: s.name,
        owner: s.owner.map(tag),
        controller: s.controller.map(tag),
        development: Development {
            tax: game::thousandths(s.development.tax),
            production: game::thousandths(s.development.production),
            manpower: game::thousandths(s.development.manpower),
        },
        terrain: s.terrain,
        fort: Fort { level: s.fort.level, mothballed: s.fort.mothballed },
        buildings: s.buildings,
        recruit: s.recruit.into_iter().map(|r| Recruit { kind: kind(r.kind), unit_type: r.unit_type, allowed: r.allowed, reasons: r.reasons }).collect(),
    }
}

/// The unit kind a remote client names, as the capability layer has it.
pub fn layer_kind(kind: UnitKind) -> layer::UnitKind {
    match kind {
        UnitKind::Infantry => layer::UnitKind::Infantry,
        UnitKind::Cavalry => layer::UnitKind::Cavalry,
        UnitKind::Artillery => layer::UnitKind::Artillery,
        UnitKind::HeavyShip => layer::UnitKind::HeavyShip,
        UnitKind::LightShip => layer::UnitKind::LightShip,
        UnitKind::Galley => layer::UnitKind::Galley,
        UnitKind::Transport => layer::UnitKind::Transport,
    }
}

/// A unit kind as remote clients name it.
pub fn kind(kind: layer::UnitKind) -> UnitKind {
    match kind {
        layer::UnitKind::Infantry => UnitKind::Infantry,
        layer::UnitKind::Cavalry => UnitKind::Cavalry,
        layer::UnitKind::Artillery => UnitKind::Artillery,
        layer::UnitKind::HeavyShip => UnitKind::HeavyShip,
        layer::UnitKind::LightShip => UnitKind::LightShip,
        layer::UnitKind::Galley => UnitKind::Galley,
        layer::UnitKind::Transport => UnitKind::Transport,
    }
}
