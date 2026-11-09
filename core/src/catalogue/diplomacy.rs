//! Diplomacy: the player's diplomatic actions. The capability layer sends
//! one at once; the answer comes at a later safe point, once the game has
//! carried it out and the other country has answered, or as `pending` after
//! [`WAITED`] or when the game returns to the main menu first.
use super::{events, Outcome as Ran, Poll, Wait};
use crate::game::{
    diplomacy::{self as layer, Act, Answer, Declaration, Details, Ticket},
    GResult,
};
use eu4_harmony_remote::diplomacy::args;
use eu4_harmony_remote::{
    diplomacy::{AnswerKind, DiplomacyAnswer, DiplomacyRequest, Options, Outcome, PassedOver, WarGoal},
    Answer as Answered, CountryTag, Error, Response,
};
use std::time::{Duration, Instant};

/// How long an action waits for what comes of it before it answers
/// `pending`. A single player game carries an action out and has the
/// other country answer within a few frames.
pub const WAITED: Duration = Duration::from_secs(5);

layer! {
    fn war_goals(asked: args::WarGoals) -> Vec<WarGoal> = session layer::war_goals;
    fn declare_war(asked: args::DeclareWar) -> Declaration = session layer::declare_war;
    fn offer_peace(asked: args::OfferPeace) -> Ticket = session layer::offer_peace;
    fn propose_alliance(asked: args::ProposeAlliance) -> Ticket = session layer::propose_alliance;
    fn options(asked: args::Options) -> Options = session layer::options;
    fn act(act: Act, country: CountryTag, details: Details<CountryTag>) -> Ticket = session layer::act;
    fn scripted_action(asked: args::ScriptedAction) -> Ticket = session layer::scripted;
    fn call_to_arms(asked: args::CallToArms) -> Ticket = session layer::call_to_arms;
    fn answer(ticket: Ticket) -> Answer = world layer::answer;
    fn add_rival(asked: args::AddRival) -> () = session layer::add_rival;
    fn remove_rival(asked: args::RemoveRival) -> () = session layer::remove_rival;
}

pub fn run(request: DiplomacyRequest, game: &mut (impl Layer + ?Sized), now: Instant) -> Ran {
    let sent = match request {
        DiplomacyRequest::WarGoals { country } => return done(game.war_goals(args::WarGoals { country }).map(DiplomacyAnswer::WarGoals)),
        DiplomacyRequest::Options { country } => return done(game.options(args::Options { country }).map(DiplomacyAnswer::Options)),
        DiplomacyRequest::AddRival { country } => return done(game.add_rival(args::AddRival { country }).map(DiplomacyAnswer::AddRival)),
        DiplomacyRequest::RemoveRival { country } => return done(game.remove_rival(args::RemoveRival { country }).map(DiplomacyAnswer::RemoveRival)),
        DiplomacyRequest::DeclareWar { country, casus_belli, war_goal, province, target, call_allies } => game
            .declare_war(args::DeclareWar { country, casus_belli, war_goal, province, target, call_allies })
            .map(|declared| (declared.ticket, Answering::Act(Act::DeclareWar), Some((declared.called, declared.passed_over)))),
        DiplomacyRequest::OfferPeace { country } => game.offer_peace(args::OfferPeace { country }).map(|ticket| (ticket, Answering::Act(Act::OfferPeace), None)),
        DiplomacyRequest::ProposeAlliance { country } => {
            game.propose_alliance(args::ProposeAlliance { country }).map(|ticket| (ticket, Answering::Act(Act::ProposeAlliance), None))
        }
        DiplomacyRequest::CallToArms { country } => game.call_to_arms(args::CallToArms { country }).map(|ticket| (ticket, Answering::Act(Act::CallToArms), None)),
        DiplomacyRequest::ScriptedAction { country, action } => {
            let answers = (|o| Answered::Diplomacy(DiplomacyAnswer::ScriptedAction(o))) as Answers;
            game.scripted_action(args::ScriptedAction { country, action }).map(|ticket| (ticket, Answering::With(answers), None))
        }
        request => panel_act(&request).and_then(|(act, country, details)| Ok((game.act(act, country, details)?, Answering::Act(act), None))),
    };
    match sent {
        Ok((ticket, answers, called)) => Ran::Later(Box::new(Waiting { ticket, answers, called, until: now + WAITED })),
        Err(fail) => Ran::Done(Err(fail.into())),
    }
}

/// A request answered at once.
fn done(answer: GResult<DiplomacyAnswer>) -> Ran {
    Ran::Done(answer.map(Answered::Diplomacy).map_err(Error::from))
}

/// Waits for what comes of a proposal sent with `ticket`, as the
/// diplomatic actions do, answering it through `answers`.
pub fn waiting(ticket: Ticket, answers: Answers, now: Instant) -> Ran {
    Ran::Later(Box::new(Waiting { ticket, answers: Answering::With(answers), called: None, until: now + WAITED }))
}

/// One of the panel's acts a capability sends, found by the capability's own
/// name, the country it sends it to, and what it chooses in the act's
/// window, taken from the capability's arguments by their names.
fn panel_act(request: &DiplomacyRequest) -> GResult<(Act, CountryTag, Details<CountryTag>)> {
    let action = request.action();
    let unknown = |why: String| crate::game::Fail::from(format!("diplomacy.{action} {why}"));
    let act = Act::named(action).ok_or_else(|| unknown("is none of the panel's acts".into()))?;
    let serde_json::Value::Object(mut arguments) = serde_json::to_value(request).map_err(|e| unknown(e.to_string()))? else {
        return Err(unknown("has no arguments by name".into()));
    };
    arguments.remove("capability");
    let country = arguments.remove("country").ok_or_else(|| unknown("names no country".into()))?;
    let country = serde_json::from_value(country).map_err(|e| unknown(e.to_string()))?;
    let details = serde_json::from_value(arguments.into()).map_err(|e| unknown(format!("takes what its act's window does not: {e}")))?;
    Ok((act, country, details))
}

// The answer of the capability that sent the action, around what came of it.
pub type Answers = fn(Outcome) -> Answered;

/// How the capability that sent the action answers: as its act's, or
/// through its own answer.
enum Answering {
    Act(Act),
    With(Answers),
}

/// An action sent, waiting for what comes of it.
struct Waiting {
    ticket: Ticket,
    answers: Answering,
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
        Ok(match self.answers {
            Answering::Act(act) => Answered::Diplomacy(act.answer(outcome)),
            Answering::With(answers) => answers(outcome),
        })
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
            Ok(Answer::Done) => (AnswerKind::Done, None),
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
    use super::super::{
        fake::{answer, quiet, Fake},
        menu, Place, Request,
    };
    use super::*;
    use crate::remote::Engine;
    use eu4_harmony_remote::{diplomacy::CallAllies, response_json, CountryTag, ProvinceId};
    use serde_json::{json, Value};

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
        assert_eq!(game.calls[..1], ["declare war BYZ \"cb_claim\" None Some(151) Some(\"WAL\") calling All"]);
        assert_eq!(game.calls[1..], ["answer 1", "answer 1"]);

        // Without a casus belli, calling no ally: pending, none called.
        let declare = |engine: &Engine, call_allies: &str| {
            let declare = DiplomacyRequest::DeclareWar {
                country: byz(),
                casus_belli: "none".into(),
                war_goal: None,
                province: None,
                target: None,
                call_allies: Some(CallAllies::parse(call_allies).unwrap()),
            };
            engine.submit(Request::Diplomacy(declare))
        };
        let reply = declare(&engine, "none");
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert!(game.calls.iter().any(|c| c == "declare war BYZ \"none\" None None None calling None"), "{:?}", game.calls);
        engine.end(menu::ending(), Duration::ZERO);
        assert_eq!(answer(&reply), json!({"ok": true, "result": {"answer": "pending", "called": [], "passed_over": []}}));

        // Allies by tag, in either case and with spaces, and a tag of no
        // country. In a process of its own, since the engine above has
        // ended. (A word that is no tag is no request: `bad_request`.)
        let engine = Engine::new(quiet);
        let replies = [declare(&engine, "wal, TUR"), declare(&engine, "WAL,QQQ")];
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        engine.end(menu::ending(), Duration::ZERO);
        assert!(game.calls.iter().any(|c| c.ends_with("calling Only([WAL, TUR])")), "{:?}", game.calls);
        assert_eq!(answer(&replies[0])["result"]["called"], json!(["WAL"]));
        assert_eq!(answer(&replies[1])["error"]["code"], "not_found");
    }

    #[test]
    fn a_call_to_arms_is_answered_with_the_ally_s_answer() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        game.answers.push_back(Ok(Answer::Accepted));
        let reply = engine.submit(Request::Diplomacy(DiplomacyRequest::CallToArms { country: CountryTag(*b"WAL") }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply), json!({"ok": true, "result": {"answer": "accepted"}}));
        assert_eq!(game.calls, ["call to arms WAL", "answer 4"]);
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
        game.answers.push_back(Err(crate::game::Fail {
            status: crate::game::Status::NotInGame,
            message: "the game the diplomatic action was sent in has ended".into(),
        }));
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
    fn the_panel_s_options_name_each_action_by_its_capability_with_its_acceptance() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let reply = engine.submit(Request::Diplomacy(DiplomacyRequest::Options { country: byz() }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(
            answer(&reply),
            json!({"ok": true, "result": {"country": "BYZ", "actions": [
                {"action": "propose_alliance", "key": "allianceaction", "name": "Offer Alliance", "can": true, "reasons": [], "cost": [],
                 "description": ["If a country accepts our Alliance offer it may come to our assistance during wars."],
                 "acceptance": {"accepts": false, "total": -12.0, "factors": [{"text": "Base reluctance", "value": -10.0}, {"text": "Relations", "value": -2.0}]}},
                {"action": "warn", "key": "warningaction", "name": "Warn", "can": true, "reasons": [], "cost": [], "description": []},
                {"action": "scripted_action", "key": "sell_indulgence", "name": "Offer Indulgence", "can": false, "reasons": ["Is not the Papal Controller."], "cost": [], "description": []},
            ]}})
        );
    }

    #[test]
    fn a_panel_action_is_sent_and_done_or_refused_with_the_game_s_reason() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        game.answers.push_back(Ok(Answer::Done));
        let warn = engine.submit(Request::Diplomacy(DiplomacyRequest::Warn { country: byz() }));
        let indulgence = engine.submit(Request::Diplomacy(DiplomacyRequest::ScriptedAction { country: byz(), action: "sell_indulgence".into() }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&warn), json!({"ok": true, "result": {"answer": "done"}}));
        assert_eq!(answer(&indulgence), json!({"ok": false, "error": {"code": "rejected", "message": "Is not the Papal Controller."}}));
        assert!(game.calls.iter().any(|c| c == "act BYZ Act(Warn)"), "{:?}", game.calls);
        assert!(game.calls.iter().any(|c| c == "act BYZ Scripted(\"sell_indulgence\")"), "{:?}", game.calls);
        // Annexing, as one more of the panel's acts.
        let annex = engine.submit(Request::Diplomacy(DiplomacyRequest::Annex { country: byz() }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&annex)["error"]["code"], "rejected");
        assert!(game.calls.iter().any(|c| c == "act BYZ Act(Annex)"), "{:?}", game.calls);

    }

    /// Each capability answering with what came of an action is one of the
    /// panel's acts, sent by its name with its arguments as the details its
    /// act's row names, but `declare_war`, whose arguments choose a war goal,
    /// and `scripted_action`, which names the action it sends. One without
    /// its act's row, or with arguments the row does not name, fails here,
    /// not in the game.
    #[test]
    fn each_capability_answering_with_an_outcome_sends_its_act_with_its_details() {
        use eu4_harmony_remote::{diplomacy as remote, shape::Shaped};
        let shapes = remote::result_shapes();
        let mut acts = 0;
        for capability in remote::CAPABILITIES {
            let name = capability.name();
            let shape = shapes.iter().find(|(n, _)| *n == name).map(|(_, s)| s.clone());
            if shape != Some(Outcome::shape()) || capability.action == "scripted_action" {
                continue;
            }
            let act = Act::named(capability.action).unwrap_or_else(|| panic!("{name} is none of the panel's acts"));
            acts += 1;
            if capability.action == "declare_war" {
                continue;
            }
            let details: Vec<&str> = capability.args.iter().map(|a| a.name).filter(|&a| a != "country").collect();
            assert_eq!(details, act.details(), "{name}");
            // Given every argument, it is sent as its act with them.
            let mut request = serde_json::Map::new();
            request.insert("capability".into(), capability.action.into());
            for argument in capability.args {
                request.insert(argument.name.into(), argument.kind.json(argument.kind.example()).unwrap());
            }
            let request: DiplomacyRequest = serde_json::from_value(request.into()).unwrap();
            assert_eq!(panel_act(&request).map(|(sent, ..)| sent), Ok(act), "{name}");
        }
        assert!(acts > 100, "{acts} acts");
    }

    #[test]
    fn an_action_with_nothing_come_of_it_in_time_is_pending() {
        let mut game = Fake::default();
        let now = Instant::now();
        let mut waiting = Waiting { ticket: Ticket(3), answers: Answering::Act(Act::ProposeAlliance), called: None, until: now + WAITED };
        let (in_game, outside) = (Place::Game, Place::Outside(None));
        let mut poll = |place, now| waiting.poll(&mut super::super::Poll::new(None, now, place, Some(&mut game)));
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
