//! Estates: the player's estates, their privileges and the estates view's
//! interactions over crown land, granting and revoking privileges and the
//! interactions, through the game's own commands
//! (docs/research/government.md, "Estates").
use super::content::checks;
use super::{action, conditions, content, modifiers, thousandths, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::{
    memory::Memory,
    objects::{campaign::country, map},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::estates::args;
use eu4_harmony_remote::{
    choices::Choice,
    estates::{Estate, EstateInteraction, Privilege},
};

/// The country's estates: a `std::vector` of 0xa8-byte entries, each with
/// its estate, loyalty, land share (thousandths of a percentage point) and
/// the privileges granted, a `std::map` whose head node is at `+0x90`.
const ESTATES: usize = 0x21b0;
const ENTRY: usize = 0xa8;
const ENTRY_ESTATE: usize = 0x10;
const ENTRY_LOYALTY: usize = 0x18;
const ENTRY_LAND: usize = 0x20;
const ENTRY_GRANTED: usize = 0x90;
/// A `std::map` node: its left, parent and right nodes, whether it is the
/// head, and its value, here the privilege.
/// The estates' database, a `std::vector` of pointers, its index what
/// commands name an estate by; an estate's key and its privileges, a game
/// array of pointers into the privileges' database.
const ESTATE_DATABASE: usize = 0x233fe58;
const ESTATE_KEY: usize = 0x10;
const ESTATE_PRIVILEGES: usize = 0x520;
/// The privileges' database, a game array of 0x888-byte entries, its index
/// what commands name a privilege by; a privilege's key and its conditions
/// for being shown (`is_valid`).
const PRIVILEGE_DATABASE: usize = 0x242bc10;
const PRIVILEGE: usize = 0x888;
const PRIVILEGE_KEY: usize = 0x18;
const IS_VALID: usize = 0x588;
/// `(entry, &tag, privilege, reasons) -> bool` and `(entry, tag, privilege,
/// 0, reasons) -> bool`: whether the country may grant and revoke the
/// privilege, writing why not into the `std::string` reasons.
const CAN_GRANT: GameFunction<4> = GameFunction::at(0x71edf0);
const CAN_REVOKE: GameFunction<5> = GameFunction::at(0x71f430);
/// `(entry, out, modifier values) -> out`: the estate's influence, in
/// thousandths, from the country's modifiers.
const INFLUENCE: GameFunction<3> = GameFunction::at(0x71a520);
/// The crown land's interactions: `() -> object`, whose `+0x60` is their
/// game array of 0x128-byte entries, each with its key; `(country,
/// interaction, reasons) -> bool`, whether the country may use it.
const CROWN_LAND: GameFunction<0> = GameFunction::at(0x728430);
const INTERACTIONS: usize = 0x58;
const INTERACTION: usize = 0x128;
const INTERACTION_KEY: usize = 0x18;
const CAN_INTERACT: GameFunction<3> = GameFunction::at(0x4128b0);
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x2ddc64usize, "49 8d 8f b0 21 00 00", "country estates").uses(&[ESTATES]),
        Point::keep(0x418880usize, "48 69 c2 a8 00 00 00", "estate entry size"),
        Point::keep(0x71b85busize, "48 8b 59 10", "estate entry's estate").uses(&[ENTRY_ESTATE]),
        Point::keep(0x71b891usize, "8b 5e 18", "estate loyalty").uses(&[ENTRY_LOYALTY]),
        Point::keep(0x71b8fbusize, "8b 5e 20", "estate land share").uses(&[ENTRY_LAND]),
        Point::keep(0x71b9dbusize, "4c 8d 86 90 00 00 00", "estate privileges granted").uses(&[ENTRY_GRANTED]),
        Point::keep(0x721f20usize, "48 8b 53 20 8b 73 28", "privilege granted node").uses(&[map::VALUE]),
        // The grant command's check: the estate by its index, the privilege
        // by its index into the database's entries.
        Point::keep(0x2c1757usize, "48 8b 05 fa e6 07 02", "estates database").uses(&[ESTATE_DATABASE]),
        Point::keep(0x2c17a0usize, "e8 ab c4 46 00 85 ff 78 12 3b 78 14 7d 0d 48 69 ff 88 08 00 00 48 03 78 08", "privileges database"),
        Point::keep(0x72dc5fusize, "48 8b 05 aa df cf 01 48 85 c0", "privileges database getter").uses(&[PRIVILEGE_DATABASE]),
        // The estate's reader: its privileges found by key and added.
        Point::keep(0x72378cusize, "4d 8d b7 20 05 00 00", "estate privileges").uses(&[ESTATE_PRIVILEGES]),
        Point::keep(CAN_GRANT, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57", "privilege grant check"),
        Point::keep(CAN_REVOKE, "48 89 5c 24 10", "privilege revoke check"),
        Point::keep(0x71832cusize, "e8 ef 21 00 00", "estate influence"),
        Point::keep(0x718308usize, "49 81 c5 c0 28 00 00 4c 89 ad 98 00 00 00 49 8b cd e8", "country modifiers").uses(&[country::MODIFIERS]),
        // The interaction command's check: the interaction by its index.
        Point::keep(0xc50acusize, "e8 7f 33 66 00 85 ff 78 3a 3b 78 6c 7d 35 48 69 d7 28 01 00 00 48 03 50 60", "crown land interactions"),
        Point::keep(INFLUENCE, "48 89 5c 24 18 57 48 83 ec 20 48 8b da 48 8b f9", "estate influence getter"),
        Point::keep(CROWN_LAND, "48 89 5c 24 18 48 89 74 24 20 57 48 83 ec 20 48", "crown land getter"),
        Point::keep(CAN_INTERACT, "48 89 5c 24 18 55 56 57 41 56 41 57", "crown land interaction check"),
    ]
}

/// The player's estates, by their entries.
fn entries(m: &Memory, country: usize) -> GResult<Vec<usize>> {
    let (begin, end) = (m.ptr(country + ESTATES)?, m.ptr(country + ESTATES + 8)?);
    Ok((0..containers::span(begin, end, ENTRY, MOST)?).map(|i| begin + i * ENTRY).collect())
}

/// The privileges a `std::map` with its head at `head` holds.
fn granted(m: &Memory, head: usize) -> GResult<Vec<usize>> {
    Ok(containers::map(m, head, MOST)?.into_iter().map(|node| m.ptr(node + map::VALUE)).collect::<crate::Result<_>>()?)
}

fn privilege_database(m: &Memory) -> GResult<(usize, usize)> {
    let database = m.ptr(m.at(PRIVILEGE_DATABASE))?;
    if database == 0 {
        return Err("the privileges are not read yet".to_string().into());
    }
    Ok((m.ptr(database + containers::ARRAY_DATA)?, containers::bounded(m.i32(database + containers::ARRAY_COUNT)?, containers::MOST)?))
}

/// The index of an estate in its database, and of a privilege in its own.
fn estate_index(m: &Memory, estate: usize) -> GResult<usize> {
    let database = m.ptr(m.at(ESTATE_DATABASE))?;
    match containers::pointers(m, database, containers::MOST)?.iter().position(|&e| e == estate) {
        Some(i) => Ok(i),
        None => Err("an estate not in the database".to_string().into()),
    }
}

fn privilege_index(m: &Memory, privilege: usize) -> GResult<usize> {
    let (data, count) = privilege_database(m)?;
    match privilege.checked_sub(data).filter(|o| o % PRIVILEGE == 0).map(|o| o / PRIVILEGE) {
        Some(i) if i < count => Ok(i),
        _ => Err("a privilege not in the database".to_string().into()),
    }
}

fn influence(m: &Memory, country: usize, entry: usize) -> GResult<i32> {
    let values = modifiers::values(m, country)?;
    let mut out = 0i32;
    INFLUENCE.call(m, [entry, &mut out as *mut i32 as usize, values])?;
    Ok(out)
}

/// Whether the player can grant `privilege`, or revoke it when `granted`,
/// and why not, as the game's checks write it.
fn choice(m: &Memory, player: Tag, entry: usize, privilege: usize, granted: bool) -> GResult<Choice> {
    let tag = player.0;
    let (can, reasons) = content::checked(m, |reasons| match granted {
        false => Ok(CAN_GRANT.call(m, [entry, &tag as *const u64 as usize, privilege, reasons])?),
        true => Ok(CAN_REVOKE.call(m, [entry, tag as usize, privilege, 0, reasons])?),
    })?;
    Ok(if can { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// The player's estates as the estates view shows them.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<Estate>> {
    let mut listed = Vec::new();
    for entry in entries(m, ctx.country)? {
        let estate = m.ptr(entry + ENTRY_ESTATE)?;
        let key = game_string::key(m, estate + ESTATE_KEY)?;
        let granted = granted(m, m.ptr(entry + ENTRY_GRANTED)?)?;
        let mut privileges = Vec::new();
        for privilege in containers::array(m, estate + ESTATE_PRIVILEGES, containers::MOST)? {
            let has = granted.contains(&privilege);
            if !has && !conditions::holds(m, privilege + IS_VALID, ctx.player, 0)? {
                continue;
            }
            let name = game_string::key(m, privilege + PRIVILEGE_KEY)?;
            privileges.push(Privilege { name: content::name(m, &name)?, privilege: name, granted: has, choice: choice(m, ctx.player, entry, privilege, has)? });
        }
        listed.push(Estate {
            name: content::name(m, &key)?,
            estate: key,
            loyalty: thousandths(m.i32(entry + ENTRY_LOYALTY)?),
            influence: thousandths(influence(m, ctx.country, entry)?),
            land_share: thousandths(m.i32(entry + ENTRY_LAND)?),
            privileges,
        });
    }
    Ok(listed)
}

/// The crown land's interactions, by their entries.
fn interactions(m: &Memory) -> GResult<Vec<usize>> {
    let crown = CROWN_LAND.call(m, [])?;
    let (data, count) =
        (m.ptr(crown + INTERACTIONS + containers::ARRAY_DATA)?, containers::bounded(m.i32(crown + INTERACTIONS + containers::ARRAY_COUNT)?, containers::MOST)?);
    Ok((0..count).map(|i| data + i * INTERACTION).collect())
}

fn interaction_choice(m: &Memory, country: usize, interaction: usize) -> GResult<Choice> {
    let (can, reasons) = content::checked(m, |reasons| Ok(CAN_INTERACT.call(m, [country, interaction, reasons])?))?;
    Ok(if can { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// The estates view's interactions over crown land.
pub fn interactions_list(Session { m, ctx, .. }: Session) -> GResult<Vec<EstateInteraction>> {
    interactions(m)?
        .into_iter()
        .map(|at| {
            let key = game_string::key(m, at + INTERACTION_KEY)?;
            Ok(EstateInteraction { name: content::name(m, &key)?, interaction: key, choice: interaction_choice(m, ctx.country, at)? })
        })
        .collect()
}

/// The player's estate `estate` and its privilege `privilege`: the entry, the
/// privilege, and whether it is granted.
fn find(m: &Memory, country: usize, estate: &str, privilege: &str) -> GResult<(usize, usize, usize, bool)> {
    for entry in entries(m, country)? {
        let at = m.ptr(entry + ENTRY_ESTATE)?;
        if game_string::key(m, at + ESTATE_KEY)? != estate {
            continue;
        }
        for p in containers::array(m, at + ESTATE_PRIVILEGES, containers::MOST)? {
            if game_string::key(m, p + PRIVILEGE_KEY)? == privilege {
                let has = granted(m, m.ptr(entry + ENTRY_GRANTED)?)?.contains(&p);
                return Ok((entry, at, p, has));
            }
        }
        return Err(checks::not_found("privilege", privilege));
    }
    Err(checks::not_found("estate", estate))
}

/// The grant and revoke commands: the country, the estate and the privilege
/// by their indices.
fn written(command: &str, country: Tag, estate: usize, privilege: usize) -> String {
    format!("{command}={{ members={{ dummy=\"{}\" dummy={estate} dummy={privilege} }} }}", content::letters(country))
}

fn written_interaction(country: Tag, interaction: usize) -> String {
    format!("crown_land_interaction={{ members={{ dummy=\"{}\" dummy={interaction} }} }}", content::letters(country))
}

/// Grants an estate of the player's a privilege.
pub fn grant_privilege(Session { m, ctx, commands, .. }: Session, args::GrantPrivilege { estate, privilege }: args::GrantPrivilege) -> GResult<()> {
    let (entry, at, p, has) = find(m, ctx.country, &estate, &privilege)?;
    if has {
        return Err(checks::not_now(format!("{estate} has the privilege {privilege} already")));
    }
    let choice = choice(m, ctx.player, entry, p, false)?;
    if !choice.can {
        return Err(action::not_allowed("grant this privilege", &choice.reasons));
    }
    action::post(commands, &written("grant_privilege", ctx.player, estate_index(m, at)?, privilege_index(m, p)?), "grant this privilege")
}

/// Revokes a privilege the player granted an estate.
pub fn revoke_privilege(Session { m, ctx, commands, .. }: Session, args::RevokePrivilege { estate, privilege }: args::RevokePrivilege) -> GResult<()> {
    let (entry, at, p, has) = find(m, ctx.country, &estate, &privilege)?;
    if !has {
        return Err(checks::not_now(format!("{estate} does not have the privilege {privilege}")));
    }
    let choice = choice(m, ctx.player, entry, p, true)?;
    if !choice.can {
        return Err(action::not_allowed("revoke this privilege", &choice.reasons));
    }
    action::post(commands, &written("revoke_privilege", ctx.player, estate_index(m, at)?, privilege_index(m, p)?), "revoke this privilege")
}

/// Uses one of the estates view's interactions over crown land.
pub fn interact(Session { m, ctx, commands, .. }: Session, args::Interact { interaction: key }: args::Interact) -> GResult<()> {
    for (index, at) in interactions(m)?.into_iter().enumerate() {
        if game_string::key(m, at + INTERACTION_KEY)? != key {
            continue;
        }
        let choice = interaction_choice(m, ctx.country, at)?;
        if !choice.can {
            return Err(action::not_allowed("use this interaction", &choice.reasons));
        }
        return action::post(commands, &written_interaction(ctx.player, index), "use this interaction");
    }
    Err(checks::not_found("crown land interaction", &key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_name_the_estate_and_privilege_by_their_indices() {
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(written("grant_privilege", fra, 3, 45), r#"grant_privilege={ members={ dummy="FRA" dummy=3 dummy=45 } }"#);
        assert_eq!(written_interaction(fra, 2), r#"crown_land_interaction={ members={ dummy="FRA" dummy=2 } }"#);
    }

    use crate::game::{
        buffers::{self, Image, Object},
        commands::record,
        Caller, Fail, Status,
    };

    /// France with the nobles, whose privileges are `nobles_land` and
    /// `nobles_army`, the first granted when `granted`; and the session of
    /// it whose commands are recorded.
    struct France {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    fn france(granted: bool) -> France {
        let image = Image::new();
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        let (land, army) =
            (Object::new(PRIVILEGE_KEY + 0x20).string(PRIVILEGE_KEY, "nobles_land"), Object::new(PRIVILEGE_KEY + 0x20).string(PRIVILEGE_KEY, "nobles_army"));
        let privileges = [land.at() as u64, army.at() as u64];
        let nobles = Object::new(ESTATE_PRIVILEGES + 0x18)
            .string(ESTATE_KEY, "estate_nobles")
            .vector(ESTATE_PRIVILEGES + containers::ARRAY_DATA, &privileges)
            .i32(ESTATE_PRIVILEGES + containers::ARRAY_COUNT, 2);
        let held = Object::map(if granted { &privileges[..1] } else { &[] });
        let entry = Object::new(ENTRY).points_to(ENTRY_ESTATE, nobles).points_to(ENTRY_GRANTED, held);
        let begin = entry.at() as u64;
        let country = Object::country(fra, ESTATES + 0x10).points_to(ESTATES, entry).u64(ESTATES + 8, begin + ENTRY as u64);
        let countries = Object::countries(&[&country]);
        let session = buffers::session(image.memory(), 0, fra, country.at(), countries.at(), Caller::Remote);
        France { _image: image, _objects: vec![country, countries, land, army], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the action was carried out") };
        (status, message)
    }

    #[test]
    fn a_privilege_is_found_by_its_estate_and_its_key_or_refused() {
        let grant = |estate: &str, privilege: &str, granted| {
            refused(grant_privilege(france(granted).session, args::GrantPrivilege { estate: estate.into(), privilege: privilege.into() }))
        };
        let revoke = |estate: &str, privilege: &str, granted| {
            refused(revoke_privilege(france(granted).session, args::RevokePrivilege { estate: estate.into(), privilege: privilege.into() }))
        };
        assert_eq!(grant("estate_church", "nobles_land", false), (Status::NotFound, "no estate \"estate_church\" for the player".into()));
        assert_eq!(grant("estate_nobles", "church_land", false), (Status::NotFound, "no privilege \"church_land\" for the player".into()));
        assert_eq!(grant("estate_nobles", "nobles_land", true), (Status::Rejected, "estate_nobles has the privilege nobles_land already".into()));
        assert_eq!(revoke("estate_nobles", "nobles_army", true), (Status::Rejected, "estate_nobles does not have the privilege nobles_army".into()));
        assert_eq!(revoke("estate_nobles", "nobles_land", false), (Status::Rejected, "estate_nobles does not have the privilege nobles_land".into()));
        assert!(record::posted().is_empty());
    }
}
