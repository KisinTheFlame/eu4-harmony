//! Diplomacy: the player's diplomatic actions. The capability layer sends
//! one at once; the answer comes at a later safe point, once the game has
//! carried it out and the other country has answered, or as `pending` after
//! [`WAITED`] or when the game returns to the main menu first.
use super::{countries, events, tag, Outcome as Ran, Poll, Wait};
use crate::game::{
    diplomacy::{self as layer, Act, Answer, Calls, Choice, Declaration, Details, Ticket, What, NO_CASUS_BELLI as NONE},
    GResult, Tag,
};
use eu4_harmony_remote::{
    diplomacy::{AnswerKind, CallAllies, DiplomacyAnswer, DiplomacyRequest, Offered, Options, Outcome, PassedOver, WarGoal},
    Answer as Answered, CountryTag, Error, Response,
};
use std::time::{Duration, Instant};

/// How long an action waits for what comes of it before it answers
/// `pending`. A single player game carries an action out and has the
/// other country answer within a few frames.
pub const WAITED: Duration = Duration::from_secs(5);

layer! {
    fn war_goals(enemy: Tag) -> Vec<layer::WarGoal> = layer::war_goals;
    fn declare_war(enemy: Tag, choice: &Choice, calls: &Calls) -> Declaration = layer::declare_war;
    fn offer_peace(enemy: Tag) -> Ticket = layer::offer_peace;
    fn propose_alliance(ally: Tag) -> Ticket = layer::propose_alliance;
    fn options(country: Tag) -> Vec<layer::Offered> = layer::options;
    fn act(country: Tag, what: &What, details: &Details) -> Ticket = layer::act;
    fn call_to_arms(ally: Tag) -> Ticket = layer::call_to_arms;
    fn answer(ticket: Ticket) -> Answer = layer::answer;
    fn add_rival(rival: Tag) -> () = layer::add_rival;
    fn remove_rival(rival: Tag) -> () = layer::remove_rival;
}

/// The panel's act a capability sends, the country it sends it to, and the
/// capability's answer; None for the capabilities that are not one of the
/// panel's plain acts.
macro_rules! panel_acts {
    ($($Act:ident)*) => {
        fn panel_act(request: DiplomacyRequest) -> Option<(CountryTag, Act, Answers)> {
            match request {
                $( DiplomacyRequest::$Act { country } => Some((country, Act::$Act, (|o| Answered::Diplomacy(DiplomacyAnswer::$Act(o))) as Answers)), )*
                _ => None,
            }
        }
    };
}

panel_acts! {
    SurrenderUnconditionally CancelAlliance SupportIndependence StopSupportingIndependence FormCoalition LeaveCoalition DemandReligiousUnity
    RemoveElector GrantElector RemoveFreeCity GrantFreeCity CallCrusade Excommunicate LiftExcommunication InviteToFederation
    RemoveFromFederation AskToJoinFederation LeaveFederation InviteToTradeLeague RemoveFromTradeLeague InviteToEmpire AskToJoinTradeLeague
    ImproveRelations StopImprovingRelations Annex CancelAnnexation Integrate CancelIntegration DesignateMarch RevokeMarch AbandonUnion
    OfferVassalization ReleaseVassal AskForMarch DemandTributary ReleaseTributary ArrangeRoyalMarriage BreakRoyalMarriage
    ConvertTributaryToVassal TradeFavorsForGold TradeFavorsForMen TradeFavorsForSailors TradeFavorsForHeir TradeFavorsForWarPreparations
    SupportHeir StopSupportingHeir CurryFavors StopCurryingFavors BuildSpyNetwork StopBuildingSpyNetwork CounterEspionage
    StopCounterEspionage JustifyTradeConflict SupportRebels StealMaps SlanderMerchants SabotageRecruitment SowDiscontent SabotageReputation
    CorruptOfficials InfiltrateAdministration AgitateForLiberty CancelTradePowerTransfer SteerTrade StopSteeringTrade Embargo LiftEmbargo
    AskToBeTributary Guarantee CancelGuarantee Warn AskMilitaryAccess CancelMilitaryAccess OfferMilitaryAccess RevokeMilitaryAccess
    AskFleetBasing CancelFleetBasing OfferFleetBasing RevokeFleetBasing ShareKnowledge StopSharingKnowledge AskForKnowledgeSharing
    ImperialRelations ImperialRealmWar SellShips ShareMaps TakeOnDebt Influence Insult ClaimThrone StopWarSubsidies RecallCondottieri
}

pub fn run(request: DiplomacyRequest, game: &mut (impl Layer + countries::Layer + ?Sized), now: Instant) -> Ran {
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
            let calls = calls(call_allies.unwrap_or_default(), game)?;
            let declared = game.declare_war(enemy, &choice, &calls)?;
            let passed_over = declared.passed_over.into_iter().map(|p| PassedOver { country: tag(p.country), reasons: p.reasons }).collect();
            Ok((
                declared.ticket,
                (|o| Answered::Diplomacy(DiplomacyAnswer::DeclareWar(o))) as Answers,
                Some((declared.called.into_iter().map(tag).collect(), passed_over)),
            ))
        })(),
        DiplomacyRequest::OfferPeace { country } => (|| {
            let enemy = game.country(country)?;
            Ok((game.offer_peace(enemy)?, (|o| Answered::Diplomacy(DiplomacyAnswer::OfferPeace(o))) as Answers, None))
        })(),
        DiplomacyRequest::ProposeAlliance { country } => (|| {
            let ally = game.country(country)?;
            Ok((game.propose_alliance(ally)?, (|o| Answered::Diplomacy(DiplomacyAnswer::ProposeAlliance(o))) as Answers, None))
        })(),
        DiplomacyRequest::CallToArms { country } => (|| {
            let ally = game.country(country)?;
            Ok((game.call_to_arms(ally)?, (|o| Answered::Diplomacy(DiplomacyAnswer::CallToArms(o))) as Answers, None))
        })(),
        DiplomacyRequest::Options { country } => {
            return Ran::Done((|| {
                let other = game.country(country)?;
                let actions = game.options(other)?.into_iter().map(offered).collect();
                Ok(Answered::Diplomacy(DiplomacyAnswer::Options(Options { country, actions })))
            })())
        }
        DiplomacyRequest::AddRival { country } => {
            return Ran::Done((|| {
                let rival = game.country(country)?;
                game.add_rival(rival)?;
                Ok(Answered::Diplomacy(DiplomacyAnswer::AddRival(())))
            })())
        }
        DiplomacyRequest::RemoveRival { country } => {
            return Ran::Done((|| {
                let rival = game.country(country)?;
                game.remove_rival(rival)?;
                Ok(Answered::Diplomacy(DiplomacyAnswer::RemoveRival(())))
            })())
        }
        DiplomacyRequest::ScriptedAction { country, action } => (|| {
            let other = game.country(country)?;
            Ok((game.act(other, &What::Scripted(action), &Details::default())?, (|o| Answered::Diplomacy(DiplomacyAnswer::ScriptedAction(o))) as Answers, None))
        })(),
        request => match detailed(request, game) {
            Ok(Some((country, act, details, answers))) => (|| {
                let other = game.country(country)?;
                Ok((game.act(other, &What::Act(act), &details)?, answers, None))
            })(),
            Err(e) => Err(e.into()),
            Ok(None) => Err(Error::from(crate::game::Fail::from("a diplomacy capability with no way to run it".to_string()))),
        },
    };
    match sent {
        Ok((ticket, answers, called)) => Ran::Later(Box::new(Waiting { ticket, answers, called, until: now + WAITED })),
        Err(e) => Ran::Done(Err(e)),
    }
}

/// Waits for what comes of a proposal sent with `ticket`, as the
/// diplomatic actions do, answering it through `answers`.
pub fn waiting(ticket: Ticket, answers: Answers, now: Instant) -> Ran {
    Ran::Later(Box::new(Waiting { ticket, answers, called: None, until: now + WAITED }))
}

/// The allies a declaration calls, the countries named found in the game.
fn calls(call: CallAllies, game: &mut (impl countries::Layer + ?Sized)) -> GResult<Calls> {
    Ok(match call {
        CallAllies::All => Calls::All,
        CallAllies::None => Calls::None,
        CallAllies::Only(tags) => Calls::Only(tags.into_iter().map(|tag| game.country(tag)).collect::<GResult<_>>()?),
    })
}

/// The country a capability sends one of the panel's acts to, the act, what
/// its window has the player choose, and the capability's answer.
type Detailed = (CountryTag, Act, Details, Answers);

/// The panel's act a capability sends, the country it sends it to, what its
/// window has the player choose, and the capability's answer; the countries
/// it names found in the game.
fn detailed(request: DiplomacyRequest, game: &mut (impl countries::Layer + ?Sized)) -> GResult<Option<Detailed>> {
    use DiplomacyRequest as R;
    let none = Details::default();
    let province = |p: eu4_harmony_remote::ProvinceId| Some(p.0);
    Ok(Some(match request {
        R::SendGift { country, amount } => {
            (country, Act::SendGift, Details { amount: Some(amount), ..none }, (|o| Answered::Diplomacy(DiplomacyAnswer::SendGift(o))) as Answers)
        }
        R::SubsidizeWar { country, amount, duration } => (
            country,
            Act::SubsidizeWar,
            Details { amount: Some(amount), second: duration, ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::SubsidizeWar(o))) as Answers,
        ),
        R::SellProvince { country, province: p, price } => (
            country,
            Act::SellProvince,
            Details { province: province(p), second: Some(price), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::SellProvince(o))) as Answers,
        ),
        R::CharterCompany { country, province: p, price } => (
            country,
            Act::CharterCompany,
            Details { province: province(p), second: Some(price), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::CharterCompany(o))) as Answers,
        ),
        R::FabricateClaim { country, province: p } => {
            (country, Act::FabricateClaim, Details { province: province(p), ..none }, (|o| Answered::Diplomacy(DiplomacyAnswer::FabricateClaim(o))) as Answers)
        }
        R::ClaimStates { country, province: p } => {
            (country, Act::ClaimStates, Details { province: province(p), ..none }, (|o| Answered::Diplomacy(DiplomacyAnswer::ClaimStates(o))) as Answers)
        }
        R::TransferTradePower { country, percent } => (
            country,
            Act::TransferTradePower,
            Details { amount: Some(percent), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::TransferTradePower(o))) as Answers,
        ),
        R::ThreatenWar { country, province: p } => {
            (country, Act::ThreatenWar, Details { province: province(p), ..none }, (|o| Answered::Diplomacy(DiplomacyAnswer::ThreatenWar(o))) as Answers)
        }
        R::BreakAlliance { country, ally } => (
            country,
            Act::BreakAlliance,
            Details { country: Some(game.country(ally)?), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::BreakAlliance(o))) as Answers,
        ),
        R::TradeFavorsToBreakAlliance { country, ally } => (
            country,
            Act::TradeFavorsToBreakAlliance,
            Details { country: Some(game.country(ally)?), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::TradeFavorsToBreakAlliance(o))) as Answers,
        ),
        R::TradeFavorsToReduceRelations { country, target } => (
            country,
            Act::TradeFavorsToReduceRelations,
            Details { country: Some(game.country(target)?), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::TradeFavorsToReduceRelations(o))) as Answers,
        ),
        R::EnforcePeace { country, enemy } => (
            country,
            Act::EnforcePeace,
            Details { country: Some(game.country(enemy)?), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::EnforcePeace(o))) as Answers,
        ),
        R::InterveneInWar { country, enemy } => (
            country,
            Act::InterveneInWar,
            Details { country: Some(game.country(enemy)?), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::InterveneInWar(o))) as Answers,
        ),
        R::TradeFavorsForCore { country, province: p } => (
            country,
            Act::TradeFavorsForCore,
            Details { province: province(p), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::TradeFavorsForCore(o))) as Answers,
        ),
        R::DemandUnlawfulTerritory { country, province: p } => (
            country,
            Act::DemandUnlawfulTerritory,
            Details { province: province(p), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::DemandUnlawfulTerritory(o))) as Answers,
        ),
        R::OfferCondottieri { country, army, price } => (
            country,
            Act::OfferCondottieri,
            Details { army: Some(army.0), second: Some(price), ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::OfferCondottieri(o))) as Answers,
        ),
        R::OfferLoan { country, amount } => {
            (country, Act::OfferLoan, Details { amount: Some(amount), ..none }, (|o| Answered::Diplomacy(DiplomacyAnswer::OfferLoan(o))) as Answers)
        }
        R::AppointCardinal { country, pay } => {
            (country, Act::AppointCardinal, Details { flag: pay, ..none }, (|o| Answered::Diplomacy(DiplomacyAnswer::AppointCardinal(o))) as Answers)
        }
        R::TradeFavorsForTrust { country, favors, trust } => (
            country,
            Act::TradeFavorsForTrust,
            Details { amount: Some(favors), second: trust, ..none },
            (|o| Answered::Diplomacy(DiplomacyAnswer::TradeFavorsForTrust(o))) as Answers,
        ),
        request => return Ok(panel_act(request).map(|(country, act, answers)| (country, act, none, answers))),
    }))
}

/// An action the panel offers, named by the capability that sends it.
fn offered(o: layer::Offered) -> Offered {
    let action = match &o.what {
        Some(What::Act(act)) => act.name(),
        Some(What::Scripted(_)) => "scripted_action",
        None => "unknown",
    };
    Offered { action: action.into(), key: o.key, name: o.name, choice: o.choice, description: o.description, acceptance: o.acceptance }
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

/// The answer of the capability that sent the action, around what came of it.
pub type Answers = fn(Outcome) -> Answered;

/// An action sent, waiting for what comes of it.
struct Waiting {
    ticket: Ticket,
    answers: Answers,
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
        Ok((self.answers)(outcome))
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
        events,
        fake::{answer, quiet, Fake},
        menu, Place, Request,
    };
    use super::*;
    use crate::remote::Engine;
    use eu4_harmony_remote::{response_json, CountryTag, ProvinceId};
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
                call_allies: Some(CallAllies::parse(call_allies).unwrap()),
            };
            engine.submit(Request::Diplomacy(declare))
        };
        let reply = declare(&engine, "none");
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert!(game.calls.iter().any(|c| c == "declare war 1000021005a5942 None None None None calling None"), "{:?}", game.calls);
        engine.end(menu::ending(), Duration::ZERO);
        assert_eq!(answer(&reply), json!({"ok": true, "result": {"answer": "pending", "called": [], "passed_over": []}}));

        // Allies by tag, in either case and with spaces, and a tag of no
        // country. In a process of its own, since the engine above has
        // ended. (A word that is no tag is no request: `bad_request`.)
        let engine = Engine::new(quiet);
        let replies = [declare(&engine, "wal, TUR"), declare(&engine, "WAL,QQQ")];
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        engine.end(menu::ending(), Duration::ZERO);
        assert!(game.calls.iter().any(|c| c.ends_with("calling Only([100012c004c4157, 100009a00525554])")), "{:?}", game.calls);
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
        assert_eq!(game.calls, ["country WAL", "call to arms 100012c004c4157", "answer 4"]);
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
        assert!(game.calls.iter().any(|c| c == "act 1000021005a5942 Act(Warn)"), "{:?}", game.calls);
        assert!(game.calls.iter().any(|c| c == "act 1000021005a5942 Scripted(\"sell_indulgence\")"), "{:?}", game.calls);
        // Annexing, as one more of the panel's acts.
        let annex = engine.submit(Request::Diplomacy(DiplomacyRequest::Annex { country: byz() }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&annex)["error"]["code"], "rejected");
        assert!(game.calls.iter().any(|c| c == "act 1000021005a5942 Act(Annex)"), "{:?}", game.calls);
    }

    #[test]
    fn an_action_with_nothing_come_of_it_in_time_is_pending() {
        let mut game = Fake::default();
        let buffer = events::Buffer::new();
        let now = Instant::now();
        let mut waiting =
            Waiting { ticket: Ticket(3), answers: |o| Answered::Diplomacy(DiplomacyAnswer::ProposeAlliance(o)), called: None, until: now + WAITED };
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
