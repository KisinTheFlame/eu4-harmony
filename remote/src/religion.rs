//! The religion view: the personal deity and the fetishist cult the
//! player's religion lets it follow, and choosing them as the view's
//! buttons do (docs/research/religion.md).
use crate::choices::Choice;
use crate::ProvinceId;
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
    /// has them (Coptic, Zoroastrian), each its religion offers; the fervor
    /// effects when it uses fervor (the Reformed), each whose conditions for
    /// being offered hold, to turn on or, once on, off; the religions it
    /// may harmonize with now when it harmonizes (Confucianism), and
    /// stopping the harmonization going on; the icons when it has them
    /// (Orthodoxy), each to commission, and consecrating a metropolitan in
    /// each of the player's states, with the state's province; the
    /// religious reforms when it has them (Nahuatl, Inti, Mesoamerican),
    /// each to pass or, once passed, current, and reforming the religion
    /// once all are. Each with whether the player can now and why not, in
    /// the game's words where it has them (the conditions that do not hold,
    /// a regency, a cult already followed until the monarch changes, too
    /// little church power, stability too low, all the church's slots
    /// taken, holy sites lost), and its cost. Empty for a religion with none
    /// of them.
    game Options "options" {} -> Vec<ReligionOption>, "each entry: action (choose_deity, choose_cult, add_aspect, remove_aspect, choose_blessing, enable_fervor, disable_fervor, harmonize, stop_harmonizing, commission_icon, consecrate_metropolitan, pass_reform or reform_religion), key, name, current, province, can, reasons, cost";

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

    /// The defender of the faith of the player's religion, as the religion
    /// view shows it: the country that defends it now, if any, and what
    /// the view's button to claim it offers, with whether the player can,
    /// why not in the game's words (a religion with no defender, technology,
    /// money, a subject, a regency, a ruler who may not, the player already
    /// the defender, too little prestige against the one who is) and its
    /// cost in ducats.
    game DefenderOfTheFaith "defender_of_the_faith" {} -> DefenderOfTheFaith, "defender (a country or null), can, reasons, cost";

    /// Claims to be the defender of the faith of the player's religion, as
    /// the religion view's button does, paying its cost in ducats.
    /// `rejected` with the game's reasons when the player cannot now, as
    /// `religion.defender_of_the_faith` says beforehand.
    game ClaimDefenderOfTheFaith "claim_defender_of_the_faith" {} -> (), "null once the player defends it";

    /// Passes one of the religion's reforms (Nahuatl, Inti, Mesoamerican),
    /// as the religious reforms window's button does once the player
    /// confirms; they are passed in any order. `rejected` with the game's
    /// reasons when the player cannot now, as `religion.options` says
    /// beforehand; `not_found` for a key that names no reform of the
    /// player's.
    game PassReform "pass_reform" {
        /// The reform's content key, such as `nahuatl_reform_1`.
        reform: String,
    } -> (), "null once the reform is passed";

    /// Reforms the religion once every religious reform is passed, as the
    /// religious reforms window's button does: the country takes another
    /// religion, government and institutions, learning from a neighbor or
    /// from its own technology. `rejected` when the player cannot now, as
    /// `religion.options` says beforehand.
    game ReformReligion "reform_religion" {} -> (), "null once the game reforms it";

    /// Commissions an Orthodox icon, as the religion view's button does,
    /// paying its patriarch authority; it lasts twenty years.
    /// `rejected` with the game's reasons when the player cannot now, as
    /// `religion.options` says beforehand; `not_found` for a key that names
    /// no icon of the player's religion.
    game CommissionIcon "commission_icon" {
        /// The icon's name, as `religion.options` gives it in `key`, such as
        /// `icon_michael`.
        icon: String,
    } -> (), "null once the icon is commissioned";

    /// Consecrates a metropolitan in a state of the player's (Orthodoxy),
    /// as the province view's button does. `rejected` with the game's
    /// reasons when the player cannot there now, as `religion.options` says
    /// beforehand.
    game ConsecrateMetropolitan "consecrate_metropolitan" {
        /// A province of the state.
        province: ProvinceId,
    } -> (), "null once the metropolitan is consecrated";

    /// Starts harmonizing with a religion (Confucianism), as the religion
    /// view's button does; with a religion of another group the game
    /// harmonizes with the whole group. `not_found` for a key that names
    /// no religion the game offers to harmonize with now (those of the
    /// player's provinces, with enough development there, not harmonized
    /// with yet), as `religion.options` lists them.
    game Harmonize "harmonize" {
        /// The religion's content key, such as `sunni`.
        religion: String,
    } -> (), "null once the game harmonizes with it";

    /// Stops the harmonization going on, as the religion view's button
    /// does. `rejected` while there is none.
    game StopHarmonizing "stop_harmonizing" {} -> (), "null once the game stops";

    /// Turns a fervor effect on (the Reformed), as the religion view's
    /// button does, paying its cost in fervor. `rejected` with the game's
    /// reasons when the player cannot now, as `religion.options` says
    /// beforehand; `not_found` for a key that names no effect the view
    /// offers the player.
    game EnableFervor "enable_fervor" {
        /// The effect's content key, such as `fervor_trade`.
        effect: String,
    } -> (), "null once the effect is on";

    /// Turns a fervor effect off, as the religion view's button does.
    /// `rejected` when it is not on; `not_found` for a key that names no
    /// effect the view offers the player.
    game DisableFervor "disable_fervor" {
        /// The effect's content key.
        effect: String,
    } -> (), "null once the effect is off";
}

/// The defender of the faith of the player's religion, and claiming it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct DefenderOfTheFaith {
    /// The country that defends the player's religion now; none while no
    /// country does.
    pub defender: Option<crate::CountryTag>,
    /// Whether the player can claim it now, why not, and what it costs.
    #[serde(flatten)]
    pub choice: Choice,
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
    /// For consecrating a metropolitan, a province of the state it is
    /// consecrated in, as `religion.consecrate_metropolitan` takes it; none
    /// for the rest.
    pub province: Option<ProvinceId>,
    /// Whether the player can follow it now, and why not.
    #[serde(flatten)]
    pub choice: Choice,
}
