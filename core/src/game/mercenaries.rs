//! Mercenary companies: what the mercenary view lists for the player, and
//! hiring one with the game's recruit command, as the view and the map post
//! it once the player has picked a company and a province
//! (docs/research/leaders.md, "Hiring a mercenary company").
use super::units::checks;
use super::{action, with_state, Ctx, GResult, Session};
use crate::{
    game_string,
    memory::Memory,
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::mercenaries::args;
use eu4_harmony_remote::ProvinceId;
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
const NAME: GameFunction<2> = GameFunction::at(0x8ffcb0);
const REGIMENTS: GameFunction<3> = GameFunction::at(0x900100);
const HIRING_COST: GameFunction<4> = GameFunction::at(0x900cb0);
const UPKEEP: GameFunction<4> = GameFunction::at(0x900660);
/// `(company, country, 1, text or null)`: whether the view lets the player
/// hire it, anywhere, with the reasons; `(company, country, province, text
/// or null)`: whether the player may hire it in that province.
const MAY_HIRE: GameFunction<4> = GameFunction::at(0x8ff0f0);
const MAY_HIRE_THERE: GameFunction<4> = GameFunction::at(0x8ff7b0);
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
    Ok(game_string::key(m, template + TEMPLATE_KEY)?)
}

/// Thousandths the game writes into an int of the framework's.
fn thousandths(m: &Memory, f: GameFunction<4>, company: usize, ctx: &Ctx) -> GResult<f64> {
    let mut out = 0i32;
    f.call(m, [company, &mut out as *mut i32 as usize, ctx.country, 0])?;
    Ok(out as f64 / 1000.0)
}

fn read(m: &Memory, ctx: &Ctx, company: usize) -> GResult<Company> {
    let (_, name) = game_string::written(m, |t| NAME.call(m, [company, t]))?;
    let mut regiments = [0i32; 3];
    REGIMENTS.call(m, [company, regiments.as_mut_ptr() as usize, ctx.country])?;
    let cost = vec![Amount::new(Resource::Ducats, thousandths(m, HIRING_COST, company, ctx)?)];
    let (may, text) = game_string::written(m, |t| MAY_HIRE.call(m, [company, ctx.country, 1, t]))?;
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
pub fn hire(Session { m, ctx, commands, .. }: Session, args::Hire { company: word, province: ProvinceId(province) }: args::Hire) -> GResult<()> {
    let mut found = None;
    for company in addresses(m, &ctx)? {
        if key(m, company)? == word {
            found = Some(company);
            break;
        }
    }
    let Some(company) = found else {
        return Err(checks::no_company(&word));
    };
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let (may, text) = game_string::written(m, |t| MAY_HIRE_THERE.call(m, [company, ctx.country, address, t]))?;
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

    use super::*;
    use crate::game::{
        buffers::{self, answers_no, answers_yes, write_inline, Image, Object, Provinces},
        commands::record,
        Caller, Fail, Status, Tag,
    };

    /// The game's answer of whether the player may hire a company there:
    /// no, the province not the player's.
    extern "C" fn not_owned(_: usize, _: usize, _: usize, text: usize) -> usize {
        write_inline(text, "- not owned");
        0
    }

    /// Castile, whose mercenary view lists the companies `swiss_guard` (ID
    /// 4321 of type 59) and `free_company`, in a world of provinces 0 to 2;
    /// the game lets the player hire there as `may_there` answers.
    struct Castile {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn castile(may_there: *const ()) -> Castile {
        let image = Image::new();
        image.function(MAY_HIRE_THERE.rva(), may_there).function(game_string::DESTROY_AT.rva(), answers_no as *const ());
        let provinces = Provinces::new(&image, 3);
        let company = |key: &str, number, kind| {
            Object::new(TEMPLATE + 8).i32(ID, kind).i32(NUMBER, number).points_to(TEMPLATE, Object::new(TEMPLATE_KEY + 0x20).string(TEMPLATE_KEY, key))
        };
        let (swiss, free) = (company("swiss_guard", 4321, 59), company("free_company", 7, 59));
        let cas = Tag(u64::from_le_bytes(*b"CAS\0\0\0\0\0"));
        let castile = Object::country(cas, COMPANIES_COUNT + 4).vector(COMPANIES, &[free.at() as u64, swiss.at() as u64]).i32(COMPANIES_COUNT, 2);
        let countries = Object::countries(&[&castile]);
        let session = buffers::session(image.memory(), provinces.world(), cas, castile.at(), countries.at(), Caller::Remote);
        Castile { _image: image, _provinces: provinces, _objects: vec![swiss, free, castile, countries], session }
    }

    fn hiring(company: &str, province: u32) -> args::Hire {
        args::Hire { company: company.into(), province: ProvinceId(province) }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the company was hired") };
        (status, message)
    }

    #[test]
    fn a_listed_company_is_hired_by_its_id_in_the_province() {
        let castile = castile(answers_yes as *const ());
        hire(castile.session, hiring("swiss_guard", 2)).unwrap();
        assert_eq!(record::posted(), [r#"buildunit={ country="CAS" province=2 mercenary={ id=4321 type=59 } category=1 }"#]);
    }

    #[test]
    fn a_company_not_listed_or_not_hired_there_is_refused() {
        let castile = castile(not_owned as *const ());
        assert_eq!(refused(hire(castile.session, hiring("swiss_guard", 2))), (Status::Rejected, "not owned".into()));
        assert_eq!(refused(hire(castile.session, hiring("black_army", 2))), (Status::NotFound, r#"the mercenary view lists no company "black_army""#.into()));
        assert_eq!(refused(hire(castile.session, hiring("swiss_guard", 3))).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }
}
