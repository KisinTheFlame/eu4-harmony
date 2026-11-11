//! Monuments: the great projects in the player's provinces, monuments and
//! canals, and upgrading one through the game's own command
//! (docs/research/institutions-ages-monuments.md, "Monuments").
use super::content::{self, checks};
use super::{action, thousandths, with_state, Ctx, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::{
    memory::Memory,
    objects::campaign::{great_project as project, province, world},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::monuments::{args, MonumentKind, MonumentOption};
use eu4_harmony_remote::ProvinceId;

/// `(project, country, flag, reasons) -> bool`: whether the country may
/// upgrade the project to its next tier, writing why not into the
/// `std::string` reasons, as the upgrade command's check asks it (with no
/// flag and no reasons): its vtable's `+0x60`.
const CAN_UPGRADE: GameFunction<4> = GameFunction::at(0x7d1560);
/// `(project, out, country, explanation or null)`: what upgrading the
/// project to its next tier costs the country, i32 thousandths of a ducat
/// into `out`: its vtable's `+0x70`.
const COST: GameFunction<4> = GameFunction::at(0x7d17a0);
/// The tier past which a project is not upgraded.
const LAST_TIER: u8 = 3;
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(CAN_UPGRADE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55", "great project upgrade check"),
        Point::keep(COST, "48 89 5c 24 08 57 48 83 ec 30 44 0f b6 91 e4 05 00 00", "great project upgrade cost").uses(&[project::TIER]),
        // The projects in a country: the manager's array, each project's
        // province.
        Point::keep(0x92d949usize, "48 8b 90 b0 25 00 00 48 8b 5a 58 48 63 42 64", "great projects")
            .uses(&[world::GREAT_PROJECTS, world::GREAT_PROJECTS_DATA, world::GREAT_PROJECTS_COUNT]),
        Point::keep(0x92d974usize, "48 63 83 e0 05 00 00", "great project province").uses(&[project::PROVINCE]),
        Point::keep(0x7d30fbusize, "48 63 48 14 48 69 c1 e8 05 00 00 49 03 42 58", "great project size").uses(&[project::SIZE]),
        // The upgrade check: the tier, the definition and its kind.
        Point::keep(0x7d159eusize, "0f b6 8b e4 05 00 00", "great project tier").uses(&[project::TIER]),
        Point::keep(0x7d15bfusize, "48 8b 43 10", "great project definition").uses(&[project::DEFINITION]),
        Point::keep(0x7d15dcusize, "0f b6 88 36 1d 00 00", "great project kind").uses(&[project::DEFINITION_KIND]),
        // The upgrade command's writer writes the definition's key.
        Point::keep(0x26a7b2usize, "48 8b 19 48 8b fa 48 8b f1 ba a5 2f 00 00 48 8b cf", "great project key"),
    ]
}

/// A great project in one of the player's provinces.
struct Mine {
    at: usize,
    key: String,
    province: u32,
}

/// The great projects in the player's provinces, in the manager's order.
fn mine(m: &Memory, ctx: &Ctx) -> GResult<Vec<Mine>> {
    let manager = m.ptr(ctx.world + world::GREAT_PROJECTS)?;
    let data = m.ptr(manager + world::GREAT_PROJECTS_DATA)?;
    let count = containers::bounded(m.i32(manager + world::GREAT_PROJECTS_COUNT)?, MOST)?;
    let mut out = Vec::new();
    for i in 0..count {
        let at = data + i * project::SIZE;
        let id = m.i32(at + project::PROVINCE)?;
        if id <= 0 {
            continue;
        }
        let place = with_state(|s| s.geography.province_address(m, ctx, id as u32))?;
        if Tag(m.u64(place + province::OWNER)?).index() != ctx.player.index() {
            continue;
        }
        let key = game_string::key(m, m.ptr(at + project::DEFINITION)? + project::DEFINITION_KEY)?;
        out.push(Mine { at, key, province: id as u32 });
    }
    Ok(out)
}

/// What the window offers for one project: upgrading it.
fn offered(m: &Memory, ctx: &Ctx, it: &Mine) -> GResult<MonumentOption> {
    let tier = m.u8(it.at + project::TIER)?;
    let kind = match m.u8(m.ptr(it.at + project::DEFINITION)? + project::DEFINITION_KIND)? {
        project::CANAL => MonumentKind::Canal,
        _ => MonumentKind::Monument,
    };
    let mut cost = Vec::new();
    if tier < LAST_TIER {
        let mut out = 0i32;
        COST.call(m, [it.at, &mut out as *mut i32 as usize, ctx.country, 0])?;
        cost.push(Amount::new(Resource::Ducats, thousandths(out)));
    }
    let (can, reasons) = content::checked(m, |reasons| Ok(CAN_UPGRADE.call(m, [it.at, ctx.country, 0, reasons])?))?;
    let choice = if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) };
    Ok(MonumentOption { name: content::name(m, &it.key)?, monument: it.key.clone(), kind, province: ProvinceId(it.province), tier: tier as u32, choice })
}

/// The great projects in the player's provinces, as the window lists them.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<MonumentOption>> {
    mine(m, &ctx)?.iter().map(|it| offered(m, &ctx, it)).collect()
}

/// The upgrade command, as the window's button sends it.
fn command(key: &str, province: u32, builder: Tag) -> String {
    format!("upgrade_great_project={{ great_projects=\"{key}\" province={province} builder=\"{}\" }}", content::letters(builder))
}

/// Starts upgrading a great project of the player's, as the window's button
/// does.
pub fn upgrade(Session { m, ctx, commands, .. }: Session, args::Upgrade { monument: wanted }: args::Upgrade) -> GResult<()> {
    let projects = mine(m, &ctx)?;
    let Some(it) = projects.iter().find(|it| it.key == wanted) else {
        return Err(checks::not_found("great project in the player's provinces", &wanted));
    };
    let choice = offered(m, &ctx, it)?.choice;
    action::carry_out(commands, &choice, &format!("upgrade {wanted}"), || Ok(command(&it.key, it.province, ctx.player)))
}

crate::catalogue::bind! {
    eu4_harmony_remote::monuments: MonumentRequest => MonumentAnswer;
    List => list;
    Upgrade => upgrade;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::tur;

    #[test]
    fn the_upgrade_command_names_the_project_its_province_and_the_builder() {
        assert_eq!(command("hagia_sophia", 151, tur()), r#"upgrade_great_project={ great_projects="hagia_sophia" province=151 builder="TUR" }"#);
    }
}
