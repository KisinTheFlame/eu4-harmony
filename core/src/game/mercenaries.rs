//! Mercenary companies: what the mercenary view lists for the player, and
//! hiring one with the game's recruit command, as the view and the map post
//! it once the player has picked a company and a province
//! (docs/research/leaders.md, "Hiring a mercenary company").
use super::{action, with_state, Ctx, GResult, Session};
use crate::{game_string, memory::Memory, points::Point, unicode::game_text};
use eu4_harmony_checks::military as checks;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    mercenaries::Company,
};

/// The companies the country may hire: an array of pointers, and its count.
const COMPANIES: usize = 0x23f0;
const COMPANIES_COUNT: usize = 0x23fc;
/// A company's ID (type at `+0x10`, number at `+0x14`), and its template,
/// whose key is a `std::string` at `+0x20`.
const ID: usize = 0x10;
const NUMBER: usize = 0x14;
const TEMPLATE: usize = 0x28;
const TEMPLATE_KEY: usize = 0x20;
/// `(company, text)`: its name; `(company, int[3], country)`: its
/// regiments by kind; `(company, int*, country, text or null)`: what hiring
/// it costs and what it costs a month, in thousandths of a ducat.
const NAME: usize = 0x8ffcb0;
const REGIMENTS: usize = 0x900100;
const HIRING_COST: usize = 0x900cb0;
const UPKEEP: usize = 0x900660;
/// `(company, country, 1, text or null)`: whether the view lets the player
/// hire it, anywhere, with the reasons; `(company, country, province, text
/// or null)`: whether the player may hire it in that province.
const MAY_HIRE: usize = 0x8ff0f0;
const MAY_HIRE_THERE: usize = 0x8ff7b0;
/// The most companies read.
const MOST: i32 = 1000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x4112efusize, "49 8b 0c 24 48 8b 81 f0 23 00 00 48 63 89 fc 23 00 00", "country mercenary companies").uses(&[COMPANIES, COMPANIES_COUNT]),
        Point::keep(0x8fdff0usize, "4c 8d 41 10", "mercenary company id").uses(&[ID]),
        Point::keep(0x8fdfffusize, "48 8b 56 28", "mercenary company template").uses(&[TEMPLATE]),
        Point::keep(NAME, "48 89 5c 24 10 57 48 83 ec 70 48 8b 41 28 48 8b da", "mercenary company name"),
        Point::keep(REGIMENTS, "48 89 5c 24 08 4c 89 44 24 18 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 50", "mercenary company regiments"),
        Point::keep(HIRING_COST, "48 89 5c 24 20 48 89 54 24 10 55 56 57 41 54 41 55 41 56 41 57", "mercenary company hiring cost"),
        Point::keep(UPKEEP, "4c 89 44 24 18 48 89 4c 24 08 55 53 56 57 41 54 41 55 41 56 41 57", "mercenary company upkeep"),
        Point::keep(MAY_HIRE, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55 41 56 41 57", "may hire a mercenary company"),
        Point::keep(MAY_HIRE_THERE, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 49 8b d9", "may hire a mercenary company there"),
    ]
}

/// The recruit command for a company: its ID, the special kind 1, and no
/// unit type.
fn hire_text(country: [u8; 3], province: u32, number: i32, kind: i32) -> String {
    format!("buildunit={{ country=\"{}\" province={province} mercenary={{ id={number} type={kind} }} category=1 }}", String::from_utf8_lossy(&country))
}

fn addresses(m: &Memory, ctx: &Ctx) -> GResult<Vec<usize>> {
    let count = m.i32(ctx.country + COMPANIES_COUNT)?;
    if !(0..MOST).contains(&count) {
        return Err(format!("{count} mercenary companies").into());
    }
    let array = m.ptr(ctx.country + COMPANIES)?;
    (0..count as usize).map(|i| Ok(m.ptr(array + i * 8)?)).collect()
}

fn key(m: &Memory, company: usize) -> GResult<String> {
    let template = m.ptr(company + TEMPLATE)?;
    if template == 0 {
        return Ok(String::new());
    }
    Ok(game_text::to_utf8(&game_string::read(m, template + TEMPLATE_KEY)?))
}

/// Thousandths the game writes into an int of the framework's.
fn thousandths(m: &Memory, f: usize, company: usize, ctx: &Ctx) -> GResult<f64> {
    let mut out = 0i32;
    m.call(m.at(f), &[company, &mut out as *mut i32 as usize, ctx.country, 0])?;
    Ok(out as f64 / 1000.0)
}

fn read(m: &Memory, ctx: &Ctx, company: usize) -> GResult<Company> {
    let (_, name) = game_string::written(m, |t| m.call(m.at(NAME), &[company, t]))?;
    let mut regiments = [0i32; 3];
    m.call(m.at(REGIMENTS), &[company, regiments.as_mut_ptr() as usize, ctx.country])?;
    let cost = vec![Amount::new(Resource::Ducats, thousandths(m, HIRING_COST, company, ctx)?)];
    let (may, text) = game_string::written(m, |t| m.call(m.at(MAY_HIRE), &[company, ctx.country, 1, t]))?;
    Ok(Company {
        company: key(m, company)?,
        name: game_text::plain(&name),
        regiments: regiments.iter().map(|r| (*r).max(0) as u32).sum(),
        upkeep: thousandths(m, UPKEEP, company, ctx)?,
        choice: if may as u8 != 0 { Choice::can(cost) } else { Choice::cannot(super::reasons(&text), cost) },
    })
}

/// The companies the mercenary view lists for the player.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<Company>> {
    addresses(m, &ctx)?.into_iter().map(|c| read(m, &ctx, c)).collect()
}

/// Hires a company the view lists, raised in a province the player picks.
pub fn hire(Session { m, ctx, commands, .. }: Session, word: String, province: u32) -> GResult<()> {
    let mut found = None;
    for company in addresses(m, &ctx)? {
        if key(m, company)? == word {
            found = Some(company);
            break;
        }
    }
    let Some(company) = found else {
        return Err(checks::no_company(&word).into());
    };
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let (may, text) = game_string::written(m, |t| m.call(m.at(MAY_HIRE_THERE), &[company, ctx.country, address, t]))?;
    let doing = format!("hire {word} there");
    if may as u8 == 0 {
        return Err(action::not_allowed(&doing, &super::reasons(&text)));
    }
    action::post(commands, &hire_text(ctx.player.letters(), province, m.i32(company + NUMBER)?, m.i32(company + ID)?), &doing)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_company_by_its_id() {
        assert_eq!(super::hire_text(*b"CAS", 219, 4321, 59), r#"buildunit={ country="CAS" province=219 mercenary={ id=4321 type=59 } category=1 }"#);
    }
}
