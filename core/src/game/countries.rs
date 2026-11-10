//! Countries: finding one by its tag, its tag, whether two are hostile, one
//! read of a country and its budget (docs/research/countries.md).
use super::{commands, country, modifiers, player::Eyes, thousandths, world, GResult, InWorld, Tag, View, World, WorldSession};
use crate::containers;
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{
        classes, country as fields, country_diplomacy, idea_group, loan, modifiers as modifiers_fields, person, rebel_faction, subject_relation,
        subject_type, technology, war, world as world_fields,
    },
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::{
    countries::{
        args, Budget, Country, Entry, GoverningCapacity, IdeaGroup, Legitimacy, LegitimacyKind, Loan, Loans, ModifierValue, Modifiers, Person, Power, Rebels, Relation,
        RelationKind, Technology,
    },
    CountryTag, Date,
};

/// The country a capability's argument names by its tag (for example
/// `CAS`); [`super::Status::NotFound`] when there is none. Every capability
/// that takes a country finds it here.
pub fn named(m: &Memory, ctx: &impl InWorld, tag: CountryTag) -> GResult<Tag> {
    match world::countries(m, ctx)?.into_iter().find(|t| t.letters() == tag.0) {
        Some(found) => Ok(found),
        None => Err(eu4_harmony_checks::countries::not_found(&tag.to_string()).into()),
    }
}

/// The three letters of a country's tag (for example `CAS`).
pub fn tag(WorldSession { m, world: ctx, .. }: WorldSession, country_id: Tag) -> GResult<[u8; 3]> {
    country(m, &ctx, country_id)?;
    Ok(country_id.letters())
}

/// Whether the two countries are at war, or either is a rebel faction.
pub fn hostile(WorldSession { m, world: ctx, .. }: WorldSession, args::Hostile { country, other }: args::Hostile) -> GResult<bool> {
    world::hostile(m, &ctx, named(m, &ctx, country)?, named(m, &ctx, other)?)
}

/// A war going on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct War {
    /// Its name, as the game shows it.
    pub name: String,
    /// The first of each side is its leader.
    pub attackers: Vec<Tag>,
    pub defenders: Vec<Tag>,
}

/// The most tags a list of a country's, or wars, are believed to hold.
const MOST: usize = 4096;

/// The maximum governing capacity: (country, out, modifier values, text).
const MAX_GOVERNING_CAPACITY: GameFunction<4> = GameFunction::at(0x3f2e10);

/// A country's government reforms, as the game finds them: (country).
const REFORMS: GameFunction<1> = GameFunction::at(0x40a6d0);
/// What the reforms make the government, each (reforms) -> bool: a horde,
/// whose legitimacy is horde unity; one with meritocracy; a monarchy; a
/// republic; one with devotion.
const NOMAD: GameFunction<1> = GameFunction::at(0x7ac7f0);
const HAS_MERITOCRACY: GameFunction<1> = GameFunction::at(0x7ac710);
const MONARCHY: GameFunction<1> = GameFunction::at(0x7ac1a0);
const REPUBLIC: GameFunction<1> = GameFunction::at(0x7ac3a0);
const HAS_DEVOTION: GameFunction<1> = GameFunction::at(0x7ac6e0);
/// A horde's horde unity and a monarchy's legitimacy, as the game shows
/// them: (country, out). A personal union's junior partner has its senior
/// partner's legitimacy.
const HORDE_UNITY: GameFunction<2> = GameFunction::at(0x362990);
const MONARCH_LEGITIMACY: GameFunction<2> = GameFunction::at(0x362b70);
/// Every kind of legitimacy is whole at 100, in thousandths, as strengthening
/// the government checks it.
const WHOLE: usize = 100_000;

pub fn points() -> Vec<Point> {
    vec![
        // The technology view's cost of a kind picks its category by these.
        Point::keep(0x37f39ausize, "8b 0d 58 0c fc 01 eb 16 8b 40 04 e9 43 ff ff ff 8b 0d 4c 0c fc 01 eb 06 8b 0d 48 0c fc 01", "technology kinds' categories")
            .uses(&[technology::ADM, technology::DIP, technology::MIL]),
        // A country's diplomatic technology level, checked against the levels' count.
        Point::keep(0x3c6143usize, "48 8b 87 40 24 00 00 4c 8b 80 28 01 00 00 48 8b 88 30 01 00 00 49 2b c8 48 c1 f9 02", "country technology levels").uses(&[
            fields::TECHNOLOGY,
            technology::LEVELS,
            technology::LEVELS + 8,
        ]),
        Point::keep(0xcf470usize, "8b 41 08 c3", "idea group's ideas taken").uses(&[fields::IDEA_GROUP_COUNT]),
        Point::keep(0x43f280usize, "8b 81 6c 05 00 00 c3", "idea group's token").uses(&[idea_group::TOKEN]),
        // Governing capacity used, as a share of its maximum.
        Point::keep(
            0x9a1f0usize,
            "48 89 5c 24 10 48 89 74 24 18 57 48 83 ec 20 48 8b fa 48 8b d9 48 63 b1 2c 0e 00 00 48 81 c1 c0 28 00 00 e8 38 35 8a 00 \
             4c 8d 40 08 45 33 c9 48 8d 54 24 30 48 8b cb e8 e4 8b 35 00",
            "governing capacity share",
        )
        .uses(&[fields::USED_GOVERNING_CAPACITY, fields::MODIFIERS]),
        Point::keep(MAX_GOVERNING_CAPACITY, "40 55 53 56 57 41 54 41 56 41 57 48 8d 6c 24 d9 48 81 ec b0 00 00 00", "maximum governing capacity"),
        // The government's kind of legitimacy, asked as strengthening the
        // government (0x3f78e0) asks it: a horde, meritocracy, a monarchy, a
        // republic, devotion, in that order, each kind whole at 100.
        Point::keep(REFORMS, "40 53 48 83 ec 20 48 8b 99 20 14 00 00 48 85 db 74 06 48 8b", "country's reforms"),
        Point::keep(NOMAD, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 80 3d 25 37", "government is a horde"),
        Point::keep(HAS_MERITOCRACY, "48 89 5c 24 08 57 48 83 ec 60 48 8b f9 80 3d 08 38 b9 01 00 74 09 80 3d", "government has meritocracy"),
        Point::keep(MONARCHY, "40 53 48 83 ec 60 48 8b 99 f0 00 00 00 48 85 db 74 45 44 8b 03 41 8b c0", "government is a monarchy"),
        Point::keep(REPUBLIC, "40 53 48 83 ec 20 48 8b d9 e8 62 03 00 00 84 c0 75 20 48 8b cb e8 26 00", "government is a republic"),
        Point::keep(HAS_DEVOTION, "40 53 48 83 ec 20 48 8b d9 e8 12 ff ff ff 84 c0 74 14 48 8b cb e8 16 00", "government has devotion"),
        Point::keep(HORDE_UNITY, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b da 48 8b f1 48 8b b9", "horde unity"),
        Point::keep(0x362a01usize, "8b 86 90 10 00 00", "horde unity's place").uses(&[fields::LEGITIMACY]),
        Point::keep(MONARCH_LEGITIMACY, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 48 8b da 48", "monarch's legitimacy"),
        Point::keep(0x3f7986usize, "e8 65 4e 3b 00", "strengthening the government asks for a horde first"),
        Point::keep(0x3f7a1eusize, "81 bf 34 14 00 00 a0 86 01 00", "meritocracy whole").uses(&[fields::MERITOCRACY, WHOLE]),
        Point::keep(0x3f7af4usize, "81 bf 94 10 00 00 a0 86 01 00", "republican tradition whole").uses(&[fields::REPUBLICAN_TRADITION, WHOLE]),
        Point::keep(0x3f7b61usize, "81 bf 30 14 00 00 a0 86 01 00", "devotion whole").uses(&[fields::DEVOTION, WHOLE]),
        // The country's modifiers made again: what legitimacy adds, scaled
        // from its middle (a value less 500 tenths), and a republic's two.
        Point::keep(0x92e7afusize, "81 c3 0c fe ff ff", "legitimacy's effects from the middle"),
        Point::keep(0x92e7bausize, "49 8b 3e 48 81 c7 08 2f 00 00", "legitimacy's effects").uses(&[modifiers_fields::LEGITIMACY_EFFECTS]),
        Point::keep(0x92e999usize, "48 8b 1f 48 81 c3 c8 2f 00 00", "republican tradition's second effects")
            .uses(&[modifiers_fields::LEGITIMACY_EFFECTS_SECOND]),
        // The country's save writer.
        Point::keep(0x2df4eeusize, "49 8d 97 40 0e 00 00", "country army tradition").uses(&[fields::ARMY_TRADITION]),
        Point::keep(0x2df516usize, "49 8d 97 44 0e 00 00", "country navy tradition").uses(&[fields::NAVY_TRADITION]),
        Point::keep(0x2df930usize, "49 8d 97 d0 0e 00 00", "country corruption").uses(&[fields::CORRUPTION]),
        Point::keep(0x2dfb94usize, "49 8d 57 4c", "country absolutism").uses(&[fields::ABSOLUTISM]),
        Point::keep(0x2dfbf8usize, "49 8d 4f 50", "country army professionalism").uses(&[fields::ARMY_PROFESSIONALISM]),
        // The country's save writer.
        Point::keep(0x2df1e1usize, "41 39 87 d4 0b 00 00", "country inflation").uses(&[fields::INFLATION]),
        Point::keep(0x2dfb51usize, "49 8d 97 98 10 00 00", "country mercantilism").uses(&[fields::MERCANTILISM]),
        Point::keep(0x2df601usize, "49 8b 87 e8 0d 00 00", "country loans").uses(&[fields::LOANS]),
        Point::keep(0x2e0046usize, "49 8b 87 f0 01 00 00", "country named modifiers").uses(&[fields::NAMED_MODIFIERS]),
        // It writes each of its persons only when it is not the null one.
        Point::keep(0x2e0588usize, "49 8b 8f 80 12 00 00 48 8b 01 ff 50 58", "country ruler").uses(&[fields::RULER, person::PRESENT]),
        Point::keep(0x2e05b5usize, "49 8b 8f 98 12 00 00 48 8b 01 ff 50 58", "country heir").uses(&[fields::HEIR, person::PRESENT]),
        Point::keep(0x2e05e2usize, "49 8b 8f b0 12 00 00 48 8b 01 ff 50 58", "country consort").uses(&[fields::CONSORT, person::PRESENT]),
        // The person's save writer.
        Point::keep(0xa4a7a3usize, "48 8d 56 28", "person's given name").uses(&[person::GIVEN_NAME]),
        Point::keep(0xa4a7efusize, "8b 7e 4c", "person's diplomatic skill").uses(&[person::DIP]),
        Point::keep(0xa4a837usize, "8b 7e 48", "person's administrative skill").uses(&[person::ADM]),
        Point::keep(0xa4a87fusize, "8b 7e 50", "person's military skill").uses(&[person::MIL]),
        Point::keep(0xa4b159usize, "48 8b 4e 68 48 8b 01 ff 50 08 84 c0 74 30 48 8b 7e 68 48 83 c7 08", "person's dynasty").uses(&[
            person::DYNASTY,
            person::DYNASTY_VALID,
            person::DYNASTY_NAME,
        ]),
        Point::keep(0xa4b197usize, "48 8d 4e 70", "person's birth date").uses(&[person::BIRTH]),
        // The loan's save writer.
        Point::keep(0x86b109usize, "48 8d 56 20", "loan lender").uses(&[loan::LENDER]),
        Point::keep(0x86b1afusize, "8b 5e 28", "loan interest").uses(&[loan::INTEREST]),
        Point::keep(0x86b202usize, "8b 5e 2c", "loan amount").uses(&[loan::AMOUNT]),
        Point::keep(0x86b24busize, "48 8d 4e 30", "loan due date").uses(&[loan::DUE]),
        // The rebel faction's save writer.
        Point::keep(0xa949beusize, "49 8d 56 60", "rebel faction name").uses(&[rebel_faction::NAME]),
        Point::keep(0xa949dausize, "49 8b 5e 20 48 83 c3 18", "rebel faction type").uses(&[rebel_faction::TYPE, rebel_faction::TYPE_KEY]),
        Point::keep(0xa94a36usize, "41 8b 9e 48 01 00 00", "rebel faction progress").uses(&[rebel_faction::PROGRESS]),
        // The economy view's budget reads both tables.
        Point::keep(0x11a9f76usize, "48 63 94 86 b4 0a 00 00", "budget income").uses(&[fields::INCOME]),
        Point::keep(0x11ade66usize, "48 63 94 86 00 0b 00 00", "budget expenses").uses(&[fields::EXPENSES]),
        // The categories' names by index: their count, and the jump tables
        // that give each index its name, which keep their order.
        Point::keep(0xaf28a0usize, "40 53 48 83 ec 20 48 8b d9 83 fa 12", "income category names").uses(&[fields::INCOME_CATEGORIES - 1]),
        Point::keep(
            0xaf29a0usize,
            "c8 28 af 00 28 29 af 00 e0 28 af 00 ec 28 af 00 d4 28 af 00 f8 28 af 00 3a 29 af 00 31 29 af 00 43 29 af 00 04 29 af 00 \
             0d 29 af 00 16 29 af 00 1f 29 af 00 4c 29 af 00 55 29 af 00 5e 29 af 00 67 29 af 00 70 29 af 00 79 29 af 00",
            "income category names' order",
        ),
        Point::keep(0xaf29f0usize, "40 53 48 83 ec 20 48 8b d9 83 fa 25", "expense category names").uses(&[fields::EXPENSE_CATEGORIES - 1]),
        Point::keep(
            0xaf2bc8usize,
            "d8 2a af 00 60 2a af 00 7d 2b af 00 2c 2b af 00 14 2b af 00 3e 2b af 00 18 2a af 00 54 2a af 00 24 2a af 00 30 2a af 00 \
             48 2a af 00 90 2a af 00 84 2a af 00 c0 2a af 00 9c 2a af 00 cc 2a af 00 6c 2a af 00 78 2a af 00 e4 2a af 00 fc 2a af 00 \
             08 2b af 00 20 2b af 00 35 2b af 00 50 2b af 00 59 2b af 00 47 2b af 00 6b 2b af 00 74 2b af 00 f0 2a af 00 86 2b af 00 \
             62 2b af 00 8f 2b af 00 aa 2b af 00 98 2b af 00 a1 2b af 00 b4 2a af 00 a8 2a af 00 3c 2a af 00",
            "expense category names' order",
        ),
    ]
}

/// The keys of the budget's categories, the localisation keys of their
/// names, in the order the game names them (`0xaf28a0` and `0xaf29f0`).
/// Expense 32, named `NONE`, is no category.
const INCOME_KEYS: [&str; fields::INCOME_CATEGORIES] = [
    "INCOMETAX",
    "INCOMEPROD",
    "INCOMETRADE",
    "INCOMEGOLD",
    "INCOMETARRIFFS",
    "INCOMEVASSAL",
    "INCOMEHARBORFEES",
    "INCOMESUBS",
    "INCOME_WAR_REPARATION",
    "INCOMELOAN",
    "INCOMEGIFT",
    "INCOMEEVENT",
    "SPOILS_OF_WAR_ECOVIEW",
    "INCOME_TREASURE_FLEET",
    "SIPHON_INCOME",
    "CONDOTTIERI",
    "KNOWLEDGE_SHARING",
    "INC_BLOCKADES",
    "INC_LOOTING",
];
const EXPENSE_KEYS: [&str; fields::EXPENSE_CATEGORIES] = [
    "EXPENSECOURTMAINTENANCE",
    "EXPENSEINTEREST",
    "EXPENSE_STATE_MAINTENANCE",
    "EXPENSEHARBORFEES",
    "EXPENSESUBS",
    "EXPENSE_WAR_REPARATIONS",
    "EXPENSEAM",
    "EXPENSEFM",
    "EXPENSEFORTM",
    "COLONIAL_MAINTENANCE",
    "EXPENSEMISSIONARY",
    "EXPENSEARMY",
    "EXPENSEFLEET",
    "EXPENSEFORT",
    "EXPENSEMAN",
    "EXPENSESUPPORTREBELS",
    "EXPENSELOAN",
    "EXPENSEGIFT",
    "EXPENSECOURT",
    "EXPENSEEVENTS",
    "EXPENSEPEACE",
    "EXPENSEVASSAL",
    "EXPENSETARIFFS",
    "SUPPORT_LOYALISTS",
    "SEND_OFFICERS",
    "EXPENSE_SIPHON_INCOME",
    "CONDOTTIERI",
    "EXPENSE_CORRUPTION",
    "EXPENSE_TECHNOLOGY",
    "EXPENSE_GREAT_POWER",
    "KNOWLEDGE_SHARING",
    "TRADE_COMPANY_INVESTMENTS",
    "NONE",
    "EXP_BLOCKADES",
    "EXP_LOOTING",
    "EXPENSEGREATPROJECTS",
    "CENTRE_OF_TRADE_UPGRADE",
    "COLONY_CHANGES",
];

/// Whether a caller reading `view` sees what the panels show only to the
/// country `tag` itself: in the full view, or of the country acted as.
fn sees_all_of(view: View, ctx: &World, tag: Tag) -> bool {
    Eyes::of(view, ctx.player).sees_all_of(tag)
}

/// One read of a country: its treasury, manpower, stability, prestige,
/// monarch points, wars, diplomatic relations, technology and ideas, and
/// what only it sees of itself. In the player's view another country's
/// treasury, monarch points and what only it sees are left out.
pub fn get(WorldSession { m, world: ctx, view, .. }: WorldSession, args::Get { country: asked }: args::Get) -> GResult<Box<Country>> {
    let tag = named(m, &ctx, asked)?;
    let c = country(m, &ctx, tag)?;
    let all = sees_all_of(view, &ctx, tag);
    let fixed = |at: usize| m.i32(c + at);
    let letters = |tag: Tag| CountryTag(tag.letters());
    let mut relations = Vec::new();
    let mut add = |kind: RelationKind, country: Tag, subject_type: Option<String>| relations.push(Relation { kind, country: letters(country), subject_type });
    for ally in checked(m, &ctx, list_tags(m, c + fields::ALLIES)?)? {
        add(RelationKind::Ally, ally, None);
    }
    for subject in checked(m, &ctx, vector_tags(m, c + fields::SUBJECTS)?)? {
        match subject_of(m, country(m, &ctx, subject)?)? {
            Some((overlord, key)) if overlord == tag => add(RelationKind::Subject, subject, Some(key)),
            _ => return Err(format!("{:#x} is no subject of {:#x}", subject.0, tag.0).into()),
        }
    }
    if let Some((overlord, key)) = subject_of(m, c)? {
        add(RelationKind::Overlord, checked(m, &ctx, vec![overlord])?[0], Some(key));
    }
    for rival in checked(m, &ctx, rival_tags(m, c)?)? {
        add(RelationKind::Rival, rival, None);
    }
    let tags = |tags: Vec<Tag>| tags.into_iter().map(letters).collect();
    let own = if all { Some(own_of(m, &ctx, c)?) } else { None };
    Ok(Box::new(Country {
        tag: letters(tag),
        treasury: if all { Some(thousandths(fixed(fields::TREASURY)?)) } else { None },
        manpower: fixed(fields::MANPOWER)?,
        max_manpower: fixed(fields::MAX_MANPOWER)?,
        stability: thousandths(fixed(fields::STABILITY)?),
        prestige: thousandths(fixed(fields::PRESTIGE)?),
        power: if all { Some(Power { adm: m.i32(c + fields::POWER)?, dip: m.i32(c + fields::POWER_DIP)?, mil: m.i32(c + fields::POWER_MIL)? }) } else { None },
        wars: wars_of(m, &ctx, tag)?
            .into_iter()
            .map(|w| eu4_harmony_remote::countries::War { name: w.name, attackers: tags(w.attackers), defenders: tags(w.defenders) })
            .collect(),
        relations,
        technology: technology_of(m, c)?,
        ideas: ideas_of(m, c)?,
        governing_capacity: own.as_ref().map(|o| o.governing_capacity),
        inflation: own.as_ref().map(|o| thousandths(o.inflation)),
        mercantilism: own.as_ref().map(|o| thousandths(o.mercantilism)),
        loans: own.as_ref().map(|o| o.loans),
        rebels: own.map(|o| o.rebels),
        ruler: person_at(m, c + fields::RULER, ctx.date)?,
        heir: person_at(m, c + fields::HEIR, ctx.date)?,
        consort: person_at(m, c + fields::CONSORT, ctx.date)?,
        legitimacy: legitimacy_of(m, c, all)?,
        corruption: if all { Some(thousandths(fixed(fields::CORRUPTION)?)) } else { None },
        absolutism: if all { Some(thousandths(fixed(fields::ABSOLUTISM)?)) } else { None },
        army_professionalism: if all { Some(professionalism(m.u64(c + fields::ARMY_PROFESSIONALISM)? as i64)) } else { None },
        army_tradition: thousandths(fixed(fields::ARMY_TRADITION)?),
        navy_tradition: thousandths(fixed(fields::NAVY_TRADITION)?),
        named_modifiers: if all { Some(named_modifiers_of(m, c, view == View::Full)?) } else { None },
    }))
}

/// The named modifiers the country has; one its scripts hide only with
/// `hidden`.
fn named_modifiers_of(m: &Memory, c: usize, hidden: bool) -> GResult<Vec<eu4_harmony_remote::countries::NamedModifier>> {
    modifiers::named_modifiers(m, containers::items(m, c + fields::NAMED_MODIFIERS, MOST)?, hidden)
}

/// What a country has of each modifier `keys` name, as the game totals it;
/// in the player's view another country's are left out, the keys still
/// checked.
pub fn modifiers(
    WorldSession { m, world: ctx, view, .. }: WorldSession,
    args::Modifiers { country: asked, keys }: args::Modifiers,
) -> GResult<Modifiers> {
    let tag = named(m, &ctx, asked)?;
    let c = country(m, &ctx, tag)?;
    let values = if sees_all_of(view, &ctx, tag) {
        Some(modifiers::totals(m, modifiers::values(m, c)?, &keys.0)?)
    } else {
        modifiers::indices(m, &keys.0)?;
        None
    };
    Ok(Modifiers { country: CountryTag(tag.letters()), values })
}

/// Army professionalism in percent, from the game's fixed point.
fn professionalism(raw: i64) -> f64 {
    raw as f64 * 100.0 / fields::ARMY_PROFESSIONALISM_ONE as f64
}

/// The country's kind of legitimacy, as strengthening the government asks
/// for it: None for a government with none.
fn legitimacy_kind(m: &Memory, c: usize) -> GResult<Option<LegitimacyKind>> {
    let reforms = REFORMS.call(m, [c])?;
    let is = |function: GameFunction<1>| -> GResult<bool> { Ok(function.call(m, [reforms])? & 0xff != 0) };
    Ok(if is(NOMAD)? {
        Some(LegitimacyKind::HordeUnity)
    } else if is(HAS_MERITOCRACY)? {
        Some(LegitimacyKind::Meritocracy)
    } else if is(MONARCHY)? {
        Some(LegitimacyKind::Legitimacy)
    } else if is(REPUBLIC)? {
        Some(LegitimacyKind::RepublicanTradition)
    } else if is(HAS_DEVOTION)? {
        Some(LegitimacyKind::Devotion)
    } else {
        None
    })
}

/// The country's legitimacy, of its government's kind, and with `effects`
/// what it adds to the country's modifiers now.
fn legitimacy_of(m: &Memory, c: usize, effects: bool) -> GResult<Option<Legitimacy>> {
    let Some(kind) = legitimacy_kind(m, c)? else { return Ok(None) };
    let shown = |getter: GameFunction<2>| -> GResult<i32> {
        let mut value = 0i32;
        let out = &mut value as *mut i32 as usize;
        if getter.call(m, [c, out])? != out {
            return Err("legitimacy getter returned an unexpected pointer".to_string().into());
        }
        Ok(value)
    };
    let value = match kind {
        LegitimacyKind::Legitimacy => shown(MONARCH_LEGITIMACY)?,
        LegitimacyKind::HordeUnity => shown(HORDE_UNITY)?,
        LegitimacyKind::RepublicanTradition => m.i32(c + fields::REPUBLICAN_TRADITION)?,
        LegitimacyKind::Devotion => m.i32(c + fields::DEVOTION)?,
        LegitimacyKind::Meritocracy => m.i32(c + fields::MERITOCRACY)?,
    };
    let effects = if effects { Some(legitimacy_effects(m, c, kind, value)?) } else { None };
    Ok(Some(Legitimacy { kind, value: thousandths(value), max: thousandths(WHOLE as i32), effects }))
}

/// What the country's legitimacy of `kind` at `value` adds to its
/// modifiers, as they were last made again (docs/research/modifiers.md): the
/// set made for it, and a republic's second; nothing for a legitimacy, horde
/// unity or devotion at its middle, which adds none.
fn legitimacy_effects(m: &Memory, c: usize, kind: LegitimacyKind, value: i32) -> GResult<Vec<ModifierValue>> {
    let sets = match kind {
        LegitimacyKind::RepublicanTradition => vec![modifiers_fields::LEGITIMACY_EFFECTS, modifiers_fields::LEGITIMACY_EFFECTS_SECOND],
        LegitimacyKind::Meritocracy => vec![modifiers_fields::LEGITIMACY_EFFECTS],
        _ if i64::from(value) * 10 / 1000 == 500 => Vec::new(),
        _ => vec![modifiers_fields::LEGITIMACY_EFFECTS],
    };
    let own = modifiers::of_country(m, c)?;
    let mut out: Vec<ModifierValue> = Vec::new();
    for set in sets {
        for effect in modifiers::named(m, own + set)? {
            match out.iter_mut().find(|e| e.modifier == effect.modifier) {
                Some(same) => same.value += effect.value,
                None => out.push(effect),
            }
        }
    }
    Ok(out)
}

/// The person a country's field holds, as the court view shows them; None
/// for the game's null person, which holds the place while there is none.
fn person_at(m: &Memory, field: usize, date: i32) -> GResult<Option<Person>> {
    let p = m.ptr(field)?;
    if p == 0 || m.call(m.vslot(p, person::PRESENT)?, &[p])? & 0xff == 0 {
        return Ok(None);
    }
    let dynasty = m.ptr(p + person::DYNASTY)?;
    let dynasty = if dynasty != 0 && m.call(m.vslot(dynasty, person::DYNASTY_VALID)?, &[dynasty])? & 0xff != 0 {
        Some(game_string::shown(m, dynasty + person::DYNASTY_NAME)?)
    } else {
        None
    };
    Ok(Some(Person {
        name: game_string::shown(m, p + person::GIVEN_NAME)?,
        dynasty,
        adm: m.i32(p + person::ADM)?,
        dip: m.i32(p + person::DIP)?,
        mil: m.i32(p + person::MIL)?,
        age: age(m.i32(p + person::BIRTH)?, date),
    }))
}

/// Whole years from `birth` to `date`, in the game's calendar of 365-day
/// years.
fn age(birth: i32, date: i32) -> i32 {
    (date - birth).div_euclid(365 * 24)
}

/// The country's technology levels, each kind's category found as the game
/// finds it.
fn technology_of(m: &Memory, c: usize) -> GResult<Technology> {
    let technology = m.ptr(c + fields::TECHNOLOGY)?;
    let begin = m.ptr(technology + technology::LEVELS)?;
    let count = containers::span(begin, m.ptr(technology + technology::LEVELS + 8)?, 4, MOST)?;
    let level = |kind: usize| -> GResult<i32> {
        let index = m.i32(m.at(kind))?;
        if !(0..count as i32).contains(&index) {
            return Err(format!("technology category {index} of {count}").into());
        }
        Ok(m.i32(begin + index as usize * 4)?)
    };
    Ok(Technology { adm: level(technology::ADM)?, dip: level(technology::DIP)?, mil: level(technology::MIL)? })
}

/// The idea groups the country has taken, each named by its token's name.
fn ideas_of(m: &Memory, c: usize) -> GResult<Vec<IdeaGroup>> {
    let begin = m.ptr(c + fields::IDEA_GROUPS)?;
    let count = containers::span(begin, m.ptr(c + fields::IDEA_GROUPS + 8)?, fields::IDEA_GROUP_SIZE, MOST)?;
    let mut out = Vec::with_capacity(count);
    for item in (0..count).map(|i| begin + i * fields::IDEA_GROUP_SIZE) {
        let group = m.ptr(item)?;
        let token = m.u32(group + idea_group::TOKEN)?;
        let key = commands::type_name(m, token).ok_or_else(|| format!("idea group token {token:#x} has no name"))?;
        out.push(IdeaGroup { group: key, ideas: m.i32(item + fields::IDEA_GROUP_COUNT)?, national: m.u8(group + idea_group::FREE)? != 0 });
    }
    Ok(out)
}

/// What a country's panels show only to the country itself.
struct Own {
    governing_capacity: GoverningCapacity,
    /// Thousandths of a percentage point.
    inflation: i32,
    mercantilism: i32,
    loans: Loans,
    rebels: Vec<Rebels>,
}

fn own_of(m: &Memory, ctx: &impl InWorld, c: usize) -> GResult<Own> {
    let loans = loans_of(m, ctx, c)?;
    Ok(Own {
        governing_capacity: governing_capacity(m, c)?,
        inflation: m.i32(c + fields::INFLATION)?,
        mercantilism: m.i32(c + fields::MERCANTILISM)?,
        loans: Loans { count: loans.len() as i32, ducats: loans.iter().fold(0.0, |sum, l| sum + l.ducats) },
        rebels: rebels_of(m, c)?,
    })
}

/// Governing capacity used and its maximum, as the game's own share of them
/// reads them.
fn governing_capacity(m: &Memory, c: usize) -> GResult<GoverningCapacity> {
    let values = modifiers::values(m, c)?;
    let mut max = 0i32;
    let out = &mut max as *mut i32 as usize;
    if MAX_GOVERNING_CAPACITY.call(m, [c, out, values, 0])? != out {
        return Err("governing capacity getter returned an unexpected pointer".to_string().into());
    }
    Ok(GoverningCapacity { used: thousandths(m.i32(c + fields::USED_GOVERNING_CAPACITY)?), max: thousandths(max) })
}

/// The country's loans, in the order it keeps them.
fn loans_of(m: &Memory, ctx: &impl InWorld, c: usize) -> GResult<Vec<Loan>> {
    let mut out = Vec::new();
    for l in containers::items(m, c + fields::LOANS, MOST)? {
        let lender = Tag(m.u64(l + loan::LENDER)?);
        // A lender that is no country now, as one gone since, leaves the loan
        // with none rather than the whole read failing.
        let lender = (lender.normal() && country(m, ctx, lender).is_ok()).then(|| CountryTag(lender.letters()));
        out.push(Loan { lender, ducats: m.i32(l + loan::AMOUNT)? as f64, interest: thousandths(m.i32(l + loan::INTEREST)?), due: Date(m.i32(l + loan::DUE)?) });
    }
    Ok(out)
}

/// The rebel faction of `kind` against the country, the first when there are
/// more.
pub(crate) fn rebel_faction(m: &Memory, c: usize, kind: &str) -> GResult<Option<usize>> {
    for faction in factions(m, c)? {
        if faction_kind(m, faction)? == kind {
            return Ok(Some(faction));
        }
    }
    Ok(None)
}

/// The rebel factions against the country.
fn rebels_of(m: &Memory, c: usize) -> GResult<Vec<Rebels>> {
    factions(m, c)?
        .into_iter()
        .map(|faction| {
            Ok(Rebels {
                kind: faction_kind(m, faction)?,
                name: game_string::shown(m, faction + rebel_faction::NAME)?,
                progress: thousandths(m.i32(faction + rebel_faction::PROGRESS)?),
            })
        })
        .collect()
}

/// The country's rebel factions' objects.
fn factions(m: &Memory, c: usize) -> GResult<Vec<usize>> {
    let begin = m.ptr(c + fields::REBEL_FACTIONS)?;
    let count = containers::span(begin, m.ptr(c + fields::REBEL_FACTIONS + 8)?, 8, MOST)?;
    (0..count)
        .map(|i| {
            let faction = m.ptr(begin + i * 8)?;
            if faction == 0 || m.ptr(faction)? != m.at(classes::REBEL_FACTION) {
                return Err("a rebel faction that is none".to_string().into());
            }
            Ok(faction)
        })
        .collect()
}

/// A rebel faction's kind, its type's key.
fn faction_kind(m: &Memory, faction: usize) -> GResult<String> {
    Ok(game_text::to_utf8(&game_string::read(m, m.ptr(faction + rebel_faction::TYPE)? + rebel_faction::TYPE_KEY)?))
}

/// A country's budget as the economy view shows it: this month's income and
/// expenses by category, and its loans; in the player's view another
/// country's are left out.
pub fn budget(WorldSession { m, world: ctx, view, .. }: WorldSession, args::Budget { country: asked }: args::Budget) -> GResult<Budget> {
    let tag = named(m, &ctx, asked)?;
    let c = country(m, &ctx, tag)?;
    let letters = CountryTag(tag.letters());
    if !sees_all_of(view, &ctx, tag) {
        return Ok(Budget { country: letters, income: None, expenses: None, loans: None });
    }
    let entries = |table: usize, keys: &[&str]| -> GResult<Vec<Entry>> {
        let mut out = Vec::new();
        for (i, key) in keys.iter().enumerate().filter(|(_, k)| **k != "NONE") {
            let name = std::ffi::CString::new(*key).map_err(|e| e.to_string())?;
            out.push(Entry {
                category: key.to_string(),
                name: game_text::plain(&game_string::localized(m, name.as_ptr() as usize)?),
                ducats: thousandths(m.i32(c + table + i * 4)?),
            });
        }
        Ok(out)
    };
    Ok(Budget {
        country: letters,
        income: Some(entries(fields::INCOME, &INCOME_KEYS)?),
        expenses: Some(entries(fields::EXPENSES, &EXPENSE_KEYS)?),
        loans: Some(loans_of(m, &ctx, c)?),
    })
}

/// The overlord of the country at `country` and the key of the subject type
/// it is to it, read as the game's own getter reads them; None when it has no
/// overlord.
fn subject_of(m: &Memory, country: usize) -> GResult<Option<(Tag, String)>> {
    let overlord = Tag(m.u64(country + fields::OVERLORD)?);
    if !overlord.valid() {
        return Ok(None);
    }
    if overlord.index() < 0 {
        return Err(format!("{:#x} is no overlord's tag", overlord.0).into());
    }
    let record = m.ptr(country + fields::RELATIONS)? + overlord.index() as usize * fields::RELATION_SIZE;
    let diplomacy = m.ptr(record)?;
    let other = Tag(m.u64(record + fields::RELATION_OTHER)?).index() as i32;
    let items = m.ptr(diplomacy + country_diplomacy::SUBJECTS)?;
    let count = m.i32(diplomacy + country_diplomacy::SUBJECT_COUNT)?;
    if !(0..=MOST as i32).contains(&count) {
        return Err("subject relation array bound".to_string().into());
    }
    for item in (0..count as usize).map(|i| items + i * country_diplomacy::SUBJECT_SIZE) {
        if m.i32(item)? != other {
            continue;
        }
        let relation = m.ptr(item + country_diplomacy::SUBJECT_RELATION)?;
        if relation == 0 || m.ptr(relation)? != m.at(classes::SUBJECT_RELATION) {
            break;
        }
        let kind = m.ptr(relation + subject_relation::TYPE)?;
        if kind == 0 {
            break;
        }
        return Ok(Some((overlord, game_string::key(m, kind + subject_type::KEY)?)));
    }
    Err(format!("no subject relation to its overlord {:#x}", overlord.0).into())
}

/// The tags, each checked to be a country.
fn checked(m: &Memory, ctx: &impl InWorld, tags: Vec<Tag>) -> GResult<Vec<Tag>> {
    for &tag in &tags {
        if !tag.normal() {
            return Err(format!("{:#x} is no country's tag", tag.0).into());
        }
        country(m, ctx, tag)?;
    }
    Ok(tags)
}

/// The tags in a vector (begin, end) of tags at `vector`.
fn vector_tags(m: &Memory, vector: usize) -> GResult<Vec<Tag>> {
    let begin = m.ptr(vector)?;
    let count = containers::span(begin, m.ptr(vector + 8)?, 8, MOST)?;
    let bytes = if count == 0 { Vec::new() } else { m.bytes(begin, count * 8)? };
    Ok(bytes.chunks_exact(8).map(|b| Tag(u64::from_le_bytes(b.try_into().unwrap()))).collect())
}

/// The tags in a [`list`] of tags at `at`.
fn list_tags(m: &Memory, at: usize) -> GResult<Vec<Tag>> {
    Ok(containers::nodes(m, at, MOST)?.into_iter().map(|node| Tag(node.item as u64)).collect())
}

fn rival_tags(m: &Memory, country: usize) -> GResult<Vec<Tag>> {
    chain(m, m.ptr(country + fields::RIVALS)?, fields::RIVAL_TAG, fields::RIVAL_NEXT)
}

/// The tags in a chain of nodes from `first`, each with a tag at `tag` and
/// the next node at `next`.
fn chain(m: &Memory, first: usize, tag: usize, next: usize) -> GResult<Vec<Tag>> {
    containers::chain(m, first, next, MOST)?.into_iter().map(|node| Ok(Tag(m.u64(node + tag)?))).collect()
}

/// The wars going on that `country` takes part in, on either side. Only
/// these are read in full, so another war cannot fail the snapshot: one that
/// is no war or whose sides cannot be read is left out, as the country is on
/// neither side of it that can be seen; one with a tag that is no country's
/// fails the snapshot only when it is the country's.
pub(super) fn wars_of(m: &Memory, ctx: &impl InWorld, country: Tag) -> GResult<Vec<War>> {
    let data = m.ptr(ctx.world() + world_fields::WARS)?;
    let count = m.i32(ctx.world() + world_fields::WAR_COUNT)?;
    if !(0..=MOST as i32).contains(&count) {
        return Err("war array bound".to_string().into());
    }
    let mut out = Vec::new();
    for i in 0..count as usize {
        let w = m.ptr(data + i * 8)?;
        let Some((attackers, defenders)) = sides(m, w) else { continue };
        if !attackers.iter().chain(&defenders).any(|t| t.valid() && t.index() == country.index()) {
            continue;
        }
        let name = game_string::shown(m, w + war::NAME)?;
        out.push(War { name, attackers: checked(m, ctx, attackers)?, defenders: checked(m, ctx, defenders)? });
    }
    Ok(out)
}

/// The tags of a war's attackers and defenders, not yet checked; None when
/// `war` is no war or its sides cannot be read.
pub(super) fn sides(m: &Memory, war: usize) -> Option<(Vec<Tag>, Vec<Tag>)> {
    if war == 0 || m.ptr(war).ok()? != m.at(classes::WAR) {
        return None;
    }
    Some((vector_tags(m, war + war::ATTACKERS).ok()?, vector_tags(m, war + war::DEFENDERS).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{catalogue::fake::game_tag, game::buffers::Object};

    /// A war of the game's class with these sides; its name is empty.
    fn war_object(m: &Memory, attackers: &[Tag], defenders: &[Tag]) -> Object {
        let tags = |tags: &[Tag]| tags.iter().map(|t| t.0).collect::<Vec<u64>>();
        Object::new(war::DEFENDERS + 16)
            .u64(0, m.at(classes::WAR) as u64)
            .string(war::NAME, "")
            .vector(war::ATTACKERS, &tags(attackers))
            .vector(war::DEFENDERS, &tags(defenders))
    }

    /// An object whose vtable's method at `slot` answers `answer`.
    fn answering(slot: usize, answer: extern "C" fn() -> usize, size: usize) -> Object {
        let vtable = Object::new(slot + 8).u64(slot, answer as usize as u64);
        Object::new(size).points_to(0, vtable)
    }

    /// A ruler of a dynasty, born 30 years and a day before the date; an
    /// heir of none; the game's null consort: each read as the court view
    /// shows them, the null one as none.
    #[test]
    fn a_country_s_persons_are_read_and_the_null_one_is_none() {
        use crate::game::buffers::{no, yes};
        let m = Memory { base: 0x1_4000_0000 };
        let date = 1500 * 8760;
        let person = |name: &str, dynasty: Object, skills: [i32; 3], birth: i32| {
            answering(person::PRESENT, yes, person::BIRTH + 4)
                .string(person::GIVEN_NAME, name)
                .points_to(person::DYNASTY, dynasty)
                .i32(person::ADM, skills[0])
                .i32(person::DIP, skills[1])
                .i32(person::MIL, skills[2])
                .i32(person::BIRTH, birth)
        };
        let dynasty = answering(person::DYNASTY_VALID, yes, person::DYNASTY_NAME + game_string::OBJECT).string(person::DYNASTY_NAME, "de Valois");
        let ruler = person("Louis XI", dynasty, [4, 5, 2], date - 30 * 8760 - 24);
        let heir = person("Charles", answering(person::DYNASTY_VALID, no, 16), [1, 0, 3], date - 24);
        let consort = answering(person::PRESENT, no, 8);
        let country = Object::new(fields::CONSORT + 8).points_to(fields::RULER, ruler).points_to(fields::HEIR, heir).points_to(fields::CONSORT, consort);
        let read = |field: usize| person_at(&m, country.at() + field, date).unwrap();
        assert_eq!(
            read(fields::RULER),
            Some(Person { name: "Louis XI".into(), dynasty: Some("de Valois".into()), adm: 4, dip: 5, mil: 2, age: 30 })
        );
        assert_eq!(read(fields::HEIR), Some(Person { name: "Charles".into(), dynasty: None, adm: 1, dip: 0, mil: 3, age: 0 }));
        assert_eq!(read(fields::CONSORT), None);
    }

    /// Wars between countries 1 to 3, one with a tag that is no country's,
    /// and an entry that is no war: each country reads only its own wars,
    /// and only the country in the bad war fails.
    #[test]
    fn a_bad_war_fails_only_its_own_countries() {
        let m = Memory { base: 0x1_4000_0000 };
        let (a, b, c) = (game_tag(b"AAA", 1), game_tag(b"BBB", 2), game_tag(b"CCC", 3));
        let objects = [a, b, c].map(|t| Object::country(t, 0));
        let countries = Object::countries(&objects.iter().collect::<Vec<_>>());
        let first = war_object(&m, &[a, c], &[b]);
        let bad = war_object(&m, &[b], &[game_tag(b"ZZZ", 9)]);
        let not_a_war = Object::new(war::DEFENDERS + 16);
        let wars = Object::new(32).u64(0, first.at() as u64).u64(8, bad.at() as u64).u64(16, not_a_war.at() as u64);
        let world = Object::new(world_fields::WAR_COUNT + 4).points_to(world_fields::WARS, wars).i32(world_fields::WAR_COUNT, 4);
        let ctx = super::super::Ctx { world: world.at(), date: 0, player: a, country: 0, countries: countries.at() };
        let first_war = War { name: String::new(), attackers: vec![a, c], defenders: vec![b] };
        assert_eq!(wars_of(&m, &ctx, a).unwrap(), std::slice::from_ref(&first_war));
        assert_eq!(wars_of(&m, &ctx, c).unwrap(), [first_war]);
        assert!(wars_of(&m, &ctx, b).is_err());
        assert_eq!(wars_of(&m, &ctx, game_tag(b"DDD", 4)).unwrap(), []);
    }

    /// A subject whose diplomacy keeps two subject relations, one to another
    /// country and one to its overlord: the overlord's is read, with its
    /// subject type's key; a country without an overlord has none; a
    /// relation that is not of the game's class fails.
    #[test]
    fn a_subject_s_type_is_read_as_the_game_reads_it() {
        let m = Memory { base: 0x1_4000_0000 };
        let overlord = game_tag(b"TUR", 2);
        let kind = Object::new(subject_type::KEY + game_string::OBJECT).string(subject_type::KEY, "march");
        let relation = Object::new(subject_relation::TYPE + 8).u64(0, m.at(classes::SUBJECT_RELATION) as u64).points_to(subject_relation::TYPE, kind);
        let items = Object::new(32).u64(0, 5).u64(16, overlord.index() as u64).u64(24, relation.at() as u64);
        let diplomacy =
            Object::new(country_diplomacy::SUBJECT_COUNT + 8).points_to(country_diplomacy::SUBJECTS, items).i32(country_diplomacy::SUBJECT_COUNT, 2);
        let record = overlord.index() as usize * fields::RELATION_SIZE;
        let records = Object::new(fields::RELATION_SIZE * 3).points_to(record, diplomacy).u64(record + fields::RELATION_OTHER, overlord.0);
        let country = |overlord: u64| Object::new(fields::OVERLORD + 8).u64(fields::OVERLORD, overlord).u64(fields::RELATIONS, records.at() as u64);
        let subject = country(overlord.0);
        assert_eq!(subject_of(&m, subject.at()).unwrap(), Some((overlord, "march".to_string())));
        assert_eq!(subject_of(&m, country(0).at()).unwrap(), None);
        relation.set(0, &(m.at(classes::WAR) as u64).to_le_bytes());
        assert!(subject_of(&m, subject.at()).is_err());
    }
}
