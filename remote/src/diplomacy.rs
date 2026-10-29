//! Diplomacy: the player's diplomatic actions, sent as the player's click in
//! the diplomacy panel sends them, so that the game's checks, costs, AI
//! answers and multiplayer rules apply.
//!
//! A refusal is `rejected`, with the game's reasons, one a line. An action
//! the game lets the player send is answered once the game has done with
//! it: in a single player game the other country answers a proposal as the
//! game carries it out, a frame or two after it is sent, so the answer is
//! `accepted` or `declined`; a declaration of war is `declared`, with the
//! war's name, once the war began. When nothing comes within a few seconds,
//! such as when a human player is to answer, the answer is `pending`: what
//! comes of it later shows in `country.get`, as a relation or a war.
use crate::{CountryTag, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's diplomatic actions.
    "diplomacy": DiplomacyRequest, DiplomacyAnswer;

    /// The war goals the game offers the player for declaring war on a
    /// country, as the declare war dialog lists them: under each casus belli
    /// the player has against it, each war goal it allows, and last, when
    /// the game offers it, declaring war without a casus belli, whose
    /// `casus_belli` is `none`.
    game WarGoals "war_goals" {
        /// The country to declare war on.
        country: CountryTag,
    } -> Vec<WarGoal>, "the war goals: casus_belli, war_goal, province, target, name";
    /// Declares war on a country for one of the war goals the game offers
    /// (`diplomacy.war_goals`): the first with the casus belli, and the war
    /// goal, province and target when given. Allies are called as the
    /// declare war dialog calls them at first. `rejected` when the game
    /// offers no such war goal, or does not let the player declare war, with
    /// its reasons.
    game DeclareWar "declare_war" {
        /// The country to declare war on.
        country: CountryTag,
        /// The casus belli's key, such as `cb_claim`, or `none` to declare
        /// war without one.
        casus_belli: String,
        /// The war goal's key, such as `take_claim`; the first under the
        /// casus belli when left out.
        war_goal: Option<String>,
        /// The province the war goal is about.
        province: Option<ProvinceId>,
        /// The country the war goal is about.
        target: Option<CountryTag>,
    } -> Outcome, "answer: declared (with war, its name) or pending";
    /// Offers a country the player is at war with a white peace, a peace
    /// treaty with no terms, in the war between them, as the peace view
    /// sends one. The game allows it to the war's leader, with the other
    /// side's leader or, as a separate peace, another country in it.
    game OfferPeace "offer_peace" {
        /// The country to offer peace to.
        country: CountryTag,
    } -> Outcome, "answer: accepted, declined or pending";
    /// Proposes an alliance to a country.
    game ProposeAlliance "propose_alliance" {
        /// The country to ally with.
        country: CountryTag,
    } -> Outcome, "answer: accepted, declined or pending";
}

/// A war goal the game offers for a declaration of war.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarGoal {
    /// Its casus belli's key, such as `cb_claim`; `none` for declaring war
    /// without one.
    pub casus_belli: String,
    /// Its type's key, such as `take_claim`; null when it has none.
    pub war_goal: Option<String>,
    /// The province it is about, if any.
    pub province: Option<ProvinceId>,
    /// The country it is about, if any.
    pub target: Option<CountryTag>,
    /// Its name, as the declare war dialog shows it.
    pub name: String,
}

/// What came of a diplomatic action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub answer: AnswerKind,
    /// The war that began, by its name as the game shows it, for `declared`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub war: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerKind {
    /// The other country accepted the proposal.
    Accepted,
    /// The other country declined it.
    Declined,
    /// The war began.
    Declared,
    /// No answer yet: the game has not carried the action out, or the
    /// other country has not answered.
    Pending,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{request_from_json, Answer, Request, Response};
    use serde_json::json;

    #[test]
    fn a_declaration_names_its_war_and_a_proposal_does_not() {
        let declared: Response = Ok(Answer::Diplomacy(DiplomacyAnswer::DeclareWar(Outcome { answer: AnswerKind::Declared, war: Some("Ottoman-Byzantine War".into()) })));
        assert_eq!(crate::response_json(&declared), json!({"ok": true, "result": {"answer": "declared", "war": "Ottoman-Byzantine War"}}));
        let declined: Response = Ok(Answer::Diplomacy(DiplomacyAnswer::ProposeAlliance(Outcome { answer: AnswerKind::Declined, war: None })));
        assert_eq!(crate::response_json(&declined), json!({"ok": true, "result": {"answer": "declined"}}));
    }

    #[test]
    fn a_war_goal_in_json() {
        let goal = WarGoal { casus_belli: "none".into(), war_goal: None, province: Some(ProvinceId(151)), target: Some(CountryTag(*b"BYZ")), name: "Conquer Constantinople".into() };
        assert_eq!(
            serde_json::to_value(&goal).unwrap(),
            json!({"casus_belli": "none", "war_goal": null, "province": 151, "target": "BYZ", "name": "Conquer Constantinople"})
        );
    }

    #[test]
    fn a_declaration_needs_only_its_country_and_casus_belli() {
        let request = request_from_json(r#"{"capability": "diplomacy.declare_war", "country": "BYZ", "casus_belli": "none"}"#).unwrap();
        assert_eq!(
            request,
            Request::Diplomacy(DiplomacyRequest::DeclareWar { country: CountryTag(*b"BYZ"), casus_belli: "none".into(), war_goal: None, province: None, target: None })
        );
    }
}
