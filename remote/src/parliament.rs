//! Parliament: the issues the parliament view offers the player, starting a
//! debate on one, giving provinces seats, and the seats of a debate with the
//! bribes they ask, paying them, as its buttons do.
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

    /// The seats of the player's parliament as the parliament view shows
    /// them in a debate: the issue debated, how many seats back it and how
    /// many it needs to pass, two thirds of them; and each seat, by its
    /// province, whether it backs the issue, the bribe it asks for its
    /// backing (a content key of the game's or a mod's parliament bribe
    /// files, null when it asks none), and whether the player can pay it now
    /// and why not: the seat backs the issue already, no debate is going on,
    /// or the bribe's conditions do not hold. What a bribe costs is what its
    /// script does, which is not given. Without a parliament, `parliament`
    /// is false and there are no seats.
    game Seats "seats" {} -> Seats, "parliament, debating, backing, needed, seats: province, backs, bribe, bribe_name, can, reasons, cost";

    /// Pays the bribe a seat asks for its backing in the debate, as its
    /// button in the parliament view does: the game carries out what the
    /// bribe's script costs, the seat backs the issue, and the debate passes
    /// once two thirds of the seats back it. `rejected` with the reasons
    /// `parliament.seats` gives; `not_found` for a province with no seat in
    /// the player's parliament.
    game Bribe "bribe" {
        /// The seat's province.
        province: ProvinceId,
    } -> (), "null once the seat backs the issue";
}

/// The seats of the player's parliament.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Seats {
    /// The player has a parliament.
    pub parliament: bool,
    /// The content key of the issue being debated; null when none is.
    pub debating: Option<String>,
    /// How many seats back the issue debated; 0 when none is.
    pub backing: u32,
    /// How many seats must back it for it to pass: two thirds of them.
    pub needed: u32,
    pub seats: Vec<Seat>,
}

/// A seat of the player's parliament.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Seat {
    /// The province the seat is in.
    pub province: ProvinceId,
    /// The seat backs the issue debated.
    pub backs: bool,
    /// The content key of the bribe the seat asks; null when it asks none.
    pub bribe: Option<String>,
    /// The bribe's name, as the view shows it, in the player language.
    pub bribe_name: Option<String>,
    /// Whether the player can pay the bribe now, and why not.
    #[serde(flatten)]
    pub choice: Choice,
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
