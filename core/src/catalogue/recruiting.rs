//! Recruiting regiments and ships: what can be recruited in a province, and
//! ordering it.
use eu4_harmony_remote::recruiting::args;
use eu4_harmony_remote::recruiting::{Recruit, RecruitAnswer, RecruitRequest};

layer! {
    run RecruitRequest => RecruitAnswer;
    Options => fn recruit_options -> Vec<Recruit> = crate::game::recruiting::options;
    Order => fn recruit -> () = crate::game::recruiting::recruit;
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
