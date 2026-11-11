//! The uses of monarch power the province view, the army panel, the siege
//! view and the stability view's rebels offer besides developing, coring
//! and the barrages: moving the capital and the trade port, a Siberian
//! frontier, tribal land and razing a province; scorched earth, seizing and
//! burning a colony and attacking natives; sorties; harsh treatment
//! (docs/research/power-spending.md, "The province view's and the army
//! panel's spendings"). Each asks the game's own check, which writes the
//! button's tooltip reasons and checks its cost, and posts the command the
//! button posts.
use super::commands::{write, Block};
use super::{action, army, power, with_state, Ctx, GResult, Session, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{country, rebel_faction, unit},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, PowerKind, Resource};

/// `(country, province, text or null, _)`: whether the country may move its
/// capital or its trade port there, with the tooltip's reasons, the points
/// among them. The capital's fourth argument lets the capital itself
/// through; the buttons pass 0, as the framework.
pub(super) const MAY_MOVE_CAPITAL: GameFunction<4> = GameFunction::at(0x3781c0);
pub(super) const MAY_MOVE_TRADE_PORT: GameFunction<4> = GameFunction::at(0x378c20);
/// `(country, province, _, text or null)`: whether the country may start a
/// Siberian frontier there; its text comes last, and the buttons pass 0
/// before it.
pub(super) const MAY_SIBERIAN_FRONTIER: GameFunction<4> = GameFunction::at(0x404970);
/// `(tribe, province, text or null)`: whether a native country's tribe
/// ([`country::NATIVE`]) may make the province its tribal land.
const MAY_ADD_TRIBAL_LAND: GameFunction<3> = GameFunction::at(0x9163f0);
/// `(province, tag, text or null)`: whether the country may raze the
/// province, the raze command's check.
pub(super) const MAY_RAZE: GameFunction<3> = GameFunction::at(0x9fcf90);
/// `(country, province, text or null)`: whether the country may sortie from
/// its besieged fort there.
pub(super) const MAY_SORTIE: GameFunction<3> = GameFunction::at(0x38c3b0);
/// `(country, rebel faction, text or null, _)`: whether the country may
/// treat the faction harshly.
const MAY_TREAT_HARSHLY: GameFunction<4> = GameFunction::at(0x38cbb0);
/// `(country, unit, text or null)`: whether the country's army may do each
/// where it is.
const MAY_SEIZE_COLONY: GameFunction<3> = GameFunction::at(0x3435d0);
const MAY_BURN_COLONY: GameFunction<3> = GameFunction::at(0x343ce0);
const MAY_ATTACK_NATIVES: GameFunction<3> = GameFunction::at(0x3442f0);
const MAY_SCORCH_EARTH: GameFunction<3> = GameFunction::at(0x344a40);

/// The kinds of spending, as the game names them (`0x94e1d0`).
const SEIZE_COLONY: i32 = 10;
const BURN_COLONY: i32 = 11;
const ATTACK_NATIVES: i32 = 12;
const SCORCH_EARTH: i32 = 13;
const MOVE_CAPITAL: i32 = 16;
const HARSH_TREATMENT: i32 = 21;
const MOVE_TRADE_PORT: i32 = 28;
pub const SIEGE_SORTIES: i32 = 30;
const SIBERIAN_FRONTIER: i32 = 38;
const ADD_TRIBAL_LAND: i32 = 41;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_MOVE_CAPITAL, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 44 88 4c 24 20 55 41 54 41 55", "may move the capital"),
        Point::keep(MAY_MOVE_TRADE_PORT, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 56 41 57 48 8d 6c 24 b9", "may move the trade port"),
        Point::keep(
            MAY_SIBERIAN_FRONTIER,
            "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55 41 56 41 57 48 8d ac 24 40 ff ff ff",
            "may start a Siberian frontier",
        ),
        Point::keep(MAY_ADD_TRIBAL_LAND, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55 48 8d 6c 24 e0 48 81 ec 20 01 00 00", "may add tribal land"),
        Point::keep(MAY_RAZE, "48 89 5c 24 18 55 56 57 41 56 41 57 48 8d 6c 24 c9 48 81 ec 00 01 00 00", "may raze"),
        Point::keep(MAY_SORTIE, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55 41 56 41 57 48 8d ac 24 40 ff ff ff", "may sortie"),
        Point::keep(MAY_TREAT_HARSHLY, "48 89 5c 24 08 48 89 74 24 10 44 88 4c 24 20 55 57 41 56 48 8d 6c 24 a0", "may treat rebels harshly"),
        Point::keep(MAY_SEIZE_COLONY, "48 89 5c 24 08 48 89 74 24 18 55 57 41 55 41 56 41 57 48 8d ac 24 20 ff ff ff", "may seize a colony"),
        Point::keep(MAY_BURN_COLONY, "48 89 5c 24 08 48 89 74 24 18 55 57 41 55 41 56 41 57 48 8d ac 24 60 ff ff ff", "may burn a colony"),
        Point::keep(
            MAY_ATTACK_NATIVES,
            "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55 41 56 41 57 48 8d ac 24 20 ff ff ff",
            "may attack natives",
        ),
        Point::keep(MAY_SCORCH_EARTH, "48 89 5c 24 18 48 89 54 24 10 48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57", "may scorch the earth"),
        // Each button's spending, its kind and kind of power: the checks'
        // own, and the army panel's and the rebels' buttons'.
        Point::keep(0x3789bfusize, "48 c7 45 c8 10 00 00 00", "move capital's spending"),
        Point::keep(0x378e36usize, "c7 45 af 1c 00 00 00 c7 45 b3 01 00 00 00", "move trade port's spending"),
        Point::keep(0xe38034usize, "c7 43 68 26 00 00 00 c7 43 6c 01 00 00 00", "Siberian frontier's spending"),
        Point::keep(0x916855usize, "48 c7 45 80 29 00 00 00", "tribal land's spending"),
        Point::keep(0x38c750usize, "c7 45 a0 1e 00 00 00 c7 45 a4 02 00 00 00", "sorties' spending"),
        Point::keep(0xf9e560usize, "c7 43 68 15 00 00 00 48 c7 43 6c 02 00 00 00", "harsh treatment's spending"),
        Point::keep(0x1484f55usize, "c7 40 68 0a 00 00 00 c7 40 6c 02 00 00 00 89 58 70 89 78 74", "seize colony's spending"),
        Point::keep(0x1484845usize, "c7 40 68 0b 00 00 00 c7 40 6c 02 00 00 00 89 58 70 89 78 74", "burn colony's spending"),
        Point::keep(0x1484325usize, "c7 40 68 0c 00 00 00 c7 40 6c 02 00 00 00 89 58 70 89 78 74", "attack natives' spending"),
        Point::keep(0x1483575usize, "c7 40 68 0d 00 00 00 c7 40 6c 02 00 00 00 89 58 70 89 78 74", "scorch earth's spending"),
        // The spend_power check asks a native country's tribe.
        Point::keep(0x94f52dusize, "48 8b 8f 78 9a 00 00", "native tribe").uses(&[country::NATIVE]),
        // The rebel faction's writer writes its ID.
        Point::keep(0xa94999usize, "4c 8d 41 10", "rebel faction ID").uses(&[rebel_faction::ID]),
        // The raze command: its writer writes the country's index and the
        // province, its check is the raze check.
        Point::keep(0x2bb09dusize, "0f bf 5f 54 ba 9d 28 00 00", "raze command's country"),
        Point::keep(0x2bb0e7usize, "8b 5f 58 ba 43 28 00 00", "raze command's province"),
        Point::method(0x1c53870, 0x40, 0x2bb2f0, "raze command check"),
    ]
}

/// A choice from one of the game's checks, which writes its reasons into a
/// string.
fn answered(m: &Memory, cost: Vec<Amount>, ask: impl FnOnce(usize) -> crate::Result<usize>) -> GResult<Choice> {
    let (yes, text) = game_string::written(m, ask)?;
    Ok(if yes as u8 != 0 { Choice::can(cost) } else { Choice::cannot(super::reasons(&text), cost) })
}

fn resource(power: PowerKind) -> Resource {
    match power {
        PowerKind::Adm => Resource::Adm,
        PowerKind::Dip => Resource::Dip,
        PowerKind::Mil => Resource::Mil,
    }
}

/// The spend_power command's written form, as the game's writer writes its
/// values: `data` only when it is not 0, `second` only when it is not −1,
/// then the kind and the power, then `extra`, a block under its key.
fn written(tag: Tag, kind: i32, power: PowerKind, data: i32, second: i32, extra: Option<(&str, Block)>) -> String {
    let mut spent = Block::new();
    if data != 0 {
        spent = spent.word("data", data);
    }
    if second != -1 {
        spent = spent.word("second", second);
    }
    spent = spent.word("category", kind).word("skill", power as i32);
    if let Some((key, block)) = extra {
        spent = spent.block(key, block);
    }
    write("spend_power_command", Block::new().tag("country", tag.letters()).block("power", spent))
}

// ---------- the province view ----------

/// One of the province view's buttons this module answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InProvince {
    MoveCapital,
    MoveTradePort,
    SiberianFrontier,
    TribalLand,
    Raze,
}

impl InProvince {
    pub const ALL: [InProvince; 5] = [Self::MoveCapital, Self::MoveTradePort, Self::SiberianFrontier, Self::TribalLand, Self::Raze];

    /// The capability's own name.
    pub fn name(self) -> &'static str {
        match self {
            Self::MoveCapital => "move_capital",
            Self::MoveTradePort => "move_trade_port",
            Self::SiberianFrontier => "establish_siberian_frontier",
            Self::TribalLand => "add_tribal_land",
            Self::Raze => "raze",
        }
    }

    fn doing(self) -> &'static str {
        match self {
            Self::MoveCapital => "move the capital there",
            Self::MoveTradePort => "move the trade port there",
            Self::SiberianFrontier => "start a Siberian frontier there",
            Self::TribalLand => "make it tribal land",
            Self::Raze => "raze it",
        }
    }

    /// The spending's kind and the power it is paid with; none for razing,
    /// which is a command of its own.
    fn spending(self) -> Option<(i32, PowerKind)> {
        match self {
            Self::MoveCapital => Some((MOVE_CAPITAL, PowerKind::Adm)),
            Self::MoveTradePort => Some((MOVE_TRADE_PORT, PowerKind::Dip)),
            Self::SiberianFrontier => Some((SIBERIAN_FRONTIER, PowerKind::Dip)),
            Self::TribalLand => Some((ADD_TRIBAL_LAND, PowerKind::Adm)),
            Self::Raze => None,
        }
    }
}

/// Whether the player can press `which` for the province at `province`
/// now, with the game's reasons and the cost.
pub fn in_province(m: &Memory, ctx: &Ctx, province: usize, which: InProvince) -> GResult<Choice> {
    let cost = match which.spending() {
        Some((kind, power)) => {
            let points = power::Spending::about_province(m, kind as usize, province, power as usize)?.cost(m, ctx.player)?;
            vec![Amount::new(resource(power), f64::from(points))]
        }
        None => Vec::new(),
    };
    let country = ctx.country;
    match which {
        InProvince::MoveCapital => answered(m, cost, |t| MAY_MOVE_CAPITAL.call(m, [country, province, t, 0])),
        InProvince::MoveTradePort => answered(m, cost, |t| MAY_MOVE_TRADE_PORT.call(m, [country, province, t, 0])),
        InProvince::SiberianFrontier => answered(m, cost, |t| MAY_SIBERIAN_FRONTIER.call(m, [country, province, 0, t])),
        // Only a native country has a tribe, and the button.
        InProvince::TribalLand => match m.ptr(country + country::NATIVE)? {
            0 => Ok(Choice::cannot(Vec::new(), cost)),
            tribe => answered(m, cost, |t| MAY_ADD_TRIBAL_LAND.call(m, [tribe, province, t])),
        },
        InProvince::Raze => answered(m, cost, |t| MAY_RAZE.call(m, [province, ctx.player.0 as usize, t])),
    }
}

/// The province's address, by its number.
fn province_at(m: &Memory, ctx: &Ctx, id: u32) -> GResult<usize> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    with_state(|s| s.geography.province_address(m, ctx, id))
}

/// Presses `which` for the province, as the button does once the player
/// confirms.
pub fn press_in_province(Session { m, ctx, commands, .. }: Session, id: u32, which: InProvince) -> GResult<()> {
    let province = province_at(m, &ctx, id)?;
    let choice = in_province(m, &ctx, province, which)?;
    action::carry_out(commands, &choice, which.doing(), || {
        Ok(match which.spending() {
            Some((kind, power)) => written(ctx.player, kind, power, id as i32, -1, None),
            None => raze_written(ctx.player, id),
        })
    })
}

/// The raze command: the country by its index in the game's countries, the
/// province by its number.
fn raze_written(tag: Tag, province: u32) -> String {
    write("raze_province", Block::new().word("country", tag.index()).word("province", province))
}

// ---------- the army panel ----------

/// One of the army panel's buttons this module answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByArmy {
    ScorchEarth,
    SeizeColony,
    BurnColony,
    AttackNatives,
}

impl ByArmy {
    pub const ALL: [ByArmy; 4] = [Self::ScorchEarth, Self::SeizeColony, Self::BurnColony, Self::AttackNatives];

    pub fn name(self) -> &'static str {
        match self {
            Self::ScorchEarth => "scorch_earth",
            Self::SeizeColony => "seize_colony",
            Self::BurnColony => "burn_colony",
            Self::AttackNatives => "attack_natives",
        }
    }

    fn doing(self) -> &'static str {
        match self {
            Self::ScorchEarth => "scorch the earth here",
            Self::SeizeColony => "seize this colony",
            Self::BurnColony => "burn this colony",
            Self::AttackNatives => "attack the natives here",
        }
    }

    fn kind(self) -> i32 {
        match self {
            Self::ScorchEarth => SCORCH_EARTH,
            Self::SeizeColony => SEIZE_COLONY,
            Self::BurnColony => BURN_COLONY,
            Self::AttackNatives => ATTACK_NATIVES,
        }
    }

    fn check(self) -> GameFunction<3> {
        match self {
            Self::ScorchEarth => MAY_SCORCH_EARTH,
            Self::SeizeColony => MAY_SEIZE_COLONY,
            Self::BurnColony => MAY_BURN_COLONY,
            Self::AttackNatives => MAY_ATTACK_NATIVES,
        }
    }
}

/// An army's spending carries the army's ID in its two values: its low
/// half first, its high half second.
fn halves(id: u64) -> (i32, i32) {
    (id as u32 as i32, (id >> 32) as u32 as i32)
}

/// Whether the player's army `unit`, of ID `id`, can do `which` where it is.
pub fn by_army(m: &Memory, ctx: &Ctx, unit: usize, which: ByArmy) -> GResult<Choice> {
    let (data, second) = halves(m.u64(unit + unit::ID)?);
    let points = power::Spending::new(m, which.kind(), PowerKind::Mil, data)?.with_second(second).cost(m, ctx.player)?;
    let cost = vec![Amount::new(Resource::Mil, f64::from(points))];
    answered(m, cost, |t| which.check().call(m, [ctx.country, unit, t]))
}

/// Has the player's land army `id` do `which`, as its button does.
pub fn press_by_army(Session { m, ctx, commands, .. }: Session, id: u64, which: ByArmy) -> GResult<()> {
    let unit = army::own_land(m, &ctx, id)?;
    let choice = by_army(m, &ctx, unit, which)?;
    let (data, second) = halves(id);
    action::carry_out(commands, &choice, which.doing(), || Ok(written(ctx.player, which.kind(), PowerKind::Mil, data, second, None)))
}

// ---------- the siege view ----------

/// Whether the player can sortie from the besieged fort in the province at
/// `province`, of number `id`.
pub fn sorties(m: &Memory, ctx: &Ctx, province: usize, id: u32) -> GResult<Choice> {
    let points = power::Spending::new(m, SIEGE_SORTIES, PowerKind::Mil, id as i32)?.cost(m, ctx.player)?;
    answered(m, vec![Amount::new(Resource::Mil, f64::from(points))], |t| MAY_SORTIE.call(m, [ctx.country, province, t]))
}

/// The sorties' command for the province.
pub fn sorties_written(tag: Tag, id: u32) -> String {
    written(tag, SIEGE_SORTIES, PowerKind::Mil, id as i32, -1, None)
}

// ---------- the stability view's rebels ----------

/// A rebel faction against the player: its object, ID, rebel type and name.
pub struct Faction {
    pub at: usize,
    pub id: u64,
    pub rebels: String,
    pub name: String,
}

impl Faction {
    /// Its number, the high half of its ID, which capabilities name it by.
    pub fn number(&self) -> u32 {
        (self.id >> 32) as u32
    }
}

/// The rebel factions against the player.
pub fn factions(m: &Memory, ctx: &Ctx) -> GResult<Vec<Faction>> {
    super::countries::rebel_factions(m, ctx.country)?
        .into_iter()
        .map(|at| {
            Ok(Faction {
                at,
                id: m.u64(at + rebel_faction::ID)?,
                rebels: super::countries::rebel_kind(m, at)?,
                name: game_string::shown(m, at + rebel_faction::NAME)?,
            })
        })
        .collect()
}

/// Whether the player can treat the faction harshly now.
pub fn harsh_treatment(m: &Memory, ctx: &Ctx, faction: &Faction) -> GResult<Choice> {
    let points = power::Spending::new(m, HARSH_TREATMENT, PowerKind::Mil, 0)?.about_id(faction.id).cost(m, ctx.player)?;
    answered(m, vec![Amount::new(Resource::Mil, f64::from(points))], |t| MAY_TREAT_HARSHLY.call(m, [ctx.country, faction.at, t, 0]))
}

/// The harsh treatment's command: the faction by its ID, `{ id type }`.
pub fn harsh_written(tag: Tag, faction: u64) -> String {
    let id = Block::new().word("id", (faction >> 32) as i32).word("type", faction as u32 as i32);
    written(tag, HARSH_TREATMENT, PowerKind::Mil, 0, -1, Some(("rebel_faction", id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRA: Tag = Tag(0x0100_0042_0041_5246);

    #[test]
    fn each_button_posts_its_spending() {
        assert_eq!(
            written(FRA, MOVE_CAPITAL, PowerKind::Adm, 183, -1, None),
            r#"spend_power_command={ country="FRA" power={ data=183 category=16 skill=0 } }"#
        );
        assert_eq!(
            written(FRA, SCORCH_EARTH, PowerKind::Mil, 54, 7, None),
            r#"spend_power_command={ country="FRA" power={ data=54 second=7 category=13 skill=2 } }"#
        );
        assert_eq!(sorties_written(FRA, 183), r#"spend_power_command={ country="FRA" power={ data=183 category=30 skill=2 } }"#);
        assert_eq!(
            harsh_written(FRA, (12 << 32) | 50),
            r#"spend_power_command={ country="FRA" power={ category=21 skill=2 rebel_faction={ id=12 type=50 } } }"#
        );
        assert_eq!(raze_written(FRA, 300), "raze_province={ country=66 province=300 }");
    }

    #[test]
    fn an_army_s_id_is_carried_low_half_first() {
        assert_eq!(halves((7 << 32) | 54), (54, 7));
    }

    #[test]
    fn every_button_has_its_capability_s_name() {
        let names: Vec<&str> = InProvince::ALL.iter().map(|b| b.name()).collect();
        assert_eq!(names, ["move_capital", "move_trade_port", "establish_siberian_frontier", "add_tribal_land", "raze"]);
        let names: Vec<&str> = ByArmy::ALL.iter().map(|b| b.name()).collect();
        assert_eq!(names, ["scorch_earth", "seize_colony", "burn_colony", "attack_natives"]);
    }
}
