//! Leaders: what the military view offers the player to recruit, the
//! player's leaders, and giving one to an army or a fleet, through the
//! game's own commands, as the military view and the unit panels post them
//! (docs/research/leaders.md).
use super::units::checks;
use super::{action, modifiers, world, Caller, Ctx, GResult, Session};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{country as country_fields, unit},
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::leaders::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    leaders::{Leader, LeaderKind, LeaderOption},
    ArmyId,
};

/// `(country, kind)`: whether the country may recruit a leader of the kind
/// now, the check of the recruit command; no reasons.
const CAN_RECRUIT: GameFunction<2> = GameFunction::at(0x307370);
/// `(tag by value, spending, 0, text or null)`: what a spending costs in
/// whole monarch points; a leader's spending is 3 plus its kind.
const POWER_COST: GameFunction<4> = GameFunction::at(0x951980);
const LEADER_SPENDING: usize = 3;
/// `(leader, text)`: whether the leader may leave the unit it leads, with
/// the tooltip's reasons; `(leader, unit, text)`: whether the unit may take
/// it.
const MAY_LEAVE: GameFunction<2> = GameFunction::at(0x862a70);
const MAY_TAKE: GameFunction<3> = GameFunction::at(0x862fa0);
/// The country's leaders, an intrusive list: head, its count; a node holds
/// the leader at `+0`, the next node at `+0x10`.
const LEADERS: usize = 0x1468;
const LEADERS_COUNT: usize = 0x1478;
const NODE_NEXT: usize = 0x10;
/// A leader's fields: its ID (type at `+0x10`, number at `+0x14`), name,
/// kind, skills (siege, maneuver, fire, shock), the unit it leads, and its
/// monarch's ID.
mod leader {
    pub const ID: usize = 0x10;
    pub const NUMBER: usize = 0x14;
    pub const NAME: usize = 0x20;
    pub const KIND: usize = 0x40;
    pub const SIEGE: usize = 0x44;
    pub const MANEUVER: usize = 0x48;
    pub const FIRE: usize = 0x4c;
    pub const SHOCK: usize = 0x50;
    pub const UNIT: usize = 0x58;
    /// The type every leader's ID has.
    pub const TYPE: u32 = 49;
}
/// The yes-or-no modifiers an explorer and a conquistador need, and the
/// one that forbids colonies.
const MAY_EXPLORE: usize = 0xe3;
const NO_COLONIES: usize = 0;
/// A unit's leader.
const UNIT_LEADER: usize = 0xd8;
/// The most leaders read.
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(CAN_RECRUIT, "48 89 5c 24 08 48 89 74 24 18 57 48 83 ec 20 8b da 48 8b f9 83 fa 04 74 4b", "may recruit a leader"),
        // The recruit check asks the two modifiers.
        Point::keep(0x30739cusize, "48 8d b1 c0 28 00 00 48 8b ce e8 a5 63 63 00 48 8d 48 08 ba e3 00 00 00 e8 a7 07 5e 00", "a leader's modifiers").uses(&[
            country_fields::MODIFIERS,
            modifiers::VALUES,
            MAY_EXPLORE,
        ]),
        Point::keep(POWER_COST, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55 41 56 41 57", "monarch point cost"),
        Point::keep(MAY_LEAVE, "48 89 5c 24 10 48 89 7c 24 20 55 48 8b ec 48 83 ec 60 48 8b da 48 8b f9", "leader may leave its unit"),
        Point::keep(MAY_TAKE, "48 89 5c 24 08 48 89 74 24 18 57 48 83 ec 60 49 8b f8 48 8b da 48 8b f1", "unit may take a leader"),
        // CCountry::AddLeader walks the list.
        Point::keep(0x306c51usize, "49 8b 87 68 14 00 00", "country leaders").uses(&[LEADERS]),
        Point::keep(0x861cd1usize, "4c 8d 46 10", "leader id").uses(&[leader::ID]),
        Point::keep(0x8618b6usize, "48 8d 56 20", "leader name").uses(&[leader::NAME]),
        Point::keep(0x86c40ausize, "89 4e 40", "leader kind").uses(&[leader::KIND]),
        Point::keep(0x86c8d4usize, "44 8b 76 4c 8b 7e 50 44 8b 66 48 8b 46 44", "leader skills").uses(&[
            leader::FIRE,
            leader::SHOCK,
            leader::MANEUVER,
            leader::SIEGE,
        ]),
        Point::keep(0x86e6e6usize, "48 83 7f 58 00", "leader's unit").uses(&[leader::UNIT]),
        Point::keep(0xd406efusize, "48 89 be d8 00 00 00", "unit's leader").uses(&[UNIT_LEADER]),
    ]
}

const KINDS: [LeaderKind; 4] = [LeaderKind::General, LeaderKind::Admiral, LeaderKind::Conquistador, LeaderKind::Explorer];

fn key(kind: LeaderKind) -> &'static str {
    match kind {
        LeaderKind::General => "general",
        LeaderKind::Admiral => "admiral",
        LeaderKind::Conquistador => "conquistador",
        LeaderKind::Explorer => "explorer",
    }
}

/// The game's number for a kind.
fn index(kind: LeaderKind) -> usize {
    KINDS.iter().position(|k| *k == kind).unwrap_or(0)
}

/// Whether a kind leads a fleet: an admiral or an explorer.
fn naval(kind: LeaderKind) -> bool {
    matches!(kind, LeaderKind::Admiral | LeaderKind::Explorer)
}

/// The kind `word` names, or the refusal.
pub fn kind(word: &str) -> GResult<LeaderKind> {
    KINDS.into_iter().find(|k| key(*k) == word).ok_or_else(|| checks::unknown("kind", word, &KINDS.map(key)))
}

/// The recruit command: the kind by its number, and no name, so that the
/// game picks the name and the sex, as it does for the name it offers.
fn create_text(country: [u8; 3], kind: LeaderKind) -> String {
    format!("create_leader={{ country=\"{}\" leader={} }}", String::from_utf8_lossy(&country), index(kind))
}

/// An ID as the game writes it: its number and its type.
fn id_text(id: u64) -> String {
    format!("{{ id={} type={} }}", (id >> 32) as i32, id as u32 as i32)
}

fn leader_id(number: u32) -> u64 {
    (number as u64) << 32 | leader::TYPE as u64
}

fn assign_text(leader: u64, unit: u64) -> String {
    format!("assign_leader={{ leader={} unit={} }}", id_text(leader), id_text(unit))
}

fn dismiss_text(country: [u8; 3], leader: u64) -> String {
    format!("dismiss_leader={{ country=\"{}\" leader={} }}", String::from_utf8_lossy(&country), id_text(leader))
}

/// What a kind of leader costs, in whole points of the power it is paid in.
fn cost(m: &Memory, ctx: &Ctx, kind: LeaderKind) -> GResult<Amount> {
    let points = POWER_COST.call(m, [ctx.player.0 as usize, LEADER_SPENDING + index(kind), 0, 0])? as i32;
    Ok(Amount::new(if naval(kind) { Resource::Dip } else { Resource::Mil }, points as f64))
}

/// Text of a localisation key, plain, with `$COST$` given.
fn line(m: &Memory, key: &str, cost: Option<f64>) -> GResult<String> {
    let mut text = game_string::unformatted(m, key)?;
    if let Some(cost) = cost {
        text = game_text::from_str(&given(&game_text::to_utf8(&text), "COST", &cost.to_string()));
    }
    Ok(super::reasons(&text).join(" "))
}

/// `text` with the value `name` given: `$NAME$`, or `$NAME|Y$` with its
/// format, as the game's localisation writes it.
fn given(text: &str, name: &str, value: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    let open = format!("${name}");
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        let end = match after.chars().next() {
            Some('$') => Some(0),
            Some('|') => after[1..].find('$').map(|e| e + 1),
            _ => None,
        };
        let Some(end) = end else {
            out.push_str(&rest[..at + open.len()]);
            rest = after;
            continue;
        };
        out.push_str(&rest[..at]);
        out.push_str(value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Whether the player may recruit a kind, and the tooltip's reasons why not:
/// the game's check answers yes or no, and the reasons are its tooltip's
/// for the conditions that check tests (docs/research/leaders.md).
fn option(m: &Memory, ctx: &Ctx, kind: LeaderKind) -> GResult<LeaderOption> {
    let amount = cost(m, ctx, kind)?;
    let cost = vec![amount];
    if CAN_RECRUIT.call(m, [ctx.country, index(kind)])? as u8 != 0 {
        return Ok(LeaderOption { kind, choice: Choice::can(cost) });
    }
    let field = if naval(kind) { country_fields::POWER_DIP } else { country_fields::POWER_MIL };
    let points = m.i32(ctx.country + field)?;
    // In the tooltip's order: the idea and the colonies an explorer and a
    // conquistador need, the points, an admiral's force limit.
    let mut reasons = Vec::new();
    let poor = (points as f64) < amount.amount;
    if matches!(kind, LeaderKind::Conquistador | LeaderKind::Explorer) {
        if !modifiers::has(m, ctx.country, MAY_EXPLORE)? {
            reasons.push(line(m, "RECRUIT_NO_IDEA", None)?);
        }
        if modifiers::has(m, ctx.country, NO_COLONIES)? {
            reasons.push(line(m, "CAN_NOT_BUILD_COLONIES", None)?);
        }
    }
    if poor {
        reasons.push(line(m, if naval(kind) { "RECRUIT_CANT_AFFORD_D" } else { "RECRUIT_CANT_AFFORD_M" }, Some(amount.amount))?);
    }
    if kind == LeaderKind::Admiral && !poor {
        reasons.push(line(m, "ADMIRAL_NO_NAVAL_FORCE_LIMIT", None)?);
    }
    reasons.retain(|r| !r.is_empty());
    Ok(LeaderOption { kind, choice: Choice::cannot(reasons, cost) })
}

/// What the military view offers to recruit now.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Vec<LeaderOption>> {
    KINDS.into_iter().map(|kind| option(m, &ctx, kind)).collect()
}

/// The player's leaders' addresses, in the country's order.
fn addresses(m: &Memory, ctx: &Ctx) -> GResult<Vec<usize>> {
    let count = m.i32(ctx.country + LEADERS_COUNT)?;
    if !(0..MOST as i32).contains(&count) {
        return Err(format!("{count} leaders").into());
    }
    let mut node = m.ptr(ctx.country + LEADERS)?;
    let mut out = Vec::new();
    while node != 0 {
        if out.len() >= count as usize {
            return Err("the leader list is longer than its count".to_string().into());
        }
        out.push(m.ptr(node)?);
        node = m.ptr(node + NODE_NEXT)?;
    }
    Ok(out)
}

fn read(m: &Memory, address: usize) -> GResult<Leader> {
    let kind = m.i32(address + leader::KIND)?;
    let Some(kind) = KINDS.get(kind as usize).copied() else {
        return Err(format!("a leader of the kind {kind}").into());
    };
    let unit = m.ptr(address + leader::UNIT)?;
    Ok(Leader {
        id: m.u32(address + leader::NUMBER)?,
        name: game_string::shown(m, address + leader::NAME)?,
        kind,
        fire: m.i32(address + leader::FIRE)?,
        shock: m.i32(address + leader::SHOCK)?,
        maneuver: m.i32(address + leader::MANEUVER)?,
        siege: m.i32(address + leader::SIEGE)?,
        army: if unit == 0 { None } else { Some(ArmyId(m.u64(unit + unit::ID)?)) },
    })
}

/// The player's leaders.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<Leader>> {
    addresses(m, &ctx)?.into_iter().map(|a| read(m, a)).collect()
}

/// The player's leader with the number `id`.
fn find(m: &Memory, ctx: &Ctx, id: u32) -> GResult<usize> {
    for address in addresses(m, ctx)? {
        if m.u32(address + leader::NUMBER)? == id && m.u32(address + leader::ID)? == leader::TYPE {
            return Ok(address);
        }
    }
    Err(checks::no_leader(id))
}

/// Recruits a leader of a kind, as the military view's button does, with the
/// name the game picks.
pub fn recruit(caller: Caller, args::Recruit { kind: word }: args::Recruit) -> GResult<Leader> {
    let kind = kind(&word)?;
    let Session { m, ctx, commands, .. } = Session::open(caller)?;
    let o = option(m, &ctx, kind)?;
    if !o.choice.can {
        return Err(action::not_allowed(&format!("recruit a leader of the kind {word}"), &o.choice.reasons));
    }
    let before = addresses(m, &ctx)?;
    action::post(commands, &create_text(ctx.player.letters(), kind), &format!("recruit a leader of the kind {word}"))?;
    let Some(new) = addresses(m, &ctx)?.into_iter().find(|a| !before.contains(a)) else {
        return Err("the game recruited no leader".to_string().into());
    };
    read(m, new)
}

/// Gives a leader to one of the player's armies or fleets.
pub fn assign(Session { m, ctx, commands, .. }: Session, args::Assign { leader: id, army: ArmyId(army) }: args::Assign) -> GResult<()> {
    let leader = find(m, &ctx, id)?;
    let unit = world::own_unit(m, &ctx, army)?;
    let kind = read(m, leader)?.kind;
    let land = m.ptr(unit)? == m.at(crate::objects::campaign::classes::ARMY);
    if naval(kind) == land {
        return Err(checks::wrong_unit(key(kind), land));
    }
    let (leaves, text) = game_string::written(m, |t| MAY_LEAVE.call(m, [leader, t]))?;
    let mut reasons = super::reasons(&text);
    let (takes, text) = game_string::written(m, |t| MAY_TAKE.call(m, [leader, unit, t]))?;
    reasons.extend(super::reasons(&text));
    if leaves as u8 == 0 || takes as u8 == 0 {
        return Err(action::not_allowed("give this leader to this unit", &reasons));
    }
    action::post(commands, &assign_text(leader_id(id), army), "give this leader to this unit")?;
    Ok(())
}

/// Takes the leader from one of the player's armies or fleets, as the
/// panel's remove button does: the command with the empty leader ID.
pub fn remove(Session { m, ctx, commands, .. }: Session, args::Remove { army: ArmyId(army) }: args::Remove) -> GResult<()> {
    let unit = world::own_unit(m, &ctx, army)?;
    let leader = m.ptr(unit + UNIT_LEADER)?;
    if leader != 0 && m.ptr(leader + leader::UNIT)? == unit {
        let (leaves, text) = game_string::written(m, |t| MAY_LEAVE.call(m, [leader, t]))?;
        if leaves as u8 == 0 {
            return Err(action::not_allowed("take the leader from this unit", &super::reasons(&text)));
        }
    }
    action::post(commands, &assign_text(0, army), "take the leader from this unit")?;
    Ok(())
}

/// Dismisses one of the player's leaders that leads no unit.
pub fn dismiss(Session { m, ctx, commands, .. }: Session, args::Dismiss { leader: id }: args::Dismiss) -> GResult<()> {
    let leader = find(m, &ctx, id)?;
    if m.ptr(leader + leader::UNIT)? != 0 {
        return Err(checks::leads_a_unit(id));
    }
    action::post(commands, &dismiss_text(ctx.player.letters(), leader_id(id)), "dismiss this leader")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_name_the_leader_by_its_number_and_type() {
        assert_eq!(create_text(*b"CAS", LeaderKind::Admiral), r#"create_leader={ country="CAS" leader=1 }"#);
        assert_eq!(assign_text(leader_id(12), 7 << 32 | 54), "assign_leader={ leader={ id=12 type=49 } unit={ id=7 type=54 } }");
        assert_eq!(assign_text(0, 7 << 32 | 54), "assign_leader={ leader={ id=0 type=0 } unit={ id=7 type=54 } }");
        assert_eq!(dismiss_text(*b"CAS", leader_id(12)), r#"dismiss_leader={ country="CAS" leader={ id=12 type=49 } }"#);
    }

    #[test]
    fn a_value_is_given_in_either_form() {
        assert_eq!(given("You need at least §Y$COST$§W£mil .", "COST", "50"), "You need at least §Y50§W£mil .");
        assert_eq!(given("至少需要$COST|Y$£mil£", "COST", "50"), "至少需要50£mil£");
        assert_eq!(given("$COSTLY$ and $COST", "COST", "50"), "$COSTLY$ and $COST");
    }

    use crate::game::{
        buffers::{self, answers_no, write_inline, Image, Object},
        commands::record,
        Caller, Fail, Status, Tag,
    };

    /// A leader or a unit of the test's says no to the leader's leaving or
    /// the unit's taking it in a byte neither uses, read by the game's
    /// functions below.
    const SAYS_NO: usize = 8;

    extern "C" fn may_leave(leader: usize, text: usize) -> usize {
        refusing(leader, text, "- in battle")
    }

    extern "C" fn may_take(_: usize, unit: usize, text: usize) -> usize {
        refusing(unit, text, "- has a leader")
    }

    fn refusing(object: usize, text: usize, why: &str) -> usize {
        if unsafe { *((object + SAYS_NO) as *const u8) } == 0 {
            return 1;
        }
        write_inline(text, why);
        0
    }

    /// The army's ID, 7 of type 54, and the fleet's, 8.
    const ARMY: u64 = 7 << 32 | 54;
    const FLEET: u64 = 8 << 32 | 54;

    /// Castile, with the general 12 leading its army, the general 13 and the
    /// admiral 14 leading none, and a fleet; the general 12 may not leave
    /// its army when `stuck`, and the fleet takes no leader when `full`.
    struct Castile {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    fn castile(stuck: bool, full: bool) -> Castile {
        let image = Image::new();
        let m = image.memory();
        image
            .function(MAY_LEAVE.rva(), may_leave as *const ())
            .function(MAY_TAKE.rva(), may_take as *const ())
            .function(game_string::DESTROY_AT.rva(), answers_no as *const ());
        let cas = Tag(u64::from_le_bytes(*b"CAS\0\0\0\0\0"));
        let army = buffers::unit(m, ARMY, cas, true, UNIT_LEADER + 8);
        let fleet = buffers::unit(m, FLEET, cas, false, UNIT_LEADER + 8).u8(SAYS_NO, full.into());
        let leader = |number: u32, kind: LeaderKind, unit: usize| {
            Object::new(leader::UNIT + 8).u32(leader::ID, leader::TYPE).u32(leader::NUMBER, number).i32(leader::KIND, index(kind) as i32).u64(leader::UNIT, unit as u64)
        };
        let leading = leader(12, LeaderKind::General, army.at()).u8(SAYS_NO, stuck.into());
        let army = army.u64(UNIT_LEADER, leading.at() as u64);
        let leaders = [leading, leader(13, LeaderKind::General, 0), leader(14, LeaderKind::Admiral, 0)];
        let mut next = 0u64;
        let mut nodes = Vec::new();
        for l in leaders.iter().rev() {
            let node = Object::new(NODE_NEXT + 8).u64(0, l.at() as u64).u64(NODE_NEXT, next);
            next = node.at() as u64;
            nodes.push(node);
        }
        buffers::units(&image, &[&army, &fleet]);
        let country = Object::country(cas, LEADERS_COUNT + 4).u64(LEADERS, next).i32(LEADERS_COUNT, 3);
        let countries = Object::countries(&[&country]);
        let session = buffers::session(m, 0, cas, country.at(), countries.at(), Caller::Remote);
        let mut objects = vec![army, fleet, country, countries];
        objects.extend(leaders);
        objects.extend(nodes);
        Castile { _image: image, _objects: objects, session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the leader was moved") };
        (status, message)
    }

    #[test]
    fn a_leader_is_given_taken_and_dismissed_by_the_panels_commands() {
        let castile = castile(false, false);
        assign(castile.session, args::Assign { leader: 14, army: ArmyId(FLEET) }).unwrap();
        remove(castile.session, args::Remove { army: ArmyId(ARMY) }).unwrap();
        dismiss(castile.session, args::Dismiss { leader: 13 }).unwrap();
        assert_eq!(
            record::posted(),
            [
                "assign_leader={ leader={ id=14 type=49 } unit={ id=8 type=54 } }",
                "assign_leader={ leader={ id=0 type=0 } unit={ id=7 type=54 } }",
                r#"dismiss_leader={ country="CAS" leader={ id=13 type=49 } }"#
            ]
        );
    }

    #[test]
    fn a_leader_the_panels_would_not_move_is_refused_with_why() {
        let castile = castile(true, true);
        let rejected = |why: &str| (Status::Rejected, why.to_string());
        assert_eq!(refused(assign(castile.session, args::Assign { leader: 13, army: ArmyId(FLEET) })), rejected("a leader of the kind general leads an army, not a fleet"));
        assert_eq!(refused(assign(castile.session, args::Assign { leader: 14, army: ArmyId(ARMY) })), rejected("a leader of the kind admiral leads a fleet, not an army"));
        assert_eq!(refused(assign(castile.session, args::Assign { leader: 14, army: ArmyId(FLEET) })), rejected("has a leader"));
        assert_eq!(refused(remove(castile.session, args::Remove { army: ArmyId(ARMY) })), rejected("in battle"));
        assert_eq!(refused(dismiss(castile.session, args::Dismiss { leader: 12 })), rejected("leader 12 leads an army or a fleet; take it from there first (leader.remove)"));
        assert_eq!(refused(dismiss(castile.session, args::Dismiss { leader: 15 })), (Status::NotFound, "the player has no leader 15".into()));
        assert_eq!(refused(remove(castile.session, args::Remove { army: ArmyId(9 << 32 | 54) })).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }

    #[test]
    fn kinds_by_their_words() {
        assert_eq!(kind("explorer").unwrap(), LeaderKind::Explorer);
        assert!(kind("captain").is_err());
        assert!(naval(LeaderKind::Explorer) && !naval(LeaderKind::Conquistador));
    }
}
