//! Cultures: the player's primary culture and the cultures it accepts, as
//! the government view's cultures window shows them, and its buttons that
//! make a culture the primary one, promote a culture to accepted and demote
//! an accepted one.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The government view's cultures window: the primary culture, the
    /// accepted cultures, promoting and demoting them.
    "culture": CultureRequest, CultureAnswer;

    /// The player's primary culture, the cultures it accepts, how many of
    /// its accepted culture slots are taken and how many it has, and each
    /// culture the window lists: the primary one, the accepted ones and
    /// every other culture of the player's provinces, with the development
    /// of the player's provinces of it and what the window's buttons offer
    /// for it now: set_primary for every one but the primary culture,
    /// promote for one not accepted, demote for an accepted one; each with
    /// whether the player can, why not as the button's tooltip says, and
    /// what it costs in diplomatic points.
    game Options "options" {} -> Cultures, "primary, accepted, slots (used, max), cultures (culture, name, development, primary, accepted, actions: action, can, reasons, cost)";

    /// Makes a culture the player's primary culture, as the window's select
    /// button does once the player confirms, paying its cost in diplomatic
    /// points. Refused with the game's reasons when the player cannot, as
    /// `culture.options` says beforehand; `not_found` for a key no culture
    /// has.
    game SetPrimary "set_primary" {
        /// The culture's content key, such as `castilian`.
        culture: String,
    } -> (), "null once the game has made it the primary culture";

    /// Promotes a culture to an accepted one, as the window's promote button
    /// does once the player confirms, taking one of the player's accepted
    /// culture slots and paying its cost in diplomatic points. Refused with
    /// the game's reasons when the player cannot.
    game Promote "promote" {
        /// The culture's content key.
        culture: String,
    } -> (), "null once the game has accepted it";

    /// Demotes an accepted culture, as the window's demote button does once
    /// the player confirms, paying its cost in diplomatic points. Refused
    /// with the game's reasons when the player cannot, and when the culture
    /// is the primary one or not accepted.
    game Demote "demote" {
        /// The culture's content key.
        culture: String,
    } -> (), "null once the game no longer accepts it";
}

/// The player's cultures and what the cultures window offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Cultures {
    /// The primary culture's content key; none when the player has none.
    pub primary: Option<String>,
    /// The cultures the player has promoted to accepted, by their content
    /// keys; the primary culture and the cultures accepted for being of its
    /// group (brother cultures) are not among them.
    pub accepted: Vec<String>,
    /// The accepted culture slots.
    pub slots: Slots,
    /// Each culture the window lists, the primary one first, then the
    /// accepted ones, then the others by their development, most first.
    pub cultures: Vec<CultureEntry>,
}

/// The accepted culture slots: how many are taken and how many the player
/// has, as the window shows them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Slots {
    pub used: i32,
    pub max: i32,
}

/// One culture the cultures window lists.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct CultureEntry {
    /// Its content key.
    pub culture: String,
    /// Its name, in the player language.
    pub name: String,
    /// The development of the player's provinces of this culture.
    pub development: f64,
    /// Whether it is the primary culture.
    pub primary: bool,
    /// Whether the player accepts it: the primary culture, a promoted one,
    /// or one of the primary culture's group the game accepts with it.
    pub accepted: bool,
    /// What the window's buttons offer for it now.
    pub actions: Vec<CultureAction>,
}

/// What the cultures window lets the player do with a culture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum CultureActionKind {
    /// Making it the primary culture.
    SetPrimary,
    /// Promoting it to an accepted culture.
    Promote,
    /// Demoting it from the accepted cultures.
    Demote,
}

/// One of the cultures window's buttons for a culture, and whether the
/// player can press it now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct CultureAction {
    pub action: CultureActionKind,
    #[serde(flatten)]
    pub choice: Choice,
}
