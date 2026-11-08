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

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_country_and_the_mission() {
        let fra = super::Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(super::written(fra, "fra_unite_the_kingdom"), r#"complete_mission={ members={ dummy="FRA" dummy="fra_unite_the_kingdom" } }"#);
    }
}
