//! Provinces, their neighbors, and one province's snapshot.
use super::tag;
use crate::game::{
    self,
    administration::{Action, Development as Kind},
    provinces as layer,
};
use eu4_harmony_remote::{
    provinces::{Development, Fort, Neighbor, Province, ProvinceAnswer, ProvinceKind, ProvinceOption, ProvinceRequest, ProvinceSnapshot},
    recruiting::UnitKind,
    Error, ProvinceId,
};

layer! {
    fn provinces() -> Vec<game::provinces::Province> = game::provinces::provinces;
    fn named_provinces(ids: Vec<u32>) -> Vec<game::provinces::Province> = game::provinces::named;
    fn neighbors(province: u32) -> Vec<game::provinces::Neighbor> = game::provinces::neighbors;
    fn province(province: u32) -> layer::Snapshot = layer::province;
    fn province_options(province: u32) -> Vec<ProvinceOption> = game::administration::options;
    fn province_act(province: u32, action: Action) -> () = game::administration::act;
    fn set_edict(province: u32, edict: String) -> () = game::administration::set_edict;
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
        ProvinceRequest::Get { province } => Ok(ProvinceAnswer::Get(snapshot(game.province(province.0)?))),
        ProvinceRequest::Options { province } => Ok(ProvinceAnswer::Options(game.province_options(province.0)?)),
        ProvinceRequest::DevelopTax { province } => Ok(ProvinceAnswer::DevelopTax(game.province_act(province.0, Action::Develop(Kind::Tax))?)),
        ProvinceRequest::DevelopProduction { province } => {
            Ok(ProvinceAnswer::DevelopProduction(game.province_act(province.0, Action::Develop(Kind::Production))?))
        }
        ProvinceRequest::DevelopManpower { province } => Ok(ProvinceAnswer::DevelopManpower(game.province_act(province.0, Action::Develop(Kind::Manpower))?)),
        ProvinceRequest::ExploitTax { province } => Ok(ProvinceAnswer::ExploitTax(game.province_act(province.0, Action::Exploit(Kind::Tax))?)),
        ProvinceRequest::ExploitProduction { province } => {
            Ok(ProvinceAnswer::ExploitProduction(game.province_act(province.0, Action::Exploit(Kind::Production))?))
        }
        ProvinceRequest::ExploitManpower { province } => Ok(ProvinceAnswer::ExploitManpower(game.province_act(province.0, Action::Exploit(Kind::Manpower))?)),
        ProvinceRequest::RaiseAutonomy { province } => Ok(ProvinceAnswer::RaiseAutonomy(game.province_act(province.0, Action::RaiseAutonomy)?)),
        ProvinceRequest::LowerAutonomy { province } => Ok(ProvinceAnswer::LowerAutonomy(game.province_act(province.0, Action::LowerAutonomy)?)),
        ProvinceRequest::MakeState { province } => Ok(ProvinceAnswer::MakeState(game.province_act(province.0, Action::MakeState)?)),
        ProvinceRequest::MakeTerritory { province } => Ok(ProvinceAnswer::MakeTerritory(game.province_act(province.0, Action::MakeTerritory)?)),
        ProvinceRequest::SetEdict { province, edict } => Ok(ProvinceAnswer::SetEdict(game.set_edict(province.0, edict)?)),
        ProvinceRequest::CancelEdict { province } => Ok(ProvinceAnswer::CancelEdict(game.province_act(province.0, Action::CancelEdict)?)),
        ProvinceRequest::Core { province } => Ok(ProvinceAnswer::Core(game.province_act(province.0, Action::Core)?)),
        ProvinceRequest::CancelCore { province } => Ok(ProvinceAnswer::CancelCore(game.province_act(province.0, Action::CancelCore)?)),
        ProvinceRequest::ChangeCulture { province } => Ok(ProvinceAnswer::ChangeCulture(game.province_act(province.0, Action::ChangeCulture)?)),
        ProvinceRequest::CancelCulture { province } => Ok(ProvinceAnswer::CancelCulture(game.province_act(province.0, Action::CancelCulture)?)),
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
