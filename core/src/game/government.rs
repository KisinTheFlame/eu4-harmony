//! Government: the player's government reforms and rank, adopting reforms
//! and raising the rank, through the game's own commands
//! (docs/research/government.md, "Reforms" and "Rank").
use super::content::checks;
use super::{action, conditions, content, thousandths, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::{
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::government::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    government::{Government, Reform, ReformTier},
};

/// The country's government, its rank and its reform progress.
const GOVERNMENT: usize = 0x1420;
const RANK: usize = 0x2860;
const REFORM_PROGRESS: usize = 0x9a68;
/// The government: its type, and its reform stack, a `std::deque` of reform
/// pointers, two to a block: the block map, its size, the first element's
/// place and the count.
const TYPE: usize = 0x10;
const STACK: usize = 0x38;
const STACK_MAP: usize = 0x20;
const STACK_MAP_SIZE: usize = 0x28;
const STACK_FIRST: usize = 0x30;
const STACK_COUNT: usize = 0x38;
/// The government type: its key, and its tiers, a game array of tiers, each
/// with its key and its reforms, a game array of reforms.
const TYPE_KEY: usize = 0x40;
const TIERS: usize = 0x60;
const TIER_KEY: usize = 0x10;
const TIER_REFORMS: usize = 0x30;
/// A reform: its key, its conditions for being offered (`potential`) and
/// for being adopted (`trigger`).
const REFORM_KEY: usize = 0x10;
const POTENTIAL: usize = 0x1b0;
const TRIGGER: usize = 0x228;
/// `(country, key) -> bool`, whether the country may enact the reform with
/// that key in its next tier; `(country, key, free) -> bool`, whether it may
/// change a tier's reform to it, paying reform progress unless `free`.
const CAN_SELECT: GameFunction<2> = GameFunction::at(0x40a9d0);
const CAN_CHANGE: GameFunction<3> = GameFunction::at(0x40ade0);
/// `(government, out, tier)`: the reform progress enacting a reform in that
/// tier costs, in thousandths; the define of what changing one costs.
const SELECT_COST: GameFunction<3> = GameFunction::at(0x79ead0);
const CHANGE_COST: usize = 0x233e4cc;
/// `(country, reasons) -> bool`: whether the country may raise its rank,
/// writing why not into the `std::string` reasons.
const CAN_RAISE: GameFunction<2> = GameFunction::at(0x3167b0);
const MOST: usize = 1_000;

pub fn points() -> Vec<Point> {
    vec![
        // The select check: the reform's `potential` and `trigger`, then the
        // country's government and its type.
        Point::keep(0x40aab9usize, "48 8b 9f b8 01 00 00", "reform potential"),
        Point::keep(0x40aaefusize, "48 8b 9f 30 02 00 00 48 63 87 3c 02 00 00", "reform trigger"),
        Point::keep(0x40ab2fusize, "49 8b b6 20 14 00 00 48 85 f6 74 06 48 8b 76 10", "country government").uses(&[GOVERNMENT, TYPE]),
        Point::keep(0x40ab97usize, "44 8b c0 48 8d 55 60 49 8b 8e 20 14 00 00 e8", "reform select cost"),
        Point::keep(0x40ac0ausize, "41 39 86 68 9a 00 00", "reform progress").uses(&[REFORM_PROGRESS]),
        Point::keep(0x40af54usize, "8b 05 72 35 f3 01 41 39 86 68 9a 00 00", "reform change cost").uses(&[CHANGE_COST]),
        Point::keep(CAN_SELECT, "48 89 5c 24 08 48 89 74 24 10 55 57 41 54 41 56 41 57", "reform select check"),
        Point::keep(CAN_CHANGE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55 41 56 41 57", "reform change check"),
        // A reform's tier: the type's tiers and each tier's reforms.
        Point::keep(0x79804cusize, "48 63 41 74 45 33 ed 48 8b fa 85 c0 7e 7f 4c 8b 61 68", "government type tiers"),
        Point::keep(0x798070usize, "49 8b 04 24 48 8b 58 38 48 63 40 44", "tier reforms"),
        // The reform stack's writer.
        Point::keep(0x7aba80usize, "4c 8b 61 30 48 8b 41 38", "reform stack").uses(&[STACK_FIRST, STACK_COUNT]),
        Point::keep(0x79ddc6usize, "48 83 c3 40 e8 b1 b4 f6", "government type key"),
        Point::keep(SELECT_COST, "40 53 48 83 ec 20 41 8b c0 48 8b da 45 85 c0 79", "reform tier cost"),
        Point::keep(CAN_RAISE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 20 55 41 54 41 55 41 56 41 57", "rank check"),
        Point::keep(0x316824usize, "45 8b be 60 28 00 00 41 ff c7", "government rank").uses(&[RANK]),
    ]
}

/// The player's government object.
fn government(m: &Memory, country: usize) -> GResult<usize> {
    match m.ptr(country + GOVERNMENT)? {
        0 => Err("the player has no government".to_string().into()),
        government => Ok(government),
    }
}

/// The reforms the player has, in the order the game keeps them.
fn adopted(m: &Memory, government: usize) -> GResult<Vec<usize>> {
    let stack = government + STACK;
    let (map, size, first, count) =
        (m.ptr(stack + STACK_MAP)?, m.u64(stack + STACK_MAP_SIZE)? as usize, m.u64(stack + STACK_FIRST)? as usize, m.u64(stack + STACK_COUNT)? as usize);
    if count > MOST || !size.is_power_of_two() {
        return Err("an unexpected reform stack".to_string().into());
    }
    (0..count)
        .map(|i| {
            let place = first + i;
            Ok(m.ptr(m.ptr(map + ((place >> 1) & (size - 1)) * 8)? + (place & 1) * 8)?)
        })
        .collect()
}

/// The government type's tiers, each with its key and reforms.
fn tiers(m: &Memory, government: usize) -> GResult<Vec<(String, Vec<usize>)>> {
    let kind = m.ptr(government + TYPE)?;
    containers::array(m, kind + TIERS, containers::MOST)?
        .into_iter()
        .map(|tier| Ok((game_string::key(m, tier + TIER_KEY)?, containers::array(m, tier + TIER_REFORMS, containers::MOST)?)))
        .collect()
}

/// Whether the player can adopt `reform` in tier `tier`, which they have
/// `reached` or not, why not, and its cost.
fn choice(m: &Memory, country: usize, player: Tag, government: usize, tier: usize, reached: bool, reform: usize) -> GResult<Choice> {
    let key = reform + REFORM_KEY;
    let (can, cost) = if reached {
        (CAN_CHANGE.call(m, [country, key, 0])? & 0xff != 0, m.i32(m.at(CHANGE_COST))?)
    } else {
        let mut cost = 0i32;
        SELECT_COST.call(m, [government, &mut cost as *mut i32 as usize, tier])?;
        (CAN_SELECT.call(m, [country, key])? & 0xff != 0, cost)
    };
    let cost = vec![Amount::new(Resource::ReformProgress, thousandths(cost))];
    if can {
        return Ok(Choice::can(cost));
    }
    let mut reasons = conditions::failing(m, reform + POTENTIAL, player, 0)?;
    // A line the potential and the trigger share is written once.
    for line in conditions::failing(m, reform + TRIGGER, player, 0)? {
        if !reasons.contains(&line) {
            reasons.push(line);
        }
    }
    let progress = m.i32(country + REFORM_PROGRESS)?;
    if progress < cost_thousandths(&cost) {
        reasons.push(format!("the reform progress is {}, and this costs {}", thousandths(progress), cost[0].amount));
    }
    Ok(Choice::cannot(reasons, cost))
}

fn cost_thousandths(cost: &[Amount]) -> i32 {
    cost.first().map_or(0, |c| (c.amount * 1000.0).round() as i32)
}

/// The player's government: its type, rank, reform progress, and whether
/// they can raise the rank.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Government> {
    let government = government(m, ctx.country)?;
    let (can, reasons) = content::checked(m, |reasons| Ok(CAN_RAISE.call(m, [ctx.country, reasons])?))?;
    Ok(Government {
        government: game_string::key(m, m.ptr(government + TYPE)? + TYPE_KEY)?,
        rank: m.i32(ctx.country + RANK)?.max(0) as u32,
        reform_progress: thousandths(m.i32(ctx.country + REFORM_PROGRESS)?),
        reforms_adopted: adopted(m, government)?.into_iter().map(|r| Ok(game_string::key(m, r + REFORM_KEY)?)).collect::<GResult<_>>()?,
        raise_rank: if can { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) },
    })
}

/// The government view's reforms, tier by tier.
pub fn reforms(Session { m, ctx, .. }: Session) -> GResult<Vec<ReformTier>> {
    let government = government(m, ctx.country)?;
    let adopted = adopted(m, government)?;
    let mut listed = Vec::new();
    for (tier, (key, reforms)) in tiers(m, government)?.into_iter().enumerate() {
        let reached = reforms.iter().any(|r| adopted.contains(r));
        let mut offered = Vec::new();
        for reform in reforms {
            let has = adopted.contains(&reform);
            if !has && !conditions::holds(m, reform + POTENTIAL, ctx.player, 0)? {
                continue;
            }
            let name = game_string::key(m, reform + REFORM_KEY)?;
            offered.push(Reform {
                name: content::name(m, &name)?,
                reform: name,
                adopted: has,
                choice: if has { Choice::cannot(Vec::new(), Vec::new()) } else { choice(m, ctx.country, ctx.player, government, tier, reached, reform)? },
            });
        }
        listed.push(ReformTier { tier: key, reached, reforms: offered });
    }
    Ok(listed)
}

/// Enacting a reform in the next tier.
fn written_select(country: Tag, reform: &str) -> String {
    format!("select_government_reform={{ members={{ dummy=\"{}\" dummy=\"{reform}\" }} }}", content::letters(country))
}

/// Changing a reached tier's reform, paying its progress: the tier's index,
/// whose reform it replaces.
fn written_change(country: Tag, reform: &str, tier: usize) -> String {
    format!("change_to_government_reform={{ members={{ dummy=\"{}\" dummy=\"{reform}\" dummy={tier} dummy=no }} }}", content::letters(country))
}

/// Raising the rank by one, with the prestige the button gives.
fn written_rank(country: Tag, rank: i32) -> String {
    format!("government_rank_command={{ country=\"{}\" value={rank} prestige=yes }}", content::letters(country))
}

/// Adopts a reform for the player, as its button does.
pub fn adopt_reform(Session { m, ctx, commands, .. }: Session, args::AdoptReform { reform: key }: args::AdoptReform) -> GResult<()> {
    let government = government(m, ctx.country)?;
    let adopted = adopted(m, government)?;
    for (tier, (_, reforms)) in tiers(m, government)?.into_iter().enumerate() {
        let Some(&reform) = reforms.iter().find(|&&r| game_string::key(m, r + REFORM_KEY).is_ok_and(|k| k == key)) else { continue };
        if adopted.contains(&reform) {
            return Err(checks::not_now(format!("the player has adopted the reform {key} already")));
        }
        let reached = reforms.iter().any(|r| adopted.contains(r));
        let choice = choice(m, ctx.country, ctx.player, government, tier, reached, reform)?;
        if !choice.can {
            return Err(action::not_allowed("adopt this reform", &choice.reasons));
        }
        let text = if reached { written_change(ctx.player, &key, tier) } else { written_select(ctx.player, &key) };
        return action::post(commands, &text, "adopt this reform");
    }
    Err(checks::not_found("government reform", &key))
}

/// Raises the player's government rank by one.
pub fn raise_rank(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    let (can, reasons) = content::checked(m, |reasons| Ok(CAN_RAISE.call(m, [ctx.country, reasons])?))?;
    if !can {
        return Err(action::not_allowed("raise the government rank", &reasons));
    }
    action::post(commands, &written_rank(ctx.player, m.i32(ctx.country + RANK)? + 1), "raise the government rank")
}

crate::catalogue::bind! {
    eu4_harmony_remote::government: GovernmentRequest => GovernmentAnswer;
    Options => options;
    Reforms => reforms;
    AdoptReform => adopt_reform;
    RaiseRank => raise_rank;
}

#[cfg(test)]
mod tests {
    use crate::game::As;
    use super::*;

    fn fra() -> Tag {
        Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"))
    }

    #[test]
    fn the_reform_commands_carry_their_members_in_order() {
        assert_eq!(written_select(fra(), "centralize_reform"), r#"select_government_reform={ members={ dummy="FRA" dummy="centralize_reform" } }"#);
        assert_eq!(
            written_change(fra(), "centralize_reform", 2),
            r#"change_to_government_reform={ members={ dummy="FRA" dummy="centralize_reform" dummy=2 dummy=no } }"#
        );
    }

    #[test]
    fn raising_the_rank_names_the_rank_reached() {
        assert_eq!(written_rank(fra(), 2), r#"government_rank_command={ country="FRA" value=2 prestige=yes }"#);
    }

    use crate::game::{
        buffers::{self, write_reason, Image, Object},
        commands::record,
        Caller, Fail, Status,
    };

    /// Whether the country may enact or change to the reform whose key is at
    /// `key`: those whose key starts with `ok`.
    extern "C" fn may(_: usize, key: usize) -> usize {
        game_string::key(&Memory { base: 0 }, key).unwrap().starts_with("ok").into()
    }

    /// What enacting a reform in the next tier costs: 50 progress.
    extern "C" fn select_cost(_: usize, out: *mut i32) -> usize {
        unsafe { *out = 50_000 };
        0
    }

    /// Whether the country may raise its rank: below rank 3.
    extern "C" fn may_raise(country: usize, reasons: usize) -> usize {
        if unsafe { *((country + RANK) as *const i32) } < 3 {
            return 1;
        }
        unsafe { write_reason(reasons, b"- highest rank") };
        0
    }

    /// A reform with `key`, its `potential` and `trigger` holding or not.
    fn reform(key: &str, potential: bool, trigger: bool) -> Object {
        Object::new(TRIGGER + 8).string(REFORM_KEY, key).u8(POTENTIAL, potential.into()).u8(TRIGGER, trigger.into())
    }

    /// France at `rank` with 30 reform progress, a monarchy of two tiers:
    /// `power`, where it has adopted `noble_rule` and may change to
    /// `ok_crown` but is not offered `secret_one`; and `law`, not reached,
    /// of `ok_courts`, and `merchant_rule`, which it may not enact.
    struct France {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    fn france(rank: i32) -> France {
        let image = Image::new();
        image.conditions().localisation();
        image
            .function(CAN_SELECT.rva(), may as *const ())
            .function(CAN_CHANGE.rva(), may as *const ())
            .function(SELECT_COST.rva(), select_cost as *const ())
            .function(CAN_RAISE.rva(), may_raise as *const ())
            .i32(CHANGE_COST, 100_000);
        let [adopted, crown, secret, courts, merchants] = [
            reform("noble_rule", true, true),
            reform("ok_crown", true, true),
            reform("secret_one", false, true),
            reform("ok_courts", true, true),
            reform("merchant_rule", true, false),
        ];
        let power =
            Object::new(TIER_REFORMS + 0x18).string(TIER_KEY, "power").game_array(TIER_REFORMS, &[adopted.at() as u64, crown.at() as u64, secret.at() as u64]);
        let law = Object::new(TIER_REFORMS + 0x18).string(TIER_KEY, "law").game_array(TIER_REFORMS, &[courts.at() as u64, merchants.at() as u64]);
        let kind = Object::new(TIERS + 0x18).string(TYPE_KEY, "monarchy").game_array(TIERS, &[power.at() as u64, law.at() as u64]);
        let block = Object::new(16).u64(0, adopted.at() as u64);
        let map = Object::new(8).u64(0, block.at() as u64);
        let stack = STACK;
        let government = Object::new(STACK + STACK_COUNT + 8)
            .u64(TYPE, kind.at() as u64)
            .u64(stack + STACK_MAP, map.at() as u64)
            .u64(stack + STACK_MAP_SIZE, 1)
            .u64(stack + STACK_FIRST, 0)
            .u64(stack + STACK_COUNT, 1);
        let country = Object::country(fra(), REFORM_PROGRESS + 4).u64(GOVERNMENT, government.at() as u64).i32(RANK, rank).i32(REFORM_PROGRESS, 30_000);
        let countries = Object::countries(&[&country]);
        let session = buffers::session(image.memory(), 0, fra(), country.at(), countries.at(), Caller::Remote(As::Player));
        France {
            _image: image,
            _objects: vec![adopted, crown, secret, courts, merchants, power, law, kind, block, map, government, country, countries],
            session,
        }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn the_government_says_its_type_rank_progress_reforms_and_rank_choice() {
        let government = options(france(2).session).unwrap();
        assert_eq!((government.government.as_str(), government.rank, government.reform_progress), ("monarchy", 2, 30.0));
        assert_eq!(government.reforms_adopted, ["noble_rule"]);
        assert!(government.raise_rank.can);
        let highest = options(france(3).session).unwrap();
        assert_eq!(highest.raise_rank.reasons, ["highest rank"]);
    }

    #[test]
    fn the_view_offers_each_tier_s_potential_reforms_with_their_costs_and_reasons() {
        let tiers = reforms(france(2).session).unwrap();
        let summary: Vec<_> = tiers
            .iter()
            .map(|t| (t.tier.as_str(), t.reached, t.reforms.iter().map(|r| (r.reform.as_str(), r.adopted, r.choice.can)).collect::<Vec<_>>()))
            .collect();
        assert_eq!(
            summary,
            [
                ("power", true, vec![("noble_rule", true, false), ("ok_crown", false, true)]),
                ("law", false, vec![("ok_courts", false, true), ("merchant_rule", false, false)]),
            ]
        );
        assert_eq!(tiers[0].reforms[1].choice.cost, [Amount::new(Resource::ReformProgress, 100.0)]);
        assert_eq!(tiers[1].reforms[0].choice.cost, [Amount::new(Resource::ReformProgress, 50.0)]);
        assert_eq!(tiers[1].reforms[1].choice.reasons, ["unmet", "the reform progress is 30, and this costs 50"]);
        assert_eq!(tiers[0].reforms[1].name, "ok_crown");
    }

    #[test]
    fn a_reform_is_enacted_in_the_next_tier_or_changed_to_in_a_reached_one() {
        let france = france(2);
        adopt_reform(france.session, args::AdoptReform { reform: "ok_crown".into() }).unwrap();
        adopt_reform(france.session, args::AdoptReform { reform: "ok_courts".into() }).unwrap();
        raise_rank(france.session).unwrap();
        assert_eq!(
            record::posted(),
            [
                r#"change_to_government_reform={ members={ dummy="FRA" dummy="ok_crown" dummy=0 dummy=no } }"#,
                r#"select_government_reform={ members={ dummy="FRA" dummy="ok_courts" } }"#,
                r#"government_rank_command={ country="FRA" value=3 prestige=yes }"#,
            ]
        );
    }

    #[test]
    fn a_reform_adopted_unknown_or_not_allowed_and_a_rank_too_high_are_refused() {
        let france = france(3);
        assert_eq!(
            refused(adopt_reform(france.session, args::AdoptReform { reform: "noble_rule".into() })),
            (Status::Rejected, "the player has adopted the reform noble_rule already".into())
        );
        assert_eq!(
            refused(adopt_reform(france.session, args::AdoptReform { reform: "merchant_rule".into() })),
            (Status::Rejected, "unmet\nthe reform progress is 30, and this costs 50".into())
        );
        assert_eq!(refused(adopt_reform(france.session, args::AdoptReform { reform: "no_such".into() })).0, Status::NotFound);
        assert_eq!(refused(raise_rank(france.session)), (Status::Rejected, "highest rank".into()));
        assert!(record::posted().is_empty());
    }
}
