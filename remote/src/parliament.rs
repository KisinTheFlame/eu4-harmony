//! Parliament: the issues the parliament view offers the player, starting a
//! debate on one, and giving provinces seats, as its buttons do.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's parliament.
    "parliament": ParliamentRequest, ParliamentAnswer;

    /// The player's parliament as the parliament view shows it: the issue
    /// being debated, the one enacted last, and the issues on offer, by
    /// content key, each with whether the player can bring it to debate now
    /// and why not: its conditions that do not hold, a debate going on.
    /// Without a parliament, `parliament` is false and nothing is offered.
    game Options "options" {} -> Parliament, "parliament, debating, enacted, issues: issue (its content key), name, can, reasons, cost";

    /// Brings an issue on offer to debate, as its button in the parliament
    /// view does. `rejected` with the game's reasons when the game would not
    /// let the player now; `not_found` for a key that names no issue on
    /// offer.
    game Debate "debate" {
        /// The issue's content key, such as `administrative_reform`.
        issue: String,
    } -> (), "null once the debate began";

    /// Gives a province of the player's a seat in parliament, as the
    /// province view's button does. `rejected` when the game would not let
    /// the player now, with no reason the game gives.
    game AddSeat "add_seat" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once the province has its seat";

    /// Takes a province's seat in parliament away, as the province view's
    /// button does. `rejected` when the game would not let the player now,
    /// with no reason the game gives.
    game RemoveSeat "remove_seat" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once the seat is gone";
}

/// The player's parliament.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Parliament {
    /// The player has a parliament.
    pub parliament: bool,
    /// The content key of the issue being debated; null when none is.
    pub debating: Option<String>,
    /// The content key of the issue enacted last; null when none is.
    pub enacted: Option<String>,
    /// The issues on offer.
    pub issues: Vec<Issue>,
}

/// An issue the parliament view offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Issue {
    /// Its content key, which the game's or a mod's parliament issue files
    /// give it.
    pub issue: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// Whether the player can bring it to debate now, and why not.
    #[serde(flatten)]
    pub choice: Choice,
}
