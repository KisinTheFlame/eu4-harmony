//! Provinces, their neighbors, and one province's snapshot.
use super::{tag, Convert};
use crate::game::{
    self,
    administration::{Action, Development as Kind},
    provinces as layer,
};
use eu4_harmony_remote::{
    provinces::{Development, Fort, Neighbor, Province, ProvinceAnswer, ProvinceKind, ProvinceOption, ProvinceRequest, ProvinceSnapshot},
    recruiting::UnitKind,
    Error, ProvinceId, ProvinceList,
};

layer! {
    run ProvinceRequest => ProvinceAnswer;
    fn provinces() -> Vec<game::provinces::Province> = game::provinces::provinces;
    fn named_provinces(ids: Vec<u32>) -> Vec<game::provinces::Province> = game::provinces::named;
    fn neighbors(province: u32) -> Vec<game::provinces::Neighbor> = game::provinces::neighbors;
    fn province_act(province: u32, action: Action) -> () = game::administration::act;
    List { provinces } => list(game, provinces);
    Neighbors { province } => neighbors(game, province.0);
    Get => fn province(province: u32) -> layer::Snapshot = layer::province;
    Options => fn province_options(province: u32) -> Vec<ProvinceOption> = game::administration::options;
    DevelopTax { province } => game.province_act(province, Action::Develop(Kind::Tax));
    DevelopProduction { province } => game.province_act(province, Action::Develop(Kind::Production));
    DevelopManpower { province } => game.province_act(province, Action::Develop(Kind::Manpower));
    ExploitTax { province } => game.province_act(province, Action::Exploit(Kind::Tax));
    ExploitProduction { province } => game.province_act(province, Action::Exploit(Kind::Production));
    ExploitManpower { province } => game.province_act(province, Action::Exploit(Kind::Manpower));
    RaiseAutonomy { province } => game.province_act(province, Action::RaiseAutonomy);
    LowerAutonomy { province } => game.province_act(province, Action::LowerAutonomy);
    MakeState { province } => game.province_act(province, Action::MakeState);
    MakeTerritory { province } => game.province_act(province, Action::MakeTerritory);
    SetEdict => fn set_edict(province: u32, edict: String) -> () = game::administration::set_edict;
    CancelEdict { province } => game.province_act(province, Action::CancelEdict);
    Core { province } => game.province_act(province, Action::Core);
    CancelCore { province } => game.province_act(province, Action::CancelCore);
    ChangeCulture { province } => game.province_act(province, Action::ChangeCulture);
    CancelCulture { province } => game.province_act(province, Action::CancelCulture);
}

/// The provinces asked for, or every province; the layer's whole list starts
/// with a placeholder for number 0.
fn list(game: &mut (impl Layer + ?Sized), provinces: Option<ProvinceList>) -> Result<Vec<Province>, Error> {
    let chosen: Vec<game::provinces::Province> = match provinces {
        None => game.provinces()?.into_iter().filter(|p| p.id != 0).collect(),
        Some(list) => game.named_provinces(list.0.iter().map(|id| id.0).collect())?,
    };
    Ok(chosen
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
        .collect())
}

fn neighbors(game: &mut (impl Layer + ?Sized), province: u32) -> Result<Vec<Neighbor>, Error> {
    Ok(game.neighbors(province)?.iter().map(|n| Neighbor { province: ProvinceId(n.to), strait: n.strait }).collect())
}

impl Convert<Box<ProvinceSnapshot>> for layer::Snapshot {
    fn convert(self) -> Box<ProvinceSnapshot> {
        Box::new(snapshot(self))
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
impl Convert<layer::UnitKind> for UnitKind {
    fn convert(self) -> layer::UnitKind {
        match self {
            UnitKind::Infantry => layer::UnitKind::Infantry,
            UnitKind::Cavalry => layer::UnitKind::Cavalry,
            UnitKind::Artillery => layer::UnitKind::Artillery,
            UnitKind::HeavyShip => layer::UnitKind::HeavyShip,
            UnitKind::LightShip => layer::UnitKind::LightShip,
            UnitKind::Galley => layer::UnitKind::Galley,
            UnitKind::Transport => layer::UnitKind::Transport,
        }
    }
}

/// A unit kind as remote clients name it.
impl Convert<UnitKind> for layer::UnitKind {
    fn convert(self) -> UnitKind {
        match self {
            layer::UnitKind::Infantry => UnitKind::Infantry,
            layer::UnitKind::Cavalry => UnitKind::Cavalry,
            layer::UnitKind::Artillery => UnitKind::Artillery,
            layer::UnitKind::HeavyShip => UnitKind::HeavyShip,
            layer::UnitKind::LightShip => UnitKind::LightShip,
            layer::UnitKind::Galley => UnitKind::Galley,
            layer::UnitKind::Transport => UnitKind::Transport,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        fake::{answer, ok, quiet, Fake, PLENTY},
        Place,
    };
    use crate::remote::Engine;
    use eu4_harmony_remote::{provinces::ProvinceRequest, recruiting::RecruitRequest, response_json, ProvinceId, Request, Response};
    use serde_json::{json, Value};
    use std::time::Duration;

    #[test]
    fn a_province_snapshot_carries_its_name_s_characters_and_every_kind() {
        let engine = Engine::new(quiet);
        let reply = engine.submit(Request::Province(ProvinceRequest::Get { province: ProvinceId(151) }));
        engine.run(&mut Fake::default(), &Place::Game, PLENTY);
        let recruit = |kind: &str, unit_type: Value, reasons: Value| json!({"kind": kind, "unit_type": unit_type, "allowed": false, "reasons": reasons});
        let not_ours = json!(["君士坦丁堡不是我们的领土"]);
        let expected = ok(json!({
            "id": 151, "name": "君士坦丁堡", "owner": "BYZ", "controller": "BYZ",
            "development": {"tax": 10.0, "production": 9.5, "manpower": 6.0},
            "terrain": "farmlands", "fort": {"level": 2, "mothballed": false},
            "buildings": ["fort_15th", "marketplace"],
            "autonomy": 25.0, "culture": "greek", "religion": "orthodox", "trade_good": "silk", "unrest": null,
            "trade_node": "constantinople", "center_of_trade": 3, "trade_company": false, "state": "state", "edict": null,
            "cores": ["BYZ"], "claims": ["TUR"], "constructions": null, "missionary": null, "colony_size": null,
        }));
        assert_eq!(answer(&reply), expected);
        // What can be recruited there is recruit.options', every kind in order.
        let options = engine.submit(Request::Recruit(RecruitRequest::Options { province: ProvinceId(151) }));
        engine.run(&mut Fake::default(), &Place::Game, PLENTY);
        assert_eq!(
            answer(&options),
            ok(json!([
                recruit("infantry", json!("eastern_medieval_infantry"), not_ours.clone()),
                recruit("cavalry", json!("eastern_knights"), not_ours),
                recruit("artillery", Value::Null, json!([])),
                recruit("heavy_ship", json!("early_carrack"), json!([])),
                recruit("light_ship", json!("early_war_galley"), json!([])),
                recruit("galley", json!("galley"), json!([])),
                recruit("transport", json!("cog"), json!([])),
            ]))
        );
        // On the pipe the name is its characters, not escapes.
        let line = response_json(&reply_again(&engine)).to_string();
        assert!(line.contains(r#""name":"君士坦丁堡""#), "{line}");
    }

    /// The snapshot of 151 asked once more.
    fn reply_again(engine: &Engine) -> Response {
        let reply = engine.submit(Request::Province(ProvinceRequest::Get { province: ProvinceId(151) }));
        engine.run(&mut Fake::default(), &Place::Game, PLENTY);
        reply.wait(Duration::ZERO).unwrap()
    }
}
