//! The religion view: the personal deity and the fetishist cult the
//! player's religion lets it follow, and choosing them as the view's
//! buttons do (docs/research/religion.md).
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The religion view: personal deities and cults.
    "religion": ReligionRequest, ReligionAnswer;

    /// What the religion view offers the player now, by content key: the
    /// personal deities when its religion has them (Hinduism, Norse), each
    /// whose conditions for being offered hold, and the cults when it has
    /// them (Fetishism, the Dreamtime), each the game has, with whether the
    /// player can follow each now and why not, in the game's words: the
    /// conditions that do not hold, a regency, a cult already followed
    /// until the monarch changes. Empty for a religion with neither.
    game Options "options" {} -> Vec<ReligionOption>, "each deity or cult: action (choose_deity or choose_cult), key, name, current, can, reasons, cost";

    /// Follows a personal deity, as the view's button for it does; it
    /// lasts until the monarch changes. `rejected` with the game's reasons
    /// when the player cannot now, as `religion.options` says beforehand;
    /// `not_found` for a key that names no deity.
    game ChooseDeity "choose_deity" {
        /// The deity's content key, such as `shiva` or `odin_deity`.
        deity: String,
    } -> (), "null once the game follows it";

    /// Embraces a cult, as the view's button for it does; it lasts until
    /// the monarch changes, and none can be chosen while one is followed.
    /// `rejected` with the game's reasons when the player cannot now, as
    /// `religion.options` says beforehand; `not_found` for a key that names
    /// no cult.
    game ChooseCult "choose_cult" {
        /// The cult's content key, such as `yemoja_cult`.
        cult: String,
    } -> (), "null once the game follows it";
}

/// A deity or a cult the religion view offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ReligionOption {
    /// The view's button that follows it: `choose_deity` or `choose_cult`,
    /// the capability of this group that does it.
    pub action: String,
    /// Its content key, which the game's or a mod's
    /// `common/personal_deities` or `common/fetishist_cults` give it.
    pub key: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The player follows it now.
    pub current: bool,
    /// Whether the player can follow it now, and why not.
    #[serde(flatten)]
    pub choice: Choice,
}
