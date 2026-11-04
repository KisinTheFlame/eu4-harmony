//! Provinces, their neighbors, and one province's snapshot.
use super::tag;
use crate::game::{self, provinces as layer};
use eu4_harmony_remote::{
    provinces::{Development, Fort, Neighbor, Province, ProvinceAnswer, ProvinceKind, ProvinceRequest, ProvinceSnapshot},
    recruiting::UnitKind,
    Error, ProvinceId,
};

layer! {
    fn provinces() -> Vec<game::provinces::Province> = game::provinces::provinces;
    fn named_provinces(ids: Vec<u32>) -> Vec<game::provinces::Province> = game::provinces::named;
    fn neighbors(province: u32) -> Vec<game::provinces::Neighbor> = game::provinces::neighbors;
    fn province(province: u32) -> layer::Snapshot = layer::province;
}

pub fn run(request: ProvinceRequest, game: &mut (impl Layer + ?Sized)) -> Result<ProvinceAnswer, Error> {
    match request {
        // The layer's whole list starts with a placeholder for number 0.
        ProvinceRequest::List { provinces } => {
            let chosen: Vec<game::provinces::Province> = match provinces {
                None => game.provinces()?.into_iter().filter(|p| p.id != 0).collect(),
                Some(list) => game.named_provinces(list.0.iter().map(|id| id.0).collect())?,
            };
            Ok(ProvinceAnswer::List(
                chosen
                    .into_iter()
                    .map(|p| Province {
                        id: ProvinceId(p.id),
                        kind: match (p.land, p.lake, p.impassable) {
                            (_, true, _) => ProvinceKind::Lake,
                            (true, _, true) => ProvinceKind::Wasteland,
                            (true, _, false) => ProvinceKind::Land,
                            (false, _, _) => ProvinceKind::Sea,
                        },
                        visible: p.visible,
                        owner: p.owner.map(tag),
                        controller: p.controller.map(tag),
                    })
                    .collect(),
            ))
        }
        ProvinceRequest::Neighbors { province } => {
            Ok(ProvinceAnswer::Neighbors(game.neighbors(province.0)?.iter().map(|n| Neighbor { province: ProvinceId(n.to), strait: n.strait }).collect()))
        }
        ProvinceRequest::Get { province } => Ok(ProvinceAnswer::Get(Box::new(snapshot(game.province(province.0)?)))),
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
        autonomy: game::thousandths(s.autonomy),
        culture: s.culture,
        religion: s.religion,
        trade_good: s.trade_good,
        unrest: s.unrest.map(game::thousandths),
        trade_node: s.trade_node,
        center_of_trade: s.center_of_trade,
        trade_company: s.trade_company,
        state: s.state,
        edict: s.edict,
        cores: s.cores,
        claims: s.claims,
        constructions: s.constructions,
        missionary: s.missionary.map(game::thousandths),
        colony_size: s.colony_size.map(game::thousandths),
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
