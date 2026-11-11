//! Monuments: the great projects in the player's provinces, monuments and
//! canals, and what the monuments window's buttons do through the game's
//! own commands: upgrading, building, moving, cancelling and boosting
//! (docs/research/institutions-ages-monuments.md, "Monuments" and "Great
//! projects' other commands").
use super::content::{self, checks};
use super::{action, commands, thousandths, with_state, Ctx, Fail, GResult, Session, Status, Tag};
use crate::containers;
use crate::game_string;
use crate::{
    memory::Memory,
    objects::campaign::{construction, country, great_project as project, province, world},
    points::{GameFunction, Point},
};
use eu4_harmony_checks::Date;
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::monuments::{args, Buildable, MonumentConstruction, MonumentKind, MonumentOption};
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
/// `(definition, country, province, reasons, flag) -> bool`: whether the
/// country may build the great project the world does not have yet in the
/// province, writing why not into the `std::string` reasons, as the build
/// command's check asks it (with no reasons): no, without a reason, for one
/// the world has; its site; the province the country's and in its control,
/// not besieged, not a colony; the cost; its requirements.
const CAN_BUILD: GameFunction<5> = GameFunction::at(0x7d49d0);
/// `(definition, out, country)`: what building the great project costs the
/// country, i32 thousandths of a ducat into `out`.
const BUILD_COST: GameFunction<3> = GameFunction::at(0x7d7b80);
/// `(project, country, province, flag, reasons) -> bool`: whether the
/// country may move the project to the province, writing why not into the
/// `std::string` reasons, as the move command's check asks it (with no
/// reasons): its vtable's `+0x98`.
const CAN_MOVE: GameFunction<5> = GameFunction::at(0x7d1ef0);
/// `(project, out, province)`: what moving the project to the province
/// costs, i32 thousandths of a ducat into `out`, by the distance; 0 where
/// there is no route: its vtable's `+0xa8`.
const MOVE_COST: GameFunction<3> = GameFunction::at(0x7d2550);
/// `(construction, reasons) -> bool`: whether its builder may bring the
/// great project's construction forward with manpower, or with money,
/// writing why not into the `std::string` reasons when it is not null, as
/// the window's boost buttons ask it.
const CAN_BOOST_MANPOWER: GameFunction<2> = GameFunction::at(0x25faf0);
const CAN_BOOST_MONEY: GameFunction<2> = GameFunction::at(0x25fbf0);
/// What a boost costs: defines, the manpower in men (i32, as the country
/// keeps it at [`country::MANPOWER`]) and the money in thousandths of a
/// ducat (i32).
const BOOST_MANPOWER: usize = 0x233da2c;
const BOOST_MONEY: usize = 0x233da30;
/// The tier past which a project is not upgraded.
const LAST_TIER: u8 = 3;
const MOST: usize = 10_000;
/// The constructions a province may have at once, at most.
const MOST_CONSTRUCTIONS: usize = 64;

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
        // Building: the check, which finds the project and asks whether the
        // world has it, then its site and whether it may stand anywhere,
        // then the cost against the treasury.
        Point::keep(CAN_BUILD, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55 41 56 41 57", "great project build check"),
        Point::keep(0x7d4858usize, "80 be 34 1d 00 00 00 75 4a 8b 86 30 1d 00 00", "great project site")
            .uses(&[project::DEFINITION_MOVABLE, project::DEFINITION_SITE]),
        Point::keep(0x7d4c32usize, "e8 49 2f 00 00 8b 5c 24 30 41 3b 9f b0 0b 00 00", "great project build cost").uses(&[country::TREASURY]),
        Point::keep(BUILD_COST, "48 89 5c 24 10 48 89 6c 24 18 56 57 41 56 48 81", "great project build cost function"),
        // The build command's check asks it with the definition, the
        // country and the province.
        Point::keep(0x26b5a3usize, "e8 28 94 56 00", "constructgreatproject check"),
        // Moving: the move command's check asks the project's +0x98 with
        // the target province, and its +0xa8 is the cost.
        Point::keep(CAN_MOVE, "48 89 5c 24 10 44 88 4c 24 20 4c 89 44 24 18 55 56 57 41 54 41 55 41 56 41 57", "great project move check"),
        Point::keep(0x26b8a4usize, "4c 8b 91 98 00 00 00", "move_great_project check"),
        Point::keep(MOVE_COST, "48 89 5c 24 18 57 48 83 ec 30 4d 8b d0 48 8b fa 48 63 81 e0 05 00 00", "great project move cost").uses(&[project::PROVINCE]),
        // Boosting: the checks, the defines they compare, and the builder
        // the cancel and boost commands' checks compare.
        Point::keep(CAN_BOOST_MANPOWER, "40 53 48 83 ec 40 48 8b 81 88 00 00 00 48 8b", "great project manpower boost check")
            .uses(&[construction::BUILDER]),
        Point::keep(0x25fb1dusize, "44 8b 80 b8 0d 00 00 44 39 05 01 df 0d 02", "great project manpower boost define")
            .uses(&[country::MANPOWER, BOOST_MANPOWER]),
        Point::keep(CAN_BOOST_MONEY, "40 53 48 83 ec 40 48 8b 81 88 00 00 00 48 8b", "great project money boost check")
            .uses(&[construction::BUILDER]),
        Point::keep(0x25fc19usize, "8b 05 11 de 0d 02 4f 8b 0c c8 41 39 81 b0 0b 00 00", "great project money boost define")
            .uses(&[BOOST_MONEY, country::TREASURY]),
        Point::keep(0x272c93usize, "48 8b 90 88 00 00 00", "cancel_great_project builder").uses(&[construction::BUILDER]),
        Point::keep(0x272ce3usize, "48 8b 9c 19 d8 04 00 00", "great project constructions").uses(&[province::CONSTRUCTIONS]),
        // The window's cancel button names the construction's project by
        // what it makes.
        Point::keep(0x1398a8dusize, "4c 8b 82 80 00 00 00", "great project construction's project").uses(&[construction::MAKES]),
    ]
}

/// A great project in one of the player's provinces.
struct Mine {
    at: usize,
    key: String,
    province: u32,
}

/// Every great project the world keeps, built or not: where it is in the
/// manager's array, its key, and its province, 0 for one the world does not
/// have.
fn every(m: &Memory, ctx: &Ctx) -> GResult<Vec<(usize, String, i32)>> {
    let manager = m.ptr(ctx.world + world::GREAT_PROJECTS)?;
    let data = m.ptr(manager + world::GREAT_PROJECTS_DATA)?;
    let count = containers::bounded(m.i32(manager + world::GREAT_PROJECTS_COUNT)?, MOST)?;
    let mut out = Vec::new();
    for i in 0..count {
        let at = data + i * project::SIZE;
        let key = game_string::key(m, m.ptr(at + project::DEFINITION)? + project::DEFINITION_KEY)?;
        out.push((at, key, m.i32(at + project::PROVINCE)?));
    }
    Ok(out)
}

/// The great projects in the player's provinces, in the manager's order.
fn mine(m: &Memory, ctx: &Ctx) -> GResult<Vec<Mine>> {
    let mut out = Vec::new();
    for (at, key, id) in every(m, ctx)? {
        if id <= 0 {
            continue;
        }
        let place = with_state(|s| s.geography.province_address(m, ctx, id as u32))?;
        if Tag(m.u64(place + province::OWNER)?).index() != ctx.player.index() {
            continue;
        }
        out.push(Mine { at, key, province: id as u32 });
    }
    Ok(out)
}

/// What kind of great project a definition is.
fn kind_of(m: &Memory, definition: usize) -> GResult<MonumentKind> {
    Ok(match m.u8(definition + project::DEFINITION_KIND)? {
        project::CANAL => MonumentKind::Canal,
        _ => MonumentKind::Monument,
    })
}

/// What the window offers for one project: upgrading it.
fn offered(m: &Memory, ctx: &Ctx, it: &Mine) -> GResult<MonumentOption> {
    let tier = m.u8(it.at + project::TIER)?;
    let kind = kind_of(m, m.ptr(it.at + project::DEFINITION)?)?;
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

/// A great project command naming the project, a province and the builder,
/// as the window's buttons send the upgrade (`upgrade_great_project`), the
/// build (`constructgreatproject`) and the move (`move_great_project`),
/// which share their writer.
fn command(name: &str, key: &str, province: u32, builder: Tag) -> String {
    format!("{name}={{ great_projects=\"{key}\" province={province} builder=\"{}\" }}", content::letters(builder))
}

/// Starts upgrading a great project of the player's, as the window's button
/// does.
pub fn upgrade(Session { m, ctx, commands, .. }: Session, args::Upgrade { monument: wanted }: args::Upgrade) -> GResult<()> {
    let projects = mine(m, &ctx)?;
    let Some(it) = projects.iter().find(|it| it.key == wanted) else {
        return Err(checks::not_found("great project in the player's provinces", &wanted));
    };
    let choice = offered(m, &ctx, it)?.choice;
    action::carry_out(commands, &choice, &format!("upgrade {wanted}"), || Ok(command("upgrade_great_project", &it.key, it.province, ctx.player)))
}

/// The address of a province, checked to be one.
fn place(m: &Memory, ctx: &Ctx, id: u32) -> GResult<usize> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    with_state(|s| s.geography.province_address(m, ctx, id))
}

/// Whether the player can build the great project of `definition` in the
/// province at `place`, as the build button's check says, and its cost.
fn build_choice(m: &Memory, ctx: &Ctx, definition: usize, place: usize) -> GResult<Choice> {
    let mut out = 0i32;
    BUILD_COST.call(m, [definition, &mut out as *mut i32 as usize, ctx.country])?;
    let cost = vec![Amount::new(Resource::Ducats, thousandths(out))];
    let (can, reasons) = content::checked(m, |reasons| Ok(CAN_BUILD.call(m, [definition, ctx.country, place, reasons, 0])?))?;
    Ok(if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// The great projects the world does not have, whose definition may be
/// built in the province `id`: its site, or anywhere.
fn unbuilt_for(m: &Memory, ctx: &Ctx, id: u32) -> GResult<Vec<(usize, String)>> {
    let mut out = Vec::new();
    for (at, key, province) in every(m, ctx)? {
        if province > 0 {
            continue;
        }
        let definition = m.ptr(at + project::DEFINITION)?;
        let site = m.i32(definition + project::DEFINITION_SITE)?;
        let anywhere = m.u8(definition + project::DEFINITION_MOVABLE)? != 0;
        if anywhere || site <= 0 || site as u32 == id {
            out.push((definition, key));
        }
    }
    Ok(out)
}

/// The great projects the world does not have that the window offers to
/// build in a province.
pub fn buildable(Session { m, ctx, .. }: Session, args::Buildable { province: ProvinceId(id) }: args::Buildable) -> GResult<Vec<Buildable>> {
    let at = place(m, &ctx, id)?;
    unbuilt_for(m, &ctx, id)?
        .into_iter()
        .map(|(definition, key)| {
            Ok(Buildable { name: content::name(m, &key)?, kind: kind_of(m, definition)?, choice: build_choice(m, &ctx, definition, at)?, monument: key })
        })
        .collect()
}

/// Starts building a great project the world does not have yet, as the
/// window's button does.
pub fn build(Session { m, ctx, commands, .. }: Session, args::Build { monument: wanted, province: ProvinceId(id) }: args::Build) -> GResult<()> {
    let at = place(m, &ctx, id)?;
    let unbuilt = every(m, &ctx)?.into_iter().find(|(_, key, province)| *key == wanted && *province <= 0);
    let Some((project_at, key, _)) = unbuilt else {
        return Err(checks::not_found("great project the world does not have", &wanted));
    };
    let choice = build_choice(m, &ctx, m.ptr(project_at + project::DEFINITION)?, at)?;
    action::carry_out(commands, &choice, &format!("build {wanted}"), || Ok(command("constructgreatproject", &key, id, ctx.player)))
}

/// Whether the player can move one of their great projects to the province
/// `id` now, and its cost.
fn move_choice(m: &Memory, ctx: &Ctx, it: &Mine, id: u32) -> GResult<Choice> {
    let at = place(m, ctx, id)?;
    let mut out = 0i32;
    MOVE_COST.call(m, [it.at, &mut out as *mut i32 as usize, at])?;
    let cost = vec![Amount::new(Resource::Ducats, thousandths(out))];
    let (can, reasons) = content::checked(m, |reasons| Ok(CAN_MOVE.call(m, [it.at, ctx.country, at, 0, reasons])?))?;
    Ok(if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// One of the player's great projects, by its key.
fn named(m: &Memory, ctx: &Ctx, wanted: &str) -> GResult<Mine> {
    mine(m, ctx)?.into_iter().find(|it| it.key == wanted).ok_or_else(|| checks::not_found("great project in the player's provinces", wanted))
}

/// Whether the player can move one of their great projects to a province.
pub fn evaluate_move(Session { m, ctx, .. }: Session, args::EvaluateMove { monument, province: ProvinceId(id) }: args::EvaluateMove) -> GResult<Choice> {
    move_choice(m, &ctx, &named(m, &ctx, &monument)?, id)
}

/// Starts moving one of the player's great projects, as the window's move
/// button does.
pub fn move_to(Session { m, ctx, commands, .. }: Session, args::Move { monument, province: ProvinceId(id) }: args::Move) -> GResult<()> {
    let it = named(m, &ctx, &monument)?;
    let choice = move_choice(m, &ctx, &it, id)?;
    action::carry_out(commands, &choice, &format!("move {monument}"), || Ok(command("move_great_project", &it.key, id, ctx.player)))
}

/// A great project's construction of the player's.
struct Building {
    at: usize,
    key: String,
    province: u32,
}

/// The player's great projects' constructions, province by province.
fn building(m: &Memory, ctx: &Ctx) -> GResult<Vec<Building>> {
    let mut out = Vec::new();
    for place in super::world::provinces_owned_by(m, ctx.world, ctx.player)? {
        for c in containers::items(m, place + province::CONSTRUCTIONS, MOST_CONSTRUCTIONS)? {
            let token = m.u32(c + construction::KIND)?;
            if commands::type_name(m, token).as_deref() != Some("great_project_construction") {
                continue;
            }
            if Tag(m.u64(c + construction::BUILDER)?).index() != ctx.player.index() {
                continue;
            }
            let key = game_string::key(m, m.ptr(c + construction::MAKES)? + project::DEFINITION_KEY)?;
            out.push(Building { at: c, key, province: m.i32(place + province::ID)? as u32 });
        }
    }
    Ok(out)
}

/// Whether the construction may be boosted with `check`, its cost `amount`.
fn boost_choice(m: &Memory, check: GameFunction<2>, at: usize, amount: Amount) -> GResult<Choice> {
    let (can, reasons) = content::checked(m, |reasons| Ok(check.call(m, [at, reasons])?))?;
    Ok(if can { Choice::can(vec![amount]) } else { Choice::cannot(reasons, vec![amount]) })
}

/// The manpower boost's cost, in men, and the money boost's, in ducats.
fn boost_costs(m: &Memory) -> GResult<(Amount, Amount)> {
    Ok((Amount::new(Resource::Manpower, m.i32(m.at(BOOST_MANPOWER))? as f64), Amount::new(Resource::Ducats, thousandths(m.i32(m.at(BOOST_MONEY))?))))
}

/// The player's great projects under construction, and what the window
/// offers for each.
pub fn constructions(Session { m, ctx, .. }: Session) -> GResult<Vec<MonumentConstruction>> {
    let (men, money) = boost_costs(m)?;
    building(m, &ctx)?
        .into_iter()
        .map(|b| {
            Ok(MonumentConstruction {
                name: content::name(m, &b.key)?,
                province: ProvinceId(b.province),
                start: Date(m.i32(b.at + construction::START)?),
                end: Date(m.i32(b.at + construction::END)?),
                cancel: Choice::can(Vec::new()),
                boost_manpower: boost_choice(m, CAN_BOOST_MANPOWER, b.at, men)?,
                boost_ducats: boost_choice(m, CAN_BOOST_MONEY, b.at, money)?,
                monument: b.key,
            })
        })
        .collect()
}

/// One of the player's great projects' constructions, by its key.
fn under_construction(m: &Memory, ctx: &Ctx, wanted: &str) -> GResult<Building> {
    building(m, ctx)?.into_iter().find(|b| b.key == wanted).ok_or_else(|| checks::not_found("great project the player is building", wanted))
}

/// The cancel command, as the window's cancel button sends it.
fn cancel_command(country: Tag, province: u32, key: &str) -> String {
    format!("cancel_great_project={{ country=\"{}\" province={province} great_projects=\"{key}\" }}", content::letters(country))
}

/// Cancels a great project's construction of the player's.
pub fn cancel(Session { m, ctx, commands, .. }: Session, args::Cancel { monument }: args::Cancel) -> GResult<()> {
    let b = under_construction(m, &ctx, &monument)?;
    action::carry_out(commands, &Choice::can(Vec::new()), &format!("cancel {monument}"), || Ok(cancel_command(ctx.player, b.province, &b.key)))
}

/// The boost command, as the window's boost buttons send it: `kind` 0 with
/// manpower, 1 with money, once.
fn boost_command(key: &str, province: u32, kind: u8, builder: Tag) -> String {
    format!("boost_great_project={{ great_projects=\"{key}\" province={province} type={kind} builder=\"{}\" }}", content::letters(builder))
}

/// Brings a great project's construction of the player's forward.
pub fn boost(Session { m, ctx, commands, .. }: Session, args::Boost { monument, with }: args::Boost) -> GResult<()> {
    let (men, money) = boost_costs(m)?;
    let (check, amount, kind) = match with.as_str() {
        "manpower" => (CAN_BOOST_MANPOWER, men, 0),
        "ducats" => (CAN_BOOST_MONEY, money, 1),
        other => return Err(Fail { status: Status::InvalidArgument, message: format!("a boost is paid with manpower or ducats, not {other}") }),
    };
    let b = under_construction(m, &ctx, &monument)?;
    let choice = boost_choice(m, check, b.at, amount)?;
    action::carry_out(commands, &choice, &format!("boost {monument} with {with}"), || Ok(boost_command(&b.key, b.province, kind, ctx.player)))
}

crate::catalogue::bind! {
    eu4_harmony_remote::monuments: MonumentRequest => MonumentAnswer;
    List => list;
    Upgrade => upgrade;
    Buildable => buildable;
    Build => build;
    EvaluateMove => evaluate_move;
    Move => move_to;
    Constructions => constructions;
    Cancel => cancel;
    Boost => boost;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::tur;

    #[test]
    fn the_upgrade_command_names_the_project_its_province_and_the_builder() {
        assert_eq!(
            command("upgrade_great_project", "hagia_sophia", 151, tur()),
            r#"upgrade_great_project={ great_projects="hagia_sophia" province=151 builder="TUR" }"#
        );
    }

    #[test]
    fn the_build_and_move_commands_are_written_as_the_upgrade_is() {
        assert_eq!(command("constructgreatproject", "kiel_canal", 45, tur()), r#"constructgreatproject={ great_projects="kiel_canal" province=45 builder="TUR" }"#);
        assert_eq!(command("move_great_project", "parthenon", 151, tur()), r#"move_great_project={ great_projects="parthenon" province=151 builder="TUR" }"#);
    }

    #[test]
    fn the_cancel_and_boost_commands_name_the_construction() {
        assert_eq!(cancel_command(tur(), 146, "parthenon"), r#"cancel_great_project={ country="TUR" province=146 great_projects="parthenon" }"#);
        assert_eq!(boost_command("parthenon", 146, 1, tur()), r#"boost_great_project={ great_projects="parthenon" province=146 type=1 builder="TUR" }"#);
    }
}
