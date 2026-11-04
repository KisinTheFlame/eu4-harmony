//! Occupation: handing a province the player occupies over to an ally.
use super::{countries, tag};
use crate::game::{
    occupation::{self as layer, Transfers as Offered},
    Tag,
};
use eu4_harmony_remote::{
    occupation::{OccupationAnswer, OccupationRequest, Transferred, Transfers},
    Error,
};

layer! {
    fn transfers(province: u32) -> Offered = layer::options;
    fn transfer_occupation(province: u32, ally: Tag) -> Option<Tag> = layer::transfer;
}

pub fn run(request: OccupationRequest, game: &mut (impl Layer + countries::Layer + ?Sized)) -> Result<OccupationAnswer, Error> {
    Ok(match request {
        OccupationRequest::Options { province } => {
            let offered = game.transfers(province.0)?;
            OccupationAnswer::Options(Transfers { choice: offered.choice, allies: offered.allies.into_iter().map(tag).collect() })
        }
        OccupationRequest::Transfer { province, country } => {
            let ally = game.country(country)?;
            OccupationAnswer::Transfer(Transferred { controller: game.transfer_occupation(province.0, ally)?.map(tag) })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        fake::{answer, quiet, Fake},
        Engine, Place,
    };
    use eu4_harmony_remote::request_from_json;
    use serde_json::json;
    use std::time::Duration;

    fn run(game: &mut Fake, json: &str) -> serde_json::Value {
        let engine = Engine::new(quiet);
        let reply = engine.submit(request_from_json(json).unwrap());
        engine.run(game, &Place::Game, Duration::from_secs(10));
        answer(&reply)
    }

    #[test]
    fn the_allies_to_take_a_province_and_the_hand_over() {
        let mut game = Fake::default();
        assert_eq!(
            run(&mut game, r#"{"capability": "occupation.options", "province": 151}"#),
            json!({"ok": true, "result": {"can": true, "reasons": [], "cost": [], "allies": ["WAL"]}})
        );
        assert_eq!(
            run(&mut game, r#"{"capability": "occupation.options", "province": 149}"#),
            json!({"ok": true, "result": {"can": false, "reasons": ["We are not the controller"], "cost": [], "allies": []}})
        );
        assert_eq!(
            run(&mut game, r#"{"capability": "occupation.transfer", "province": 151, "country": "WAL"}"#),
            json!({"ok": true, "result": {"controller": "WAL"}})
        );
        assert_eq!(game.calls.last().unwrap(), "transfer occupation 151 100012c004c4157");
    }
}
