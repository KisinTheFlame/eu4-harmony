//! Policies: those the policies view offers, enabling and disabling them,
//! through the game's own commands (docs/research/government.md,
//! "Policies").
use super::content::checks;
use super::{action, conditions, content, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::{
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::policies::args;
use eu4_harmony_remote::{choices::Choice, policies::Policy};

/// The policies' database, made on first use by its getter: the policies at
/// `+0x10`, a `std::vector` of pointers.
const DATABASE: usize = 0x242bd78;
const POLICIES: usize = 0x10;
/// A policy: its key, its category (0 administrative, 1 diplomatic, 2
/// military), and its conditions for being offered (`potential`).
const KEY: usize = 0x158;
const CATEGORY: usize = 0x178;
const POTENTIAL: usize = 0x180;
/// The country's active policies: three game arrays, one for each category,
/// 0x30 bytes apart from `+0x10`, of active policies, each with its policy at
/// `+0x10`.
const ACTIVE: usize = 0x140;
const ACTIVE_LISTS: usize = 0x10;
const ACTIVE_STRIDE: usize = 0x30;
const ACTIVE_POLICY: usize = 0x10;
/// `(country + ACTIVE, policy, reasons) -> bool`: whether the country may
/// enable the policy, writing why not into the `std::string` reasons.
const CHECK: GameFunction<3> = GameFunction::at(0x958e10);

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x95770ausize, "48 8b 05 67 46 ad 01 48 85 c0", "policies database").uses(&[DATABASE]),
        Point::keep(0x959867usize, "48 8b 70 10", "policies database's walk").uses(&[POLICIES]),
        Point::keep(0x959882usize, "45 3b b0 78 01 00 00", "policy category").uses(&[CATEGORY]),
        Point::keep(CHECK, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 41 54", "policy check"),
        // The enable command's check: the active policies of the country
        // named, the policy found by its key.
        Point::keep(0x48b340usize, "48 81 c1 40 01 00 00 48 8b d0 e8", "policy command check").uses(&[ACTIVE]),
        // The active policies' writer: three arrays from `+0x10`, and each
        // active policy's policy and key.
        Point::keep(0x4209fbusize, "4c 8d 71 10 41 bf 03 00 00 00 49 63 6e 0c", "active policies").uses(&[ACTIVE_LISTS]),
        Point::keep(0x95870fusize, "48 8b 59 10", "active policy").uses(&[ACTIVE_POLICY]),
        Point::keep(0x958721usize, "48 81 c3 58 01 00 00", "policy key").uses(&[KEY]),
    ]
}

fn database(m: &Memory) -> GResult<usize> {
    match m.ptr(m.at(DATABASE))? {
        0 => Err("the policies are not read yet".to_string().into()),
        database => Ok(database),
    }
}

/// The policies the player has enabled.
fn active(m: &Memory, country: usize) -> GResult<Vec<usize>> {
    let mut active = Vec::new();
    for list in 0..3 {
        for item in containers::array(m, country + ACTIVE + ACTIVE_LISTS + list * ACTIVE_STRIDE - containers::ARRAY_DATA, containers::MOST)? {
            active.push(m.ptr(item + ACTIVE_POLICY)?);
        }
    }
    Ok(active)
}

fn category(category: u32) -> &'static str {
    match category {
        1 => "dip",
        2 => "mil",
        _ => "adm",
    }
}

/// Whether the player can enable `policy`, and why not, as the game's check
/// writes it.
fn choice(m: &Memory, country: usize, policy: usize) -> GResult<Choice> {
    let (can, reasons) = content::checked(m, |reasons| Ok(CHECK.call(m, [country + ACTIVE, policy, reasons])?))?;
    Ok(if can { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// The policies the policies view offers the player.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<Policy>> {
    let active = active(m, ctx.country)?;
    let mut listed = Vec::new();
    for policy in containers::pointers(m, database(m)? + POLICIES, containers::MOST)? {
        let enabled = active.contains(&policy);
        if !enabled && !conditions::holds(m, policy + POTENTIAL, ctx.player, 0)? {
            continue;
        }
        let key = game_string::key(m, policy + KEY)?;
        listed.push(Policy {
            name: content::name(m, &key)?,
            policy: key,
            category: category(m.u32(policy + CATEGORY)?).to_string(),
            enabled,
            choice: choice(m, ctx.country, policy)?,
        });
    }
    Ok(listed)
}

fn find(m: &Memory, key: &str) -> GResult<usize> {
    for policy in containers::pointers(m, database(m)? + POLICIES, containers::MOST)? {
        if game_string::key(m, policy + KEY)? == key {
            return Ok(policy);
        }
    }
    Err(checks::not_found("policy", key))
}

/// The enable and disable commands: the policy by its key and the country.
fn written(command: &str, country: Tag, policy: &str) -> String {
    format!("{command}={{ policy=\"{policy}\" country=\"{}\" }}", content::letters(country))
}

/// Enables a policy for the player.
pub fn enable(Session { m, ctx, commands, .. }: Session, args::Enable { policy: key }: args::Enable) -> GResult<()> {
    let policy = find(m, &key)?;
    let choice = choice(m, ctx.country, policy)?;
    if !choice.can {
        return Err(action::not_allowed("enable this policy", &choice.reasons));
    }
    action::post(commands, &written("enable_unlockable", ctx.player, &key), "enable this policy")
}

/// Disables a policy the player has enabled.
pub fn disable(Session { m, ctx, commands, .. }: Session, args::Disable { policy: key }: args::Disable) -> GResult<()> {
    let policy = find(m, &key)?;
    if !active(m, ctx.country)?.contains(&policy) {
        return Err(checks::not_now(format!("the player has not enabled the policy {key}")));
    }
    action::post(commands, &written("disable_unlockable", ctx.player, &key), "disable this policy")
}

crate::catalogue::bind! {
    eu4_harmony_remote::policies: PolicyRequest => PolicyAnswer;
    List => list;
    Enable => enable;
    Disable => disable;
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_commands_name_the_policy_and_the_country() {
        let fra = super::Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(super::written("enable_unlockable", fra, "the_spy_discovery_act"), r#"enable_unlockable={ policy="the_spy_discovery_act" country="FRA" }"#);
    }

    use super::*;
    use crate::game::{
        buffers::{self, Image, Object},
        commands::record,
        Caller, Fail, Status,
    };

    /// France, of the policies `spy_act` and `land_act`, with the first
    /// enabled; the policies' database a global of the game's.
    fn disabling(key: &str) -> GResult<()> {
        let image = Image::new();
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        let (spy, land) = (Object::new(KEY + 0x20).string(KEY, "spy_act"), Object::new(KEY + 0x20).string(KEY, "land_act"));
        let database = Object::new(POLICIES + 0x10).vector(POLICIES, &[spy.at() as u64, land.at() as u64]);
        image.u64(DATABASE, database.at() as u64);
        let enabled = Object::new(ACTIVE_POLICY + 8).u64(ACTIVE_POLICY, spy.at() as u64);
        let country = Object::country(fra, ACTIVE + ACTIVE_LISTS + 3 * ACTIVE_STRIDE)
            .vector(ACTIVE + ACTIVE_LISTS, &[enabled.at() as u64])
            .i32(ACTIVE + ACTIVE_LISTS - containers::ARRAY_DATA + containers::ARRAY_COUNT, 1);
        let countries = Object::countries(&[&country]);
        let result = disable(buffers::session(image.memory(), 0, fra, country.at(), countries.at(), Caller::Remote), args::Disable { policy: key.into() });
        drop((spy, land, database, enabled, country, countries, image));
        result
    }

    #[test]
    fn a_policy_is_disabled_only_when_enabled() {
        disabling("spy_act").unwrap();
        assert_eq!(record::posted(), [r#"disable_unlockable={ policy="spy_act" country="FRA" }"#]);
        let Err(Fail { status, message }) = disabling("land_act") else { panic!("a policy not enabled was disabled") };
        assert_eq!((status, message.as_str()), (Status::Rejected, "the player has not enabled the policy land_act"));
        let Err(Fail { status, .. }) = disabling("war_act") else { panic!("an unknown policy was disabled") };
        assert_eq!(status, Status::NotFound);
        assert!(record::posted().is_empty());
    }
}
