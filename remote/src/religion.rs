//! The religion view: the personal deity and the fetishist cult the
//! player's religion lets it follow, and choosing them as the view's
//! buttons do (docs/research/religion.md).
use crate::choices::Choice;
use crate::ProvinceId;
use serde::{Deserialize, Serialize};

group! {
    /// The religion view: personal deities and cults, a church's aspects,
    /// blessings, religious reforms, fervor, harmonizing, icons and
    /// metropolitans, piety's actions, the papacy and the curia, scholars,
    /// holy orders, a secondary religion, the gurus' teachings and the
    /// defender of the faith.
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
    /// once all are; piety's two actions when it uses piety (Islam); and
    /// when it has a papacy (Catholicism), the papal actions whose
    /// conditions for being offered hold, calling the ecumenical council,
    /// buying indulgence, investing papal influence in the curia and the
    /// golden bulls the curia controller may enact, the one enacted
    /// current; the schools of the religion's group when it has them
    /// (Islam), each to invite a scholar of, the player's own school
    /// current; every holy order the game has, each to establish in a
    /// state of the player's, with its own conditions; and the religions
    /// the player may take as its secondary one when its religion has one
    /// (Tengri's syncretic faith), the one it has current; and the
    /// teachings of its religion's gurus when it has them (Sikhism), each
    /// to take or, once taken, current. Each with whether the player can now and why not, in
    /// the game's words where it has them (the conditions that do not hold,
    /// a regency, a cult already followed until the monarch changes, too
    /// little church power, stability too low, all the church's slots
    /// taken, holy sites lost, piety too far from the action's end, an
    /// action used in the last years, too little papal influence, money or
    /// curia treasury, the curia locked, not the curia controller), and its
    /// cost. Empty for a religion with none
    /// of them.
    game Options "options" {} -> Vec<ReligionOption>, "each entry: action (choose_deity, choose_cult, add_aspect, remove_aspect, choose_blessing, enable_fervor, disable_fervor, harmonize, stop_harmonizing, commission_icon, consecrate_metropolitan, pass_reform, reform_religion, enforce_faithful_adherence, call_on_religious_followers, take_papal_action, call_ecumenical_council, buy_indulgence, invest_papal_influence, pick_golden_bull, invite_scholar, establish_holy_order, choose_secondary_religion or select_teaching), key, name, current, province, can, reasons, cost";

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

    /// Enforces faithful adherence (Islam), as the religion view's button
    /// does: with piety far enough toward Legalism, it lowers corruption
    /// and moves piety toward Mysticism by its cost; once every few years.
    /// `rejected` with the game's reasons when the player cannot now, as
    /// `religion.options` says beforehand.
    game EnforceFaithfulAdherence "enforce_faithful_adherence" {} -> (), "null once the game does it";

    /// Calls on religious followers (Islam), as the religion view's button
    /// does: with piety far enough toward Mysticism, it gives manpower and
    /// moves piety toward Legalism by its cost; once every few years.
    /// `rejected` with the game's reasons when the player cannot now, as
    /// `religion.options` says beforehand.
    game CallOnReligiousFollowers "call_on_religious_followers" {} -> (), "null once the game does it";

    /// Takes one of the papacy's actions (Catholicism), as the papacy
    /// view's button does, paying its papal influence, and from the curia's
    /// treasury what it costs there. `rejected` with the game's reasons
    /// when the player cannot now, as `religion.options` says beforehand;
    /// `not_found` for a key that names no action the view offers the
    /// player.
    game TakePapalAction "take_papal_action" {
        /// The action's content key, such as `bless_monarch`.
        action: String,
    } -> (), "null once the game takes it";

    /// Calls the ecumenical council, as the papacy view's button does,
    /// paying its price in ducats. `rejected` with the game's reasons when
    /// the player cannot now, as `religion.options` says beforehand.
    game CallEcumenicalCouncil "call_ecumenical_council" {} -> (), "null once the game calls it";

    /// Buys indulgence for the player's sins, as the papacy view's button
    /// does, paying its price in ducats. `rejected` with the game's reasons
    /// when the player cannot now, as `religion.options` says beforehand.
    game BuyIndulgence "buy_indulgence" {} -> (), "null once it is bought";

    /// Invests papal influence in becoming the next curia controller, as
    /// the papacy view's button does, as much as the button invests.
    /// `rejected` with the game's reasons when the player cannot now, as
    /// `religion.options` says beforehand.
    game InvestPapalInfluence "invest_papal_influence" {} -> (), "null once it is invested";

    /// Enacts a golden bull as the curia controller, as the papacy view's
    /// button does, paying from the curia's treasury. `rejected` with the
    /// game's reasons when the player cannot now, as `religion.options`
    /// says beforehand; `not_found` for a key that names no golden bull.
    game PickGoldenBull "pick_golden_bull" {
        /// The golden bull's content key, such as `apostolicae_servitutis`.
        bull: String,
    } -> (), "null once the game enacts it";
    /// Invites a scholar of a school of the player's religion's group
    /// (Islam), as the religion view's button does: from a country that
    /// follows the school and is willing, as the school's own conditions
    /// say with that country as FROM, its cost as they take it. `rejected`
    /// with the game's reasons when no country can send one now, as
    /// `religion.options` says beforehand; `not_found` for a key that names
    /// no school of the group.
    game InviteScholar "invite_scholar" {
        /// The school's content key, such as `hanafi_school`.
        school: String,
    } -> (), "null once the scholar is invited";

    /// Establishes a holy order in a state of the player's, as the
    /// province view's button does, paying its cost in monarch points.
    /// `rejected` with the game's reasons when the player cannot there now:
    /// the order's own conditions, the state's (a province of the state not
    /// the player's, …), an order established there already, too few
    /// points; `not_found` for a key that names no holy order.
    game EstablishHolyOrder "establish_holy_order" {
        /// The order's content key, such as `jesuit_order`.
        order: String,
        /// A province of the state.
        province: ProvinceId,
    } -> (), "null once the order is established";

    /// Takes a religion as the player's secondary one (Tengri's syncretic
    /// faith), as the religion view's convert button does, in place of the
    /// one it has. `not_found` for a key that names no religion the view
    /// offers now (those `religion.options` lists to choose).
    game ChooseSecondaryReligion "choose_secondary_religion" {
        /// The religion's content key, such as `sunni`.
        religion: String,
    } -> (), "null once the game takes it";

    /// Takes one of the religion's gurus' teachings (Sikhism), paying its
    /// cost in monarch points. `rejected` when the player has too few
    /// points, the only thing the game's check asks; `not_found` for a key
    /// that names no teaching of the player's religion's gurus.
    game SelectTeaching "select_teaching" {
        /// The teaching's content key, such as `share_and_consume`.
        teaching: String,
    } -> (), "null once the teaching is taken";
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
    /// The view's button that does it, the capability of this group that
    /// does it, as `religion.options` lists them.
    pub action: String,
    /// Its content key, which the game's or a mod's
    /// `common/personal_deities`, `common/fetishist_cults`,
    /// `common/church_aspects`, the papacy's actions in
    /// `common/religions` or `common/golden_bulls`, a school in
    /// `common/religions` or `common/holy_orders` give it; for a button of
    /// its own, such as `buy_indulgence`, the action's name.
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
