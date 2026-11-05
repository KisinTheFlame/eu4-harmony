//! Government: the player's government reforms and rank, adopting reforms
//! and raising the rank, through the game's own commands
//! (docs/research/government.md, "Reforms" and "Rank").
use super::{conditions, content, context, game, thousandths, Caller, GResult, Tag};
use crate::{memory::Memory, points::Point};
use eu4_harmony_checks::government as checks;
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
const CAN_SELECT: usize = 0x40a9d0;
const CAN_CHANGE: usize = 0x40ade0;
/// `(government, out, tier)`: the reform progress enacting a reform in that
/// tier costs, in thousandths; the define of what changing one costs.
const SELECT_COST: usize = 0x79ead0;
const CHANGE_COST: usize = 0x233e4cc;
/// `(country, reasons) -> bool`: whether the country may raise its rank,
/// writing why not into the `std::string` reasons.
const CAN_RAISE: usize = 0x3167b0;
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
    content::pointers(m, kind + TIERS)?.into_iter().map(|tier| Ok((content::key(m, tier + TIER_KEY)?, content::pointers(m, tier + TIER_REFORMS)?))).collect()
}

/// Whether the player can adopt `reform` in tier `tier`, which they have
/// `reached` or not, why not, and its cost.
fn choice(m: &Memory, country: usize, player: Tag, government: usize, tier: usize, reached: bool, reform: usize) -> GResult<Choice> {
    let key = reform + REFORM_KEY;
    let (can, cost) = if reached {
        (m.call(m.at(CAN_CHANGE), &[country, key, 0])? & 0xff != 0, m.i32(m.at(CHANGE_COST))?)
    } else {
        let mut cost = 0i32;
        m.call(m.at(SELECT_COST), &[government, &mut cost as *mut i32 as usize, tier])?;
        (m.call(m.at(CAN_SELECT), &[country, key])? & 0xff != 0, cost)
    };
    let cost = vec![Amount::new(Resource::ReformProgress, thousandths(cost))];
    if can {
        return Ok(Choice::can(cost));
    }
    let mut reasons = conditions::failing(m, reform + POTENTIAL, player, 0)?;
    reasons.extend(conditions::failing(m, reform + TRIGGER, player, 0)?);
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
pub fn options(_: Caller) -> GResult<Government> {
    let m = game()?;
    let ctx = context(m)?;
    let government = government(m, ctx.country)?;
    let (can, reasons) = content::checked(m, |reasons| Ok(m.call(m.at(CAN_RAISE), &[ctx.country, reasons])?))?;
    Ok(Government {
        government: content::key(m, m.ptr(government + TYPE)? + TYPE_KEY)?,
        rank: m.i32(ctx.country + RANK)?.max(0) as u32,
        reform_progress: thousandths(m.i32(ctx.country + REFORM_PROGRESS)?),
        reforms_adopted: adopted(m, government)?.into_iter().map(|r| content::key(m, r + REFORM_KEY)).collect::<GResult<_>>()?,
        raise_rank: if can { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) },
    })
}

/// The government view's reforms, tier by tier.
pub fn reforms(_: Caller) -> GResult<Vec<ReformTier>> {
    let m = game()?;
    let ctx = context(m)?;
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
            let name = content::key(m, reform + REFORM_KEY)?;
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
pub fn adopt_reform(_: Caller, key: String) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let government = government(m, ctx.country)?;
    let adopted = adopted(m, government)?;
    for (tier, (_, reforms)) in tiers(m, government)?.into_iter().enumerate() {
        let Some(&reform) = reforms.iter().find(|&&r| content::key(m, r + REFORM_KEY).is_ok_and(|k| k == key)) else { continue };
        if adopted.contains(&reform) {
            return Err(checks::not_now(format!("the player has adopted the reform {key} already")).into());
        }
        let reached = reforms.iter().any(|r| adopted.contains(r));
        let choice = choice(m, ctx.country, ctx.player, government, tier, reached, reform)?;
        if !choice.can {
            return content::refused(&choice.reasons, "adopting this reform");
        }
        let text = if reached { written_change(ctx.player, &key, tier) } else { written_select(ctx.player, &key) };
        return content::post(&text, "adopting this reform");
    }
    Err(checks::not_found("government reform", &key).into())
}

/// Raises the player's government rank by one.
pub fn raise_rank(_: Caller) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let (can, reasons) = content::checked(m, |reasons| Ok(m.call(m.at(CAN_RAISE), &[ctx.country, reasons])?))?;
    if !can {
        return content::refused(&reasons, "raising the government rank");
    }
    content::post(&written_rank(ctx.player, m.i32(ctx.country + RANK)? + 1), "raising the government rank")
}

#[cfg(test)]
mod tests {
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
}
