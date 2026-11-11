//! The religion view: the personal deity and the fetishist cult the
//! player's religion lets it follow, and choosing them as the view's
//! buttons do (docs/research/religion.md).
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The religion view: personal deities and cults, a church's aspects,
    /// blessings.
    "religion": ReligionRequest, ReligionAnswer;

    /// What the religion view offers the player now, by content key: the
    /// personal deities when its religion has them (Hinduism, Norse), each
    /// whose conditions for being offered hold; the cults when it has
    /// them (Fetishism, the Dreamtime), each the game has; a church's
    /// aspects when it has a church (Protestantism), each its religion
    /// offers, to add or, once chosen, to remove; and the blessings when it
    /// has them (Coptic, Zoroastrian), each its religion offers. Each with
    /// whether the player can now and why not, in the game's words (the
    /// conditions that do not hold, a regency, a cult already followed
    /// until the monarch changes, too little church power, stability too
    /// low, all the church's slots taken, holy sites lost), and its cost.
    /// Empty for a religion with none of them.
    game Options "options" {} -> Vec<ReligionOption>, "each entry: action (choose_deity, choose_cult, add_aspect, remove_aspect or choose_blessing), key, name, current, can, reasons, cost";

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

    /// Adds an aspect to the player's church, in a free slot, as the
    /// church's button does, paying its church power. `rejected` with the
    /// game's reasons when the player cannot now, as `religion.options`
    /// says beforehand; `not_found` for a key that names no aspect the
    /// player's religion offers.
    game AddAspect "add_aspect" {
        /// The aspect's content key, such as `allow_usury_aspect`.
        aspect: String,
    } -> (), "null once the church has it";

    /// Removes an aspect from the player's church, as the church's button
    /// does, which the game lets the player do only while stability is
    /// high enough. `rejected` with the game's reasons when the player
    /// cannot now; `not_found` for a key that names no aspect the church
    /// has.
    game RemoveAspect "remove_aspect" {
        /// The aspect's content key.
        aspect: String,
    } -> (), "null once the church no longer has it";

    /// Chooses a blessing, as the religion view's button does. `rejected`
    /// with the game's reasons when the player cannot now, as
    /// `religion.options` says beforehand; `not_found` for a key that names
    /// no blessing the player's religion offers.
    game ChooseBlessing "choose_blessing" {
        /// The blessing's content key, such as `legitimize_government`.
        blessing: String,
    } -> (), "null once the game has taken it";
}

/// A deity, a cult, an aspect or a blessing the religion view offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ReligionOption {
    /// The view's button that does it: `choose_deity`, `choose_cult`,
    /// `add_aspect`, `remove_aspect` or `choose_blessing`, the capability of
    /// this group that does it.
    pub action: String,
    /// Its content key, which the game's or a mod's
    /// `common/personal_deities`, `common/fetishist_cults` or
    /// `common/church_aspects` give it.
    pub key: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The player follows it, its church has it or the player has taken
    /// it, now.
    pub current: bool,
    /// Whether the player can follow it now, and why not.
    #[serde(flatten)]
    pub choice: Choice,
}
