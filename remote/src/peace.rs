//! Peace: the peace view, where the player demands terms of a country they
//! are at war with, or offers terms to it (docs/player-actions.md, part A5).
//!
//! A term is named by its key, as the game names its peace treaty terms
//! without their `po_` prefix (`demand_provinces`, `humiliate`, `gold`,
//! `release_vassals`), or by a scripted peace treaty's own key, and by what
//! it is about: a province, the country it names, a colonial region, or an
//! amount of ducats. `peace.terms` lists every term the peace view offers,
//! each with whether the player can add it to the treaty and its war score
//! cost; `peace.evaluate` answers, for a set of terms, whether the player can
//! send it, what it costs in war score, and whether the other country would
//! accept it, as the peace view's send button and its accept or decline icon
//! tell the player; `peace.demand` and `peace.offer` send it, as the send
//! button does, and answer what the other country answered.
use crate::{
    choices::{Acceptance, Choice},
    diplomacy::Outcome,
    CountryTag, ProvinceId,
};
use serde::{Deserialize, Serialize};

group! {
    /// The peace view: terms demanded of a country the player is at war with,
    /// or offered to it.
    "peace": PeaceRequest, PeaceAnswer;

    /// Every term the peace view offers against a country the player is at
    /// war with: on the demand tab, what the player could demand of it; with
    /// `offer`, on the offer tab, what the player could give it. Each says
    /// whether the player can add it to the treaty now and, when not, why,
    /// as the term's tooltip does, and its war score cost; `gold`, which the
    /// view sets with its buttons rather than lists, with the most ducats
    /// the view lets the player ask (or give), the whole that the receiving
    /// side shares.
    game TermOptions "terms" {
        /// The country the player is at war with.
        country: CountryTag,
        /// The offer tab's terms, instead of the demand tab's.
        offer: Option<bool>,
    } -> Vec<TermOption>, "the terms: term, province, country, region, amount (gold: the most), name, can, reasons, cost (war_score)";
    /// Whether the player can send a peace treaty of these terms, what it
    /// costs in war score, and whether the other country would accept it, as
    /// the peace view's send button and its accept or decline icon tell the
    /// player. Takes the terms as `peace.demand` (or with `offer`,
    /// `peace.offer`) does. `invalid_argument` for a term the peace view does
    /// not offer; `rejected` for a term the view would not let the player
    /// add, with the view's reasons.
    game Evaluate "evaluate" {
        /// The country the player is at war with.
        country: CountryTag,
        /// The terms, such as `[{"term":"demand_provinces","province":236},{"term":"humiliate"}]`.
        terms: Terms,
        /// Terms the player offers, instead of terms they demand.
        offer: Option<bool>,
    } -> Evaluation, "can, reasons, cost (war_score), and acceptance: accepts, total, factors";
    /// Demands these terms of a country the player is at war with, as the
    /// peace view's demand tab sends them, and answers what it answered.
    /// Gold demanded is paid in whole by the other country, and the game
    /// shares it among the countries on the player's side of the war by
    /// their participation in it, the player getting their share and what rounding leaves;
    /// the peace view, as the term's `amount`, shows the whole.
    /// `rejected` when the view would not let the player add a term, or send
    /// the treaty, with the view's reasons; `invalid_argument` for a term
    /// the view does not offer.
    game Demand "demand" {
        /// The country the player is at war with.
        country: CountryTag,
        /// The terms, such as `[{"term":"demand_provinces","province":236}]`.
        terms: Terms,
    } -> Outcome, "answer: accepted, declined or pending";
    /// Offers these terms to a country the player is at war with, as the
    /// peace view's offer tab sends them, and answers what it answered;
    /// refused as `peace.demand` is.
    game Offer "offer" {
        /// The country the player is at war with.
        country: CountryTag,
        /// The terms, such as `[{"term":"concede_defeat"}]`.
        terms: Terms,
    } -> Outcome, "answer: accepted, declined or pending";
}

/// One term of a peace treaty, as a request names it: its key and what it is
/// about. A term the peace view lists more than once (a province to demand)
/// needs what tells them apart; a part left out matches any.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(deny_unknown_fields)]
pub struct Term {
    /// Its key: the game's peace treaty term without `po_`
    /// (`demand_provinces`, `humiliate`, `gold`), or a scripted peace
    /// treaty's key.
    pub term: String,
    /// The province it is about (`demand_provinces`, `revoke_cores`,
    /// `return_cores`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub province: Option<ProvinceId>,
    /// The other country it names, as the peace view's entry shows it: who
    /// takes the province, the subject to release, the country to release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<CountryTag>,
    /// The colonial region it is about (`concede_colonial_region`), by key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// For `gold`, the ducats, to a tenth: what the other country pays, which
    /// the countries of the receiving side share by their participation in
    /// the war.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
}

/// The terms of a peace treaty, as an argument: a list of [`Term`]s, in JSON
/// or, on a command line, as the JSON text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Terms(pub Vec<Term>);

impl Terms {
    /// Read from its JSON text.
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("terms are a JSON list of terms, such as [{{\"term\":\"humiliate\"}}]: {e}"))
    }
}

/// A term the peace view offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct TermOption {
    /// Its key, as [`Term`] names it.
    pub term: String,
    /// The province it is about, if any.
    pub province: Option<ProvinceId>,
    /// The other country it names, if any.
    pub country: Option<CountryTag>,
    /// The colonial region it is about, by key, if any.
    pub region: Option<String>,
    /// For `gold`, the most ducats the view lets the player set.
    pub amount: Option<f64>,
    /// Its name, as the peace view lists it, in the player language.
    pub name: String,
    /// Whether the player can add it to the treaty now; its cost, in war
    /// score.
    #[serde(flatten)]
    pub choice: Choice,
}

/// A peace treaty evaluated: whether the player can send it, its war score
/// cost, and whether the other country would accept it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Evaluation {
    #[serde(flatten)]
    pub choice: Choice,
    pub acceptance: Acceptance,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        choices::{Amount, Factor, Resource},
        request_from_json, Answer, Request, Response,
    };
    use serde_json::json;

    #[test]
    fn terms_are_a_list_of_keys_and_what_each_is_about() {
        let request =
            request_from_json(r#"{"capability": "peace.evaluate", "country": "ENG", "terms": [{"term": "demand_provinces", "province": 236}, {"term": "humiliate"}]}"#)
                .unwrap();
        let province = Term { term: "demand_provinces".into(), province: Some(ProvinceId(236)), country: None, region: None, amount: None };
        let humiliate = Term { term: "humiliate".into(), province: None, country: None, region: None, amount: None };
        assert_eq!(request, Request::Peace(PeaceRequest::Evaluate { country: CountryTag(*b"ENG"), terms: Terms(vec![province, humiliate]), offer: None }));
        assert!(request_from_json(r#"{"capability": "peace.demand", "country": "ENG", "terms": [{"term": "gold", "ducats": 5}]}"#).is_err());
        assert_eq!(Terms::parse(r#"[{"term":"gold","amount":12.5}]"#).unwrap().0[0].amount, Some(12.5));
        assert!(Terms::parse("humiliate").is_err());
    }

    #[test]
    fn an_evaluation_flattens_its_choice_beside_the_acceptance() {
        let evaluation = Evaluation {
            choice: Choice::can(vec![Amount::new(Resource::WarScore, 27.0)]),
            acceptance: Acceptance { accepts: true, total: 8.0, factors: vec![Factor { text: "War score".into(), value: 20.0 }] },
        };
        let response: Response = Ok(Answer::Peace(PeaceAnswer::Evaluate(evaluation)));
        assert_eq!(
            crate::response_json(&response),
            json!({"ok": true, "result": {"can": true, "reasons": [], "cost": [{"resource": "war_score", "amount": 27.0}],
                "acceptance": {"accepts": true, "total": 8.0, "factors": [{"text": "War score", "value": 20.0}]}}})
        );
    }
}
