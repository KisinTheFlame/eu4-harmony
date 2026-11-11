//! The religion view: following a personal deity or a cult, through the
//! game's own commands, with the view's own checks (docs/research/religion.md).
use super::{action, conditions, content, power, GResult, Session, Tag};
use crate::objects::campaign::country;
use crate::{
    containers, game_string,
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, PowerKind, Resource};
use eu4_harmony_remote::ProvinceId;
use eu4_harmony_remote::religion::{args, ReligionOption};

/// The personal deities' database, made on first use by its getter
/// (`0x7e6910`): the deities at `+0x10`, a `std::vector` of pointers, in file
/// order.
const DEITIES: usize = 0x242bca8;
const DEITY_LIST: usize = 0x10;
/// A deity: its key, its conditions for being offered (`potential`) and for
/// being followed (`trigger`). A condition's reader is its part at
/// [`READER`], which the deity's reader hands the block.
const DEITY_KEY: usize = 0x10;
const DEITY_POTENTIAL: usize = 0x278;
const DEITY_TRIGGER: usize = 0x2f0;
const READER: usize = 0x38;
/// The game's no-deity, which a country follows while it follows none, and
/// which the deities' database holds among the deities.
const NO_DEITY: usize = 0x233fe90;
/// The cults: a `std::vector` of pointers of the game's own, its first and
/// its end, in file order, and the game's no-cult object (`nocult`), which a
/// country that uses cults follows while it has none, and which the vector
/// holds among the cults.
const CULTS: usize = 0x2328a78;
const NO_CULT: usize = 0x245b190;
/// A cult: its key and its conditions for being embraced (`allow`).
const CULT_KEY: usize = 0x10;
const CULT_ALLOW: usize = 0x200;
/// `(religion) -> bool`: whether the religion has personal deities, and
/// cults (and the expansion that brings them).
const HAS_DEITIES: GameFunction<1> = GameFunction::at(0xa7eda0);
const HAS_CULTS: GameFunction<1> = GameFunction::at(0xa7ee00);
/// `(country, reasons or null) -> bool`: whether the country may follow
/// another deity now (not during a regency), and embrace a cult now (not
/// during a regency, and only while it follows none), writing the regency's
/// line into the `std::string`.
const MAY_CHANGE_DEITY: GameFunction<2> = GameFunction::at(0x3d6e70);
const MAY_CHANGE_CULT: GameFunction<2> = GameFunction::at(0x3d7850);

/// A religion's church aspects, which a church of its chooses among
/// (Protestantism's), and its blessings (the Copts', the Zoroastrians'):
/// `std::vector`s of pointers to aspects (`common/church_aspects`).
const RELIGION_ASPECTS: usize = 0x730;
const RELIGION_BLESSINGS: usize = 0x760;
/// An aspect: its key, and its cost in church power, i32 thousandths.
const ASPECT_KEY: usize = 0x10;
const ASPECT_COST: usize = 0x358;
/// A country's church, at [`country::CHURCH`]: its church power, i32
/// thousandths, and its slots, a `std::vector` of pointers to the aspects
/// chosen, the game's no-aspect ([`NO_ASPECT`]) in a free one.
const CHURCH_POWER: usize = 0x10;
const CHURCH_SLOTS: usize = 0x20;
/// The game's no-aspect, a pointer to it.
const NO_ASPECT: usize = 0x2458be0;
/// `(church, aspect, reasons or null) -> bool`: whether the church may take
/// the aspect into a free slot now (a free slot, the church power), with the
/// church's tooltip lines; the command's operation 0.
const MAY_ADD_ASPECT: GameFunction<3> = GameFunction::at(0x7f8e70);
/// `(church, reasons or null) -> bool`: whether the church's aspects may be
/// changed now (stability), which removing one asks.
const MAY_CHANGE_ASPECTS: GameFunction<2> = GameFunction::at(0x7f9610);
/// `(country, blessing, reasons or null) -> bool`: whether the country may
/// take the blessing now (its religion's, the holy sites kept), the
/// blessing command's check.
const MAY_BLESS: GameFunction<3> = GameFunction::at(0x37adb0);
/// The church aspect command's operations: taking an aspect into a free
/// slot, and removing one.
const ADD: u8 = 0;
const REMOVE: u8 = 1;

/// A country's religious reforms ([`country::RELIGIOUS_REFORMS`]): entries of
/// [`REFORMS_ENTRY`] bytes, each a reform group (`common/religious_reforms`)
/// and, from [`REFORMS_PASSED`], a `std::vector` of i32, one for each of the
/// group's reforms, nonzero once it is passed.
const REFORMS_ENTRY: usize = 0x20;
const REFORMS_PASSED: usize = 8;
/// A reform group, laid out as an idea group: its reforms (a `std::vector`
/// of pointers), its `can_buy_idea` and its category, the kind of monarch
/// power a reform is spent in.
const REFORM_GROUP_REFORMS: usize = 0x30;
const REFORM_GROUP_CAN_BUY: usize = 0xc0;
const REFORM_GROUP_CATEGORY: usize = 0x570;
/// A reform, an idea of the reforms' own database: its key and its index
/// there, which the spending names it by.
const REFORM_KEY: usize = 0x158;
const REFORM_INDEX: usize = 0x1a0;
/// The spending's kind for passing a religious reform.
const BUY_RELIGIOUS_REFORM: i32 = 31;
/// `(country, tag out, i32 out)`: whom the country would learn from in
/// reforming its religion; the tag out's valid byte (`+7`) is set when there
/// is one, as the reform command's check asks.
const REFORM_SPONSOR: GameFunction<3> = GameFunction::at(0x3cd850);

/// The fervor effects' database (`common/fervor`), made on first use by its
/// getter (`0x735500`): the effects at `+0x10`, a `std::vector` of pointers,
/// in file order, which the toggle command names an effect by its place in.
const FERVOR_EFFECTS: usize = 0x242bc20;
const FERVOR_LIST: usize = 0x10;
/// A fervor effect, of class [`FERVOR_CLASS`]: its key, which the toggle
/// command names it by, its cost in fervor, whole, and its `potential`,
/// which the view lists it by. The list also holds the game's placeholder,
/// `nullfervor`, of another class.
const FERVOR_CLASS: usize = 0x1cae138;
const FERVOR_KEY: usize = 0x10;
const FERVOR_COST: usize = 0x5c;
const FERVOR_POTENTIAL: usize = 0x210;
/// A country's fervor ([`country::FERVOR`]): the effects active, a
/// `std::vector` of pointers, and its fervor, i32 thousandths.
const FERVOR_ACTIVE: usize = 0x10;
const FERVOR_VALUE: usize = 0x28;
/// `(religion) -> bool`: whether the religion uses fervor (the Reformed).
const HAS_FERVOR: GameFunction<1> = GameFunction::at(0xa7b5c0);
/// `(fervor, effect, reasons or null) -> bool`: whether the country may turn
/// the effect on now, with the view's lines: its conditions and its cost.
const MAY_ENABLE_FERVOR: GameFunction<3> = GameFunction::at(0x736a40);

/// `(religion) -> bool`: whether the religion harmonizes (Confucianism).
const HAS_HARMONY: GameFunction<1> = GameFunction::at(0xa7eed0);
/// `(country, list, i32 out or null, _)`: the religions and religion groups
/// the country may harmonize with now, into a list of [`CANDIDATE`]-byte
/// entries, each a religion of its own religion's group (and no group) or a
/// group of another (and no religion), as the harmonize command's check
/// asks; the list is the game's small vector of [`CANDIDATE_LIST`]'s class,
/// its storage inline.
const HARMONY_CANDIDATES: GameFunction<4> = GameFunction::at(0x3ec460);
const CANDIDATE_LIST: usize = 0x1c40840;
const CANDIDATE: usize = 0x18;
/// The most candidates the framework's list holds without the game growing
/// it: more religions than any game has.
const MOST_CANDIDATES: usize = 512;
/// A religion: its number in the religions' list, which the harmonize
/// command names it by, and its group; a group starts with a `std::vector`
/// of pointers to its religions.
const RELIGION_INDEX: usize = 0x70;
const RELIGION_GROUP: usize = 0xd8;

/// `(religion) -> bool`: whether the religion has icons (Orthodoxy).
const HAS_ICONS: GameFunction<1> = GameFunction::at(0xa7ec60);
/// A religion's icons, a `std::vector` of pointers, which the icon command
/// names one by its place in; an icon's key, a `std::string`, which the
/// game looks icons up by (`0xa7a8b0`).
const RELIGION_ICONS: usize = 0x7b8;
const ICON_KEY: usize = 0x158;
/// `(country, icon, reasons or null) -> bool`: whether the country may
/// commission the icon now, with the view's lines (the icon commissioned
/// already, too little patriarch authority, its conditions).
const MAY_COMMISSION_ICON: GameFunction<3> = GameFunction::at(0x404f20);
/// What an icon costs in patriarch authority, i32 thousandths
/// (`ORTHODOX_ICON_AUTHORITY_COST`).
const ICON_COST: usize = 0x233dc28;
/// `(state, reasons or null) -> bool`: whether a metropolitan may be
/// consecrated in the state, with the province view's lines.
const MAY_CONSECRATE: GameFunction<2> = GameFunction::at(0xabf060);

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x7e691ausize, "48 8b 05 87 53 c4 01 48 85 c0", "personal deities database").uses(&[DEITIES]),
        Point::keep(0x7e6961usize, "48 89 7b 10 48 89 7b 18 48 89 7b 20", "personal deities' vector").uses(&[DEITY_LIST]),
        Point::keep(0xaae096usize, "48 83 c3 10", "personal deity key").uses(&[DEITY_KEY]),
        Point::keep(0x7e5cd0usize, "48 81 c1 b0 02 00 00", "personal deity potential").uses(&[DEITY_POTENTIAL + READER]),
        Point::keep(0x7e5c39usize, "48 81 c1 28 03 00 00", "personal deity trigger").uses(&[DEITY_TRIGGER + READER]),
        Point::keep(0x7e5766usize, "48 89 1d 23 a7 b5 01", "no deity").uses(&[NO_DEITY]),
        Point::keep(0x7faf40usize, "48 8d 05 31 db b2 01", "cults").uses(&[CULTS]),
        Point::keep(0x1b5ec79usize, "48 3b 1d 00 9e 7c 00", "cults' end").uses(&[CULTS + 8]),
        Point::keep(0x7fae40usize, "48 8d 05 49 03 c6 01", "no cult").uses(&[NO_CULT]),
        Point::keep(0xaae5a6usize, "48 83 c3 10", "cult key").uses(&[CULT_KEY]),
        Point::keep(0x7fad78usize, "48 81 c1 38 02 00 00", "cult allow").uses(&[CULT_ALLOW + READER]),
        Point::keep(HAS_DEITIES, "48 83 ec 28 80 b9 17 01 00 00 00 74 4a", "religion has deities"),
        Point::keep(HAS_CULTS, "48 83 ec 28 80 b9 18 01 00 00 00 74 4a", "religion has cults"),
        Point::keep(MAY_CHANGE_DEITY, "40 53 48 83 ec 40 48 8b da 48 8b 81 80 12 00 00 48 85 c0 74 55 48 83 78 78 00", "may change deity"),
        Point::keep(MAY_CHANGE_CULT, "40 53 48 83 ec 40 48 8b da 48 8b 81 80 12 00 00 48 85 c0 74 55 48 83 78 78 00", "may change cult"),
        // The deity command's execute, and the cult check: the country's
        // deity and cult.
        Point::keep(0x3d6c12usize, "48 8b 89 a0 0f 00 00", "country's personal deity").uses(&[country::PERSONAL_DEITY]),
        Point::keep(0x3d78bausize, "48 8b 99 a8 0f 00 00", "country's cult").uses(&[country::CULT]),
        // The church's aspects (`0x7f8270`) and the blessing check: the
        // religion's lists.
        Point::keep(0x7f82a5usize, "48 05 30 07 00 00", "religion's church aspects").uses(&[RELIGION_ASPECTS]),
        Point::keep(0x37adebusize, "48 8b 90 68 07 00 00 48 8b 88 60 07 00 00", "religion's blessings").uses(&[RELIGION_BLESSINGS]),
        // The church aspect command's execute: the country's church; the
        // add check: the aspect's cost against the church power, the slots.
        Point::keep(0xaae7c7usize, "48 8b b8 68 0f 00 00", "country's church").uses(&[country::CHURCH]),
        Point::keep(0x7f8fe2usize, "8b 87 58 03 00 00 39 43 10", "aspect cost, church power").uses(&[ASPECT_COST, CHURCH_POWER]),
        Point::keep(0x7f8bf2usize, "48 8b 4b 28 48 8b 43 20", "church slots").uses(&[CHURCH_SLOTS]),
        Point::keep(0x7f8ed4usize, "4c 8b 05 05 fd c5 01", "no aspect").uses(&[NO_ASPECT]),
        // The blessing check: the country's blessings.
        Point::keep(0x37aef6usize, "48 8b bb 78 0f 00 00 48 2b bb 70 0f 00 00", "country's blessings").uses(&[country::BLESSINGS]),
        Point::keep(MAY_ADD_ASPECT, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55", "may add aspect"),
        Point::keep(MAY_CHANGE_ASPECTS, "48 89 5c 24 10 48 89 74 24 18 57 48 81 ec c0 00 00 00 48 8b", "may change aspects"),
        Point::keep(MAY_BLESS, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 40 48 8b 02 48 8b", "may take blessing"),
        // The reform command's check walks the country's reform groups and
        // their flags, then asks whom it would learn from.
        Point::keep(0x2af50dusize, "4c 8b 8e 50 14 00 00 48 8b be 58 14 00 00", "country's religious reforms").uses(&[country::RELIGIOUS_REFORMS]),
        Point::keep(0x2af581usize, "80 7c 24 3f 00 74 1a", "reform sponsor's valid byte"),
        // Adding an idea that is a reform finds its group's entry and the
        // reform among the group's reforms.
        Point::keep(0x3321d1usize, "4d 8b 03 4c 3b 87 98 01 00 00 75 d7 33 d2 4d 8b 50 30", "religious reform group's reforms").uses(&[REFORM_GROUP_REFORMS]),
        // The reform's button: the spending's power from the group's
        // category, its data the reform's index.
        Point::keep(0x139d9bbusize, "48 8b 87 98 01 00 00 8b 88 70 05 00 00", "reform's power").uses(&[REFORM_GROUP_CATEGORY]),
        Point::keep(0x139da22usize, "8b 87 a0 01 00 00", "reform's index").uses(&[REFORM_INDEX]),
        Point::keep(0x139da14usize, "ba 1f 00 00 00", "religious reform's spending"),
        Point::keep(REFORM_SPONSOR, "4c 89 44 24 18 48 89 4c 24 08 53 55 56 57 41 54 41 55 41 56 41 57", "religious reform sponsor"),
        // Fervor: the effects' database and its list, as the getter and the
        // loader keep them; an effect's fields, as its constructor and its
        // cost method keep them; the country's fervor, as the toggle
        // command's execute and the toggle itself read it.
        Point::keep(0x73550ausize, "48 8b 05 0f 67 cf 01 48 85 c0", "fervor effects database").uses(&[FERVOR_EFFECTS]),
        Point::keep(0x7363c7usize, "48 8d 4f 10 48 8b 57 18", "fervor effects' list").uses(&[FERVOR_LIST]),
        Point::keep(0x735150usize, "48 8d 71 10", "fervor effect key").uses(&[FERVOR_KEY]),
        Point::keep(0x735146usize, "48 8d 05 eb 8f 57 01", "fervor effect class").uses(&[FERVOR_CLASS]),
        Point::keep(0x735030usize, "8b 41 5c c3", "fervor effect cost").uses(&[FERVOR_COST]),
        Point::keep(0x735237usize, "49 8d bf 10 02 00 00", "fervor effect potential").uses(&[FERVOR_POTENTIAL]),
        Point::keep(0xaadc6busize, "48 8b 89 60 0f 00 00", "country's fervor").uses(&[country::FERVOR]),
        Point::keep(0x736f7busize, "48 8b 51 10", "fervor effects active").uses(&[FERVOR_ACTIVE]),
        Point::keep(0x736fb2usize, "01 4e 28", "fervor value").uses(&[FERVOR_VALUE]),
        Point::keep(HAS_FERVOR, "48 83 ec 28 80 b9 11 01 00 00 00 74 4a", "religion has fervor"),
        Point::keep(MAY_ENABLE_FERVOR, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55 41 56 41 57", "may enable fervor effect"),
        // Harmonization: the check's list of candidates, the religion's
        // group it compares, and what it reads of a harmonization going on;
        // the button names a religion by its number.
        Point::keep(HAS_HARMONY, "40 57 48 83 ec 20 80 3d 4f 10 8c 01 00 48 8b f9", "religion harmonizes"),
        Point::keep(HARMONY_CANDIDATES, "48 89 5c 24 08 44 88 4c 24 20 4c 89 44 24 18 55 56 57 41 54 41 55 41 56 41 57", "harmonization candidates"),
        Point::keep(0xab0bedusize, "48 c7 44 24 30 0a 00 00 00 48 8d 2d 43 fc 18 01", "harmonization candidates' list").uses(&[CANDIDATE_LIST]),
        Point::keep(0xab0c74usize, "49 8b 90 d8 00 00 00 48 39 90 d8 00 00 00", "religion's group").uses(&[RELIGION_GROUP]),
        Point::keep(0xe40641usize, "8b 46 70", "religion's number").uses(&[RELIGION_INDEX]),
        Point::keep(0xab0d04usize, "48 83 b8 40 10 00 00 00 75 0a 48 83 b8 48 10 00 00 00", "harmonizing now")
            .uses(&[country::HARMONIZING, country::HARMONIZING_GROUP]),
        // Icons: the religion's list, as the icon command's check reads it;
        // the icon's name, the icon commissioned, the authority and its cost,
        // as the check's lines read them.
        Point::keep(HAS_ICONS, "40 57 48 83 ec 20 80 3d bb 12 8c 01 00 48 8b f9", "religion has icons"),
        Point::keep(0xab17f5usize, "4c 8b 88 b8 07 00 00 48 8b 90 c0 07 00 00", "religion's icons").uses(&[RELIGION_ICONS]),
        Point::keep(0xa7aa40usize, "48 8d 8f 58 01 00 00", "icon key").uses(&[ICON_KEY]),
        Point::keep(0x404fa3usize, "4d 3b b7 90 0f 00 00", "icon commissioned").uses(&[country::ICON]),
        Point::keep(0x405015usize, "8b 05 0d 8c f3 01 41 39 87 88 0f 00 00", "icon cost and patriarch authority")
            .uses(&[ICON_COST, country::PATRIARCH_AUTHORITY]),
        Point::keep(MAY_COMMISSION_ICON, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55 41 56 41 57 48 8d ac 24 c0 fd ff ff", "may commission icon"),
        Point::keep(MAY_CONSECRATE, "48 89 5c 24 10 48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 60 fe ff ff", "may consecrate metropolitan"),
        // The patriarch command's check finds the province's state.
        Point::keep(0xab0ef9usize, "48 8b 8c 0a e0 02 00 00", "patriarch command's state"),
    ]
}

/// The refusals the framework words itself.
pub mod checks {
    use crate::game::{Fail, Status};

    /// Of a deity or a cult when the player's religion has none.
    pub fn not_of_religion(what: &str) -> Fail {
        Fail::new(Status::Rejected, format!("the player's religion has no {what}"))
    }

    /// The line for a cult not embraced while one is followed, which the
    /// game's check says without words.
    pub const CULT_FOLLOWED: &str = "A cult is followed until the monarch changes.";

    /// The lines for reforming the religion, which the game's check refuses
    /// without words: a reform not yet passed, and no one to learn from.
    pub const REFORMS_FIRST: &str = "Every religious reform must be passed first.";
    pub const NO_SPONSOR: &str = "There is no country to learn from: a European neighbor, or enough technology of our own, is needed.";
}

/// The deities, the game's no-deity left out.
fn deities(m: &Memory) -> GResult<Vec<usize>> {
    let none = m.ptr(m.at(NO_DEITY))?;
    match m.ptr(m.at(DEITIES))? {
        0 => Ok(Vec::new()),
        database => Ok(containers::pointers(m, database + DEITY_LIST, containers::MOST)?.into_iter().filter(|&d| d != none).collect()),
    }
}

/// The cults, the game's no-cult left out.
fn cults(m: &Memory) -> GResult<Vec<usize>> {
    let none = m.at(NO_CULT);
    Ok(containers::pointers(m, m.at(CULTS), containers::MOST)?.into_iter().filter(|&c| c != none).collect())
}

fn religion(m: &Memory, country: usize) -> GResult<usize> {
    Ok(m.ptr(country + country::STATE_RELIGION)?)
}

fn has(m: &Memory, function: GameFunction<1>, country: usize) -> GResult<bool> {
    match religion(m, country)? {
        0 => Ok(false),
        religion => Ok(function.call(m, [religion])? & 0xff != 0),
    }
}

/// Whether the player may follow `deity` now, and why not: the regency, then
/// the deity's conditions that do not hold.
fn deity_choice(m: &Memory, country: usize, player: Tag, deity: usize) -> GResult<Choice> {
    let (may, mut reasons) = content::checked(m, |t| Ok(MAY_CHANGE_DEITY.call(m, [country, t])?))?;
    let allowed = conditions::holds(m, deity + DEITY_TRIGGER, player, 0)?;
    if !allowed {
        reasons.extend(conditions::failing(m, deity + DEITY_TRIGGER, player, 0)?);
    }
    Ok(if may && allowed { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// Whether the player may embrace `cult` now, and why not: the regency or a
/// cult followed already, then the cult's conditions that do not hold.
fn cult_choice(m: &Memory, country: usize, player: Tag, cult: usize) -> GResult<Choice> {
    let (may, mut reasons) = content::checked(m, |t| Ok(MAY_CHANGE_CULT.call(m, [country, t])?))?;
    if !may && reasons.is_empty() {
        reasons.push(checks::CULT_FOLLOWED.to_string());
    }
    let allowed = conditions::holds(m, cult + CULT_ALLOW, player, 0)?;
    if !allowed {
        reasons.extend(conditions::failing(m, cult + CULT_ALLOW, player, 0)?);
    }
    Ok(if may && allowed { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

fn option(m: &Memory, action: &str, key: String, current: bool, choice: Choice) -> GResult<ReligionOption> {
    Ok(ReligionOption { action: action.into(), name: content::name(m, &key)?, key, current, province: None, choice })
}

/// The deities and cults the religion view offers the player.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Vec<ReligionOption>> {
    let mut out = Vec::new();
    if has(m, HAS_DEITIES, ctx.country)? {
        let current = m.ptr(ctx.country + country::PERSONAL_DEITY)?;
        for deity in deities(m)? {
            if deity != current && !conditions::holds(m, deity + DEITY_POTENTIAL, ctx.player, 0)? {
                continue;
            }
            let key = game_string::key(m, deity + DEITY_KEY)?;
            out.push(option(m, "choose_deity", key, deity == current, deity_choice(m, ctx.country, ctx.player, deity)?)?);
        }
    }
    if has(m, HAS_CULTS, ctx.country)? {
        let current = m.ptr(ctx.country + country::CULT)?;
        for cult in cults(m)? {
            let key = game_string::key(m, cult + CULT_KEY)?;
            out.push(option(m, "choose_cult", key, cult == current, cult_choice(m, ctx.country, ctx.player, cult)?)?);
        }
    }
    if let Some(church) = church(m, ctx.country)? {
        let chosen = slots(m, church)?;
        for aspect in religion_list(m, ctx.country, RELIGION_ASPECTS)? {
            let key = game_string::key(m, aspect + ASPECT_KEY)?;
            out.push(if chosen.contains(&aspect) {
                option(m, "remove_aspect", key, true, remove_choice(m, church)?)?
            } else {
                option(m, "add_aspect", key, false, add_choice(m, church, aspect)?)?
            });
        }
    }
    let taken = containers::pointers(m, ctx.country + country::BLESSINGS, containers::MOST)?;
    for blessing in religion_list(m, ctx.country, RELIGION_BLESSINGS)? {
        let key = game_string::key(m, blessing + ASPECT_KEY)?;
        out.push(option(m, "choose_blessing", key, taken.contains(&blessing), blessing_choice(m, ctx.country, blessing)?)?);
    }
    if let Some(fervor) = fervor(m, ctx.country)? {
        let active = containers::pointers(m, fervor + FERVOR_ACTIVE, containers::MOST)?;
        for effect in fervor_effects(m)? {
            let on = active.contains(&effect);
            if !on && !conditions::holds(m, effect + FERVOR_POTENTIAL, ctx.player, 0)? {
                continue;
            }
            let key = game_string::key(m, effect + FERVOR_KEY)?;
            let (action, choice) = if on { ("disable_fervor", Choice::can(Vec::new())) } else { ("enable_fervor", enable_choice(m, fervor, effect)?) };
            out.push(option(m, action, key, on, choice)?);
        }
    }
    if has(m, HAS_ICONS, ctx.country)? {
        let current = m.ptr(ctx.country + country::ICON)?;
        for icon in religion_list(m, ctx.country, RELIGION_ICONS)? {
            let key = game_string::key(m, icon + ICON_KEY)?;
            out.push(option(m, "commission_icon", key, icon == current, icon_choice(m, ctx.country, icon)?)?);
        }
        let name = content::name(m, "CONSECRATE_BUTTON")?;
        for (id, state) in states(m, &ctx)? {
            let choice = consecrate_choice(m, state)?;
            out.push(ReligionOption { action: "consecrate_metropolitan".into(), key: "consecrate_metropolitan".into(), name: name.clone(), current: false, province: Some(ProvinceId(id)), choice });
        }
    }
    if has(m, HAS_HARMONY, ctx.country)? {
        if let Some(now) = harmonizing(m, ctx.country)? {
            out.push(option(m, "stop_harmonizing", religion_key(m, now)?, true, Choice::can(Vec::new()))?);
        }
        for religion in harmony_candidates(m, ctx.country)? {
            out.push(option(m, "harmonize", religion_key(m, religion)?, false, Choice::can(Vec::new()))?);
        }
    }
    let reforms = reforms(m, ctx.country)?;
    if !reforms.is_empty() {
        for reform in &reforms {
            let choice = if reform.passed { Choice::can(Vec::new()) } else { reform_choice(m, ctx.country, ctx.player, reform)? };
            out.push(ReligionOption { action: "pass_reform".into(), name: content::name(m, &reform.key)?, key: reform.key.clone(), current: reform.passed, province: None, choice });
        }
        let choice = reform_religion_choice(m, ctx.country, &reforms)?;
        out.push(ReligionOption { action: "reform_religion".into(), key: "reform_religion".into(), name: content::name(m, "REFORM_RELIGION")?, current: false, province: None, choice });
    }
    out.extend(super::piety::options(m, &ctx)?);
    out.extend(super::curia::options(m, &ctx)?);
    Ok(out)
}

/// Whether the country may commission `icon` now, with the view's lines, and
/// its cost in patriarch authority, in percent as the view writes it.
fn icon_choice(m: &Memory, country: usize, icon: usize) -> GResult<Choice> {
    let (may, reasons) = content::checked(m, |t| Ok(MAY_COMMISSION_ICON.call(m, [country, icon, t])?))?;
    let cost = vec![Amount::new(Resource::PatriarchAuthority, f64::from(m.i32(m.at(ICON_COST))?) / 10.0)];
    Ok(if may { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// The player's states, each once: the number of its first province of the
/// player's and the state.
fn states(m: &Memory, ctx: &super::Ctx) -> GResult<Vec<(u32, usize)>> {
    let (ids, count) = (m.ptr(ctx.country + country::OWNED)?, m.i32(ctx.country + country::OWNED_COUNT)?);
    let mut out: Vec<(u32, usize)> = Vec::new();
    for i in 0..count.clamp(0, containers::MOST as i32) as usize {
        let id = m.u32(ids + i * 4)?;
        let province = super::with_state(|s| s.geography.province_address(m, ctx, id))?;
        let state = m.ptr(province + crate::objects::campaign::province::STATE)?;
        if state != 0 && out.iter().all(|&(_, s)| s != state) {
            out.push((id, state));
        }
    }
    Ok(out)
}

/// Whether a metropolitan may be consecrated in `state` now, with the
/// province view's lines.
fn consecrate_choice(m: &Memory, state: usize) -> GResult<Choice> {
    let (may, reasons) = content::checked(m, |t| Ok(MAY_CONSECRATE.call(m, [state, t])?))?;
    Ok(if may { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// A religion's key, the name of its token.
fn religion_key(m: &Memory, religion: usize) -> GResult<String> {
    let token = m.u32(religion + crate::objects::campaign::province::RELIGION_TOKEN)?;
    Ok(super::commands::type_name(m, token).ok_or_else(|| format!("no token {token:#x}"))?)
}

/// The religions the country may harmonize with now, as the game lists the
/// candidates: a religion of its own group as itself, another group as its
/// first religion, as the view's button names it, since harmonizing with
/// one of them harmonizes with the group.
fn harmony_candidates(m: &Memory, country: usize) -> GResult<Vec<usize>> {
    // The game's small vector: its class, its data, its capacity and count,
    // then its storage, all in the framework's memory.
    let mut list = vec![0u64; 3 + MOST_CANDIDATES * CANDIDATE / 8];
    let base = list.as_mut_ptr() as usize;
    list[0] = m.at(CANDIDATE_LIST) as u64;
    list[1] = (base + 0x18) as u64;
    list[2] = MOST_CANDIDATES as u64;
    HARMONY_CANDIDATES.call(m, [country, base, 0, 0])?;
    if list[1] != (base + 0x18) as u64 {
        return Err("the game grew the harmonization candidates' list".to_string().into());
    }
    let count = (list[2] >> 32) as usize;
    let mut out = Vec::new();
    for i in 0..count.min(MOST_CANDIDATES) {
        let (religion, group) = (list[3 + i * 3] as usize, list[4 + i * 3] as usize);
        if religion != 0 {
            out.push(religion);
        } else if group != 0 {
            // As the view's button names a group: by its first religion.
            match m.ptr(group)? {
                0 => {}
                first => match m.ptr(first)? {
                    0 => {}
                    religion => out.push(religion),
                },
            }
        }
    }
    Ok(out)
}

/// The religion the country is harmonizing with now, or the first of the
/// group: none while it harmonizes with none.
fn harmonizing(m: &Memory, country: usize) -> GResult<Option<usize>> {
    let religion = m.ptr(country + country::HARMONIZING)?;
    if religion != 0 {
        return Ok(Some(religion));
    }
    Ok(match m.ptr(country + country::HARMONIZING_GROUP)? {
        0 => None,
        group => match m.ptr(group)? {
            0 => None,
            first => Some(m.ptr(first)?).filter(|&r| r != 0),
        },
    })
}

/// The country's fervor, while its religion uses fervor.
fn fervor(m: &Memory, country: usize) -> GResult<Option<usize>> {
    let fervor = m.ptr(country + country::FERVOR)?;
    Ok((fervor != 0 && has(m, HAS_FERVOR, country)?).then_some(fervor))
}

/// The fervor effects, in the database's order.
fn fervor_effects(m: &Memory) -> GResult<Vec<usize>> {
    match m.ptr(m.at(FERVOR_EFFECTS))? {
        0 => Ok(Vec::new()),
        database => {
            let class = m.at(FERVOR_CLASS);
            let effects = containers::pointers(m, database + FERVOR_LIST, containers::MOST)?;
            Ok(effects.into_iter().filter(|&e| m.ptr(e).is_ok_and(|vtable| vtable == class)).collect())
        }
    }
}

/// Whether the country may turn `effect` on now, with the view's lines, and
/// what it costs.
fn enable_choice(m: &Memory, fervor: usize, effect: usize) -> GResult<Choice> {
    let (may, reasons) = content::checked(m, |t| Ok(MAY_ENABLE_FERVOR.call(m, [fervor, effect, t])?))?;
    let cost = vec![Amount::new(Resource::Fervor, f64::from(m.i32(effect + FERVOR_COST)?))];
    Ok(if may { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// One of the country's religious reforms.
struct Reform {
    key: String,
    group: usize,
    /// Its index in the reforms' database, which the spending names it by.
    index: i32,
    passed: bool,
}

/// The country's religious reforms, group by group, each in its group's
/// order.
fn reforms(m: &Memory, country: usize) -> GResult<Vec<Reform>> {
    let (begin, end) = (m.ptr(country + country::RELIGIOUS_REFORMS)?, m.ptr(country + country::RELIGIOUS_REFORMS + 8)?);
    let mut out = Vec::new();
    for i in 0..containers::span(begin, end, REFORMS_ENTRY, containers::MOST)? {
        let entry = begin + i * REFORMS_ENTRY;
        let group = m.ptr(entry)?;
        let (flags, flags_end) = (m.ptr(entry + REFORMS_PASSED)?, m.ptr(entry + REFORMS_PASSED + 8)?);
        let count = containers::span(flags, flags_end, 4, containers::MOST)?;
        for (n, reform) in containers::pointers(m, group + REFORM_GROUP_REFORMS, containers::MOST)?.into_iter().enumerate() {
            let passed = n < count && m.i32(flags + n * 4)? != 0;
            out.push(Reform { key: game_string::key(m, reform + REFORM_KEY)?, group, index: m.i32(reform + REFORM_INDEX)?, passed });
        }
    }
    Ok(out)
}

/// The kind of monarch power a reform group's reforms are spent in.
fn reform_power(m: &Memory, group: usize) -> GResult<PowerKind> {
    Ok(match m.u32(group + REFORM_GROUP_CATEGORY)? {
        1 => PowerKind::Dip,
        2 => PowerKind::Mil,
        _ => PowerKind::Adm,
    })
}

/// Whether the player may pass `reform` now: the spending's check, which
/// asks the group's `can_buy_idea` and the points, and the conditions that
/// do not hold.
fn reform_choice(m: &Memory, country: usize, player: Tag, reform: &Reform) -> GResult<Choice> {
    let power = reform_power(m, reform.group)?;
    let mut spending = power::Spending::new(m, BUY_RELIGIOUS_REFORM, power, reform.index)?;
    let points = spending.cost(m, player)?;
    let cost = if points > 0 { vec![Amount::new(power.resource(), f64::from(points))] } else { Vec::new() };
    if spending.allowed(m, player)? {
        return Ok(Choice::can(cost));
    }
    let mut reasons = conditions::failing(m, reform.group + REFORM_GROUP_CAN_BUY, player, 0)?;
    let have = power::points_of(m, country, power)?;
    if have < points {
        reasons.push(format!("passing it takes {points} points, and the player has {have}"));
    }
    Ok(Choice::cannot(reasons, cost))
}

/// Whether the player may reform its religion now: every reform passed, and
/// someone to learn from, as the reform command's check asks.
fn reform_religion_choice(m: &Memory, country: usize, reforms: &[Reform]) -> GResult<Choice> {
    let mut reasons = Vec::new();
    if reforms.iter().any(|r| !r.passed) {
        reasons.push(checks::REFORMS_FIRST.to_string());
    }
    // The game writes a tag and a level here, as into the check's own stack.
    let mut sponsor = [0u8; 8];
    let mut level = 0i32;
    REFORM_SPONSOR.call(m, [country, sponsor.as_mut_ptr() as usize, &mut level as *mut i32 as usize])?;
    if sponsor[7] == 0 {
        reasons.push(checks::NO_SPONSOR.to_string());
    }
    Ok(if reasons.is_empty() { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// One of the religion's lists of aspects, none without a religion.
fn religion_list(m: &Memory, country: usize, list: usize) -> GResult<Vec<usize>> {
    match religion(m, country)? {
        0 => Ok(Vec::new()),
        religion => Ok(containers::pointers(m, religion + list, containers::MOST)?),
    }
}

/// The country's church, while its religion has church aspects to choose
/// among.
fn church(m: &Memory, country: usize) -> GResult<Option<usize>> {
    let church = m.ptr(country + country::CHURCH)?;
    Ok((church != 0 && !religion_list(m, country, RELIGION_ASPECTS)?.is_empty()).then_some(church))
}

/// The aspects the church has chosen, its free slots left out.
fn slots(m: &Memory, church: usize) -> GResult<Vec<usize>> {
    let none = m.ptr(m.at(NO_ASPECT))?;
    Ok(containers::pointers(m, church + CHURCH_SLOTS, containers::MOST)?.into_iter().filter(|&a| a != 0 && a != none).collect())
}

/// An aspect's cost, in church power.
fn aspect_cost(m: &Memory, aspect: usize) -> GResult<Vec<Amount>> {
    Ok(vec![Amount::new(Resource::ChurchPower, f64::from(m.i32(aspect + ASPECT_COST)?) / 1000.0)])
}

/// Whether the church may take `aspect` into a free slot now, with the
/// church's lines, and its cost.
fn add_choice(m: &Memory, church: usize, aspect: usize) -> GResult<Choice> {
    let (may, reasons) = content::checked(m, |t| Ok(MAY_ADD_ASPECT.call(m, [church, aspect, t])?))?;
    let cost = aspect_cost(m, aspect)?;
    Ok(if may { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// Whether the church may give up one of its aspects now.
fn remove_choice(m: &Memory, church: usize) -> GResult<Choice> {
    let (may, reasons) = content::checked(m, |t| Ok(MAY_CHANGE_ASPECTS.call(m, [church, t])?))?;
    Ok(if may { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// Whether the country may take `blessing` now, with the game's lines.
fn blessing_choice(m: &Memory, country: usize, blessing: usize) -> GResult<Choice> {
    let (may, reasons) = content::checked(m, |t| Ok(MAY_BLESS.call(m, [country, blessing, t])?))?;
    Ok(if may { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

fn find(m: &Memory, list: Vec<usize>, key_at: usize, kind: &str, key: &str) -> GResult<usize> {
    for item in list {
        if game_string::key(m, item + key_at)? == key {
            return Ok(item);
        }
    }
    Err(content::checks::not_found(kind, key))
}

/// The commands the view's buttons post: the deity or the cult by its key
/// and the country.
fn deity_text(country: Tag, deity: &str) -> String {
    format!("personal_deity={{ personal_deity=\"{deity}\" country=\"{}\" }}", content::letters(country))
}

fn cult_text(country: Tag, cult: &str) -> String {
    format!("fetishist_cult={{ country=\"{}\" fetishist_cult=\"{cult}\" }}", content::letters(country))
}

/// Follows a personal deity.
pub fn choose_deity(Session { m, ctx, commands, .. }: Session, args::ChooseDeity { deity: key }: args::ChooseDeity) -> GResult<()> {
    if !has(m, HAS_DEITIES, ctx.country)? {
        return Err(checks::not_of_religion("personal deities"));
    }
    let deity = find(m, deities(m)?, DEITY_KEY, "personal deity", &key)?;
    if !conditions::holds(m, deity + DEITY_POTENTIAL, ctx.player, 0)? {
        return Err(content::checks::not_found("personal deity", &key));
    }
    action::carry_out(commands, &deity_choice(m, ctx.country, ctx.player, deity)?, "follow this deity", || Ok(deity_text(ctx.player, &key)))
}

/// Embraces a cult.
pub fn choose_cult(Session { m, ctx, commands, .. }: Session, args::ChooseCult { cult: key }: args::ChooseCult) -> GResult<()> {
    if !has(m, HAS_CULTS, ctx.country)? {
        return Err(checks::not_of_religion("cults"));
    }
    let cult = find(m, cults(m)?, CULT_KEY, "cult", &key)?;
    action::carry_out(commands, &cult_choice(m, ctx.country, ctx.player, cult)?, "embrace this cult", || Ok(cult_text(ctx.player, &key)))
}

/// The church aspect command, for `aspect` and the operation; with no
/// slot, which the game then finds itself (a free one to add to, the
/// aspect's own to remove).
fn aspect_text(country: Tag, aspect: &str, operation: u8) -> String {
    format!("change_church_aspect={{ aspect=\"{aspect}\" country=\"{}\" operation={operation} }}", content::letters(country))
}

fn blessing_text(country: Tag, blessing: &str) -> String {
    format!("change_blessing={{ aspect=\"{blessing}\" country=\"{}\" }}", content::letters(country))
}

/// The player's church, or the refusal for a religion without one.
fn own_church(m: &Memory, country: usize) -> GResult<usize> {
    church(m, country)?.ok_or_else(|| checks::not_of_religion("church aspects"))
}

/// Adds an aspect to the player's church.
pub fn add_aspect(Session { m, ctx, commands, .. }: Session, args::AddAspect { aspect: key }: args::AddAspect) -> GResult<()> {
    let church = own_church(m, ctx.country)?;
    let aspect = find(m, religion_list(m, ctx.country, RELIGION_ASPECTS)?, ASPECT_KEY, "church aspect", &key)?;
    if slots(m, church)?.contains(&aspect) {
        return Err(content::checks::not_now(format!("the church has {key} already")));
    }
    action::carry_out(commands, &add_choice(m, church, aspect)?, "add this aspect", || Ok(aspect_text(ctx.player, &key, ADD)))
}

/// Removes an aspect from the player's church.
pub fn remove_aspect(Session { m, ctx, commands, .. }: Session, args::RemoveAspect { aspect: key }: args::RemoveAspect) -> GResult<()> {
    let church = own_church(m, ctx.country)?;
    find(m, slots(m, church)?, ASPECT_KEY, "aspect of the church", &key)?;
    action::carry_out(commands, &remove_choice(m, church)?, "remove this aspect", || Ok(aspect_text(ctx.player, &key, REMOVE)))
}

/// Takes a blessing.
pub fn choose_blessing(Session { m, ctx, commands, .. }: Session, args::ChooseBlessing { blessing: key }: args::ChooseBlessing) -> GResult<()> {
    let blessings = religion_list(m, ctx.country, RELIGION_BLESSINGS)?;
    if blessings.is_empty() {
        return Err(checks::not_of_religion("blessings"));
    }
    let blessing = find(m, blessings, ASPECT_KEY, "blessing", &key)?;
    action::carry_out(commands, &blessing_choice(m, ctx.country, blessing)?, "take this blessing", || Ok(blessing_text(ctx.player, &key)))
}

/// The spending that passes a reform: the reform by its index in the
/// reforms' database, in its group's kind of power.
fn reform_text(country: Tag, index: i32, power: PowerKind) -> String {
    power::written(country.letters(), BUY_RELIGIOUS_REFORM, power, index)
}

fn reform_religion_text(country: Tag) -> String {
    format!("reform_religion_command={{ country=\"{}\" }}", content::letters(country))
}

/// Passes a religious reform.
pub fn pass_reform(Session { m, ctx, commands, .. }: Session, args::PassReform { reform: key }: args::PassReform) -> GResult<()> {
    let reforms = reforms(m, ctx.country)?;
    if reforms.is_empty() {
        return Err(checks::not_of_religion("religious reforms"));
    }
    let Some(reform) = reforms.iter().find(|r| r.key == key) else {
        return Err(content::checks::not_found("religious reform", &key));
    };
    if reform.passed {
        return Err(content::checks::not_now(format!("{key} is passed already")));
    }
    let power = reform_power(m, reform.group)?;
    action::carry_out(commands, &reform_choice(m, ctx.country, ctx.player, reform)?, "pass this reform", || Ok(reform_text(ctx.player, reform.index, power)))
}

/// Reforms the religion, once every reform is passed.
pub fn reform_religion(Session { m, ctx, commands, .. }: Session, _: args::ReformReligion) -> GResult<()> {
    let reforms = reforms(m, ctx.country)?;
    if reforms.is_empty() {
        return Err(checks::not_of_religion("religious reforms"));
    }
    action::carry_out(commands, &reform_religion_choice(m, ctx.country, &reforms)?, "reform the religion", || Ok(reform_religion_text(ctx.player)))
}

/// The fervor command, which turns the effect `key` on, or off when it is
/// on: its writer writes the effect by its key, and its reader looks the
/// key up.
fn fervor_text(country: Tag, key: &str) -> String {
    format!("toggle_fervor_effect={{ country=\"{}\" effect={key} }}", content::letters(country))
}

/// The player's fervor and the effect named `key` the view lists, or the
/// refusals.
fn fervor_effect(m: &Memory, ctx: &super::Ctx, key: &str) -> GResult<(usize, usize, bool)> {
    let fervor = fervor(m, ctx.country)?.ok_or_else(|| checks::not_of_religion("fervor"))?;
    let effect = find(m, fervor_effects(m)?, FERVOR_KEY, "fervor effect", key)?;
    let on = containers::pointers(m, fervor + FERVOR_ACTIVE, containers::MOST)?.contains(&effect);
    if !on && !conditions::holds(m, effect + FERVOR_POTENTIAL, ctx.player, 0)? {
        return Err(content::checks::not_found("fervor effect", key));
    }
    Ok((fervor, effect, on))
}

/// Turns a fervor effect on.
pub fn enable_fervor(Session { m, ctx, commands, .. }: Session, args::EnableFervor { effect: key }: args::EnableFervor) -> GResult<()> {
    let (fervor, effect, on) = fervor_effect(m, &ctx, &key)?;
    if on {
        return Err(content::checks::not_now(format!("{key} is on already")));
    }
    action::carry_out(commands, &enable_choice(m, fervor, effect)?, "turn this fervor effect on", || Ok(fervor_text(ctx.player, &key)))
}

/// Turns a fervor effect off.
pub fn disable_fervor(Session { m, ctx, commands, .. }: Session, args::DisableFervor { effect: key }: args::DisableFervor) -> GResult<()> {
    let (_, _, on) = fervor_effect(m, &ctx, &key)?;
    if !on {
        return Err(content::checks::not_now(format!("{key} is not on")));
    }
    action::carry_out(commands, &Choice::can(Vec::new()), "turn this fervor effect off", || Ok(fervor_text(ctx.player, &key)))
}

/// The icon command: the icon by its place in the religion's icons.
fn icon_text(country: Tag, icon: usize) -> String {
    format!("commission_icon={{ country=\"{}\" icon={icon} }}", content::letters(country))
}

/// The patriarch command: a province of the state.
fn consecrate_text(country: Tag, province: u32) -> String {
    format!("consecrate_patriarch_command={{ country=\"{}\" province={province} }}", content::letters(country))
}

/// Commissions an icon.
pub fn commission_icon(Session { m, ctx, commands, .. }: Session, args::CommissionIcon { icon: key }: args::CommissionIcon) -> GResult<()> {
    if !has(m, HAS_ICONS, ctx.country)? {
        return Err(checks::not_of_religion("icons"));
    }
    let icons = religion_list(m, ctx.country, RELIGION_ICONS)?;
    for (place, &icon) in icons.iter().enumerate() {
        if game_string::key(m, icon + ICON_KEY)? == key {
            return action::carry_out(commands, &icon_choice(m, ctx.country, icon)?, "commission this icon", || Ok(icon_text(ctx.player, place)));
        }
    }
    Err(content::checks::not_found("icon", &key))
}

/// Consecrates a metropolitan in a state of the player's.
pub fn consecrate_metropolitan(Session { m, ctx, commands, .. }: Session, args::ConsecrateMetropolitan { province: ProvinceId(id) }: args::ConsecrateMetropolitan) -> GResult<()> {
    if !has(m, HAS_ICONS, ctx.country)? {
        return Err(checks::not_of_religion("metropolitans"));
    }
    let province = super::with_state(|s| s.geography.province_address(m, &ctx, id))?;
    if m.u64(province + crate::objects::campaign::province::OWNER)? != ctx.player.0 {
        return Err(content::checks::not_now(format!("province {id} is not the player's")));
    }
    let state = m.ptr(province + crate::objects::campaign::province::STATE)?;
    if state == 0 {
        return Err(content::checks::not_now(format!("province {id} is in no state")));
    }
    action::carry_out(commands, &consecrate_choice(m, state)?, "consecrate a metropolitan there", || Ok(consecrate_text(ctx.player, id)))
}

/// The harmonize command: a religion by its number, or −1 to stop.
fn harmonize_text(country: Tag, religion: i32) -> String {
    format!("harmonize_with_religion={{ country=\"{}\" religion={religion} }}", content::letters(country))
}

/// Starts harmonizing with a religion, or its group when it is of another.
pub fn harmonize(Session { m, ctx, commands, .. }: Session, args::Harmonize { religion: key }: args::Harmonize) -> GResult<()> {
    if !has(m, HAS_HARMONY, ctx.country)? {
        return Err(checks::not_of_religion("harmony"));
    }
    for religion in harmony_candidates(m, ctx.country)? {
        if religion_key(m, religion)? == key {
            let number = m.i32(religion + RELIGION_INDEX)?;
            return action::carry_out(commands, &Choice::can(Vec::new()), "harmonize with this religion", || Ok(harmonize_text(ctx.player, number)));
        }
    }
    Err(content::checks::not_found("religion to harmonize with", &key))
}

/// Stops the harmonization going on.
pub fn stop_harmonizing(Session { m, ctx, commands, .. }: Session, _: args::StopHarmonizing) -> GResult<()> {
    if !has(m, HAS_HARMONY, ctx.country)? {
        return Err(checks::not_of_religion("harmony"));
    }
    if harmonizing(m, ctx.country)?.is_none() {
        return Err(content::checks::not_now("the player harmonizes with no religion now"));
    }
    action::carry_out(commands, &Choice::can(Vec::new()), "stop harmonizing", || Ok(harmonize_text(ctx.player, -1)))
}

crate::catalogue::bind! {
    eu4_harmony_remote::religion: ReligionRequest => ReligionAnswer;
    CommissionIcon => commission_icon;
    ConsecrateMetropolitan => consecrate_metropolitan;
    Harmonize => harmonize;
    StopHarmonizing => stop_harmonizing;
    EnableFervor => enable_fervor;
    DisableFervor => disable_fervor;
    Options => options;
    ChooseDeity => choose_deity;
    ChooseCult => choose_cult;
    AddAspect => add_aspect;
    RemoveAspect => remove_aspect;
    ChooseBlessing => choose_blessing;
    DefenderOfTheFaith => super::faith::defender;
    ClaimDefenderOfTheFaith => super::faith::claim;
    PassReform => pass_reform;
    ReformReligion => reform_religion;
    EnforceFaithfulAdherence => super::piety::enforce;
    CallOnReligiousFollowers => super::piety::call;
    TakePapalAction => super::curia::take_papal_action;
    CallEcumenicalCouncil => super::curia::call_ecumenical_council;
    BuyIndulgence => super::curia::buy_indulgence;
    InvestPapalInfluence => super::curia::invest_papal_influence;
    PickGoldenBull => super::curia::pick_golden_bull;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        buffers::{self, Image, Object},
        commands::record,
        As, Caller, Fail, Status,
    };

    fn tag(letters: &[u8; 3]) -> Tag {
        let mut bytes = [0u8; 8];
        bytes[..3].copy_from_slice(letters);
        Tag(u64::from_le_bytes(bytes))
    }

    #[test]
    fn the_commands_name_the_deity_or_the_cult_and_the_country() {
        assert_eq!(deity_text(tag(b"VIJ"), "shiva"), r#"personal_deity={ personal_deity="shiva" country="VIJ" }"#);
        assert_eq!(cult_text(tag(b"KON"), "yemoja_cult"), r#"fetishist_cult={ country="KON" fetishist_cult="yemoja_cult" }"#);
        assert_eq!(aspect_text(tag(b"SWE"), "allow_usury_aspect", ADD), r#"change_church_aspect={ aspect="allow_usury_aspect" country="SWE" operation=0 }"#);
        assert_eq!(aspect_text(tag(b"SWE"), "allow_usury_aspect", REMOVE), r#"change_church_aspect={ aspect="allow_usury_aspect" country="SWE" operation=1 }"#);
        assert_eq!(blessing_text(tag(b"ETH"), "legitimize_government"), r#"change_blessing={ aspect="legitimize_government" country="ETH" }"#);
        assert_eq!(reform_text(tag(b"AZT"), 3, PowerKind::Adm), r#"spend_power_command={ country="AZT" power={ data=3 category=31 skill=0 } }"#);
        assert_eq!(reform_religion_text(tag(b"AZT")), r#"reform_religion_command={ country="AZT" }"#);
        assert_eq!(fervor_text(tag(b"SWI"), "fervor_war"), r#"toggle_fervor_effect={ country="SWI" effect=fervor_war }"#);
        assert_eq!(harmonize_text(tag(b"MNG"), 14), r#"harmonize_with_religion={ country="MNG" religion=14 }"#);
        assert_eq!(harmonize_text(tag(b"MNG"), -1), r#"harmonize_with_religion={ country="MNG" religion=-1 }"#);
        assert_eq!(icon_text(tag(b"RUS"), 2), r#"commission_icon={ country="RUS" icon=2 }"#);
        assert_eq!(consecrate_text(tag(b"RUS"), 295), r#"consecrate_patriarch_command={ country="RUS" province=295 }"#);
    }

    /// A country whose state religion is none: neither deities nor cults are
    /// offered, and choosing one is refused before anything is posted.
    #[test]
    fn a_religion_without_them_offers_none() {
        let image = Image::new();
        let fra = tag(b"FRA");
        let country = Object::country(fra, country::RELIGIOUS_REFORMS + 0x10);
        let countries = Object::countries(&[&country]);
        let session = || buffers::session(image.memory(), 0, fra, country.at(), countries.at(), Caller::Remote(As::Player));
        assert!(options(session()).unwrap().is_empty());
        let Err(Fail { status, message }) = choose_deity(session(), args::ChooseDeity { deity: "shiva".into() }) else { panic!("a deity was followed") };
        assert_eq!((status, message.as_str()), (Status::Rejected, "the player's religion has no personal deities"));
        let Err(Fail { status, .. }) = choose_cult(session(), args::ChooseCult { cult: "yemoja_cult".into() }) else { panic!("a cult was embraced") };
        assert_eq!(status, Status::Rejected);
        assert!(record::posted().is_empty());
        drop((country, countries, image));
    }
}
