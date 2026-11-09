//! Peace: the peace view. The terms and their evaluation answer at once; a
//! demand or an offer is sent at once and answered as the diplomatic actions
//! are ([`super::diplomacy::waiting`]).
use super::{diplomacy, Outcome as Ran};
use crate::game::{diplomacy::Ticket, peace as layer, GResult};
use eu4_harmony_remote::peace::args;
use eu4_harmony_remote::{
    peace::{Evaluation, PeaceAnswer, PeaceRequest, TermOption},
    Answer as Answered, Error,
};
use std::time::Instant;

layer! {
    fn peace_terms(asked: args::TermOptions) -> Vec<TermOption> = session layer::terms;
    fn evaluate_peace(asked: args::Evaluate) -> Evaluation = session layer::evaluate;
    fn demand_peace(asked: args::Demand) -> Ticket = session layer::demand;
    fn offer_terms(asked: args::Offer) -> Ticket = session layer::offer;
}

pub fn run(request: PeaceRequest, game: &mut (impl Layer + ?Sized), now: Instant) -> Ran {
    let answered = |answer: GResult<PeaceAnswer>| Ran::Done(answer.map(Answered::Peace).map_err(Error::from));
    let sent = match request {
        PeaceRequest::TermOptions { country, offer } => return answered(game.peace_terms(args::TermOptions { country, offer }).map(PeaceAnswer::TermOptions)),
        PeaceRequest::Evaluate { country, terms, offer } => {
            return answered(game.evaluate_peace(args::Evaluate { country, terms, offer }).map(PeaceAnswer::Evaluate))
        }
        PeaceRequest::Demand { country, terms } => {
            game.demand_peace(args::Demand { country, terms }).map(|ticket| (ticket, (|o| Answered::Peace(PeaceAnswer::Demand(o))) as diplomacy::Answers))
        }
        PeaceRequest::Offer { country, terms } => {
            game.offer_terms(args::Offer { country, terms }).map(|ticket| (ticket, (|o| Answered::Peace(PeaceAnswer::Offer(o))) as diplomacy::Answers))
        }
    };
    match sent {
        Ok((ticket, answers)) => diplomacy::waiting(ticket, answers, now),
        Err(fail) => Ran::Done(Err(fail.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        diplomacy,
        fake::{answer, quiet, Fake},
        menu, Place,
    };
    use crate::game::diplomacy::Answer;
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
    fn the_terms_are_listed_with_their_choice_and_cost() {
        let mut game = Fake::default();
        assert_eq!(
            run(&mut game, r#"{"capability": "peace.terms", "country": "BYZ"}"#),
            json!({"ok": true, "result": [
                {"term": "demand_provinces", "province": 151, "country": "TUR", "region": null, "amount": null, "name": "Constantinople",
                 "can": true, "reasons": [], "cost": [{"resource": "war_score", "amount": 40.0}]},
                {"term": "humiliate", "province": null, "country": null, "region": null, "amount": null, "name": "Humiliate",
                 "can": false, "reasons": ["Not our rival"], "cost": [{"resource": "war_score", "amount": 10.0}]},
            ]})
        );
        assert_eq!(run(&mut game, r#"{"capability": "peace.terms", "country": "WAL", "offer": true}"#)["error"]["code"], "rejected");
        assert_eq!(game.calls[0], "peace terms BYZ false");
        assert!(game.calls.iter().any(|c| c.ends_with(" true")));
    }

    #[test]
    fn terms_are_evaluated_and_sent_with_their_countries_found() {
        let mut game = Fake::default();
        let evaluated = run(
            &mut game,
            r#"{"capability": "peace.evaluate", "country": "BYZ", "terms": [{"term": "demand_provinces", "province": 151, "country": "TUR"}, {"term": "gold", "amount": 25.5}]}"#,
        );
        assert_eq!(
            evaluated,
            json!({"ok": true, "result": {"can": true, "reasons": [], "cost": [{"resource": "war_score", "amount": 40.0}],
                "acceptance": {"accepts": false, "total": -5.0, "factors": [{"text": "War score", "value": -5.0}]}}})
        );
        let evaluated = "evaluate peace BYZ demand_provinces:Some(151):Some(\"TUR\"):None,gold:None:None:Some(25.5) false".to_string();
        assert!(game.calls.contains(&evaluated), "{:?}", game.calls);
        // A country named that there is not.
        let missing = run(&mut game, r#"{"capability": "peace.evaluate", "country": "BYZ", "terms": [{"term": "release_vassals", "country": "QQQ"}]}"#);
        assert_eq!(missing["error"]["code"], "not_found");

        let engine = Engine::new(quiet);
        game.answers.push_back(Ok(Answer::Declined));
        let demand = engine.submit(request_from_json(r#"{"capability": "peace.demand", "country": "BYZ", "terms": [{"term": "humiliate"}]}"#).unwrap());
        let offer = engine.submit(request_from_json(r#"{"capability": "peace.offer", "country": "BYZ", "terms": [{"term": "concede_defeat"}]}"#).unwrap());
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&demand), json!({"ok": true, "result": {"answer": "declined"}}));
        engine.end(menu::ending(), Duration::ZERO);
        assert_eq!(answer(&offer), json!({"ok": true, "result": {"answer": "pending"}}));
        assert!(game.calls.contains(&"demand peace BYZ humiliate:None:None:None".to_string()));
        assert!(game.calls.contains(&"offer terms BYZ concede_defeat:None:None:None".to_string()));
        let _ = diplomacy::WAITED;
    }
}
