//! Provinces, their neighbors, and one province's snapshot.
use crate::game::{
    self,
    administration::{Action, Development as Kind},
};
use eu4_harmony_remote::provinces::args;
use eu4_harmony_remote::countries::ModifierValue;
use eu4_harmony_remote::provinces::{Neighbor, Province, ProvinceAnswer, ProvinceOption, ProvinceRequest, ProvinceSnapshot};

layer! {
    run ProvinceRequest => ProvinceAnswer;
    fn province_act(province: u32, action: Action) -> () = session game::administration::act;
    List => fn provinces -> Vec<Province> = game::provinces::list;
    Neighbors => fn neighbors -> Vec<Neighbor> = game::provinces::neighbors;
    Get => fn province -> Box<ProvinceSnapshot> = game::provinces::province;
    Modifiers => fn province_modifiers -> Vec<ModifierValue> = game::provinces::modifiers;
    Options => fn province_options -> Vec<ProvinceOption> = game::administration::options;
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
    SetEdict => fn set_edict -> () = game::administration::set_edict;
    CancelEdict { province } => game.province_act(province, Action::CancelEdict);
    Core { province } => game.province_act(province, Action::Core);
    CancelCore { province } => game.province_act(province, Action::CancelCore);
    ChangeCulture { province } => game.province_act(province, Action::ChangeCulture);
    CancelCulture { province } => game.province_act(province, Action::CancelCulture);
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
            "cores": ["BYZ"], "claims": ["TUR"], "constructions": null, "missionary": null, "colony_size": null, "named_modifiers": [],
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
