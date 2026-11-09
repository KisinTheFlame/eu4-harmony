//! Parliament: the issues the parliament view offers, starting a debate,
//! giving provinces seats, through the game's own commands
//! (docs/research/government.md, "Parliament").
use super::content::checks;
use super::{action, conditions, content, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::{memory::Memory, points::Point};
use eu4_harmony_remote::parliament::args;
use eu4_harmony_remote::ProvinceId;
use eu4_harmony_remote::{
    choices::Choice,
    parliament::{Issue, Parliament},
};

/// The country's parliament, null without one: the issues on offer, a game
/// array of i32 indices into the issues' database; the debate going on,
/// null when none is; the issue enacted last.
const PARLIAMENT: usize = 0x24c8;
const OFFER: usize = 0x20;
const DEBATE: usize = 0x50;
const ENACTED: usize = 0x58;
/// The parliament issues' database, an object of the game's at a fixed
/// place: a game array of 0xe8-byte issues, each with its key and its
/// conditions (`allow`), a pointer to a block of them.
const DATABASE: usize = 0x2329390;
const ISSUE: usize = 0xe8;
const ISSUE_KEY: usize = 0x10;
const ISSUE_ALLOW: usize = 0x48;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x2de7a8usize, "4d 8b 87 c8 24 00 00", "country parliament").uses(&[PARLIAMENT]),
        // The parliament's writer: the issues on offer, the debate, the
        // issue enacted and its key.
        Point::keep(0x960984usize, "83 7e 34 00 7e 11 4c 8d 46 20", "parliament issues on offer").uses(&[OFFER]),
        Point::keep(0x96099busize, "48 8b 5e 50", "parliament debate").uses(&[DEBATE]),
        Point::keep(0x9609bdusize, "48 8b 5e 58", "parliament issue enacted").uses(&[ENACTED]),
        Point::keep(0x9609d3usize, "48 83 7b 28 10 48 8d 53 10", "parliament issue key").uses(&[ISSUE_KEY]),
        // The issues' getter, and a walk of them.
        Point::keep(0x968900usize, "48 8d 05 89 0a 9c 01", "parliament issues").uses(&[DATABASE]),
        Point::keep(0x96694cusize, "49 81 c4 e8 00 00 00", "parliament issue size"),
        // The debate command's check: a parliament, and no debate.
        Point::keep(0x2a4c1eusize, "48 8b 88 c8 24 00 00 48 85 c9 74 0a 48 83 79 50 00", "debate check"),
    ]
}

/// The issues of the database.
fn issues(m: &Memory) -> GResult<Vec<usize>> {
    let database = m.at(DATABASE);
    let (data, count) = (m.ptr(database + containers::ARRAY_DATA)?, containers::bounded(m.i32(database + containers::ARRAY_COUNT)?, containers::MOST)?);
    Ok((0..count).map(|i| data + i * ISSUE).collect())
}

/// The issues the player's parliament offers.
fn offered(m: &Memory, parliament: usize) -> GResult<Vec<usize>> {
    let issues = issues(m)?;
    let (data, count) =
        (m.ptr(parliament + OFFER + containers::ARRAY_DATA)?, containers::bounded(m.i32(parliament + OFFER + containers::ARRAY_COUNT)?, containers::MOST)?);
    (0..count)
        .map(|i| match issues.get(m.i32(data + i * 4)? as usize) {
            Some(&issue) => Ok(issue),
            None => Err("an issue on offer not in the database".to_string().into()),
        })
        .collect()
}

fn choice(m: &Memory, parliament: usize, player: Tag, issue: usize) -> GResult<Choice> {
    let allow = m.ptr(issue + ISSUE_ALLOW)?;
    let mut reasons = Vec::new();
    if allow != 0 && !conditions::holds(m, allow, player, 0)? {
        // Conditions that do not hold refuse the issue even when the
        // tooltip lists no line for them.
        reasons = conditions::failing(m, allow, player, 0)?;
        if reasons.is_empty() {
            reasons.push("the issue's conditions do not hold".to_string());
        }
    }
    if m.ptr(parliament + DEBATE)? != 0 {
        reasons.push("a debate is going on".to_string());
    }
    Ok(if reasons.is_empty() { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// The player's parliament as the parliament view shows it.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Parliament> {
    let parliament = m.ptr(ctx.country + PARLIAMENT)?;
    if parliament == 0 {
        return Ok(Parliament { parliament: false, debating: None, enacted: None, issues: Vec::new() });
    }
    let enacted = m.ptr(parliament + ENACTED)?;
    let mut issues = Vec::new();
    for issue in offered(m, parliament)? {
        let key = game_string::key(m, issue + ISSUE_KEY)?;
        issues.push(Issue { name: content::name(m, &key)?, issue: key, choice: choice(m, parliament, ctx.player, issue)? });
    }
    Ok(Parliament {
        parliament: true,
        debating: debating(m, parliament)?,
        enacted: if enacted == 0 { None } else { Some(game_string::key(m, enacted + ISSUE_KEY)?) },
        issues,
    })
}

/// The issue being debated: the debate keeps the issue as the parliament
/// keeps the one enacted.
fn debating(m: &Memory, parliament: usize) -> GResult<Option<String>> {
    let debate = m.ptr(parliament + DEBATE)?;
    if debate == 0 {
        return Ok(None);
    }
    let issue = m.ptr(debate + ISSUE_KEY)?;
    Ok(if issues(m)?.contains(&issue) { Some(game_string::key(m, issue + ISSUE_KEY)?) } else { None })
}

fn written_debate(country: Tag, issue: &str) -> String {
    format!("start_debate_command={{ country=\"{}\" which=\"{issue}\" }}", content::letters(country))
}

fn written_seat(command: &str, country: Tag, province: u32) -> String {
    format!("{command}={{ country=\"{}\" province={province} }}", content::letters(country))
}

/// Brings an issue on offer to debate.
pub fn debate(Session { m, ctx, commands, .. }: Session, args::Debate { issue: key }: args::Debate) -> GResult<()> {
    let parliament = m.ptr(ctx.country + PARLIAMENT)?;
    if parliament == 0 {
        return Err(checks::not_now("the player has no parliament"));
    }
    for issue in offered(m, parliament)? {
        if game_string::key(m, issue + ISSUE_KEY)? != key {
            continue;
        }
        let choice = choice(m, parliament, ctx.player, issue)?;
        if !choice.can {
            return Err(action::not_allowed("debate this issue", &choice.reasons));
        }
        return action::post(commands, &written_debate(ctx.player, &key), "debate this issue");
    }
    Err(checks::not_found("parliament issue on offer", &key))
}

/// Gives a province of the player's a seat in parliament.
pub fn add_seat(session: Session, args::AddSeat { province: ProvinceId(province) }: args::AddSeat) -> GResult<()> {
    seat(session, "add_to_parliament_command", province, "give this province a seat")
}

/// Takes a province's seat in parliament away.
pub fn remove_seat(session: Session, args::RemoveSeat { province: ProvinceId(province) }: args::RemoveSeat) -> GResult<()> {
    seat(session, "remove_from_parliament_command", province, "take this province's seat away")
}

fn seat(Session { m, ctx, commands, .. }: Session, command: &str, province: u32, what: &str) -> GResult<()> {
    if m.ptr(ctx.country + PARLIAMENT)? == 0 {
        return Err(checks::not_now("the player has no parliament"));
    }
    action::post(commands, &written_seat(command, ctx.player, province), what)
}

crate::catalogue::bind! {
    eu4_harmony_remote::parliament: ParliamentRequest => ParliamentAnswer;
    Options => options;
    Debate => debate;
    AddSeat => add_seat;
    RemoveSeat => remove_seat;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_name_the_country_and_the_issue_or_province() {
        let gbr = Tag(u64::from_le_bytes(*b"GBR\0\0\0\0\0"));
        assert_eq!(written_debate(gbr, "restore_integration"), r#"start_debate_command={ country="GBR" which="restore_integration" }"#);
        assert_eq!(written_seat("add_to_parliament_command", gbr, 942), r#"add_to_parliament_command={ country="GBR" province=942 }"#);
    }

    use crate::game::{
        buffers::{self, no, write_inline, Image, Object},
        commands::record,
        conditions::EVALUATE,
        Caller, Fail, Status,
    };

    /// A condition of the test's: whether it holds, and whether the game's
    /// tooltip lists a line for it when it does not.
    const HOLDS: usize = 0;
    const LISTED: usize = 8;

    extern "C" fn evaluate(trigger: usize) -> usize {
        unsafe { *((trigger + HOLDS) as *const u8) as usize }
    }

    extern "C" fn lines(trigger: usize, out: usize) -> usize {
        if unsafe { *((trigger + LISTED) as *const u8) } != 0 {
            write_inline(out, "no_war");
        }
        0
    }

    /// Great Britain, whose parliament offers the issues `free` (no
    /// conditions), `peace` (a condition that does not hold, listed as
    /// `no_war`) and `unlisted` (one that does not hold, which the tooltip
    /// lists no line for), of the database's four, `taxes` not on offer;
    /// a debate going on when `debating`. No parliament when `none`.
    struct Britain {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    fn britain(debating: bool, none: bool) -> Britain {
        let image = Image::new();
        image.function(EVALUATE.rva(), evaluate as *const ()).function(game_string::DESTROY_AT.rva(), no as *const ());
        buffers::conditions(&image, lines as *const ());
        let (peace, unlisted) = (Object::new(16), Object::new(16).u8(LISTED, 0));
        let peace = peace.u8(LISTED, 1);
        let issues = Object::new(4 * ISSUE)
            .string(ISSUE_KEY, "free")
            .string(ISSUE + ISSUE_KEY, "peace")
            .u64(ISSUE + ISSUE_ALLOW, peace.at() as u64)
            .string(2 * ISSUE + ISSUE_KEY, "unlisted")
            .u64(2 * ISSUE + ISSUE_ALLOW, unlisted.at() as u64)
            .string(3 * ISSUE + ISSUE_KEY, "taxes");
        image.u64(DATABASE + containers::ARRAY_DATA, issues.at() as u64).i32(DATABASE + containers::ARRAY_COUNT, 4);
        let offer = Object::new(12).i32(0, 0).i32(4, 1).i32(8, 2);
        let debate = Object::new(ISSUE_KEY + 8).u64(ISSUE_KEY, issues.at() as u64);
        let parliament = Object::new(ENACTED + 8)
            .u64(OFFER + containers::ARRAY_DATA, offer.at() as u64)
            .i32(OFFER + containers::ARRAY_COUNT, 3)
            .u64(DEBATE, if debating { debate.at() as u64 } else { 0 });
        let gbr = Tag(u64::from_le_bytes(*b"GBR\0\0\0\0\0"));
        let country = Object::country(gbr, PARLIAMENT + 8).u64(PARLIAMENT, if none { 0 } else { parliament.at() as u64 });
        let countries = Object::countries(&[&country]);
        let session = buffers::session(image.memory(), 0, gbr, country.at(), countries.at(), Caller::Remote);
        Britain { _image: image, _objects: vec![peace, unlisted, issues, offer, debate, parliament, country, countries], session }
    }

    fn debating(issue: &str, britain: Britain) -> GResult<()> {
        debate(britain.session, args::Debate { issue: issue.into() })
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the parliament acted") };
        (status, message)
    }

    #[test]
    fn an_issue_on_offer_is_debated_and_a_province_given_a_seat() {
        debating("free", britain(false, false)).unwrap();
        add_seat(britain(false, false).session, args::AddSeat { province: ProvinceId(942) }).unwrap();
        remove_seat(britain(false, false).session, args::RemoveSeat { province: ProvinceId(942) }).unwrap();
        assert_eq!(
            record::posted(),
            [
                r#"start_debate_command={ country="GBR" which="free" }"#,
                r#"add_to_parliament_command={ country="GBR" province=942 }"#,
                r#"remove_from_parliament_command={ country="GBR" province=942 }"#
            ]
        );
    }

    #[test]
    fn an_issue_not_on_offer_or_not_allowed_now_is_refused_with_why() {
        let rejected = |why: &str| (Status::Rejected, why.to_string());
        assert_eq!(refused(debating("peace", britain(false, false))), rejected("no_war"));
        assert_eq!(refused(debating("unlisted", britain(false, false))), rejected("the issue's conditions do not hold"));
        assert_eq!(refused(debating("free", britain(true, false))), rejected("a debate is going on"));
        assert_eq!(refused(debating("taxes", britain(false, false))), (Status::NotFound, r#"no parliament issue on offer "taxes" for the player"#.into()));
        assert_eq!(refused(debating("free", britain(false, true))), rejected("the player has no parliament"));
        assert_eq!(refused(add_seat(britain(false, true).session, args::AddSeat { province: ProvinceId(942) })), rejected("the player has no parliament"));
        assert!(record::posted().is_empty());
    }
}
