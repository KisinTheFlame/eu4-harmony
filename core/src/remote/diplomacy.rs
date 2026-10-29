//! Diplomacy: the player's diplomatic actions. The capability layer sends
//! one at once; the answer comes at a later safe point, once the game has
//! carried it out and the other country has answered, or after
//! [`WAITED`] as `pending`.
use super::{countries, tag, Live, Outcome as Ran, Poll, Wait};
use crate::game::{
    diplomacy::{self as layer, Answer, Choice, Ticket, NO_CASUS_BELLI as NONE},
    Caller, GResult, Tag,
};
use eu4_harmony_remote::{
    diplomacy::{AnswerKind, DiplomacyAnswer, DiplomacyRequest, Outcome, WarGoal},
    Answer as Answered, Error, Response,
};
use std::time::{Duration, Instant};

/// How long an action waits for what comes of it before it answers
/// `pending`. A single player game carries an action out and has the
/// other country answer within a few frames.
pub const WAITED: Duration = Duration::from_secs(5);

/// The group's slice of the capability layer.
pub trait Layer {
    fn war_goals(&mut self, enemy: Tag) -> GResult<Vec<layer::WarGoal>>;
    fn declare_war(&mut self, enemy: Tag, choice: &Choice) -> GResult<Ticket>;
    fn offer_peace(&mut self, enemy: Tag) -> GResult<Ticket>;
    fn propose_alliance(&mut self, ally: Tag) -> GResult<Ticket>;
    fn answer(&mut self, ticket: Ticket) -> GResult<Answer>;
}

impl Layer for Live {
    fn war_goals(&mut self, enemy: Tag) -> GResult<Vec<layer::WarGoal>> {
        layer::war_goals(Caller::Remote, enemy)
    }

    fn declare_war(&mut self, enemy: Tag, choice: &Choice) -> GResult<Ticket> {
        layer::declare_war(Caller::Remote, enemy, choice)
    }

    fn offer_peace(&mut self, enemy: Tag) -> GResult<Ticket> {
        layer::offer_peace(Caller::Remote, enemy)
    }

    fn propose_alliance(&mut self, ally: Tag) -> GResult<Ticket> {
        layer::propose_alliance(Caller::Remote, ally)
    }

    fn answer(&mut self, ticket: Ticket) -> GResult<Answer> {
        layer::answer(Caller::Remote, ticket)
    }
}

pub fn run(request: DiplomacyRequest, game: &mut (impl Layer + countries::Layer + ?Sized)) -> Ran {
    let sent = match request {
        DiplomacyRequest::WarGoals { country } => {
            return Ran::Done((|| {
                let enemy = game.country(country)?;
                Ok(Answered::Diplomacy(DiplomacyAnswer::WarGoals(game.war_goals(enemy)?.iter().map(war_goal).collect())))
            })())
        }
        DiplomacyRequest::DeclareWar { country, casus_belli, war_goal, province, target } => (|| {
            let enemy = game.country(country)?;
            let target = target.map(|t| game.country(t)).transpose()?;
            let choice = Choice { casus_belli: (casus_belli != NONE).then_some(casus_belli), war_goal, province: province.map(|p| p.0), target };
            Ok((game.declare_war(enemy, &choice)?, Kind::DeclareWar))
        })(),
        DiplomacyRequest::OfferPeace { country } => (|| {
            let enemy = game.country(country)?;
            Ok((game.offer_peace(enemy)?, Kind::OfferPeace))
        })(),
        DiplomacyRequest::ProposeAlliance { country } => (|| {
            let ally = game.country(country)?;
            Ok((game.propose_alliance(ally)?, Kind::ProposeAlliance))
        })(),
    };
    match sent {
        Ok((ticket, kind)) => Ran::Later(Box::new(Waiting { ticket, kind, until: Instant::now() + WAITED })),
        Err(e) => Ran::Done(Err(e)),
    }
}

fn war_goal(g: &layer::WarGoal) -> WarGoal {
    WarGoal {
        casus_belli: g.casus_belli.clone().unwrap_or_else(|| NONE.into()),
        war_goal: g.war_goal.clone(),
        province: g.province.map(eu4_harmony_remote::ProvinceId),
        target: g.target.map(tag),
        name: g.name.clone(),
    }
}

/// Which capability sent the action, for the answer's variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    DeclareWar,
    OfferPeace,
    ProposeAlliance,
}

/// An action sent, waiting for what comes of it.
struct Waiting {
    ticket: Ticket,
    kind: Kind,
    until: Instant,
}

impl Waiting {
    fn respond(&self, outcome: Outcome) -> Response {
        let answer = match self.kind {
            Kind::DeclareWar => DiplomacyAnswer::DeclareWar(outcome),
            Kind::OfferPeace => DiplomacyAnswer::OfferPeace(outcome),
            Kind::ProposeAlliance => DiplomacyAnswer::ProposeAlliance(outcome),
        };
        Ok(Answered::Diplomacy(answer))
    }
}

impl Wait for Waiting {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response> {
        let pending = Outcome { answer: AnswerKind::Pending, war: None };
        let Some(game) = at.game.as_deref_mut() else {
            return (at.now >= self.until).then(|| self.respond(pending));
        };
        let outcome = match game.answer(self.ticket) {
            Ok(Answer::Pending) if at.now < self.until => return None,
            Ok(Answer::Pending) => pending,
            Ok(Answer::Accepted) => Outcome { answer: AnswerKind::Accepted, war: None },
            Ok(Answer::Declined) => Outcome { answer: AnswerKind::Declined, war: None },
            Ok(Answer::Declared(war)) => Outcome { answer: AnswerKind::Declared, war: Some(war) },
            Err(fail) => return Some(Err(Error::from(fail))),
        };
        Some(self.respond(outcome))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{events, fake::Fake, Engine, Place, Reply, Request};
    use super::*;
    use crate::game::{Fail, Status};
    use eu4_harmony_remote::{response_json, CountryTag, ProvinceId};
    use serde_json::{json, Value};

    fn quiet(_: &str) {}

    fn answer(reply: &Reply) -> Value {
        reply.wait(Duration::ZERO).map_or(Value::Null, |r| response_json(&r))
    }

    fn byz() -> CountryTag {
        CountryTag(*b"BYZ")
    }

    #[test]
    fn the_war_goals_offered_are_listed_the_declaration_without_one_as_none() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let reply = engine.submit(Request::Diplomacy(DiplomacyRequest::WarGoals { country: byz() }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(
            answer(&reply),
            json!({"ok": true, "result": [
                {"casus_belli": "cb_claim", "war_goal": "take_claim", "province": 151, "target": null, "name": "Conquer Constantinople"},
                {"casus_belli": "none", "war_goal": "take_province_no_cb", "province": null, "target": null, "name": "No casus belli"},
            ]})
        );
    }

    #[test]
    fn a_declaration_is_answered_once_the_war_began() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        game.answers.push_back(Ok(Answer::Pending));
        game.answers.push_back(Ok(Answer::Declared("Ottoman-Byzantine War".into())));
        let declare = DiplomacyRequest::DeclareWar { country: byz(), casus_belli: "cb_claim".into(), war_goal: None, province: Some(ProvinceId(151)), target: Some(CountryTag(*b"WAL")) };
        let reply = engine.submit(Request::Diplomacy(declare));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply), Value::Null);
        // Not while there is no game frame.
        engine.run(&mut game, &Place::Outside(None), Duration::from_secs(10));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply), Value::Null);
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply), json!({"ok": true, "result": {"answer": "declared", "war": "Ottoman-Byzantine War"}}));
        assert_eq!(game.calls[..3], ["country BYZ", "country WAL", "declare war 1000021005a5942 Some(\"cb_claim\") None Some(151) Some(72058882533114199)"]);
        assert_eq!(game.calls[3..], ["answer 1", "answer 1"]);

        // Without a casus belli.
        let reply = engine.submit(Request::Diplomacy(DiplomacyRequest::DeclareWar { country: byz(), casus_belli: "none".into(), war_goal: None, province: None, target: None }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert!(game.calls.iter().any(|c| c == "declare war 1000021005a5942 None None None None"), "{:?}", game.calls);
        drop(reply);
    }

    #[test]
    fn a_refusal_comes_at_once_with_the_game_s_reasons() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let reply = engine.submit(Request::Diplomacy(DiplomacyRequest::DeclareWar { country: CountryTag(*b"WAL"), casus_belli: "none".into(), war_goal: None, province: None, target: None }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply), json!({"ok": false, "error": {"code": "rejected", "message": "They are our subject.\nWe have a truce with them."}}));
        let reply = engine.submit(Request::Diplomacy(DiplomacyRequest::ProposeAlliance { country: CountryTag(*b"QQQ") }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply)["error"]["code"], "not_found");
    }

    #[test]
    fn a_proposal_is_answered_with_the_other_country_s_answer() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        game.answers.push_back(Ok(Answer::Accepted));
        game.answers.push_back(Ok(Answer::Declined));
        game.answers.push_back(Err(Fail { status: Status::NotInGame, message: "the game the diplomatic action was sent in has ended".into() }));
        let alliance = engine.submit(Request::Diplomacy(DiplomacyRequest::ProposeAlliance { country: byz() }));
        let peace = engine.submit(Request::Diplomacy(DiplomacyRequest::OfferPeace { country: byz() }));
        let again = engine.submit(Request::Diplomacy(DiplomacyRequest::OfferPeace { country: byz() }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&alliance), json!({"ok": true, "result": {"answer": "accepted"}}));
        assert_eq!(answer(&peace), json!({"ok": true, "result": {"answer": "declined"}}));
        assert_eq!(answer(&again)["error"]["code"], "not_in_game");
    }

    #[test]
    fn an_action_with_nothing_come_of_it_in_time_is_pending() {
        let mut game = Fake::default();
        let buffer = events::Buffer::new();
        let now = Instant::now();
        let mut waiting = Waiting { ticket: Ticket(3), kind: Kind::ProposeAlliance, until: now + WAITED };
        let mut poll = |game: Option<&mut Fake>, now| waiting.poll(&mut super::super::Poll { events: &buffer, now, game: game.map(|g| g as &mut dyn super::super::Game) });
        assert_eq!(poll(Some(&mut game), now), None);
        assert_eq!(poll(None, now), None);
        let late = poll(None, now + WAITED).map(|r| response_json(&r));
        assert_eq!(late, Some(json!({"ok": true, "result": {"answer": "pending"}})));
        let late = poll(Some(&mut game), now + WAITED).map(|r| response_json(&r));
        assert_eq!(late, Some(json!({"ok": true, "result": {"answer": "pending"}})));
    }
}
