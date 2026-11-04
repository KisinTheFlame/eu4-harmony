//! Missions: the player's missions as the missions view shows them, and
//! completing one, through the game's own command
//! (docs/research/government.md, "Missions").
use super::{conditions, content, context, game, Caller, GResult, Tag};
use crate::{game_string, memory::Memory, points::Point};
use eu4_harmony_checks::government as checks;
use eu4_harmony_remote::{choices::Choice, missions::Mission};

/// The country's missions: a game array of columns from `+0x10`, 0x18-byte
/// items, each a game array of missions.
const MISSIONS: usize = 0x99d8;
const COLUMNS: usize = 0x10;
const COLUMN: usize = 0x18;
/// A mission: its key, its conditions (`trigger`), and the missions before
/// it, a game array of their keys (`std::string`s).
const KEY: usize = 0x10;
const TRIGGER: usize = 0x70;
const REQUIRED: usize = 0x120;
const STRING: usize = 0x20;
/// `(country, mission) -> bool`, whether the country has completed it;
/// `(mission, tag) -> bool`, whether it can complete it now.
const COMPLETED: usize = 0x1796c0;
const CAN_COMPLETE: usize = 0x8d4fc0;

pub fn points() -> Vec<Point> {
    vec![
        // The country's writer hands the missions to their own writer, which
        // walks the columns and each column's missions by key.
        Point::keep(0x2e10ceusize, "4d 8d 87 d8 99 00 00", "country missions").uses(&[MISSIONS]),
        Point::keep(0x44554eusize, "4c 8b 71 18 48 8b fa 48 63 41 24", "mission columns"),
        Point::keep(0x445645usize, "49 8b 5e 08 49 63 46 14", "mission column"),
        Point::keep(0x8d4e6ausize, "48 8b 99 28 01 00 00", "missions required"),
        Point::keep(COMPLETED, "4c 8b d1 44 0f b6 ca 49 bb b3 01 00 00 00 01 00 00", "mission completed"),
        Point::keep(CAN_COMPLETE, "48 89 5c 24 08", "mission check"),
    ]
}

/// The player's missions, column by column.
fn columns(m: &Memory, country: usize) -> GResult<Vec<Vec<usize>>> {
    let missions = country + MISSIONS + COLUMNS;
    let (data, count) = (m.ptr(missions + content::ARRAY_DATA)?, content::bounded(m.i32(missions + content::ARRAY_COUNT)?)?);
    (0..count).map(|i| content::pointers(m, data + i * COLUMN)).collect()
}

fn completed(m: &Memory, country: usize, mission: usize) -> GResult<bool> {
    Ok(m.call(m.at(COMPLETED), &[country, mission])? & 0xff != 0)
}

/// Whether the player can complete `mission`, and why not: its conditions
/// that do not hold, the missions before it not completed.
fn choice(m: &Memory, country: usize, player: Tag, mission: usize, done: bool, all: &[usize]) -> GResult<Choice> {
    if done {
        return Ok(Choice::cannot(vec!["the mission is completed".to_string()], Vec::new()));
    }
    if m.call(m.at(CAN_COMPLETE), &[mission, player.0 as usize])? & 0xff != 0 {
        return Ok(Choice::can(Vec::new()));
    }
    let mut reasons = Vec::new();
    let required = m.ptr(mission + REQUIRED + content::ARRAY_DATA)?;
    for i in 0..content::bounded(m.i32(mission + REQUIRED + content::ARRAY_COUNT)?)? {
        let key = game_string::read(m, required + i * STRING)?;
        let before = all.iter().find(|&&other| game_string::read(m, other + KEY).is_ok_and(|k| k == key));
        if let Some(&before) = before {
            if !completed(m, country, before)? {
                let key = content::key(m, before + KEY)?;
                reasons.push(format!("{} is not completed", content::name(m, &format!("{key}_title"))?));
            }
        }
    }
    reasons.extend(conditions::failing(m, mission + TRIGGER, player, 0)?);
    Ok(Choice::cannot(reasons, Vec::new()))
}

/// The player's missions as the missions view shows them.
pub fn list(_: Caller) -> GResult<Vec<Mission>> {
    let m = game()?;
    let ctx = context(m)?;
    let columns = columns(m, ctx.country)?;
    let all: Vec<usize> = columns.iter().flatten().copied().collect();
    let mut listed = Vec::new();
    for (column, missions) in columns.iter().enumerate() {
        for &mission in missions {
            let key = content::key(m, mission + KEY)?;
            let done = completed(m, ctx.country, mission)?;
            listed.push(Mission {
                name: content::name(m, &format!("{key}_title"))?,
                mission: key,
                column: column as u32,
                completed: done,
                choice: choice(m, ctx.country, ctx.player, mission, done, &all)?,
            });
        }
    }
    Ok(listed)
}

fn written(country: Tag, mission: &str) -> String {
    format!("complete_mission={{ members={{ dummy=\"{}\" dummy=\"{mission}\" }} }}", content::letters(country))
}

/// Completes one of the player's missions.
pub fn complete(_: Caller, key: String) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let all: Vec<usize> = columns(m, ctx.country)?.into_iter().flatten().collect();
    for &mission in &all {
        if content::key(m, mission + KEY)? != key {
            continue;
        }
        let choice = choice(m, ctx.country, ctx.player, mission, completed(m, ctx.country, mission)?, &all)?;
        if !choice.can {
            return content::refused(&choice.reasons, "completing this mission");
        }
        return content::post(&written(ctx.player, &key), "completing this mission");
    }
    Err(checks::not_found("mission", &key).into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_country_and_the_mission() {
        let fra = super::Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(super::written(fra, "fra_unite_the_kingdom"), r#"complete_mission={ members={ dummy="FRA" dummy="fra_unite_the_kingdom" } }"#);
    }
}
