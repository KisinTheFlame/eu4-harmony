//! Islamic piety's two actions, enforcing faithful adherence and calling on
//! religious followers, as the religion view's buttons post them, with the
//! view's own lines (docs/research/religion.md, "Piety").
use super::commands::{write, Block};
use super::{action, content, Ctx, GResult, Session, Tag};
use crate::{
    memory::Memory,
    objects::campaign::country,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::religion::ReligionOption;

/// A religion's byte that says it uses piety.
const RELIGION_USES_PIETY: usize = 0x114;
/// `(country, text) -> bool`: whether the country may enforce faithful
/// adherence now, its piety far enough toward Legalism and the action not
/// used in the last years, writing the view's lines into the `std::string`
/// text when it is not null (when it may, its line of what the action
/// does).
const MAY_ENFORCE: GameFunction<2> = GameFunction::at(0x3e5030);
/// The same for calling on religious followers, toward Mysticism.
const MAY_CALL: GameFunction<2> = GameFunction::at(0x3e5560);
/// How far an action moves piety toward the other end, i32 thousandths of
/// the scale's half (`PIETY_COST`).
const PIETY_COST: usize = 0x233dcc0;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(
            MAY_ENFORCE,
            "48 89 5c 24 10 48 89 74 24 20 55 57 41 54 41 56 41 57 48 8d 6c 24 80 48 81 ec 80 01 00 00 48 8b f2 48 8b f9",
            "may enforce faithful adherence",
        ),
        Point::keep(
            MAY_CALL,
            "48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 30 ff ff ff 48 81 ec d0 01 00 00 4c 8b f2 48 8b",
            "may call on religious followers",
        ),
        // The checks first ask the religion whether it uses piety.
        Point::keep(0x3e5596usize, "44 38 a0 14 01 00 00", "religion uses piety").uses(&[RELIGION_USES_PIETY]),
        // The view's line of what calling on followers does: the cost, in
        // percent.
        Point::keep(0x3e5c7busize, "6b 05 3e 80 f5 01 64", "piety action cost").uses(&[PIETY_COST]),
        // The commands' checks: the country by the tag their members hold.
        Point::keep(0xc1370usize, "48 89 5c 24 08 57 48 83 ec 20 48 8b 41 50 48 c1 e8 20", "use_high_piety check"),
        Point::keep(0xc1250usize, "48 83 ec 28 48 8b 41 50 33 d2 48 c1 e8 20", "use_low_piety check"),
    ]
}

/// Whether the player's religion uses piety.
fn uses_piety(m: &Memory, country: usize) -> GResult<bool> {
    match m.ptr(country + country::STATE_RELIGION)? {
        0 => Ok(false),
        religion => Ok(m.u8(religion + RELIGION_USES_PIETY)? != 0),
    }
}

/// Whether the player may take the action `check` asks about now, with
/// the view's lines when it may not, and how far it moves piety.
fn choice(m: &Memory, country: usize, check: GameFunction<2>) -> GResult<Choice> {
    let (may, reasons) = content::checked(m, |text| Ok(check.call(m, [country, text])?))?;
    let cost = vec![Amount::new(Resource::Piety, f64::from(m.i32(m.at(PIETY_COST))?) / 10.0)];
    Ok(if may { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// The two actions, while the player's religion uses piety.
pub fn options(m: &Memory, ctx: &Ctx) -> GResult<Vec<ReligionOption>> {
    if !uses_piety(m, ctx.country)? {
        return Ok(Vec::new());
    }
    let entry = |action: &str, name: &str, check| -> GResult<ReligionOption> {
        Ok(ReligionOption {
            action: action.into(),
            key: action.into(),
            name: content::name(m, name)?,
            current: false,
            province: None,
            choice: choice(m, ctx.country, check)?,
        })
    };
    Ok(vec![entry("enforce_faithful_adherence", "positive_piety", MAY_ENFORCE)?, entry("call_on_religious_followers", "negative_piety", MAY_CALL)?])
}

/// The command of either action: the country, as their members hold it.
fn text(command: &str, country: Tag) -> String {
    write(command, Block::new().block("members", Block::new().tag("dummy", country.letters())))
}

fn take(Session { m, ctx, commands, .. }: Session, command: &str, check: GameFunction<2>, doing: &str) -> GResult<()> {
    if !uses_piety(m, ctx.country)? {
        return Err(super::religion::checks::not_of_religion("piety"));
    }
    action::carry_out(commands, &choice(m, ctx.country, check)?, doing, || Ok(text(command, ctx.player)))
}

/// Enforces faithful adherence: Legalism's action, `use_high_piety`.
pub fn enforce(session: Session, _: eu4_harmony_remote::religion::args::EnforceFaithfulAdherence) -> GResult<()> {
    take(session, "use_high_piety", MAY_ENFORCE, "enforce faithful adherence")
}

/// Calls on religious followers: Mysticism's action, `use_low_piety`.
pub fn call(session: Session, _: eu4_harmony_remote::religion::args::CallOnReligiousFollowers) -> GResult<()> {
    take(session, "use_low_piety", MAY_CALL, "call on religious followers")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_name_the_country_as_their_members() {
        let mut bytes = [0u8; 8];
        bytes[..3].copy_from_slice(b"TUR");
        let tur = Tag(u64::from_le_bytes(bytes));
        assert_eq!(text("use_high_piety", tur), r#"use_high_piety={ members={ dummy="TUR" } }"#);
        assert_eq!(text("use_low_piety", tur), r#"use_low_piety={ members={ dummy="TUR" } }"#);
    }
}
