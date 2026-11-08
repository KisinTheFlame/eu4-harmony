//! Occupation: handing a province the player occupies over to an ally.
use super::{countries, tag, Convert};
use crate::game::{
    occupation::{self as layer, Transfers as Offered},
    Tag,
};
use eu4_harmony_remote::{
    occupation::{OccupationAnswer, OccupationRequest, Transferred, Transfers},
    CountryTag, Error,
};
use eu4_harmony_remote::occupation::args;

layer! {
    run OccupationRequest => OccupationAnswer + countries::Layer;
    fn transfers(province: u32) -> Offered = session layer::options;
    fn transfer_occupation(province: u32, ally: Tag) -> Option<Tag> = session layer::transfer;
    Options { province } => game.transfers(province);
    Transfer { province, country } => transfer(game, province.0, country);
}

impl Convert<Transfers> for Offered {
    fn convert(self) -> Transfers {
        Transfers { choice: self.choice, allies: self.allies.into_iter().map(tag).collect() }
    }
}

/// Hands the province over to the ally; who controls it now.
fn transfer(game: &mut (impl Layer + countries::Layer + ?Sized), province: u32, ally: CountryTag) -> Result<Transferred, Error> {
    let ally = game.country(ally)?;
    Ok(Transferred { controller: game.transfer_occupation(province, ally)?.map(tag) })
}

#[cfg(test)]
mod tests {
    use super::super::{
        fake::{answer, quiet, Fake},
        Place,
    };
    use crate::remote::Engine;
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
