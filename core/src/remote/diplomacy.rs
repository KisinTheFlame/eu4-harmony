//! Diplomacy: the player's diplomatic actions. The capability layer sends
//! one at once; the answer comes at a later safe point, once the game has
//! carried it out and the other country has answered, or as `pending` after
//! [`WAITED`] or when the game returns to the main menu first.
use super::{countries, events, tag, Live, Outcome as Ran, Poll, Wait};
use crate::game::{
    diplomacy::{self as layer, Answer, Calls, Choice, Declaration, Ticket, NO_CASUS_BELLI as NONE},
    Caller, Fail, GResult, Status, Tag,
};
use eu4_harmony_remote::{
    diplomacy::{AnswerKind, DiplomacyAnswer, DiplomacyRequest, Outcome, PassedOver, WarGoal},
    Answer as Answered, CountryTag, Error, Response,
};
use std::time::{Duration, Instant};

/// How long an action waits for what comes of it before it answers
/// `pending`. A single player game carries an action out and has the
/// other country answer within a few frames.
pub const WAITED: Duration = Duration::from_secs(5);

/// The group's slice of the capability layer.
pub trait Layer {
    fn war_goals(&mut self, enemy: Tag) -> GResult<Vec<layer::WarGoal>>;
    fn declare_war(&mut self, enemy: Tag, choice: &Choice, calls: &Calls) -> GResult<Declaration>;
    fn offer_peace(&mut self, enemy: Tag) -> GResult<Ticket>;
    fn propose_alliance(&mut self, ally: Tag) -> GResult<Ticket>;
    fn answer(&mut self, ticket: Ticket) -> GResult<Answer>;
}

impl Layer for Live {
    fn war_goals(&mut self, enemy: Tag) -> GResult<Vec<layer::WarGoal>> {
        layer::war_goals(Caller::Remote, enemy)
    }

    fn declare_war(&mut self, enemy: Tag, choice: &Choice, calls: &Calls) -> GResult<Declaration> {
        layer::declare_war(Caller::Remote, enemy, choice, calls)
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
        DiplomacyRequest::DeclareWar { country, casus_belli, war_goal, province, target, call_allies } => (|| {
            let enemy = game.country(country)?;
            let target = target.map(|t| game.country(t)).transpose()?;
            let choice = Choice { casus_belli: (casus_belli != NONE).then_some(casus_belli), war_goal, province: province.map(|p| p.0), target };
            let calls = calls(call_allies.as_deref(), game)?;
            let declared = game.declare_war(enemy, &choice, &calls)?;
            let passed_over = declared.passed_over.into_iter().map(|p| PassedOver { country: tag(p.country), reasons: p.reasons }).collect();
            Ok((declared.ticket, Kind::DeclareWar, Some((declared.called.into_iter().map(tag).collect(), passed_over))))
        })(),
        DiplomacyRequest::OfferPeace { country } => (|| {
            let enemy = game.country(country)?;
            Ok((game.offer_peace(enemy)?, Kind::OfferPeace, None))
        })(),
        DiplomacyRequest::ProposeAlliance { country } => (|| {
            let ally = game.country(country)?;
            Ok((game.propose_alliance(ally)?, Kind::ProposeAlliance, None))
        })(),
    };
    match sent {
        Ok((ticket, kind, called)) => Ran::Later(Box::new(Waiting { ticket, kind, called, until: Instant::now() + WAITED })),
        Err(e) => Ran::Done(Err(e)),
    }
}

/// The allies a declaration calls, from its `call_allies`: `all` (or left
/// out), `none`, or tags separated by commas.
fn calls(text: Option<&str>, game: &mut (impl countries::Layer + ?Sized)) -> GResult<Calls> {
    match text.map(str::trim) {
        None | Some("all") => Ok(Calls::All),
        Some("none") => Ok(Calls::None),
        Some(list) => {
            let mut tags = Vec::new();
            for word in list.split(',').map(str::trim) {
                let Some(tag) = CountryTag::parse(word) else {
                    return Err(Fail {
                        status: Status::InvalidArgument,
                        message: format!("call_allies is all, none, or country tags separated by commas; {word:?} is not a tag"),
                    });
                };
                tags.push(game.country(tag)?);
            }
            Ok(Calls::Only(tags))
        }
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
    /// The allies a declaration called, and those it passed over.
    called: Option<(Vec<CountryTag>, Vec<PassedOver>)>,
    until: Instant,
}

impl Waiting {
    fn pending(&self) -> Response {
        self.respond(AnswerKind::Pending, None)
    }

    fn respond(&self, answer: AnswerKind, war: Option<String>) -> Response {
        let (called, passed_over) = match self.called.clone() {
            Some((called, passed_over)) => (Some(called), Some(passed_over)),
            None => (None, None),
        };
        let outcome = Outcome { answer, war, called, passed_over };
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
        let now = at.now;
        let Some(game) = at.game() else {
            return (now >= self.until).then(|| self.pending());
        };
        let (answer, war) = match game.answer(self.ticket) {
            Ok(Answer::Pending) if now < self.until => return None,
            Ok(Answer::Pending) => (AnswerKind::Pending, None),
            Ok(Answer::Accepted) => (AnswerKind::Accepted, None),
            Ok(Answer::Declined) => (AnswerKind::Declined, None),
            Ok(Answer::Declared(war)) => (AnswerKind::Declared, Some(war)),
            Err(fail) => return Some(Err(Error::from(fail))),
        };
        Some(self.respond(answer, war))
    }

    /// The game is returning to the main menu, which starts `eu4.exe` again:
    /// the action was sent, and what comes of it can no longer be seen.
    fn ending(&mut self, _: &events::Buffer) -> Option<Response> {
        Some(self.pending())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{events, fake::Fake, menu, Engine, Place, Reply, Request};
    use super::*;
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
                {"casus_belli": "none", "war_goal": "nowargoal", "province": null, "target": null, "name": "No War Goal"},
            ]})
        );
    }

    #[test]
    fn a_declaration_is_answered_once_the_war_began() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        game.answers.push_back(Ok(Answer::Pending));
        game.answers.push_back(Ok(Answer::Declared("Ottoman-Byzantine War".into())));
        let declare = DiplomacyRequest::DeclareWar {
            country: byz(),
            casus_belli: "cb_claim".into(),
            war_goal: None,
            province: Some(ProvinceId(151)),
            target: Some(CountryTag(*b"WAL")),
            call_allies: None,
        };
        let reply = engine.submit(Request::Diplomacy(declare));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply), Value::Null);
        // Not while there is no game frame.
        engine.run(&mut game, &Place::Outside(None), Duration::from_secs(10));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply), Value::Null);
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(
            answer(&reply),
            json!({"ok": true, "result": {"answer": "declared", "war": "Ottoman-Byzantine War", "called": ["WAL"], "passed_over": [{"country": "TUR", "reasons": ["They would not accept."]}]}})
        );
        assert_eq!(
            game.calls[..3],
            ["country BYZ", "country WAL", "declare war 1000021005a5942 Some(\"cb_claim\") None Some(151) Some(72058882533114199) calling All"]
        );
        assert_eq!(game.calls[3..], ["answer 1", "answer 1"]);

        // Without a casus belli, calling no ally: pending, none called.
        let declare = |engine: &Engine, call_allies: &str| {
            let declare = DiplomacyRequest::DeclareWar {
                country: byz(),
                casus_belli: "none".into(),
                war_goal: None,
                province: None,
                target: None,
                call_allies: Some(call_allies.into()),
            };
            engine.submit(Request::Diplomacy(declare))
        };
        let reply = declare(&engine, "none");
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert!(game.calls.iter().any(|c| c == "declare war 1000021005a5942 None None None None calling None"), "{:?}", game.calls);
        engine.end(menu::ending(), Duration::ZERO);
        assert_eq!(answer(&reply), json!({"ok": true, "result": {"answer": "pending", "called": [], "passed_over": []}}));

        // Allies by tag, in either case and with spaces; a word that is no
        // tag, and a tag of no country. In a process of its own, since the
        // engine above has ended.
        let engine = Engine::new(quiet);
        let replies = [declare(&engine, "wal, TUR"), declare(&engine, "WAL;TUR"), declare(&engine, "WAL,QQQ")];
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        engine.end(menu::ending(), Duration::ZERO);
        assert!(game.calls.iter().any(|c| c.ends_with("calling Only([100012c004c4157, 100009a00525554])")), "{:?}", game.calls);
        assert_eq!(answer(&replies[0])["result"]["called"], json!(["WAL"]));
        assert_eq!(answer(&replies[1])["error"]["code"], "invalid_argument");
        assert_eq!(answer(&replies[2])["error"]["code"], "not_found");
    }

    #[test]
    fn a_refusal_comes_at_once_with_the_game_s_reasons() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let reply = engine.submit(Request::Diplomacy(DiplomacyRequest::DeclareWar {
            country: CountryTag(*b"WAL"),
            casus_belli: "none".into(),
            war_goal: None,
            province: None,
            target: None,
            call_allies: None,
        }));
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
        let mut waiting = Waiting { ticket: Ticket(3), kind: Kind::ProposeAlliance, called: None, until: now + WAITED };
        let (in_game, outside) = (Place::Game, Place::Outside(None));
        let mut poll = |place, now| waiting.poll(&mut super::super::Poll::new(&buffer, now, place, &mut game));
        assert_eq!(poll(&in_game, now), None);
        assert_eq!(poll(&outside, now), None);
        let late = poll(&outside, now + WAITED).map(|r| response_json(&r));
        assert_eq!(late, Some(json!({"ok": true, "result": {"answer": "pending"}})));
        let late = poll(&in_game, now + WAITED).map(|r| response_json(&r));
        assert_eq!(late, Some(json!({"ok": true, "result": {"answer": "pending"}})));
    }

    #[test]
    fn an_action_waiting_as_the_game_returns_to_the_main_menu_is_pending() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let reply = engine.submit(Request::Diplomacy(DiplomacyRequest::ProposeAlliance { country: byz() }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply), Value::Null);
        engine.end(menu::ending(), Duration::ZERO);
        assert_eq!(answer(&reply), json!({"ok": true, "result": {"answer": "pending"}}));
    }
}
