//! The wars going on and the provinces' owners, watched day by day for the
//! signals of a war declared, a peace made and a province changing owner
//! (docs/plans/player-actions.md, part A5; docs/research/peace.md, "The signals").
//!
//! What the game keeps is compared with what it kept the day before: a war
//! that was not there was declared; countries no longer on a side of a war
//! left it, by a peace or by ceasing to be, and a war no longer there ended;
//! a province whose owner differs changed owner. However it happened, a
//! peace, an annexation, a rebellion or the game console, it is heard. What
//! is not listened to is not read, and its record is dropped, so that
//! listening again starts from that day without telling of what came
//! between.
use super::{countries, GResult, InWorld, Tag};
use crate::{game_string, memory::Memory, objects::campaign::world as world_fields};

/// The most wars the game is believed to keep.
const MOST_WARS: usize = 10_000;

/// Something the watch saw happen since the day before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Happened {
    /// A war began: its name and its sides' leaders.
    WarDeclared { war: String, attacker: Tag, defender: Tag },
    /// Countries left a war: those on each side, and whether the war is
    /// over.
    PeaceMade { war: String, attackers: Vec<Tag>, defenders: Vec<Tag>, ended: bool },
    /// A province changed owner.
    OwnerChanged { province: u32, from: Option<Tag>, to: Option<Tag> },
}

/// A war as the watch keeps it: the game's object, its name and its sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub at: usize,
    pub name: String,
    pub attackers: Vec<Tag>,
    pub defenders: Vec<Tag>,
}

/// What the watch kept of the day before; nothing for what nobody listens
/// to.
#[derive(Default)]
pub struct Watch {
    wars: Option<Vec<Seen>>,
    owners: Option<Vec<Option<Tag>>>,
}

impl Watch {
    /// Forgets what it kept: a new session.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The wars declared and the peaces made since the day it last looked,
    /// comparing the wars the game keeps now: someone listens to war or
    /// peace.
    pub fn wars(&mut self, m: &Memory, ctx: &impl InWorld) -> GResult<Vec<Happened>> {
        let now = wars_now(m, ctx)?;
        let happened = self.wars.as_ref().map(|before| wars_diff(before, &now)).unwrap_or_default();
        self.wars = Some(now);
        Ok(happened)
    }

    /// Nobody listens to war or peace: what was kept of the wars is dropped.
    pub fn forget_wars(&mut self) {
        self.wars = None;
    }

    /// The provinces that changed owner since the day it last looked, `now`
    /// the provinces' owners by number: someone listens to them.
    pub fn owners(&mut self, now: Vec<Option<Tag>>) -> Vec<Happened> {
        let happened = self.owners.as_ref().map(|before| owners_diff(before, &now)).unwrap_or_default();
        self.owners = Some(now);
        happened
    }

    /// Nobody listens to the provinces' owners: what was kept is dropped.
    pub fn forget_owners(&mut self) {
        self.owners = None;
    }
}

/// The wars going on, as the game keeps them; one whose sides cannot be read
/// is left out.
fn wars_now(m: &Memory, ctx: &impl InWorld) -> GResult<Vec<Seen>> {
    let data = m.ptr(ctx.world() + world_fields::WARS)?;
    let count = m.i32(ctx.world() + world_fields::WAR_COUNT)?;
    if !(0..=MOST_WARS as i32).contains(&count) {
        return Err("war array bound".to_string().into());
    }
    let mut out = Vec::new();
    for i in 0..count as usize {
        let at = m.ptr(data + i * 8)?;
        let Some((attackers, defenders)) = countries::sides(m, at) else { continue };
        let name = game_string::shown(m, at + crate::objects::campaign::war::NAME)?;
        let valid = |tags: Vec<Tag>| tags.into_iter().filter(|t| t.valid() && t.normal()).collect();
        out.push(Seen { at, name, attackers: valid(attackers), defenders: valid(defenders) });
    }
    Ok(out)
}

/// What happened to the wars between `before` and `now`: each war there now
/// and not before was declared; each country on a side before and not now
/// left the war, and a war not there now ended. A war is the same war when
/// it is the same object with the same name.
pub fn wars_diff(before: &[Seen], now: &[Seen]) -> Vec<Happened> {
    let same = |a: &Seen, b: &Seen| a.at == b.at && a.name == b.name;
    let gone = |was: &[Tag], is: &[Tag]| -> Vec<Tag> { was.iter().copied().filter(|t| !is.iter().any(|i| i.index() == t.index())).collect() };
    let mut happened = Vec::new();
    for war in before {
        match now.iter().find(|w| same(w, war)) {
            Some(still) => {
                let (attackers, defenders) = (gone(&war.attackers, &still.attackers), gone(&war.defenders, &still.defenders));
                if !attackers.is_empty() || !defenders.is_empty() {
                    happened.push(Happened::PeaceMade { war: war.name.clone(), attackers, defenders, ended: false });
                }
            }
            None => {
                happened.push(Happened::PeaceMade { war: war.name.clone(), attackers: war.attackers.clone(), defenders: war.defenders.clone(), ended: true })
            }
        }
    }
    for war in now.iter().filter(|w| !before.iter().any(|b| same(b, w))) {
        if let (Some(&attacker), Some(&defender)) = (war.attackers.first(), war.defenders.first()) {
            happened.push(Happened::WarDeclared { war: war.name.clone(), attacker, defender });
        }
    }
    happened
}

/// The provinces whose owner differs between `before` and `now`, by number.
pub fn owners_diff(before: &[Option<Tag>], now: &[Option<Tag>]) -> Vec<Happened> {
    let index = |t: &Option<Tag>| t.map(|t| t.index());
    before
        .iter()
        .zip(now)
        .enumerate()
        .filter(|(_, (b, n))| index(b) != index(n))
        .map(|(province, (&from, &to))| Happened::OwnerChanged { province: province as u32, from, to })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::game_tag;

    fn war(at: usize, name: &str, attackers: &[Tag], defenders: &[Tag]) -> Seen {
        Seen { at, name: name.into(), attackers: attackers.to_vec(), defenders: defenders.to_vec() }
    }

    #[test]
    fn a_war_appearing_is_declared_and_one_gone_ended_with_its_countries() {
        let (tur, byz, dul, ven) = (game_tag(b"TUR", 1), game_tag(b"BYZ", 2), game_tag(b"DUL", 3), game_tag(b"VEN", 4));
        let before = [war(0x100, "Ottoman-Byzantine War", &[tur, dul], &[byz, ven])];
        // An ally leaving is a separate peace; the war goes on.
        let separate = [war(0x100, "Ottoman-Byzantine War", &[tur, dul], &[byz])];
        assert_eq!(
            wars_diff(&before, &separate),
            [Happened::PeaceMade { war: "Ottoman-Byzantine War".into(), attackers: vec![], defenders: vec![ven], ended: false }]
        );
        // The war gone: everyone in it left, and it ended; a new one begun.
        let after = [war(0x200, "Venetian-Ottoman War", &[ven], &[tur])];
        assert_eq!(
            wars_diff(&separate, &after),
            [
                Happened::PeaceMade { war: "Ottoman-Byzantine War".into(), attackers: vec![tur, dul], defenders: vec![byz], ended: true },
                Happened::WarDeclared { war: "Venetian-Ottoman War".into(), attacker: ven, defender: tur },
            ]
        );
        // The same object under another name is another war.
        let reused = [war(0x100, "Second War", &[tur], &[byz])];
        assert_eq!(wars_diff(&before, &reused).len(), 2);
        assert!(wars_diff(&before, &before).is_empty());
    }

    #[test]
    fn a_province_whose_owner_differs_changed_owner() {
        let (tur, byz) = (game_tag(b"TUR", 1), game_tag(b"BYZ", 2));
        let before = [None, Some(byz), Some(byz), None];
        let now = [None, Some(tur), Some(byz), Some(tur)];
        assert_eq!(
            owners_diff(&before, &now),
            [Happened::OwnerChanged { province: 1, from: Some(byz), to: Some(tur) }, Happened::OwnerChanged { province: 3, from: None, to: Some(tur) }]
        );
    }
}
