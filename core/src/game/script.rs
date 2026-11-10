//! The scripts' flags, variables and events, read, set and fired as the
//! scripts' own effects do (docs/research/scripts.md).
//!
//! A flag is kept in a flag set of its scope's: a country's, a province's,
//! the world's, or a monarch's (a ruler's, an heir's or a consort's). The set
//! holds flag ids and, beside them, the day each was set; an id is the
//! flag's name's place among every flag name the game has met. A variable is
//! kept in a country's or a province's variables, by its name, in
//! thousandths. An event is found among the game's events by its id and
//! fired through the game's own `TriggerEvent` once its trigger holds, as
//! `country_event` and `province_event` fire one.
use super::{conditions, countries, country, with_state, GResult, Status, Tag, World, WorldSession};
use crate::{
    containers, game_string,
    memory::Memory,
    objects::campaign::{country as country_fields, event, globals::WORLD, province as province_fields, route_options::NO_TAG},
    points::{GameFunction, Point},
};
use eu4_harmony_checks::script::{self as checks, Use};
use eu4_harmony_remote::{
    script::{args, FlagState, Scope, VariableState},
    Date, ProvinceId,
};

/// The flag names the game has met, a `std::vector` of `std::string`s: a
/// flag's id is its name's place here.
const FLAG_NAMES: usize = 0x2357d80;
/// The game's events, a pointer to an `std::unordered_map` from an event's
/// number to the event, its items' list head at `+8`.
const EVENTS: usize = 0x2340308;

/// `(name: &std::string) -> u16`: a flag name's id, the name added to
/// [`FLAG_NAMES`] when the game has not met it yet.
const FLAG_ID: GameFunction<1> = GameFunction::at(0x737ea0);
/// `(flags, id, day)`: sets a flag in a flag set, on today's date when `day`
/// is before the game's start, as every `set_*_flag` passes
/// [`BEFORE_ANY_DAY`].
const SET_FLAG: GameFunction<3> = GameFunction::at(0x7371f0);
/// `(flags, id)`: clears a flag in a flag set; nothing when it is not set.
const CLEAR_FLAG: GameFunction<2> = GameFunction::at(0x7372f0);
/// `(variables, &name: &std::string, &value: &i32)`: sets a variable,
/// making it when there is none of that name.
const SET_VARIABLE: GameFunction<3> = GameFunction::at(0xd7c450);
/// `() -> manager`: the event manager.
const EVENT_MANAGER: GameFunction<0> = GameFunction::at(0x6f2fa0);
/// `(manager, event, scope)`: fires an event in a scope (eventmanager.cpp
/// "TriggerEvent"; docs/research/questions-and-offers.md).
const TRIGGER_EVENT: GameFunction<3> = GameFunction::at(0x6f4f40);

/// The day every `set_*_flag` effect hands [`SET_FLAG`]: 1.1.1, before the
/// game's start, which the flag set takes for today.
const BEFORE_ANY_DAY: usize = 0x29c77f8;

/// A flag set: the flags' ids, a `std::vector` of u16, and beside them the
/// days they were set, a `std::vector` of i32.
mod flags {
    pub const IDS: usize = 0x10;
    pub const DAYS: usize = 0x28;
}

/// Where each scope keeps its flags and variables.
mod kept {
    /// A country's flags, and its ruler's while it has none.
    pub const COUNTRY_FLAGS: usize = 0x270;
    pub const COUNTRY_RULERLESS_FLAGS: usize = 0x2b0;
    /// A country's variables.
    pub const COUNTRY_VARIABLES: usize = 0x330;
    /// A province's flags and variables.
    pub const PROVINCE_FLAGS: usize = 0x1b8;
    pub const PROVINCE_VARIABLES: usize = 0x238;
    /// The game's own flags, in the world.
    pub const GLOBAL_FLAGS: usize = 0xb38;
    /// A monarch's flags.
    pub const MONARCH_FLAGS: usize = 0xa0;
    /// A monarch's vtable: whether it is a person, not the placeholder a
    /// country has without one, and whether, a consort ruling as regent, it
    /// is its country's consort too (u8 each, through the vtable).
    pub const IS_PERSON: usize = 0x58;
    pub const IS_CONSORT_REGENT: usize = 0x60;
}

/// A variables map: an `std::unordered_map` from a name to a value, its
/// items' list head at [`HEAD`]; an item has its name, a `std::string`, at
/// [`NAME`] and its value, i32 thousandths, at [`VALUE`].
mod variables {
    pub const MAP: usize = 0x10;
    pub const HEAD: usize = MAP + 8;
    pub const NAME: usize = 0x10;
    pub const VALUE: usize = 0x30;
}

/// The game's events map: its items' list head at [`HEAD`]; an item has the
/// event at [`EVENT`].
mod events {
    pub const HEAD: usize = 8;
    pub const EVENT: usize = 0x18;
}

/// The most flag names, variables of a scope and events believed.
const MOST: usize = 1 << 20;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(FLAG_ID, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 50 48 8b d9 e8 69 f2 ff ff 48", "flag id"),
        // A name met for the first time goes into the flag names.
        Point::keep(0x737f52usize, "48 8b d3 48 8d 0d 24 fe c1 01 e8 bf eb ab ff", "flag names").uses(&[FLAG_NAMES]),
        Point::keep(SET_FLAG, "48 89 5c 24 08 44 89 44 24 18 66 89 54 24 10 57 48 83 ec 20 48 8b f9 41 8b d8 48 8d 0d 43 b1 88", "set flag"),
        // A flag found among the ids has its day written beside it.
        Point::keep(0x73726dusize, "48 2b c8 48 8b 47 28 48 d1 f9 8b c9 89 1c 88", "a flag's day").uses(&[flags::DAYS]),
        Point::keep(CLEAR_FLAG, "4c 8b 51 10 4c 8b c9 48 8b 49 18 4d 8b c2 4c 3b d1 74 0f 66 41 39 10", "clear flag")
            .uses(&[flags::IDS, flags::IDS + 8]),
        // set_country_flag: the country's flags, set with the day before any.
        Point::keep(0x611bd7usize, "48 81 c1 70 02 00 00 41 b8 f8 77 9c 02", "a country's flags").uses(&[kept::COUNTRY_FLAGS, BEFORE_ANY_DAY]),
        // set_ruler_flag: the ruler's flags while it is a person, else the
        // country's for a ruler it has not.
        Point::keep(0x612381usize, "48 8b b0 80 12 00 00", "a country's ruler").uses(&[country_fields::RULER]),
        Point::keep(0x612397usize, "48 8b 16 48 8b ce ff 52 58", "a monarch being a person").uses(&[kept::IS_PERSON]),
        Point::keep(0x6123adusize, "48 8d 8e a0 00 00 00", "a monarch's flags").uses(&[kept::MONARCH_FLAGS]),
        Point::keep(0x6123c8usize, "48 81 c1 b0 02 00 00", "a rulerless country's ruler flags").uses(&[kept::COUNTRY_RULERLESS_FLAGS]),
        Point::keep(0x612b17usize, "48 8b b0 98 12 00 00", "a country's heir").uses(&[country_fields::HEIR]),
        // set_consort_flag: the ruler when it is a consort ruling as regent,
        // else the consort.
        Point::keep(0x613273usize, "ff 50 60 84 c0 74 09 48 8b 9b 80 12 00 00 eb 07 48 8b 9b b0 12 00 00", "a country's consort")
            .uses(&[kept::IS_CONSORT_REGENT, country_fields::RULER, country_fields::CONSORT]),
        // set_province_flag: the province by its number, and its flags.
        Point::keep(0x6139ccusize, "4c 69 c1 10 2e 00 00 48 8b 0d 9e c4 d2 01 48 8b 89 a8 1c 00 00 48 81 c1 b8 01 00 00", "a province's flags")
            .uses(&[province_fields::SIZE, WORLD, kept::PROVINCE_FLAGS]),
        Point::keep(0x61410dusize, "48 8b 0d 64 bd d2 01 41 b8 f8 77 9c 02 48 81 c1 38 0b 00 00", "the game's flags").uses(&[WORLD, BEFORE_ANY_DAY, kept::GLOBAL_FLAGS]),
        // The variables of a scope's country, else of its province.
        Point::keep(0xd7cc81usize, "48 05 30 03 00 00", "a country's variables").uses(&[kept::COUNTRY_VARIABLES]),
        Point::keep(0xd7cca8usize, "48 8d 81 38 02 00 00", "a province's variables").uses(&[kept::PROVINCE_VARIABLES]),
        Point::keep(SET_VARIABLE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 41 56 48 83 ec 30 48 83 7a 18 10 4d 8b f0 48 8b da", "set variable"),
        // Reading one: the map, its list head, an item's value.
        Point::keep(0xd7c3feusize, "48 83 c1 10 e8 99 0d 48 ff 48 8b 48 08 48 85 c9 75 04 48 8b 4f 18 48 3b 4f 18 74 18 8b 41 30", "a variable's value")
            .uses(&[variables::MAP, variables::HEAD, variables::VALUE]),
        Point::keep(0x1fd200usize, "48 83 7b 28 10 48 8d 53 10", "a variable's name").uses(&[variables::NAME]),
        // country_event: the events by number, the event an item holds, its
        // trigger's conditions, then TriggerEvent.
        Point::keep(0x6612bbusize, "4c 8b 0d 46 f0 cd 01", "the events").uses(&[EVENTS]),
        Point::keep(0x66132busize, "49 8b 51 08", "the events' list").uses(&[events::HEAD]),
        Point::keep(0x661360usize, "4c 8b 70 18 4d 85 f6", "an event of the events").uses(&[events::EVENT]),
        Point::keep(0x661376usize, "49 8b 5e 48 49 63 46 54", "an event's trigger").uses(&[event::TRIGGER + 8, event::TRIGGER + 0x14]),
        Point::keep(EVENT_MANAGER, "48 89 5c 24 10 57 48 83 ec 20 48 8b 05 3f 8c d3 01 48 85 c0 0f 85 f8 00", "event manager"),
        Point::keep(TRIGGER_EVENT, "48 89 5c 24 18 48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 30 4d 8b f8 48 8b f2", "trigger event"),
    ]
}

/// The day a flag was set on `scope`, or none.
pub fn flag(WorldSession { m, world, .. }: WorldSession, args::Flag { scope, name }: args::Flag) -> GResult<FlagState> {
    checked(Use::Flag, scope, "flag", &name)?;
    let day = match (flag_set(m, &world, scope)?, flag_id(m, name.as_bytes())?) {
        (Some(set), Some(id)) => flag_day(m, set, id)?.map(Date),
        _ => None,
    };
    Ok(FlagState { set: day.is_some(), day })
}

/// Sets a flag on `scope`, today.
pub fn set_flag(WorldSession { m, world, .. }: WorldSession, args::SetFlag { scope, name }: args::SetFlag) -> GResult<()> {
    checked(Use::Flag, scope, "flag", &name)?;
    let Some(set) = flag_set(m, &world, scope)? else { return absent(scope) };
    let id = with_string(m, name.as_bytes(), |at| FLAG_ID.call(m, [at]))? as u16;
    SET_FLAG.call(m, [set, id as usize, BEFORE_ANY_DAY])?;
    Ok(())
}

/// Clears a flag of `scope`.
pub fn clear_flag(WorldSession { m, world, .. }: WorldSession, args::ClearFlag { scope, name }: args::ClearFlag) -> GResult<()> {
    checked(Use::Flag, scope, "flag", &name)?;
    let Some(set) = flag_set(m, &world, scope)? else { return Ok(()) };
    if let Some(id) = flag_id(m, name.as_bytes())? {
        CLEAR_FLAG.call(m, [set, id as usize])?;
    }
    Ok(())
}

/// A variable's value, 0 when there is none, and whether there is one.
pub fn variable(WorldSession { m, world, .. }: WorldSession, args::Variable { scope, name }: args::Variable) -> GResult<VariableState> {
    checked(Use::Variable, scope, "variable", &name)?;
    let held = variables_of(m, &world, scope)?;
    for item in containers::linked(m, held + variables::HEAD, MOST)? {
        if game_string::read(m, item + variables::NAME)? == name.as_bytes() {
            return Ok(VariableState { set: true, value: f64::from(m.i32(item + variables::VALUE)?) / 1000.0 });
        }
    }
    Ok(VariableState { set: false, value: 0.0 })
}

/// Sets a variable.
pub fn set_variable(WorldSession { m, world, .. }: WorldSession, args::SetVariable { scope, name, value }: args::SetVariable) -> GResult<()> {
    checked(Use::Variable, scope, "variable", &name)?;
    checks::value(value)?;
    let held = variables_of(m, &world, scope)?;
    let thousandths: i32 = (value * 1000.0).round() as i32;
    with_string(m, name.as_bytes(), |at| SET_VARIABLE.call(m, [held, at, &thousandths as *const i32 as usize]))?;
    Ok(())
}

/// Fires an event, once its trigger holds.
pub fn fire_event(WorldSession { m, world, .. }: WorldSession, args::FireEvent { scope, id }: args::FireEvent) -> GResult<()> {
    checked(Use::Event, scope, "event", &id)?;
    let (tag, province) = match scope {
        Scope::Country(tag) => (countries::named(m, &world, tag)?, 0),
        Scope::Province(ProvinceId(number)) => {
            let at = province_address(m, &world, number)?;
            if !Tag(m.u64(at + province_fields::OWNER)?).valid() {
                return super::fail(Status::NotFound, format!("no one owns province {number}, whose owner an event is for"));
            }
            (Tag(NO_TAG), number)
        }
        _ => unreachable!("checked: an event's scope is a country or a province"),
    };
    let fired = find_event(m, id.as_bytes())?.ok_or_else(|| checks::no_event(&id))?;
    let mut scope = conditions::Scope::new(m, tag, province)?;
    if conditions::EVALUATE.call(m, [fired + event::TRIGGER, scope.address()])? & 0xff == 0 {
        return Err(checks::trigger_fails(&id).into());
    }
    let manager = EVENT_MANAGER.call(m, [])?;
    TRIGGER_EVENT.call(m, [manager, fired, scope.address()])?;
    Ok(())
}

/// The checks every call makes of its scope and name.
fn checked(use_: Use, scope: Scope, what: &str, name: &str) -> GResult<()> {
    checks::scope(use_, scope.kind(), scope == Scope::Global(true))?;
    checks::name(what, name)?;
    Ok(())
}

/// The refusal of a flag set on a monarch a country has not.
fn absent<T>(scope: Scope) -> GResult<T> {
    let (who, tag) = match scope {
        Scope::Heir(tag) => ("heir", tag),
        Scope::Consort(tag) => ("consort", tag),
        _ => ("ruler", eu4_harmony_remote::CountryTag(*b"---")),
    };
    super::fail(Status::Rejected, format!("{tag} has no {who}"))
}

/// The flag set `scope` keeps its flags in; none for an heir or a consort
/// its country has not.
fn flag_set(m: &Memory, world: &World, scope: Scope) -> GResult<Option<usize>> {
    let country_of = |tag| -> GResult<usize> { country(m, world, countries::named(m, world, tag)?) };
    Ok(match scope {
        Scope::Country(tag) => Some(country_of(tag)? + kept::COUNTRY_FLAGS),
        Scope::Province(ProvinceId(number)) => Some(province_address(m, world, number)? + kept::PROVINCE_FLAGS),
        Scope::Global(_) => Some(m.ptr(m.at(WORLD))? + kept::GLOBAL_FLAGS),
        Scope::Ruler(tag) => {
            let at = country_of(tag)?;
            let ruler = m.ptr(at + country_fields::RULER)?;
            Some(if person(m, ruler)? { ruler + kept::MONARCH_FLAGS } else { at + kept::COUNTRY_RULERLESS_FLAGS })
        }
        Scope::Heir(tag) => {
            let heir = m.ptr(country_of(tag)? + country_fields::HEIR)?;
            person(m, heir)?.then_some(heir + kept::MONARCH_FLAGS)
        }
        Scope::Consort(tag) => {
            let at = country_of(tag)?;
            let ruler = m.ptr(at + country_fields::RULER)?;
            let consort = if ruler != 0 && asks(m, ruler, kept::IS_CONSORT_REGENT)? { ruler } else { m.ptr(at + country_fields::CONSORT)? };
            person(m, consort)?.then_some(consort + kept::MONARCH_FLAGS)
        }
    })
}

/// Whether the monarch at `monarch` is a person, not a placeholder.
fn person(m: &Memory, monarch: usize) -> GResult<bool> {
    Ok(monarch != 0 && asks(m, monarch, kept::IS_PERSON)?)
}

/// What the method of `object`'s vtable at `slot` answers, a bool.
fn asks(m: &Memory, object: usize, slot: usize) -> GResult<bool> {
    Ok(m.call(m.vslot(object, slot)?, &[object])? & 0xff != 0)
}

/// The variables `scope` keeps.
fn variables_of(m: &Memory, world: &World, scope: Scope) -> GResult<usize> {
    match scope {
        Scope::Country(tag) => Ok(country(m, world, countries::named(m, world, tag)?)? + kept::COUNTRY_VARIABLES),
        Scope::Province(ProvinceId(number)) => Ok(province_address(m, world, number)? + kept::PROVINCE_VARIABLES),
        _ => unreachable!("checked: a variable's scope is a country or a province"),
    }
}

fn province_address(m: &Memory, world: &World, number: u32) -> GResult<usize> {
    if number == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    with_state(|s| s.geography.province_address(m, world, number))
}

/// The id of the flag named `name`, when the game has met the name; the
/// framework only looks, so that asking for a flag no script names leaves
/// the game's names as they are.
fn flag_id(m: &Memory, name: &[u8]) -> GResult<Option<u16>> {
    for (id, at) in containers::entries(m, m.at(FLAG_NAMES), game_string::OBJECT, MOST)?.into_iter().enumerate() {
        if game_string::read(m, at)? == name {
            return Ok(u16::try_from(id).ok());
        }
    }
    Ok(None)
}

/// The day the flag `id` was set in the flag set at `set`, or none.
fn flag_day(m: &Memory, set: usize, id: u16) -> GResult<Option<i32>> {
    let ids = containers::entries(m, set + flags::IDS, 2, MOST)?;
    let days = containers::entries(m, set + flags::DAYS, 4, MOST)?;
    for (i, at) in ids.into_iter().enumerate() {
        if m.u16(at)? == id {
            let day = days.get(i).ok_or_else(|| format!("flag set at {set:#x}: fewer days than ids"))?;
            return Ok(Some(m.i32(*day)?));
        }
    }
    Ok(None)
}

/// The event whose id is `id`, or none.
fn find_event(m: &Memory, id: &[u8]) -> GResult<Option<usize>> {
    let map = m.ptr(m.at(EVENTS))?;
    if map == 0 {
        return Ok(None);
    }
    for item in containers::linked(m, m.ptr(map + events::HEAD)?, MOST)? {
        let found = m.ptr(item + events::EVENT)?;
        if found != 0 && game_string::read(m, found + event::NAME)? == id {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// Calls `call` with the address of a `std::string` of the framework's
/// holding `text`, and destroys it as the game destroys its own.
fn with_string<T>(m: &Memory, text: &[u8], call: impl FnOnce(usize) -> crate::Result<T>) -> GResult<T> {
    let mut string = game_string::empty();
    let at = string.as_mut_ptr() as usize;
    let result = game_string::append_checked(m, at, text).and_then(|()| call(at));
    game_string::destroy_checked(m, at)?;
    Ok(result?)
}

crate::catalogue::bind! {
    eu4_harmony_remote::script: ScriptRequest => ScriptAnswer;
    Flag => flag;
    SetFlag => set_flag;
    ClearFlag => clear_flag;
    Variable => variable;
    SetVariable => set_variable;
    FireEvent => fire_event;
}
