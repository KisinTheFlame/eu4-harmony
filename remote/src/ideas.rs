//! Ideas: the idea groups the player can take and the ideas they can buy,
//! as the ideas view shows them, and taking, buying and abandoning them as
//! its buttons do.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The player's idea groups and ideas.
    "idea": IdeaRequest, IdeaAnswer;

    /// The idea groups of the ideas view, each by its content key, those the
    /// player has taken first, in the order taken: how many of its ideas the
    /// player has bought, and what its button does now, with whether the
    /// player can and why not. A group the player has taken offers its next
    /// idea, at its cost in monarch points; one not taken offers taking it.
    /// The reasons are the game's: the conditions that do not hold, as the
    /// view's tooltip lists them, and its own lines. The national ideas are
    /// not listed: the player has them without choosing.
    game List "list" {} -> Vec<IdeaGroup>, "the idea groups: group (its content key), name, category, taken, ideas, of, button, next, can, reasons, cost";

    /// Takes an idea group, as the player does with its button in the ideas
    /// view: it fills one of the player's idea group slots. `rejected` with
    /// the game's reasons when the game would not let the player take it
    /// now; `not_found` for a key that names no idea group.
    game TakeGroup "take_group" {
        /// The idea group's content key, such as `defensive_ideas`.
        group: String,
    } -> (), "null once the game has taken it";

    /// Buys the next idea of an idea group the player has taken, as its
    /// button in the ideas view does, paying its cost in monarch points.
    /// `rejected` with the game's reasons when the game would not let the
    /// player buy it now, or when the group is not taken or has no idea left.
    game Buy "buy" {
        /// The idea group's content key.
        group: String,
    } -> (), "null once the game has bought it";

    /// Abandons an idea group the player has taken, as the ideas view's
    /// button for it does, losing the ideas bought in it. `rejected` when
    /// the player has not taken it, or the game would not let them abandon
    /// it now.
    game AbandonGroup "abandon_group" {
        /// The idea group's content key.
        group: String,
    } -> (), "null once the game has abandoned it";
}

/// An idea group of the ideas view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct IdeaGroup {
    /// Its content key, which the game's or a mod's idea files give it.
    pub group: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The kind of monarch points its ideas cost: `adm`, `dip` or `mil`.
    pub category: String,
    /// The player has taken it.
    pub taken: bool,
    /// How many of its ideas the player has bought.
    pub ideas: u32,
    /// How many ideas it has.
    pub of: u32,
    /// What its button does now: `take_group`, `buy`, or null when every
    /// idea is bought.
    pub button: Option<String>,
    /// The content key of the idea `buy` buys; null when none is left.
    pub next: Option<String>,
    /// Whether the player can press the button now, why not, and what it
    /// costs.
    #[serde(flatten)]
    pub choice: Choice,
}
