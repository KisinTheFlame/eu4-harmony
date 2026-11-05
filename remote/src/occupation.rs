//! Occupation: handing a province the player occupies over to an ally, as
//! the province view's transfer occupation window does (docs/player-actions.md,
//! part A5).
use crate::{choices::Choice, CountryTag, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The transfer occupation window: handing a province the player
    /// occupies over to an ally.
    "occupation": OccupationRequest, OccupationAnswer;

    /// Whether the player can hand a province they occupy over to an ally,
    /// and to which, as the province view's transfer occupation button and
    /// window tell the player: when not, the button's reasons (not the
    /// controller, under siege, no ally to take it).
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> Transfers, "can, reasons, cost, and allies: the countries the window lists to take it";
    /// Hands a province the player occupies over to an ally the transfer
    /// occupation window lists, as picking it there does: the ally then
    /// occupies it. `rejected` when the player cannot hand it over, with
    /// the button's reasons, or when the window does not list that ally.
    game Transfer "transfer" {
        /// The province.
        province: ProvinceId,
        /// The ally to take it.
        country: CountryTag,
    } -> Transferred, "controller: the country that occupies it now";
}

/// Whether the player can hand a province over, and to whom.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Transfers {
    #[serde(flatten)]
    pub choice: Choice,
    /// The allies the window lists to take it; empty when the player cannot
    /// hand it over.
    pub allies: Vec<CountryTag>,
}

/// A province handed over.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Transferred {
    /// The country that occupies it now.
    pub controller: Option<CountryTag>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Answer, Response};
    use serde_json::json;

    #[test]
    fn the_options_flatten_the_choice_beside_the_allies() {
        let options = Transfers { choice: Choice::can(Vec::new()), allies: vec![CountryTag(*b"DUL")] };
        let response: Response = Ok(Answer::Occupation(OccupationAnswer::Options(options)));
        assert_eq!(crate::response_json(&response), json!({"ok": true, "result": {"can": true, "reasons": [], "cost": [], "allies": ["DUL"]}}));
    }
}
