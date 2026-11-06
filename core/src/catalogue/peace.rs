//! Peace: the peace view. The terms and their evaluation answer at once; a
//! demand or an offer is sent at once and answered as the diplomatic actions
//! are ([`super::diplomacy::waiting`]).
use super::{countries, diplomacy, tag, Outcome as Ran};
use crate::game::{
    diplomacy::Ticket,
    peace::{self as layer, Evaluation as Evaluated, Term, TermOption as Offered},
    GResult, Tag,
};
use eu4_harmony_remote::{
    peace::{self as remote, Evaluation, PeaceAnswer, PeaceRequest, TermOption},
    Answer as Answered, ProvinceId,
};
use std::time::Instant;

layer! {
    fn peace_terms(enemy: Tag, offer: bool) -> Vec<Offered> = caller layer::terms;
    fn evaluate_peace(enemy: Tag, terms: &[Term], offer: bool) -> Evaluated = caller layer::evaluate;
    fn demand_peace(enemy: Tag, terms: &[Term]) -> Ticket = caller layer::demand;
    fn offer_terms(enemy: Tag, terms: &[Term]) -> Ticket = caller layer::offer;
}

pub fn run(request: PeaceRequest, game: &mut (impl Layer + countries::Layer + ?Sized), now: Instant) -> Ran {
    let sent = match request {
        PeaceRequest::Terms { country, offer } => {
            return Ran::Done((|| {
                let enemy = game.country(country)?;
                Ok(Answered::Peace(PeaceAnswer::Terms(game.peace_terms(enemy, offer.unwrap_or(false))?.into_iter().map(option).collect())))
            })())
        }
        PeaceRequest::Evaluate { country, terms, offer } => {
            return Ran::Done((|| {
                let enemy = game.country(country)?;
                let terms = asked(terms, game)?;
                let evaluated = game.evaluate_peace(enemy, &terms, offer.unwrap_or(false))?;
                Ok(Answered::Peace(PeaceAnswer::Evaluate(Evaluation { choice: evaluated.choice, acceptance: evaluated.acceptance })))
            })())
        }
        PeaceRequest::Demand { country, terms } => (|| {
            let enemy = game.country(country)?;
            let terms = asked(terms, game)?;
            Ok((game.demand_peace(enemy, &terms)?, (|o| Answered::Peace(PeaceAnswer::Demand(o))) as diplomacy::Answers))
        })(),
        PeaceRequest::Offer { country, terms } => (|| {
            let enemy = game.country(country)?;
            let terms = asked(terms, game)?;
            Ok((game.offer_terms(enemy, &terms)?, (|o| Answered::Peace(PeaceAnswer::Offer(o))) as diplomacy::Answers))
        })(),
    };
    match sent {
        Ok((ticket, answers)) => diplomacy::waiting(ticket, answers, now),
        Err(e) => Ran::Done(Err(e)),
    }
}

/// The terms a request names, its countries found in the game.
fn asked(terms: remote::Terms, game: &mut (impl countries::Layer + ?Sized)) -> GResult<Vec<Term>> {
    terms
        .0
        .into_iter()
        .map(|t| {
            Ok(Term {
                term: t.term,
                province: t.province.map(|p| p.0),
                country: t.country.map(|c| game.country(c)).transpose()?,
                region: t.region,
                amount: t.amount,
            })
        })
        .collect()
}

fn option(o: Offered) -> TermOption {
    TermOption {
        term: o.term,
        province: o.province.map(ProvinceId),
        country: o.country.map(tag),
        region: o.region,
        amount: o.amount,
        name: o.name,
        choice: o.choice,
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
        assert_eq!(game.calls[1], "peace terms 1000021005a5942 false");
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
        let evaluated =
            format!("evaluate peace 1000021005a5942 demand_provinces:Some(151):Some({}):None,gold:None:None:Some(25.5) false", super::super::fake::tur().0);
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
        assert!(game.calls.contains(&"demand peace 1000021005a5942 humiliate:None:None:None".to_string()));
        assert!(game.calls.contains(&"offer terms 1000021005a5942 concede_defeat:None:None:None".to_string()));
        let _ = diplomacy::WAITED;
    }
}
