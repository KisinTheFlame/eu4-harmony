//! Province administration: what the province view's buttons do in a
//! province (docs/research/province-view.md). Each action checks as its button
//! does, with the reasons its tooltip gives, and posts the game's own command
//! in its written form through the session's commands ([`super::action`]);
//! the command's own check is asked again there.
use super::commands::{write, Block};
use super::{action, conditions, fail, with_state, Ctx, GResult, Session, Status, Tag};
use crate::containers;
use crate::{
    game_string,
    memory::Memory,
    objects::{
        campaign::{construction, country, province as field},
        list,
    },
    points::{GameFunction, Point},
};
use eu4_harmony_remote::provinces::args;
use eu4_harmony_remote::ProvinceId;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    provinces::ProvinceOption,
};

/// The three kinds of development, in the game's order, which is also the
/// monarch power each is developed with: tax with administrative points,
/// production with diplomatic, manpower with military.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Development {
    Tax,
    Production,
    Manpower,
}

impl Development {
    pub const ALL: [Development; 3] = [Self::Tax, Self::Production, Self::Manpower];

    fn index(self) -> usize {
        self as usize
    }

    fn power(self) -> Resource {
        [Resource::Adm, Resource::Dip, Resource::Mil][self.index()]
    }

    fn name(self) -> &'static str {
        ["tax", "production", "manpower"][self.index()]
    }
}

/// One of the province view's actions on a province, as its options name
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Develop(Development),
    Exploit(Development),
    RaiseAutonomy,
    LowerAutonomy,
    MakeState,
    MakeTerritory,
    CancelEdict,
    Core,
    CancelCore,
    ChangeCulture,
    CancelCulture,
}

impl Action {
    /// Every action but setting an edict, which is one for each edict.
    const ALL: [Action; 15] = [
        Self::Develop(Development::Tax),
        Self::Develop(Development::Production),
        Self::Develop(Development::Manpower),
        Self::Exploit(Development::Tax),
        Self::Exploit(Development::Production),
        Self::Exploit(Development::Manpower),
        Self::RaiseAutonomy,
        Self::LowerAutonomy,
        Self::MakeState,
        Self::MakeTerritory,
        Self::CancelEdict,
        Self::Core,
        Self::CancelCore,
        Self::ChangeCulture,
        Self::CancelCulture,
    ];

    /// The capability's own name.
    pub fn name(self) -> String {
        match self {
            Self::Develop(kind) => format!("develop_{}", kind.name()),
            Self::Exploit(kind) => format!("exploit_{}", kind.name()),
            Self::RaiseAutonomy => "raise_autonomy".into(),
            Self::LowerAutonomy => "lower_autonomy".into(),
            Self::MakeState => "make_state".into(),
            Self::MakeTerritory => "make_territory".into(),
            Self::CancelEdict => "cancel_edict".into(),
            Self::Core => "core".into(),
            Self::CancelCore => "cancel_core".into(),
            Self::ChangeCulture => "change_culture".into(),
            Self::CancelCulture => "cancel_culture".into(),
        }
    }

    /// What the player does, for a refusal with no reason of the game's.
    fn doing(self) -> &'static str {
        match self {
            Self::Develop(_) => "develop it",
            Self::Exploit(_) => "exploit it",
            Self::RaiseAutonomy => "raise its autonomy",
            Self::LowerAutonomy => "lower its autonomy",
            Self::MakeState => "make it a state",
            Self::MakeTerritory => "make it a territory",
            Self::CancelEdict => "cancel the edict",
            Self::Core => "core it",
            Self::CancelCore => "cancel coring it",
            Self::ChangeCulture => "change its culture",
            Self::CancelCulture => "cancel changing its culture",
        }
    }
}

// ---------- the game's checks, with the reasons their tooltips give ----------

/// Whether a country may develop a province: (country, province, kind,
/// reasons, describe), the reasons appended to a `std::string`; when it may,
/// the string gets the effect instead.
const CAN_DEVELOP: GameFunction<5> = GameFunction::at(0x3e6ec0);
/// Whether a country may exploit a province's development: (country, kind,
/// province, reasons).
const CAN_EXPLOIT: GameFunction<4> = GameFunction::at(0x407350);
/// The raise and lower autonomy buttons' tooltips: (country, province,
/// reasons), whether it may, with the reason why not, or the effect.
const CAN_RAISE_AUTONOMY: GameFunction<3> = GameFunction::at(0x3dc1c0);
const CAN_LOWER_AUTONOMY: GameFunction<3> = GameFunction::at(0x3dca00);
/// Whether a country may make the province's area a state, and abandon its
/// state there: (country, province, reasons).
const CAN_MAKE_STATE: GameFunction<3> = GameFunction::at(0x3f34f0);
const CAN_MAKE_TERRITORY: GameFunction<3> = GameFunction::at(0x3f3bd0);
/// Whether a country state may take an edict, or none to cancel it: (state,
/// edict or 0, reasons), the reasons, or the effect.
const CAN_SET_EDICT: GameFunction<3> = GameFunction::at(0xabcbb0);
/// Whether a country may core a province: (country, province, reasons, 0,
/// 0, 1), as the core button asks it.
const CAN_CORE: GameFunction<6> = GameFunction::at(0x382930);
/// Whether a country may change a province's culture: (country, province,
/// reasons).
const CAN_CHANGE_CULTURE: GameFunction<3> = GameFunction::at(0x38ba40);

// ---------- prices ----------

/// The spending kinds of developing (with the power developed) and coring
/// (with administrative points).
const DEVELOPING: usize = 7;
const CORING: usize = 17;
/// The diplomatic points changing a province's culture costs: (province,
/// country tag, culture, explanation or 0, primary or 0). The culture button
/// says so when the player has fewer, with this text, its `$COST$` and
/// `$VAL$` the price and the points.
const CULTURE_PRICE: GameFunction<5> = GameFunction::at(0xa14e60);
const NO_POINTS: &std::ffi::CStr = c"NOCHA_NO_POINTS";
/// The game's lookup of a localisation with two values: (out, key, name,
/// value, name, value), each value a `std::string`.
const LOCALIZE_TWO: GameFunction<6> = GameFunction::at(0x8edc0);

// ---------- objects ----------

/// A state's country tag; its edict and the edict's key are the province's
/// ([`field::STATE`]).
const STATE_TAG: usize = 0x1c;
/// The edicts' database, its edicts by index a vector of pointers at
/// `+0x10`; an edict's index (its key is [`field::EDICT_KEY`]).
const EDICTS: usize = 0x242bdc8;
const EDICTS_LIST: usize = 0x10;
const EDICT_INDEX: usize = 0x2d0;
/// The most edicts believed.
const MOST_EDICTS: usize = 1000;
/// The kinds of a core's and a culture's construction.
const CORE_CONSTRUCTION: u32 = 0x2dfd;
const CULTURE_CONSTRUCTION: u32 = 0x2dfe;
/// The most constructions believed in one province.
pub const MOST_CONSTRUCTIONS: usize = 1000;

pub fn points() -> Vec<Point> {
    let prologue = "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55";
    vec![
        Point::keep(CAN_DEVELOP, "48 89 5c 24 10 44 89 44 24 18 48 89 4c 24 08 55 56 57 41 54", "develop check"),
        Point::keep(CAN_EXPLOIT, prologue, "exploit check"),
        Point::keep(CAN_RAISE_AUTONOMY, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24", "raise autonomy check"),
        Point::keep(CAN_LOWER_AUTONOMY, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24", "lower autonomy check"),
        Point::keep(CAN_MAKE_STATE, "48 89 5c 24 18 48 89 54 24 10 48 89 4c 24 08 55 56 57 41 54", "make state check"),
        Point::keep(CAN_MAKE_TERRITORY, "48 89 5c 24 18 48 89 74 24 20 55 57 41 54 41 56 41 57 48 8d", "abandon state check"),
        Point::keep(CAN_SET_EDICT, prologue, "edict check"),
        // It packs its arguments for the core check and calls it.
        Point::keep(CAN_CORE, "48 83 ec 48 0f b6 44 24 78 48 89 54 24 20 0f b6 54 24 70 88", "core check"),
        Point::keep(CAN_CHANGE_CULTURE, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24", "culture check"),
        Point::keep(LOCALIZE_TWO, "48 89 5c 24 10 48 89 6c 24 18 48 89 4c 24 08 56 57 41 56 48", "localisation of two values"),
        Point::keep(CULTURE_PRICE, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55", "culture price"),
        // The province view's culture button converts to the primary
        // culture; the culture command writes a culture's token.
        Point::keep(0x137b682usize, "48 8b 8f 28 12 00 00", "primary culture").uses(&[country::PRIMARY_CULTURE]),
        // The province writer in saves writes its culture.
        Point::keep(0x2706eausize, "48 8b 47 58 ba 3a 28 00 00 48 8b ce 8b 58 10", "culture token").uses(&[field::CULTURE_TOKEN]),
        // The edict command's check asks the province's state, its execute
        // replaces the state's edict, its writer writes the edict's index,
        // its reader takes the edict by index from the database.
        Point::keep(0x2bd65dusize, "49 8b 84 10 e0 02 00 00", "province state").uses(&[field::STATE]),
        Point::keep(0xabd57fusize, "48 8b 41 28 48 8b fa 48 8b d9 48 85 c0 74 06 48 8b 70 10 eb 02 33 f6", "state edict")
            .uses(&[field::STATE_EDICT, field::EDICT_DEFINITION]),
        Point::keep(0xabd5a4usize, "48 8b 51 1c", "state country").uses(&[STATE_TAG]),
        Point::keep(0x2bd42cusize, "48 8b 46 60 48 85 c0 74 14 44 8b 80 d0 02 00 00", "edict index").uses(&[EDICT_INDEX]),
        Point::keep(0xabaa0ausize, "48 8b 05 b7 13 97 01", "edicts database").uses(&[EDICTS]),
        Point::keep(0x2bd4c9usize, "48 8b 43 10", "edicts by index").uses(&[EDICTS_LIST]),
        // The cancel building check walks the constructions for a building's.
        Point::keep(
            0xa1b58busize,
            "48 8b 86 d8 04 00 00 48 85 c0 74 25 66 0f 1f 84 00 00 00 00 00 48 8b 08 81 79 08 c7 28 00 00 0f 84 a7 00 00 00 48 8b 48 10",
            "province constructions",
        )
        .uses(&[field::CONSTRUCTIONS, construction::KIND, list::NEXT]),
        // The command types: their written forms are made here.
        Point::method(0x1c554c0, 0x40, 0x2a76d0, "spend power check"),
        Point::method(0x1c38810, 0x40, 0xc6250, "exploit check"),
        Point::method(0x1cdfea8, 0x40, 0xaaa190, "raise autonomy command check"),
        Point::method(0x1cdfe00, 0x40, 0xaaa560, "lower autonomy command check"),
        Point::method(0x1c8d6c0, 0x40, 0x5dc560, "make state command check"),
        Point::method(0x1c8d768, 0x40, 0x5dc760, "abandon state command check"),
        Point::method(0x1c53678, 0x40, 0x2bd640, "edict command check"),
        Point::method(0x1c4de00, 0x40, 0x270900, "culture command check"),
        Point::method(0x1c4dc08, 0x48, 0x274000, "cancel core execute"),
        Point::method(0x1c4dcb0, 0x48, 0x273e80, "cancel culture execute"),
    ]
}

// ---------- the written forms ----------

/// A use of monarch power: the spending's kind, the province, the power
/// (0 administrative, 1 diplomatic, 2 military); its second ID left as its constructor leaves it (the reader does not read it back),
/// as its constructor leaves it.
fn spend(tag: Tag, kind: usize, province: u32, power: usize) -> String {
    let power = Block::new().word("data", province).word("category", kind).word("skill", power);
    write("spend_power_command", Block::new().tag("country", tag.letters()).block("power", power))
}

/// Exploiting: a tuple of the country, the kind and the province.
fn exploit(tag: Tag, kind: Development, province: u32) -> String {
    let members = Block::new().tag("dummy", tag.letters()).word("dummy", kind.index()).word("dummy", province);
    write("exploit_development", Block::new().block("members", members))
}

/// The commands of a country and a province: the autonomy commands
/// (`location`), the cancelling ones and the edict's (`province`).
fn country_and(kind: &str, tag: Tag, key: &str, province: u32) -> String {
    write(kind, Block::new().tag("country", tag.letters()).word(key, province))
}

/// The state commands, for the province's owner.
fn state(kind: &str, province: u32) -> String {
    write(kind, Block::new().word("province", province))
}

fn enact(tag: Tag, province: u32, index: u32) -> String {
    write("enact_edict", Block::new().tag("country", tag.letters()).word("province", province).word("which", index))
}

/// Changing a province's culture, for its owner, to a culture by its key.
fn culture(province: u32, key: &str) -> String {
    write("change_culture", Block::new().word("province", province).word("culture", key))
}

// ---------- reading ----------

/// A province the player acts on: its address and its owner.
struct Place {
    address: usize,
    owner: Option<Tag>,
}

fn place(m: &Memory, ctx: &Ctx, id: u32) -> GResult<Place> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    let address = with_state(|s| s.geography.province_address(m, ctx, id))?;
    let owner = Some(Tag(m.u64(address + field::OWNER)?)).filter(|t| t.valid());
    Ok(Place { address, owner })
}

/// What a check of the game's answers and writes: whether it may, and the
/// lines it wrote, as plain text, which are its reasons when it may not.
fn checked(m: &Memory, call: impl FnOnce(usize) -> crate::Result<usize>) -> GResult<(bool, Vec<String>)> {
    let (may, text) = game_string::written(m, |out| call(out).map_err(super::Fail::from))?;
    Ok((may as u8 != 0, reasons(&text)))
}

/// A tooltip's lines as plain text, without the bullets some begin with
/// (the core button's `- `, the exploit button's `－`).
pub fn reasons(text: &[u8]) -> Vec<String> {
    conditions::lines(text).into_iter().map(|line| super::unbulleted(&line).to_string()).collect()
}

/// The game's text for a localisation key with two values filled in, as
/// the game's lookup of two values writes it.
fn localized_with(m: &Memory, key: &std::ffi::CStr, values: [(&std::ffi::CStr, String); 2]) -> GResult<Vec<u8>> {
    let mut strings = [game_string::empty(), game_string::empty()];
    let at = [strings[0].as_mut_ptr() as usize, strings[1].as_mut_ptr() as usize];
    let filled = (|| -> GResult<Vec<u8>> {
        for (string, (_, value)) in at.iter().zip(&values) {
            game_string::append_checked(m, *string, value.as_bytes())?;
        }
        let names = [values[0].0.as_ptr() as usize, values[1].0.as_ptr() as usize];
        let call = |out| LOCALIZE_TWO.call(m, [out, key.as_ptr() as usize, names[0], at[0], names[1], at[1]]).map_err(super::Fail::from);
        Ok(game_string::written(m, call)?.1)
    })();
    for string in at {
        game_string::destroy_checked(m, string)?;
    }
    filled
}

/// A choice from a check: its reasons only when it may not.
fn choice((may, reasons): (bool, Vec<String>), cost: Vec<Amount>) -> Choice {
    if may {
        Choice::can(cost)
    } else {
        Choice::cannot(reasons, cost)
    }
}

/// The game text of the token `token`'s name: a culture's key.
fn token_name(m: &Memory, token: u32) -> GResult<String> {
    super::commands::type_name(m, token).ok_or_else(|| format!("no token {token}").into())
}

/// The key of the player's primary culture.
fn primary_culture(m: &Memory, ctx: &Ctx) -> GResult<(usize, String)> {
    let culture = m.ptr(ctx.country + country::PRIMARY_CULTURE)?;
    if culture == 0 {
        return fail(Status::Rejected, "the player has no primary culture");
    }
    Ok((culture, token_name(m, m.u32(culture + field::CULTURE_TOKEN)?)?))
}

/// The price of a use of monarch power, in whole points.
fn spending_price(m: &Memory, ctx: &Ctx, kind: usize, province: usize, power: usize) -> GResult<f64> {
    Ok(f64::from(super::power::Spending::about_province(m, kind, province, power)?.cost(m, ctx.player)?))
}

/// The edicts the game has, in their order: each one's address, key and
/// index.
pub fn edicts(m: &Memory) -> GResult<Vec<(usize, String, u32)>> {
    let database = m.ptr(m.at(EDICTS))?;
    if database == 0 {
        return Ok(Vec::new());
    }
    let begin = m.ptr(database + EDICTS_LIST)?;
    let count = containers::span(begin, m.ptr(database + EDICTS_LIST + 8)?, 8, MOST_EDICTS)?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let edict = m.ptr(begin + i * 8)?;
        let index = m.u32(edict + EDICT_INDEX)?;
        if index as usize != i {
            return Err(format!("edict {i} has the index {index}").into());
        }
        out.push((edict, game_string::key(m, edict + field::EDICT_KEY)?, index));
    }
    Ok(out)
}

/// The player's country state the province is in, when it is.
fn player_state(m: &Memory, ctx: &Ctx, place: &Place) -> GResult<Option<usize>> {
    let state = m.ptr(place.address + field::STATE)?;
    Ok(Some(state).filter(|&s| s != 0 && place.owner == Some(ctx.player)).filter(|&s| m.u64(s + STATE_TAG).ok() == Some(ctx.player.0)))
}

/// Whether the player is building a construction of `kind` in the province.
fn doing(m: &Memory, ctx: &Ctx, place: &Place, kind: u32) -> GResult<bool> {
    for node in containers::chain(m, m.ptr(place.address + field::CONSTRUCTIONS)?, list::NEXT, MOST_CONSTRUCTIONS)? {
        let construction = m.ptr(node)?;
        if m.u32(construction + construction::KIND)? == kind && m.u64(construction + construction::COUNTRY)? == ctx.player.0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether the player can take `action` in the province now, with the
/// game's reasons and the cost.
fn option(m: &Memory, ctx: &Ctx, place: &Place, action: Action) -> GResult<Choice> {
    let (country, province) = (ctx.country, place.address);
    Ok(match action {
        Action::Develop(kind) => {
            let cost = spending_price(m, ctx, DEVELOPING, province, kind.index())?;
            choice(checked(m, |out| CAN_DEVELOP.call(m, [country, province, kind.index(), out, 0]))?, vec![Amount::new(kind.power(), cost)])
        }
        Action::Exploit(kind) => choice(checked(m, |out| CAN_EXPLOIT.call(m, [country, kind.index(), province, out]))?, Vec::new()),
        Action::RaiseAutonomy => choice(checked(m, |out| CAN_RAISE_AUTONOMY.call(m, [country, province, out]))?, Vec::new()),
        Action::LowerAutonomy => choice(checked(m, |out| CAN_LOWER_AUTONOMY.call(m, [country, province, out]))?, Vec::new()),
        Action::MakeState => choice(checked(m, |out| CAN_MAKE_STATE.call(m, [country, province, out]))?, Vec::new()),
        Action::MakeTerritory => choice(checked(m, |out| CAN_MAKE_TERRITORY.call(m, [country, province, out]))?, Vec::new()),
        Action::CancelEdict => match player_state(m, ctx, place)? {
            Some(state) => choice(checked(m, |out| CAN_SET_EDICT.call(m, [state, 0, out]))?, Vec::new()),
            None => Choice::cannot(Vec::new(), Vec::new()),
        },
        Action::Core => {
            let cost = spending_price(m, ctx, CORING, province, 0)?;
            choice(checked(m, |out| CAN_CORE.call(m, [country, province, out, 0, 0, 1]))?, vec![Amount::new(Resource::Adm, cost)])
        }
        // The province view shows no button where the culture is the
        // primary one already; the game's check does not ask it.
        Action::ChangeCulture => {
            let (culture, _) = primary_culture(m, ctx)?;
            let cost = vec![Amount::new(Resource::Dip, CULTURE_PRICE.call(m, [province, ctx.player.0 as usize, culture, 0, 0])? as i32 as f64)];
            let price = cost[0].amount;
            let points = m.i32(ctx.country + country::POWER_DIP)?;
            if m.ptr(province + field::CULTURE)? == culture {
                Choice::cannot(Vec::new(), cost)
            } else {
                match checked(m, |out| CAN_CHANGE_CULTURE.call(m, [country, province, out]))? {
                    (true, _) if (points as f64) < price => {
                        let text = localized_with(m, NO_POINTS, [(c"COST", format!("{price}")), (c"VAL", points.to_string())])?;
                        Choice::cannot(reasons(&text), cost)
                    }
                    checked => choice(checked, cost),
                }
            }
        }
        Action::CancelCore => choice((doing(m, ctx, place, CORE_CONSTRUCTION)?, Vec::new()), Vec::new()),
        Action::CancelCulture => choice((doing(m, ctx, place, CULTURE_CONSTRUCTION)?, Vec::new()), Vec::new()),
    })
}

/// What the province view offers the player in a province now.
pub fn options(Session { m, ctx, .. }: Session, args::Options { province: ProvinceId(id) }: args::Options) -> GResult<Vec<ProvinceOption>> {
    let place = place(m, &ctx, id)?;
    let mut out = Vec::new();
    for action in Action::ALL {
        out.push(ProvinceOption { action: action.name(), edict: None, choice: option(m, &ctx, &place, action)? });
        if action == Action::MakeTerritory {
            let state = player_state(m, &ctx, &place)?;
            for (edict, key, _) in edicts(m)? {
                let choice = match state {
                    Some(state) => choice(checked(m, |out| CAN_SET_EDICT.call(m, [state, edict, out]))?, Vec::new()),
                    None => Choice::cannot(Vec::new(), Vec::new()),
                };
                out.push(ProvinceOption { action: "set_edict".into(), edict: Some(key), choice });
            }
        }
    }
    for which in super::spendings::InProvince::ALL {
        out.push(ProvinceOption { action: which.name().into(), edict: None, choice: super::spendings::in_province(m, &ctx, place.address, which)? });
    }
    Ok(out)
}

/// Takes `action` in the province for the player, as its button does:
/// refused with the game's reasons when the button would be refused, and
/// done once the game has carried the command out.
pub fn act(Session { m, ctx, commands, .. }: Session, id: u32, action: Action) -> GResult<()> {
    let place = place(m, &ctx, id)?;
    let choice = option(m, &ctx, &place, action)?;
    if !choice.can {
        return Err(match action {
            Action::CancelCore => checks::not_doing("coring", id),
            Action::CancelCulture => checks::not_doing("changing the culture of", id),
            Action::CancelEdict if player_state(m, &ctx, &place)?.is_none() => checks::not_in_a_state(id),
            Action::CancelEdict if m.ptr(m.ptr(place.address + field::STATE)? + field::STATE_EDICT)? == 0 => checks::no_edict_set(id),
            Action::ChangeCulture if choice.reasons.is_empty() && m.ptr(place.address + field::CULTURE)? == primary_culture(m, &ctx)?.0 => {
                checks::primary_culture_already(id)
            }
            _ => action::not_allowed(&format!("{} there", action.doing()), &choice.reasons),
        });
    }
    // The state and culture commands act for the province's owner.
    if matches!(action, Action::MakeState | Action::MakeTerritory | Action::ChangeCulture) && place.owner != Some(ctx.player) {
        return Err(checks::not_owned(id));
    }
    let tag = ctx.player;
    let text = match action {
        Action::Develop(kind) => spend(tag, DEVELOPING, id, kind.index()),
        Action::Exploit(kind) => exploit(tag, kind, id),
        Action::RaiseAutonomy => country_and("increase_autonomy_command", tag, "location", id),
        Action::LowerAutonomy => country_and("decrease_autonomy_command", tag, "location", id),
        Action::MakeState => state("make_state_command", id),
        Action::MakeTerritory => state("abandon_state_command", id),
        Action::CancelEdict => country_and("enact_edict", tag, "province", id),
        Action::Core => spend(tag, CORING, id, 0),
        Action::CancelCore => country_and("cancel_build_core", tag, "province", id),
        Action::ChangeCulture => culture(id, &primary_culture(m, &ctx)?.1),
        Action::CancelCulture => country_and("cancel_change_culture", tag, "province", id),
    };
    action::post(commands, &text, action.doing())
}

/// Sets the edict `key` in the player's state the province is in.
pub fn set_edict(Session { m, ctx, commands, .. }: Session, args::SetEdict { province: ProvinceId(id), edict: key }: args::SetEdict) -> GResult<()> {
    let place = place(m, &ctx, id)?;
    let Some((edict, _, index)) = edicts(m)?.into_iter().find(|(_, k, _)| *k == key) else {
        return Err(checks::no_such_edict(&key));
    };
    let Some(state) = player_state(m, &ctx, &place)? else {
        return Err(checks::not_in_a_state(id));
    };
    let (may, reasons) = checked(m, |out| CAN_SET_EDICT.call(m, [state, edict, out]))?;
    if !may {
        return Err(action::not_allowed("set the edict there", &reasons));
    }
    action::post(commands, &enact(ctx.player, id, index), "set the edict")
}

/// The refusals the framework words itself in the province view's actions,
/// of administration and buildings; those the game gives reasons for are
/// [`super::action`]'s.
pub mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of an action on a province the player does not own.
    pub fn not_owned(province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("the player does not own province {province}"))
    }

    /// The refusal of an edict for a province in no state of the player's.
    pub fn not_in_a_state(province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("province {province} is in no state of the player's"))
    }

    /// The refusal of cancelling an edict where none is set.
    pub fn no_edict_set(province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("no edict is set in the state of province {province}"))
    }

    /// The refusal of changing a province's culture to the primary culture it
    /// has.
    pub fn primary_culture_already(province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("the culture of province {province} is the player's primary culture already"))
    }

    /// The refusal of a content key that names no edict.
    pub fn no_such_edict(key: &str) -> Fail {
        Fail::new(Status::InvalidArgument, format!("no edict {key}"))
    }

    /// The refusal of a content key that names no building.
    pub fn no_such_building(key: &str) -> Fail {
        Fail::new(Status::InvalidArgument, format!("no building {key}"))
    }

    /// The refusal of destroying a building that does not stand in the province.
    pub fn building_absent(key: &str, province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("there is no {key} in province {province}"))
    }

    /// The refusal of cancelling what the player is not doing in a province:
    /// `what` is "building", "coring" or "changing the culture of".
    pub fn not_doing(what: &str, province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("the player is not {what} province {province}"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn unknown_keys_are_invalid_arguments() {
            assert_eq!(no_such_edict("edict_x").status, Status::InvalidArgument);
            assert_eq!(no_such_building("castle").message, "no building castle");
            assert_eq!(not_doing("coring", 151).message, "the player is not coring province 151");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::As;

    const CAS: Tag = Tag(0x0100_0001_0053_4143);

    #[test]
    fn the_commands_name_the_country_the_province_and_what_is_done() {
        assert_eq!(spend(CAS, DEVELOPING, 217, 1), r#"spend_power_command={ country="CAS" power={ data=217 category=7 skill=1 } }"#);
        assert_eq!(spend(CAS, CORING, 217, 0), r#"spend_power_command={ country="CAS" power={ data=217 category=17 skill=0 } }"#);
        assert_eq!(exploit(CAS, Development::Manpower, 217), r#"exploit_development={ members={ dummy="CAS" dummy=2 dummy=217 } }"#);
        assert_eq!(country_and("increase_autonomy_command", CAS, "location", 217), r#"increase_autonomy_command={ country="CAS" location=217 }"#);
        assert_eq!(state("make_state_command", 217), "make_state_command={ province=217 }");
        assert_eq!(enact(CAS, 217, 3), r#"enact_edict={ country="CAS" province=217 which=3 }"#);
        assert_eq!(country_and("enact_edict", CAS, "province", 217), r#"enact_edict={ country="CAS" province=217 }"#);
        assert_eq!(culture(217, "castilian"), "change_culture={ province=217 culture=castilian }");
    }

    #[test]
    fn bullets_are_left_out_of_reasons() {
        let text = crate::unicode::game_text::from_str("- 这\n－最\n有");
        assert_eq!(reasons(&text), ["这", "最", "有"]);
    }

    use crate::catalogue::fake::{byz, tur};
    use crate::game::{
        buffers::{self, write_reason, Image, Object, Provinces},
        commands::record,
        power::{ABOUT_PROVINCE, COST, NO_ID},
        spendings, Fail,
    };
    use std::cell::Cell;

    thread_local! {
        /// Whether the game's checks refuse, in this test's thread.
        static REFUSING: Cell<bool> = const { Cell::new(false) };
    }

    fn check(out: usize) -> usize {
        if REFUSING.with(Cell::get) {
            unsafe { write_reason(out, b"- not now") };
            return 0;
        }
        1
    }

    /// A check whose reasons go to its third argument.
    extern "C" fn third(_: usize, _: usize, out: usize) -> usize {
        check(out)
    }

    /// A check whose reasons go to its fourth argument.
    extern "C" fn fourth(_: usize, _: usize, _: usize, out: usize) -> usize {
        check(out)
    }

    extern "C" fn fifty() -> usize {
        50
    }

    /// Changing a culture costs 30 diplomatic points.
    extern "C" fn thirty() -> usize {
        30
    }

    /// The lookup of two values, for a test: the key, then each value as
    /// ` NAME=text`.
    extern "C" fn localize_two(
        out: usize,
        key: *const std::ffi::c_char,
        first: *const std::ffi::c_char,
        first_value: usize,
        second: *const std::ffi::c_char,
        second_value: usize,
    ) -> usize {
        let name = |n| unsafe { std::ffi::CStr::from_ptr(n) }.to_string_lossy().into_owned();
        let value = |v| String::from_utf8(game_string::read(&Memory { base: 0 }, v).unwrap()).unwrap();
        let text = format!("{} {}={} {}={}", name(key), name(first), value(first_value), name(second), value(second_value));
        unsafe { write_reason(out, text.as_bytes()) };
        0
    }

    /// The Ottomans, of Turkish culture, with 100 points of each power but
    /// `dip` diplomatic; the edicts `edict_a` and `edict_b`. Their province 1
    /// is Greek, in a state of theirs with an edict set or not, and they are
    /// coring it; province 2 is Byzantium's; their province 3 is Turkish and
    /// in no state. The game's checks refuse with `- not now` or let them.
    struct Ottomans {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn ottomans(refusing: bool, dip: i32, edict_set: bool) -> Ottomans {
        REFUSING.with(|r| r.set(refusing));
        let image = Image::new();
        image
            .localisation()
            .u64(NO_ID, 0)
            .function(COST.rva(), fifty as *const ())
            .function(ABOUT_PROVINCE.rva(), buffers::no as *const ())
            .function(CULTURE_PRICE.rva(), thirty as *const ())
            .function(LOCALIZE_TWO.rva(), localize_two as *const ())
            .function(CAN_DEVELOP.rva(), fourth as *const ())
            .function(CAN_EXPLOIT.rva(), fourth as *const ())
            .function(CAN_RAISE_AUTONOMY.rva(), third as *const ())
            .function(CAN_LOWER_AUTONOMY.rva(), third as *const ())
            .function(CAN_MAKE_STATE.rva(), third as *const ())
            .function(CAN_MAKE_TERRITORY.rva(), third as *const ())
            .function(CAN_SET_EDICT.rva(), third as *const ())
            .function(CAN_CORE.rva(), third as *const ())
            .function(CAN_CHANGE_CULTURE.rva(), third as *const ())
            .function(spendings::MAY_MOVE_CAPITAL.rva(), third as *const ())
            .function(spendings::MAY_MOVE_TRADE_PORT.rva(), third as *const ())
            .function(spendings::MAY_SIBERIAN_FRONTIER.rva(), fourth as *const ())
            .function(spendings::MAY_RAZE.rva(), third as *const ());
        let tokens = image.tokens(&["none", "turkish", "greek"]);
        let (turkish, greek) = (Object::new(0x18).u32(field::CULTURE_TOKEN, 1), Object::new(0x18).u32(field::CULTURE_TOKEN, 2));
        let edict = |index: u32, key: &str| Object::new(EDICT_INDEX + 4).string(field::EDICT_KEY, key).u32(EDICT_INDEX, index);
        let (edict_a, edict_b) = (edict(0, "edict_a"), edict(1, "edict_b"));
        let database = Object::new(EDICTS_LIST + 16).vector(EDICTS_LIST, &[edict_a.at() as u64, edict_b.at() as u64]);
        image.u64(EDICTS, database.at() as u64);
        let enacted = Object::new(field::EDICT_DEFINITION + 8).u64(field::EDICT_DEFINITION, edict_a.at() as u64);
        let state = Object::new(field::STATE_EDICT + 8).u64(STATE_TAG, tur().0).u64(field::STATE_EDICT, if edict_set { enacted.at() as u64 } else { 0 });
        let coring = Object::new(construction::COUNTRY + 8).u32(construction::KIND, CORE_CONSTRUCTION).u64(construction::COUNTRY, tur().0);
        let node = Object::new(list::NODE_SIZE).u64(list::ITEM, coring.at() as u64);
        let provinces = Provinces::new(&image, 4);
        provinces.owner(1, tur()).u64(1, field::CULTURE, greek.at() as u64).u64(1, field::STATE, state.at() as u64).u64(
            1,
            field::CONSTRUCTIONS,
            node.at() as u64,
        );
        provinces.owner(2, byz()).u64(2, field::CULTURE, greek.at() as u64);
        provinces.owner(3, tur()).u64(3, field::CULTURE, turkish.at() as u64);
        let player = Object::country(tur(), country::NATIVE + 8)
            .i32(country::POWER, 100)
            .i32(country::POWER_DIP, dip)
            .i32(country::POWER + 8, 100)
            .u64(country::PRIMARY_CULTURE, turkish.at() as u64);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), provinces.world(), tur(), player.at(), countries.at(), Caller::Remote(As::Player));
        let objects = vec![tokens, turkish, greek, edict_a, edict_b, database, enacted, state, coring, node, player, countries];
        Ottomans { _image: image, _provinces: provinces, _objects: objects, session }
    }

    use crate::game::Caller;

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn the_view_offers_every_action_with_its_cost_and_the_state_s_edicts() {
        let player = ottomans(false, 100, true);
        let options = options(player.session, args::Options { province: ProvinceId(1) }).unwrap();
        let can: Vec<_> = options.iter().map(|o| (o.action.as_str(), o.edict.as_deref(), o.choice.can)).collect();
        assert_eq!(
            can,
            [
                ("develop_tax", None, true),
                ("develop_production", None, true),
                ("develop_manpower", None, true),
                ("exploit_tax", None, true),
                ("exploit_production", None, true),
                ("exploit_manpower", None, true),
                ("raise_autonomy", None, true),
                ("lower_autonomy", None, true),
                ("make_state", None, true),
                ("make_territory", None, true),
                ("set_edict", Some("edict_a"), true),
                ("set_edict", Some("edict_b"), true),
                ("cancel_edict", None, true),
                ("core", None, true),
                ("cancel_core", None, true),
                ("change_culture", None, true),
                ("cancel_culture", None, false),
                ("move_capital", None, true),
                ("move_trade_port", None, true),
                ("establish_siberian_frontier", None, true),
                // Only a native country has the button.
                ("add_tribal_land", None, false),
                ("raze", None, true),
            ]
        );
        let cost = |action: &str| options.iter().find(|o| o.action == action).unwrap().choice.cost.clone();
        assert_eq!(cost("develop_production"), [Amount::new(Resource::Dip, 50.0)]);
        assert_eq!(cost("core"), [Amount::new(Resource::Adm, 50.0)]);
        assert_eq!(cost("change_culture"), [Amount::new(Resource::Dip, 30.0)]);
        // A province in no state of the player's offers no edicts.
        let elsewhere = super::options(player.session, args::Options { province: ProvinceId(3) }).unwrap();
        assert!(elsewhere.iter().filter(|o| o.action == "set_edict" || o.action == "cancel_edict").all(|o| !o.choice.can));
    }

    #[test]
    fn each_action_posts_its_button_s_command() {
        let player = ottomans(false, 100, true);
        let all = [Action::Develop(Development::Tax), Action::Exploit(Development::Production), Action::RaiseAutonomy, Action::LowerAutonomy];
        for action in
            all.into_iter().chain([Action::MakeState, Action::MakeTerritory, Action::CancelEdict, Action::Core, Action::CancelCore, Action::ChangeCulture])
        {
            act(player.session, 1, action).unwrap();
        }
        set_edict(player.session, args::SetEdict { province: ProvinceId(1), edict: "edict_b".into() }).unwrap();
        assert_eq!(
            record::posted(),
            [
                r#"spend_power_command={ country="TUR" power={ data=1 category=7 skill=0 } }"#,
                r#"exploit_development={ members={ dummy="TUR" dummy=1 dummy=1 } }"#,
                r#"increase_autonomy_command={ country="TUR" location=1 }"#,
                r#"decrease_autonomy_command={ country="TUR" location=1 }"#,
                "make_state_command={ province=1 }",
                "abandon_state_command={ province=1 }",
                r#"enact_edict={ country="TUR" province=1 }"#,
                r#"spend_power_command={ country="TUR" power={ data=1 category=17 skill=0 } }"#,
                r#"cancel_build_core={ country="TUR" province=1 }"#,
                "change_culture={ province=1 culture=turkish }",
                r#"enact_edict={ country="TUR" province=1 which=1 }"#,
            ]
        );
    }

    #[test]
    fn the_framework_s_own_refusals_say_what_is_missing() {
        let player = ottomans(false, 100, true);
        let session = player.session;
        assert_eq!(refused(act(session, 1, Action::CancelCulture)), (Status::Rejected, "the player is not changing the culture of province 1".into()));
        assert_eq!(refused(act(session, 3, Action::CancelCore)), (Status::Rejected, "the player is not coring province 3".into()));
        assert_eq!(refused(act(session, 3, Action::CancelEdict)), (Status::Rejected, "province 3 is in no state of the player's".into()));
        assert_eq!(
            refused(act(session, 3, Action::ChangeCulture)),
            (Status::Rejected, "the culture of province 3 is the player's primary culture already".into())
        );
        assert_eq!(refused(act(session, 2, Action::MakeState)), (Status::Rejected, "the player does not own province 2".into()));
        assert_eq!(
            refused(set_edict(session, args::SetEdict { province: ProvinceId(1), edict: "edict_x".into() })),
            (Status::InvalidArgument, "no edict edict_x".into())
        );
        assert_eq!(
            refused(set_edict(session, args::SetEdict { province: ProvinceId(3), edict: "edict_a".into() })),
            (Status::Rejected, "province 3 is in no state of the player's".into())
        );
        assert_eq!(refused(act(session, 0, Action::Core)).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }

    #[test]
    fn the_game_s_refusals_pass_on_its_reasons() {
        let refusing = ottomans(true, 100, false);
        assert_eq!(refused(act(refusing.session, 1, Action::Develop(Development::Manpower))), (Status::Rejected, "not now".into()));
        assert_eq!(refused(act(refusing.session, 1, Action::CancelEdict)), (Status::Rejected, "no edict is set in the state of province 1".into()));
        assert_eq!(
            refused(set_edict(refusing.session, args::SetEdict { province: ProvinceId(1), edict: "edict_a".into() })),
            (Status::Rejected, "not now".into())
        );
        drop(refusing);
        let poor = ottomans(false, 10, true);
        assert_eq!(refused(act(poor.session, 1, Action::ChangeCulture)), (Status::Rejected, "NOCHA_NO_POINTS COST=30 VAL=10".into()));
        assert!(record::posted().is_empty());
    }

    #[test]
    fn every_action_has_its_capability_s_name() {
        let names: Vec<String> = Action::ALL.iter().map(|a| a.name()).collect();
        assert_eq!(names[..3], ["develop_tax", "develop_production", "develop_manpower"]);
        assert_eq!(names[3..6], ["exploit_tax", "exploit_production", "exploit_manpower"]);
        assert!(names.contains(&"cancel_culture".to_string()));
        assert_eq!(Development::ALL.map(Development::power), [Resource::Adm, Resource::Dip, Resource::Mil]);
    }
}
