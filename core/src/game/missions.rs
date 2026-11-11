//! Missions: the player's missions as the missions view shows them, and
//! completing one, through the game's own command
//! (docs/research/government.md, "Missions").
use super::content::checks;
use super::{action, conditions, content, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::{
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::missions::args;
use eu4_harmony_remote::{choices::Choice, missions::Mission};

/// The country's missions: a game array of columns from `+0x10`, 0x18-byte
/// items, each a game array of mission series, each series with a game
/// array of its missions.
const MISSIONS: usize = 0x99d8;
const COLUMNS: usize = 0x10;
const COLUMN: usize = 0x18;
const SERIES_MISSIONS: usize = 0x120;
/// A mission: its key and its conditions (`trigger`).
const KEY: usize = 0x10;
const TRIGGER: usize = 0x70;
/// `(country, mission) -> bool`, whether the country has completed it;
/// `(mission, tag) -> bool`, whether it can complete it now.
const COMPLETED: GameFunction<2> = GameFunction::at(0x1796c0);
const CAN_COMPLETE: GameFunction<2> = GameFunction::at(0x8d4fc0);

pub fn points() -> Vec<Point> {
    vec![
        // The country's writer hands the missions to their own writer, which
        // walks the columns and each column's missions by key.
        Point::keep(0x2e10ceusize, "4d 8d 87 d8 99 00 00", "country missions").uses(&[MISSIONS]),
        Point::keep(0x44554eusize, "4c 8b 71 18 48 8b fa 48 63 41 24", "mission columns"),
        Point::keep(0x445645usize, "49 8b 5e 08 49 63 46 14", "mission column"),
        // The series' reader adds each mission it reads to the series's
        // array.
        Point::keep(0x8d78a9usize, "48 8d b5 20 01 00 00", "series missions").uses(&[SERIES_MISSIONS]),
        Point::keep(COMPLETED, "4c 8b d1 44 0f b6 ca 49 bb b3 01 00 00 00 01 00 00", "mission completed"),
        Point::keep(CAN_COMPLETE, "48 89 5c 24 08", "mission check"),
    ]
}

/// The player's missions, column by column.
fn columns(m: &Memory, country: usize) -> GResult<Vec<Vec<usize>>> {
    let missions = country + MISSIONS + COLUMNS;
    let (data, count) = (m.ptr(missions + containers::ARRAY_DATA)?, containers::bounded(m.i32(missions + containers::ARRAY_COUNT)?, containers::MOST)?);
    (0..count)
        .map(|i| {
            let mut missions = Vec::new();
            for series in containers::array(m, data + i * COLUMN, containers::MOST)? {
                missions.extend(containers::array(m, series + SERIES_MISSIONS, containers::MOST)?);
            }
            Ok(missions)
        })
        .collect()
}

fn completed(m: &Memory, country: usize, mission: usize) -> GResult<bool> {
    Ok(COMPLETED.call(m, [country, mission])? & 0xff != 0)
}

/// Whether the player can complete `mission`, and why not: its conditions
/// that do not hold, or else that the missions before it are not all
/// completed, the check's only other condition.
fn choice(m: &Memory, player: Tag, mission: usize, done: bool) -> GResult<Choice> {
    if done {
        return Ok(Choice::cannot(vec!["the mission is completed".to_string()], Vec::new()));
    }
    if CAN_COMPLETE.call(m, [mission, player.0 as usize])? & 0xff != 0 {
        return Ok(Choice::can(Vec::new()));
    }
    let mut reasons = conditions::failing(m, mission + TRIGGER, player, 0)?;
    if reasons.is_empty() {
        reasons.push("the missions before it are not all completed".to_string());
    }
    Ok(Choice::cannot(reasons, Vec::new()))
}

/// The player's missions as the missions view shows them.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<Mission>> {
    let columns = columns(m, ctx.country)?;
    let mut listed = Vec::new();
    for (column, missions) in columns.iter().enumerate() {
        for &mission in missions {
            let key = game_string::key(m, mission + KEY)?;
            let done = completed(m, ctx.country, mission)?;
            listed.push(Mission {
                name: content::name(m, &format!("{key}_title"))?,
                mission: key,
                column: column as u32,
                completed: done,
                choice: choice(m, ctx.player, mission, done)?,
            });
        }
    }
    Ok(listed)
}

fn written(country: Tag, mission: &str) -> String {
    format!("complete_mission={{ members={{ dummy=\"{}\" dummy=\"{mission}\" }} }}", content::letters(country))
}

/// Completes one of the player's missions.
pub fn complete(Session { m, ctx, commands, .. }: Session, args::Complete { mission: key }: args::Complete) -> GResult<()> {
    let all: Vec<usize> = columns(m, ctx.country)?.into_iter().flatten().collect();
    for &mission in &all {
        if game_string::key(m, mission + KEY)? != key {
            continue;
        }
        let choice = choice(m, ctx.player, mission, completed(m, ctx.country, mission)?)?;
        if !choice.can {
            return Err(action::not_allowed("complete this mission", &choice.reasons));
        }
        return action::post(commands, &written(ctx.player, &key), "complete this mission");
    }
    Err(checks::not_found("mission", &key))
}

crate::catalogue::bind! {
    eu4_harmony_remote::missions: MissionRequest => MissionAnswer;
    List => list;
    Complete => complete;
}

#[cfg(test)]
mod tests {
    use crate::game::As;
    #[test]
    fn the_command_names_the_country_and_the_mission() {
        let fra = super::Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(super::written(fra, "fra_unite_the_kingdom"), r#"complete_mission={ members={ dummy="FRA" dummy="fra_unite_the_kingdom" } }"#);
    }

    use super::*;
    use crate::game::{
        buffers::{self, no, write_inline, Image, Object},
        commands::record,
        Caller, Fail, Status,
    };

    /// A mission of the test's: whether it is completed, and whether it can
    /// be, in two bytes the game's functions below read.
    const DONE: usize = 8;
    const CAN: usize = 9;

    extern "C" fn is_completed(_: usize, mission: usize) -> usize {
        unsafe { *((mission + DONE) as *const u8) as usize }
    }

    extern "C" fn can_complete(mission: usize, _: usize) -> usize {
        unsafe { *((mission + CAN) as *const u8) as usize }
    }

    /// The conditions' lines: `at_war` does not hold for the mission whose
    /// trigger is not null, none for the others.
    extern "C" fn lines(trigger: usize, out: usize) -> usize {
        if unsafe { *(trigger as *const u64) } != 0 {
            write_inline(out, "at_war");
        }
        0
    }

    /// France's missions: in one column, a series of `done` (completed),
    /// `ready` (can be completed), `at_war` (one of its conditions does not
    /// hold) and `after` (it waits for the missions before it).
    struct France {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    fn france() -> France {
        let image = Image::new();
        image
            .function(COMPLETED.rva(), is_completed as *const ())
            .function(CAN_COMPLETE.rva(), can_complete as *const ())
            .function(game_string::DESTROY_AT.rva(), no as *const ());
        buffers::conditions(&image, lines as *const ());
        let mission = |key: &str, done: bool, can: bool, failing: bool| {
            Object::new(TRIGGER + 8).string(KEY, key).u8(DONE, done.into()).u8(CAN, can.into()).u64(TRIGGER, failing.into())
        };
        let missions = [
            mission("done", true, false, false),
            mission("ready", false, true, false),
            mission("at_war", false, false, true),
            mission("after", false, false, false),
        ];
        let addresses: Vec<u64> = missions.iter().map(|m| m.at() as u64).collect();
        let series = Object::new(SERIES_MISSIONS + 0x18)
            .vector(SERIES_MISSIONS + containers::ARRAY_DATA, &addresses)
            .i32(SERIES_MISSIONS + containers::ARRAY_COUNT, addresses.len() as i32);
        let column = Object::new(COLUMN).vector(containers::ARRAY_DATA, &[series.at() as u64]).i32(containers::ARRAY_COUNT, 1);
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        let at = MISSIONS + COLUMNS;
        let country = Object::country(fra, at + 0x18).u64(at + containers::ARRAY_DATA, column.at() as u64).i32(at + containers::ARRAY_COUNT, 1);
        let countries = Object::countries(&[&country]);
        let session = buffers::session(image.memory(), 0, fra, country.at(), countries.at(), Caller::Remote(As::Player));
        let mut objects = vec![series, column, country, countries];
        objects.extend(missions);
        France { _image: image, _objects: objects, session }
    }

    fn completing(key: &str) -> GResult<()> {
        complete(france().session, args::Complete { mission: key.into() })
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the mission was completed") };
        (status, message)
    }

    #[test]
    fn a_mission_the_player_can_complete_is_completed_by_its_key() {
        completing("ready").unwrap();
        assert_eq!(record::posted(), [r#"complete_mission={ members={ dummy="FRA" dummy="ready" } }"#]);
    }

    #[test]
    fn a_mission_the_player_cannot_complete_is_refused_with_why() {
        let rejected = |why: &str| (Status::Rejected, why.to_string());
        assert_eq!(refused(completing("done")), rejected("the mission is completed"));
        assert_eq!(refused(completing("at_war")), rejected("at_war"));
        assert_eq!(refused(completing("after")), rejected("the missions before it are not all completed"));
        assert_eq!(refused(completing("conquer_paris")), (Status::NotFound, r#"no mission "conquer_paris" for the player"#.into()));
        assert!(record::posted().is_empty());
    }
}
