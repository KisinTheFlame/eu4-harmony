//! Recruiting regiments and ships: what can be recruited in a province, and
//! ordering it.
use super::Convert;
use crate::game::{self};
use eu4_harmony_remote::{
    recruiting::{Recruit, RecruitAnswer, RecruitRequest},
    Error,
};
use eu4_harmony_remote::recruiting::args;

layer! {
    run RecruitRequest => RecruitAnswer;
    fn province_snapshot(province: u32) -> game::provinces::Snapshot = session game::provinces::acting_province;
    Options { province } => options(game, province.0);
    Order => fn recruit -> () = game::recruiting::recruit;
}

/// What can be recruited in the province, and whether it can now.
fn options(game: &mut (impl Layer + ?Sized), province: u32) -> Result<Vec<Recruit>, Error> {
    Ok(game
        .province_snapshot(province)?
        .recruit
        .into_iter()
        .map(|r| Recruit { kind: r.kind.convert(), unit_type: r.unit_type, allowed: r.allowed, reasons: r.reasons })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::super::{
        fake::{answer, ok, quiet, Fake, PLENTY},
        Place,
    };
    use crate::{game, remote::Engine};
    use eu4_harmony_remote::Code;
    use serde_json::{json, Value};

    #[test]
    fn a_recruit_order_from_json_reaches_the_layer_with_its_kind() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let replies: Vec<_> = ["infantry", "cavalry", "artillery", "heavy_ship", "light_ship", "galley", "transport"]
            .iter()
            .map(|kind| {
                engine
                    .submit(eu4_harmony_remote::request_from_json(&json!({"capability": "recruit.order", "province": 149, "kind": kind}).to_string()).unwrap())
            })
            .collect();
        engine.run(&mut game, &Place::Game, PLENTY);
        assert!(replies.iter().all(|r| answer(r) == ok(Value::Null)));
        let keys: Vec<String> = game::provinces::UnitKind::ALL.iter().map(|k| format!("recruit 149 {}", k.key())).collect();
        assert_eq!(game.calls, keys);
        let unknown = eu4_harmony_remote::request_from_json(r#"{"capability": "recruit", "province": 149, "kind": "marines"}"#).unwrap_err();
        assert_eq!(unknown.code, Code::BadRequest);
    }
}
