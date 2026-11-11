//! The Holy Roman Empire's view: its reforms, each with whether the player
//! can pass it as the game's own check says, and passing and revoking them
//! through the game's own commands; and what the Celestial Empire's view
//! shares with it, an empire's reforms (docs/research/empires.md).
use super::{action, conditions, content, envoys, questions, script, thousandths, Ctx, GResult, Session, Tag};
use crate::{
    containers, game_string,
    memory::Memory,
    objects::campaign::event_option,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::empire::{args, EmpireReforms, ImperialIncident, ImperialReform, IncidentOption};
use eu4_harmony_remote::{CountryTag, Date};

/// The Holy Roman Empire and the Celestial Empire, in the world itself.
pub(super) const HRE: usize = 0x15d0;
pub(super) const CELESTIAL: usize = 0x1b40;

/// An empire's fields, the Holy Roman Empire's and the Celestial
/// Empire's alike.
pub(super) mod field {
    /// Its emperor's tag.
    pub const EMPEROR: usize = 0x10;
    /// Its imperial authority, or the Celestial Empire's mandate, i32
    /// thousandths.
    pub const VALUE: usize = 0x18;
    /// The reforms it has passed, in order, a game array of pointers.
    pub const PASSED: usize = 0x20;
    /// Whether this emperor has revoked a reform (the Holy Roman Empire's).
    pub const REVOKED: usize = 0x504;
}

/// The imperial reforms' database: at `+0x10` the reforms in load order, a
/// `std::vector` of pointers.
const REFORMS: usize = 0x242bcd0;
const ORDER: usize = 0x10;
/// An imperial reform's fields.
pub(super) mod reform {
    /// Its key.
    pub const KEY: usize = 0x10;
    /// The content trigger for whether the empire's view offers it.
    pub const POTENTIAL: usize = 0xae8;
    /// The token of the empire it is a reform of (`empire = hre`).
    pub const EMPIRE: usize = 0xbdc;
    /// The part of the view it is in (`gui_container`), a `std::string`.
    pub const CONTAINER: usize = 0xc28;
}
/// The empires' tokens, as a reform names its empire.
pub(super) const HRE_TOKEN: u32 = 0x2749;
pub(super) const CELESTIAL_TOKEN: u32 = 0x35b7;

/// `(reform, out) -> out`: the name the empire's view gives the reform.
const REFORM_NAME: GameFunction<2> = GameFunction::at(0x800300);
/// `(empire, tag, reform, reasons or null) -> bool`: whether the country
/// may pass the reform now, writing the view's reasons why not (the
/// empire's own check, its vtable's `+0x60`): the Holy Roman Empire's.
const MAY_PASS: GameFunction<4> = GameFunction::at(0x706890);
/// `(empire, reasons or null) -> bool`: whether the emperor may revoke the
/// last reform passed now.
const MAY_REVOKE: GameFunction<2> = GameFunction::at(0x7061e0);
/// The least imperial authority a reform is passed at
/// (`IMPERIAL_REFORM_COST`), an i32 of whole points: passing one takes all
/// the authority there is. What revoking one costs, an i32 of thousandths.
const REFORM_COST: usize = 0x233dad8;
const REVOKE_COST: usize = 0x233fb10;
/// How many reforms are read at most.
const MOST: usize = 10_000;

/// The Holy Roman Empire's membership and its incident, in the world.
mod world_field {
    /// Nonzero once the Empire is gone, u8.
    pub const GONE: usize = 0x1620;
    /// Whether each country is a member: a pointer to bytes, one for each
    /// country by its [`super::MEMBER_INDEX`].
    pub const MEMBERS: usize = 0x1648;
    /// How many princes the Empire has, i32, as the incident's options count
    /// their votes of them.
    pub const PRINCES: usize = 0x162c;
    /// The imperial incident in progress, a pointer to an [`super::incident`];
    /// null with none.
    pub const INCIDENT: usize = 0x1b18;
}
/// A country's index into [`world_field::MEMBERS`], i32.
const MEMBER_INDEX: usize = 0x14b8;
/// The imperial incident in progress.
mod incident {
    /// Its definition, a pointer to a [`super::definition`].
    pub const DEFINITION: usize = 0x0;
    /// The scope its options are asked in, a scope of the game's own.
    pub const SCOPE: usize = 0x8;
    /// The option the emperor favors, i32.
    pub const FAVORED: usize = 0xd8;
    /// The day the emperor's choice comes into effect, i32, as the game
    /// keeps dates.
    pub const ENDS: usize = 0xe0;
    /// The members' picks, a game array of 12-byte entries: the member's tag
    /// (u64) and the option it supports (i32, at [`VOTE_OPTION`]).
    pub const VOTES: usize = 0xe8;
    pub const VOTE_SIZE: usize = 0xc;
    pub const VOTE_OPTION: usize = 0x8;
}
/// An imperial incident as its file defines it.
mod definition {
    /// Its key, a `std::string`, which is also the localisation key of its
    /// name.
    pub const KEY: usize = 0x8;
    /// The id of its event, a `std::string`: the event's options are the
    /// incident's.
    pub const EVENT: usize = 0x28;
}
/// How many votes are read at most.
const MOST_VOTES: usize = 1_000;

pub fn points() -> Vec<Point> {
    vec![
        // The database's maker, which keeps it in the global.
        Point::keep(0x80144ausize, "48 8b 05 7f a8 c2 01", "imperial reforms database").uses(&[REFORMS]),
        // The empire's reforms walk the database's vector, keeping those
        // of its token.
        Point::keep(0x701baeusize, "48 8b 50 18 4c 8b 40 10", "imperial reforms in order").uses(&[ORDER]),
        Point::keep(0x701be8usize, "39 87 dc 0b 00 00", "reform's empire").uses(&[reform::EMPIRE]),
        // The enact command's check picks the empire by the reform's token.
        Point::keep(0x7f3d5dusize, "41 8b 80 dc 0b 00 00 3d 49 27 00 00", "reform of the Holy Roman Empire"),
        Point::keep(0x7f3d7busize, "3d b7 35 00 00", "reform of the Celestial Empire"),
        Point::keep(0x7f3d72usize, "48 81 c1 d0 15 00 00", "the Holy Roman Empire in the world").uses(&[HRE]),
        Point::keep(0x7f3d89usize, "48 81 c1 40 1b 00 00", "the Celestial Empire in the world").uses(&[CELESTIAL]),
        Point::keep(0x7f3d9busize, "48 8b 51 10", "emperor").uses(&[field::EMPEROR]),
        // The Holy Roman Empire's check reads the passed reforms and the
        // authority, and asks the cost.
        Point::keep(0x706a47usize, "49 63 45 34", "passed reforms count").uses(&[field::PASSED + containers::ARRAY_COUNT]),
        Point::keep(0x706a58usize, "49 8b 45 28", "passed reforms").uses(&[field::PASSED + containers::ARRAY_DATA]),
        Point::keep(0x706f88usize, "45 8b 45 18", "imperial authority").uses(&[field::VALUE]),
        Point::keep(0x706f60usize, "8b 0d 72 6b c3 01", "imperial reform cost").uses(&[REFORM_COST]),
        Point::keep(0x706368usize, "44 8b 05 a1 97 c3 01", "revoke cost").uses(&[REVOKE_COST]),
        Point::keep(0x7062c9usize, "80 b9 04 05 00 00 00", "reform revoked").uses(&[field::REVOKED]),
        // The Celestial Empire's check asks the reform's potential, and the
        // Empire's view sorts reforms by their container.
        Point::keep(0x7135afusize, "48 8d 8b e8 0a 00 00", "reform potential").uses(&[reform::POTENTIAL]),
        Point::keep(0x115ece3usize, "49 8d 8e 28 0c 00 00", "reform container").uses(&[reform::CONTAINER]),
        Point::keep(0x7f3b75usize, "48 8b 5b 50", "enact command's reform"),
        Point::keep(0x7f3b81usize, "48 83 c3 10", "reform key").uses(&[reform::KEY]),
        Point::keep(REFORM_NAME, "48 89 5c 24 08 57 48 83 ec 50 48 8b da 48 8b f9", "reform name"),
        Point::keep(MAY_PASS, "48 89 5c 24 08 4c 89 4c 24 20 4c 89 44 24 18 55 56 57 41 54 41 55", "may pass an imperial reform"),
        Point::keep(MAY_REVOKE, "48 89 5c 24 08 55 48 8b ec 48 81 ec 80 00 00 00 48 8b da", "may revoke an imperial reform"),
        // The pick command's check: the Empire standing, the member's byte,
        // the incident, its event's id, its options and the scope they are
        // asked in.
        Point::keep(0xc19d0usize, "80 ba 20 16 00 00 00", "the Empire gone").uses(&[world_field::GONE]),
        Point::keep(0xc19f7usize, "48 63 81 b8 14 00 00", "a country's member index").uses(&[MEMBER_INDEX]),
        Point::keep(0xc19feusize, "48 03 82 48 16 00 00", "the Empire's members").uses(&[world_field::MEMBERS]),
        Point::keep(0xc1a1fusize, "48 8b 9a 18 1b 00 00", "imperial incident").uses(&[world_field::INCIDENT]),
        Point::keep(0xc1a2fusize, "48 8b 0b 48 85 c9", "incident's definition"),
        Point::keep(0xc1a3busize, "48 83 c1 28", "incident's event").uses(&[definition::EVENT]),
        Point::keep(0xc1a6eusize, "48 8d 53 08", "incident's scope").uses(&[incident::SCOPE]),
        // The view's option row: the emperor's favored option, the members'
        // picks, and the princes they are counted of; and the title's day.
        Point::keep(0x1162bd0usize, "8b 8d d8 00 00 00", "incident's favored option").uses(&[incident::FAVORED]),
        Point::keep(0x1162be9usize, "48 8b 8d f0 00 00 00", "incident's votes").uses(&[incident::VOTES + containers::ARRAY_DATA]),
        Point::keep(0x1162bf0usize, "48 63 95 fc 00 00 00", "incident's votes count").uses(&[incident::VOTES + containers::ARRAY_COUNT]),
        Point::keep(0x1162cd3usize, "39 4a 08", "a vote's option").uses(&[incident::VOTE_OPTION]),
        Point::keep(0x1162ce4usize, "8b 9b 2c 16 00 00", "the Empire's princes").uses(&[world_field::PRINCES]),
        Point::keep(0x1161167usize, "49 8d 89 e0 00 00 00", "incident's end").uses(&[incident::ENDS]),
        Point::keep(0x70d29eusize, "48 8d 51 08", "incident's key").uses(&[definition::KEY]),
    ]
}

/// The reforms of the empire of `token` whose conditions for being offered
/// hold for `player`, in load order.
pub(super) fn offered(m: &Memory, player: Tag, token: u32) -> GResult<Vec<usize>> {
    let database = m.ptr(m.at(REFORMS))?;
    if database == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for reform in containers::pointers(m, database + ORDER, MOST)? {
        if m.u32(reform + reform::EMPIRE)? == token && conditions::holds(m, reform + reform::POTENTIAL, player, 0)? {
            out.push(reform);
        }
    }
    Ok(out)
}

/// The reforms the empire at `empire` has passed.
pub(super) fn passed(m: &Memory, empire: usize) -> GResult<Vec<usize>> {
    Ok(containers::array(m, empire + field::PASSED, MOST)?)
}

/// The empire's emperor, when it has one.
pub(super) fn emperor(m: &Memory, empire: usize) -> GResult<Option<CountryTag>> {
    let tag = Tag(m.u64(empire + field::EMPEROR)?);
    Ok(tag.normal().then(|| CountryTag(tag.letters())))
}

/// A reform as the view lists it, with whether the player may pass it as
/// `may_pass` says, at `cost`.
pub(super) fn listed(m: &Memory, ctx: &Ctx, empire: usize, at: usize, passed: &[usize], may_pass: GameFunction<4>, cost: Vec<Amount>) -> GResult<ImperialReform> {
    let reform = game_string::key(m, at + reform::KEY)?;
    let (_, name) = game_string::written(m, |out| REFORM_NAME.call(m, [at, out]))?;
    let container = game_string::key(m, at + reform::CONTAINER)?;
    let is_passed = passed.contains(&at);
    let choice = if is_passed {
        Choice::cannot(Vec::new(), cost)
    } else {
        let (can, reasons) = content::checked(m, |reasons| Ok(may_pass.call(m, [empire, ctx.player.0 as usize, at, reasons])?))?;
        if can {
            Choice::can(cost)
        } else {
            Choice::cannot(reasons, cost)
        }
    };
    Ok(ImperialReform {
        reform,
        name: crate::unicode::game_text::plain(&name),
        branch: (!container.is_empty()).then_some(container),
        passed: is_passed,
        choice,
    })
}

/// Passes a reform of the empire of `token` the view offers, as its button
/// does, through the game's own command.
pub(super) fn pass(Session { m, ctx, commands, .. }: Session, empire: usize, token: u32, may_pass: GameFunction<4>, key: &str) -> GResult<()> {
    let found = offered(m, ctx.player, token)?.into_iter().map(|at| Ok((game_string::key(m, at + reform::KEY)?, at))).collect::<GResult<Vec<_>>>()?;
    let Some(&(_, at)) = found.iter().find(|(k, _)| k == key) else {
        return Err(content::checks::not_found("reform", key));
    };
    let (can, reasons) = content::checked(m, |reasons| Ok(may_pass.call(m, [empire, ctx.player.0 as usize, at, reasons])?))?;
    if !can || passed(m, empire)?.contains(&at) {
        return Err(action::not_allowed("pass this reform", &reasons));
    }
    action::post(commands, &written_pass(key), "pass this reform")
}

/// The enact command: the reform by its key. The command names no country:
/// the game enacts it as the empire's emperor.
fn written_pass(reform: &str) -> String {
    format!("enact_imperial_reform={{ which=\"{reform}\" }}")
}

/// The revoke command: the emperor's tag.
fn written_revoke(emperor: Tag) -> String {
    format!("revoke_imperial_reform={{ members={{ dummy=\"{}\" }} }}", content::letters(emperor))
}

/// The Empire as its view shows it.
pub fn reforms(Session { m, ctx, .. }: Session) -> GResult<EmpireReforms> {
    let hre = ctx.world + HRE;
    // Passing a reform takes all the authority, which is at least the
    // least it may be passed at.
    let least = m.i32(m.at(REFORM_COST))?.saturating_mul(1000);
    let cost = vec![Amount::new(Resource::ImperialAuthority, thousandths(m.i32(hre + field::VALUE)?.max(least)))];
    let passed = passed(m, hre)?;
    let reforms = offered(m, ctx.player, HRE_TOKEN)?.into_iter().map(|at| listed(m, &ctx, hre, at, &passed, MAY_PASS, cost.clone())).collect::<GResult<_>>()?;
    Ok(EmpireReforms { emperor: emperor(m, hre)?, authority: thousandths(m.i32(hre + field::VALUE)?), reforms, revoke: revoke_choice(m, &ctx)? })
}

/// Whether the player may revoke the last reform passed now.
fn revoke_choice(m: &Memory, ctx: &Ctx) -> GResult<Choice> {
    let hre = ctx.world + HRE;
    let cost = vec![Amount::new(Resource::ImperialAuthority, thousandths(m.i32(m.at(REVOKE_COST))?))];
    if Tag(m.u64(hre + field::EMPEROR)?).index() != ctx.player.index() {
        return Ok(Choice::cannot(vec![checks::NOT_EMPEROR.to_string()], cost));
    }
    let (can, reasons) = content::checked(m, |reasons| Ok(MAY_REVOKE.call(m, [hre, reasons])?))?;
    Ok(if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// Passes one of the Empire's reforms.
pub fn pass_reform(session: Session, args::PassReform { reform }: args::PassReform) -> GResult<()> {
    let hre = session.ctx.world + HRE;
    pass(session, hre, HRE_TOKEN, MAY_PASS, &reform)
}

/// Revokes the last reform the Empire passed.
pub fn revoke_reform(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    let choice = revoke_choice(m, &ctx)?;
    if !choice.can {
        return Err(action::not_allowed("revoke a reform", &choice.reasons));
    }
    action::post(commands, &written_revoke(ctx.player), "revoke a reform")
}

/// The imperial incident in progress, read as the view reads it.
struct Incident {
    at: usize,
    definition: usize,
    event: usize,
}

/// The incident in progress and its event, when there is one and the
/// Empire stands.
fn in_progress(m: &Memory, ctx: &Ctx) -> GResult<Option<Incident>> {
    if m.u8(ctx.world + world_field::GONE)? != 0 {
        return Ok(None);
    }
    let at = m.ptr(ctx.world + world_field::INCIDENT)?;
    if at == 0 {
        return Ok(None);
    }
    let definition = m.ptr(at + incident::DEFINITION)?;
    if definition == 0 {
        return Ok(None);
    }
    let id = game_string::read(m, definition + definition::EVENT)?;
    Ok(script::find_event(m, &id)?.map(|event| Incident { at, definition, event }))
}

/// Whether the player is the emperor, and whether a member: who may pick an
/// incident's option, as the pick command's check asks.
fn standing(m: &Memory, ctx: &Ctx) -> GResult<(bool, bool)> {
    let emperor = Tag(m.u64(ctx.world + HRE + field::EMPEROR)?).index() == ctx.player.index();
    let members = m.ptr(ctx.world + world_field::MEMBERS)?;
    let index = m.i32(ctx.country + MEMBER_INDEX)?;
    let member = members != 0 && index >= 0 && m.u8(members + index as usize)? != 0;
    Ok((emperor, member))
}

/// The members' picks: each member's tag and the option it supports.
fn votes(m: &Memory, at: usize) -> GResult<Vec<(Tag, i32)>> {
    let data = m.ptr(at + incident::VOTES + containers::ARRAY_DATA)?;
    let count = m.i32(at + incident::VOTES + containers::ARRAY_COUNT)?;
    if data == 0 || count <= 0 {
        return Ok(Vec::new());
    }
    let count = (count as usize).min(MOST_VOTES);
    (0..count)
        .map(|i| {
            let entry = data + i * incident::VOTE_SIZE;
            Ok((Tag(m.u64(entry)?), m.i32(entry + incident::VOTE_OPTION)?))
        })
        .collect()
}

/// The option the player has picked: the one the emperor favors, or the
/// one the player supports; none before a pick.
fn picked(m: &Memory, ctx: &Ctx, at: usize, emperor: bool) -> GResult<Option<i32>> {
    if emperor {
        return Ok(Some(m.i32(at + incident::FAVORED)?));
    }
    Ok(votes(m, at)?.into_iter().find(|(tag, _)| tag.index() == ctx.player.index()).map(|(_, option)| option))
}

/// Whether the player may pick each of the incident's options, as the pick
/// command's check asks: the player must be the emperor or a member, and
/// the option's conditions must hold in the incident's scope.
fn choices(m: &Memory, ctx: &Ctx, incident: &Incident) -> GResult<Vec<(usize, Choice)>> {
    let (emperor, member) = standing(m, ctx)?;
    let mut scope = conditions::Scope::copy(m, incident.at + incident::SCOPE)?;
    questions::options(m, incident.event)?
        .into_iter()
        .map(|option| {
            let choice = if !emperor && !member {
                Choice::cannot(vec![checks::NOT_A_MEMBER.to_string()], Vec::new())
            } else {
                let (can, reasons) = scope.check(option + event_option::TRIGGER)?;
                if can {
                    Choice::can(Vec::new())
                } else {
                    Choice::cannot(reasons, Vec::new())
                }
            };
            Ok((option, choice))
        })
        .collect()
}

/// The imperial incident in progress, as the Empire's view shows it.
pub fn incident(Session { m, ctx, .. }: Session) -> GResult<ImperialIncident> {
    let princes = m.i32(ctx.world + world_field::PRINCES)?.max(0) as u32;
    let Some(found) = in_progress(m, &ctx)? else {
        return Ok(ImperialIncident { incident: None, name: String::new(), event: None, ends: None, princes, options: Vec::new() });
    };
    let key = game_string::key(m, found.definition + definition::KEY)?;
    let (emperor, _) = standing(m, &ctx)?;
    let favored = m.i32(found.at + incident::FAVORED)?;
    let mine = picked(m, &ctx, found.at, emperor)?;
    let votes = votes(m, found.at)?;
    let mut scope = conditions::Scope::copy(m, found.at + incident::SCOPE)?;
    let options = choices(m, &ctx, &found)?
        .into_iter()
        .enumerate()
        .map(|(index, (option, choice))| {
            let index = index as i32;
            Ok(IncidentOption {
                option: index as u32,
                name: questions::localize(m, scope.address(), &game_string::read(m, option + event_option::NAME)?)?,
                votes: votes.iter().filter(|(_, o)| *o == index).count() as u32,
                favored: favored == index,
                picked: mine == Some(index),
                choice,
            })
        })
        .collect::<GResult<_>>()?;
    Ok(ImperialIncident {
        name: envoys::localized(m, &key)?,
        incident: Some(key),
        event: Some(game_string::key(m, found.definition + definition::EVENT)?),
        ends: Some(Date(m.i32(found.at + incident::ENDS)?)),
        princes,
        options,
    })
}

/// Picks one of the incident's options, as its button does.
pub fn pick_incident_option(Session { m, ctx, commands, .. }: Session, args::PickIncidentOption { option }: args::PickIncidentOption) -> GResult<()> {
    let Some(found) = in_progress(m, &ctx)? else {
        return Err(checks::no_incident());
    };
    let choices = choices(m, &ctx, &found)?;
    let Some((_, choice)) = choices.get(option as usize) else {
        return Err(content::checks::not_found("incident option", &option.to_string()));
    };
    if !choice.can {
        return Err(action::not_allowed("pick this option", &choice.reasons));
    }
    action::post(commands, &written_pick(ctx.player, option), "pick this option")
}

/// The pick command: the member's tag and the option's number, in that
/// order, both under `dummy`, as its writer (`0xc88b0`) writes them.
fn written_pick(member: Tag, option: u32) -> String {
    format!("pick_imperial_incident_option={{ members={{ dummy=\"{}\" dummy={option} }} }}", content::letters(member))
}

/// The refusals the framework words itself.
pub mod checks {
    use crate::game::{Fail, Status};

    /// Revoking is the emperor's alone; the game's check does not say so,
    /// since its button is not shown to another.
    pub const NOT_EMPEROR: &str = "Only the emperor can revoke a reform.";
    /// Picking an incident's option is the emperor's and the members'; the
    /// view shows no buttons to another.
    pub const NOT_A_MEMBER: &str = "Only the emperor and the Empire's members can pick an option of an imperial incident.";

    /// No imperial incident is in progress.
    pub fn no_incident() -> Fail {
        Fail::new(Status::NotFound, "no imperial incident is in progress")
    }
}

crate::catalogue::bind! {
    eu4_harmony_remote::empire: EmpireRequest => EmpireAnswer;
    Reforms => reforms;
    PassReform => pass_reform;
    RevokeReform => revoke_reform;
    Incident => incident;
    PickIncidentOption => pick_incident_option;
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_commands_name_the_reform_and_the_emperor() {
        assert_eq!(super::written_pass("emperor_reichsreform"), r#"enact_imperial_reform={ which="emperor_reichsreform" }"#);
        let hab = super::Tag(u64::from_le_bytes(*b"HAB\0\0\0\0\0"));
        assert_eq!(super::written_revoke(hab), r#"revoke_imperial_reform={ members={ dummy="HAB" } }"#);
        assert_eq!(super::written_pick(hab, 2), r#"pick_imperial_incident_option={ members={ dummy="HAB" dummy=2 } }"#);
    }
}
