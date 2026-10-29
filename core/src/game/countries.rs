//! Countries: finding one by its tag, its tag, whether two are hostile, and
//! one read of a country (docs/research/countries.md).
use super::{context, country, fail, game, span, world, Caller, Ctx, GResult, Status, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{classes, country as fields, country_diplomacy, list, subject_relation, subject_type, war, world as world_fields},
    unicode::game_text,
};
use std::collections::BTreeSet;

/// The country whose tag is these three letters (for example `CAS`);
/// [`Status::NotFound`] when there is none.
pub fn find(_: Caller, letters: [u8; 3]) -> GResult<Tag> {
    let m = game()?;
    let ctx = context(m)?;
    match world::countries(m, &ctx)?.into_iter().find(|t| t.letters() == letters) {
        Some(tag) => Ok(tag),
        None => fail(Status::NotFound, format!("no country {}", String::from_utf8_lossy(&letters))),
    }
}

/// The three letters of a country's tag (for example `CAS`).
pub fn tag(_: Caller, country_id: Tag) -> GResult<[u8; 3]> {
    let m = game()?;
    let ctx = context(m)?;
    country(m, &ctx, country_id)?;
    Ok(country_id.letters())
}

/// Whether the two countries are at war, or either is a rebel faction.
pub fn hostile(_: Caller, a: Tag, b: Tag) -> GResult<bool> {
    let m = game()?;
    let ctx = context(m)?;
    world::hostile(m, &ctx, a, b)
}

/// A country at one moment, as the game keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Country {
    pub tag: Tag,
    /// Thousandths of a ducat.
    pub treasury: i32,
    /// Men.
    pub manpower: i32,
    pub max_manpower: i32,
    /// Thousandths, from -3000 to 3000.
    pub stability: i32,
    /// Thousandths, from -100,000 to 100,000.
    pub prestige: i32,
    /// Monarch points: administrative, diplomatic, military.
    pub power: [i32; 3],
    /// The wars it takes part in, in the order the game keeps them.
    pub wars: Vec<War>,
    /// Its allies, its subjects, its overlord and the countries it rivals,
    /// in that order.
    pub relations: Vec<Relation>,
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

/// How a country stands to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationKind {
    /// They are allied.
    Ally,
    /// The other is its subject, of the relation's subject type.
    Subject,
    /// The other is its overlord, and it is the relation's subject type to
    /// it.
    Overlord,
    /// It has chosen the other as a rival.
    Rival,
}

impl RelationKind {
    /// In the order a snapshot lists them.
    pub const ALL: [RelationKind; 4] = [Self::Ally, Self::Subject, Self::Overlord, Self::Rival];

    /// Its name in a snapshot's JSON (`ally`).
    pub fn key(self) -> &'static str {
        ["ally", "subject", "overlord", "rival"][self as usize]
    }
}

/// One of a country's diplomatic relations: what the other country is to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub kind: RelationKind,
    pub country: Tag,
    /// For a subject or an overlord, the kind of subject the subject is: the
    /// key `common/subject_types` gives it (`vassal`, `march`,
    /// `personal_union`, `tributary_state`, or one a mod adds). None for the
    /// other kinds.
    pub subject_type: Option<String>,
}

/// The most tags a list of a country's, or wars, are believed to hold.
const MOST: usize = 4096;

/// One read of a country: its treasury, manpower, stability, prestige,
/// monarch points, wars and diplomatic relations.
pub fn snapshot(_: Caller, tag: Tag) -> GResult<Country> {
    let m = game()?;
    let ctx = context(m)?;
    let c = country(m, &ctx, tag)?;
    let fixed = |at: usize| m.i32(c + at);
    let mut relations = Vec::new();
    let mut add = |kind: RelationKind, country: Tag, subject_type: Option<String>| relations.push(Relation { kind, country, subject_type });
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
    Ok(Country {
        tag,
        treasury: fixed(fields::TREASURY)?,
        manpower: fixed(fields::MANPOWER)?,
        max_manpower: fixed(fields::MAX_MANPOWER)?,
        stability: fixed(fields::STABILITY)?,
        prestige: fixed(fields::PRESTIGE)?,
        power: [m.i32(c + fields::POWER)?, m.i32(c + fields::POWER_DIP)?, m.i32(c + fields::POWER_MIL)?],
        wars: wars_of(m, &ctx, tag)?,
        relations,
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
        return Ok(Some((overlord, game_text::to_utf8(&game_string::read(m, kind + subject_type::KEY)?))));
    }
    Err(format!("no subject relation to its overlord {:#x}", overlord.0).into())
}

/// The tags, each checked to be a country.
fn checked(m: &Memory, ctx: &Ctx, tags: Vec<Tag>) -> GResult<Vec<Tag>> {
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
    let count = span(begin, m.ptr(vector + 8)?, 8, MOST)?;
    let bytes = if count == 0 { Vec::new() } else { m.bytes(begin, count * 8)? };
    Ok(bytes.chunks_exact(8).map(|b| Tag(u64::from_le_bytes(b.try_into().unwrap()))).collect())
}

/// The tags in a [`list`] of tags at `at`.
fn list_tags(m: &Memory, at: usize) -> GResult<Vec<Tag>> {
    chain(m, m.ptr(at + list::HEAD)?, list::ITEM, list::NEXT)
}

fn rival_tags(m: &Memory, country: usize) -> GResult<Vec<Tag>> {
    chain(m, m.ptr(country + fields::RIVALS)?, fields::RIVAL_TAG, fields::RIVAL_NEXT)
}

/// The tags in a chain of nodes from `first`, each with a tag at `tag` and
/// the next node at `next`.
fn chain(m: &Memory, first: usize, tag: usize, next: usize) -> GResult<Vec<Tag>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut node = first;
    while node != 0 {
        if seen.len() >= MOST || !seen.insert(node) {
            return Err("country list cycle".to_string().into());
        }
        out.push(Tag(m.u64(node + tag)?));
        node = m.ptr(node + next)?;
    }
    Ok(out)
}

/// The wars going on that `country` takes part in, on either side. Only
/// these are read in full, so another war cannot fail the snapshot: one that
/// is no war or whose sides cannot be read is left out, as the country is on
/// neither side of it that can be seen; one with a tag that is no country's
/// fails the snapshot only when it is the country's.
fn wars_of(m: &Memory, ctx: &Ctx, country: Tag) -> GResult<Vec<War>> {
    let data = m.ptr(ctx.world + world_fields::WARS)?;
    let count = m.i32(ctx.world + world_fields::WAR_COUNT)?;
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
        let name = game_text::to_utf8(&game_string::read(m, w + war::NAME)?);
        out.push(War { name, attackers: checked(m, ctx, attackers)?, defenders: checked(m, ctx, defenders)? });
    }
    Ok(out)
}

/// The tags of a war's attackers and defenders, not yet checked; None when
/// `war` is no war or its sides cannot be read.
fn sides(m: &Memory, war: usize) -> Option<(Vec<Tag>, Vec<Tag>)> {
    if war == 0 || m.ptr(war).ok()? != m.at(classes::WAR) {
        return None;
    }
    Some((vector_tags(m, war + war::ATTACKERS).ok()?, vector_tags(m, war + war::DEFENDERS).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::fake::game_tag;

    /// A war of the game's class with these sides; its name is empty.
    fn war_object(m: &Memory, attackers: &[Tag], defenders: &[Tag]) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
        let tags = |tags: &[Tag]| tags.iter().map(|t| t.0).collect::<Vec<u64>>();
        let (attackers, defenders) = (tags(attackers), tags(defenders));
        let mut object = vec![0u64; (war::DEFENDERS + 16) / 8];
        object[0] = m.at(classes::WAR) as u64;
        object[(war::NAME + game_string::CAPACITY) / 8] = game_string::INLINE as u64;
        let range = |v: &Vec<u64>| [v.as_ptr() as u64, v.as_ptr() as u64 + 8 * v.len() as u64];
        object[war::ATTACKERS / 8..war::ATTACKERS / 8 + 2].copy_from_slice(&range(&attackers));
        object[war::DEFENDERS / 8..war::DEFENDERS / 8 + 2].copy_from_slice(&range(&defenders));
        (object, attackers, defenders)
    }

    /// Wars between countries 1 to 3, one with a tag that is no country's,
    /// and an entry that is no war: each country reads only its own wars,
    /// and only the country in the bad war fails.
    #[test]
    fn a_bad_war_fails_only_its_own_countries() {
        let m = Memory { base: 0x1_4000_0000 };
        let (a, b, c) = (game_tag(b"AAA", 1), game_tag(b"BBB", 2), game_tag(b"CCC", 3));
        let objects: Vec<Vec<u64>> = [a, b, c]
            .iter()
            .map(|t| {
                let mut object = vec![0u64; fields::TAG / 8 + 1];
                object[fields::TAG / 8] = t.0;
                object
            })
            .collect();
        let countries: Vec<u64> = [0].into_iter().chain(objects.iter().map(|o| o.as_ptr() as u64)).collect();
        let first = war_object(&m, &[a, c], &[b]);
        let bad = war_object(&m, &[b], &[game_tag(b"ZZZ", 9)]);
        let not_a_war = vec![0u64; first.0.len()];
        let wars = [first.0.as_ptr() as u64, bad.0.as_ptr() as u64, not_a_war.as_ptr() as u64, 0];
        let mut world = vec![0u8; world_fields::WAR_COUNT + 4];
        world[world_fields::WARS..world_fields::WARS + 8].copy_from_slice(&(wars.as_ptr() as u64).to_le_bytes());
        world[world_fields::WAR_COUNT..].copy_from_slice(&(wars.len() as i32).to_le_bytes());
        let ctx = Ctx { world: world.as_ptr() as usize, date: 0, player: a, country: 0, countries: countries.as_ptr() as usize };
        let first_war = War { name: String::new(), attackers: vec![a, c], defenders: vec![b] };
        assert_eq!(wars_of(&m, &ctx, a).unwrap(), [first_war.clone()]);
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
        let mut kind = vec![0u64; (subject_type::KEY + game_string::OBJECT) / 8];
        kind[subject_type::KEY / 8] = u64::from_le_bytes(*b"march\0\0\0");
        kind[(subject_type::KEY + game_string::SIZE) / 8] = 5;
        kind[(subject_type::KEY + game_string::CAPACITY) / 8] = game_string::INLINE as u64;
        let mut relation = vec![0u64; subject_relation::TYPE / 8 + 1];
        relation[0] = m.at(classes::SUBJECT_RELATION) as u64;
        relation[subject_relation::TYPE / 8] = kind.as_ptr() as u64;
        let items = [5, 0, overlord.index() as u64, relation.as_ptr() as u64];
        let mut diplomacy = vec![0u64; country_diplomacy::SUBJECT_COUNT / 8 + 1];
        diplomacy[country_diplomacy::SUBJECTS / 8] = items.as_ptr() as u64;
        diplomacy[country_diplomacy::SUBJECT_COUNT / 8] = 2 << 32;
        let mut records = vec![0u64; fields::RELATION_SIZE * 3 / 8];
        let record = overlord.index() as usize * fields::RELATION_SIZE / 8;
        records[record] = diplomacy.as_ptr() as u64;
        records[record + fields::RELATION_OTHER / 8] = overlord.0;
        let country = |overlord: u64| {
            let mut object = vec![0u64; fields::OVERLORD / 8 + 1];
            object[fields::OVERLORD / 8] = overlord;
            object[fields::RELATIONS / 8] = records.as_ptr() as u64;
            object
        };
        let subject = country(overlord.0);
        assert_eq!(subject_of(&m, subject.as_ptr() as usize).unwrap(), Some((overlord, "march".to_string())));
        assert_eq!(subject_of(&m, country(0).as_ptr() as usize).unwrap(), None);
        relation[0] = m.at(classes::WAR) as u64;
        assert!(subject_of(&m, subject.as_ptr() as usize).is_err());
    }
}
