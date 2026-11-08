//! Games let run long, on the player build unless said (docs/plans/whole-game.md):
//! - `late`: each of the late game's saves loaded, observed, a year let run,
//!   the autosave written and loaded, a month more, and a return;
//! - `long`: a game in observe mode from 1444 to the end of the game, a
//!   year at a time, the autosave written and loaded every so many years,
//!   the game's memory kept under a bound;
//! - `checked`: the same for fewer years on the developer build with the
//!   test switches, every speed-up that can checking its answers against the
//!   game's, none of which may differ.
//!
//! Each records, for each stretch, how many game days a minute it ran and
//! the game's working set and private bytes after it.
use super::{game::days_between, logs::Mark, perf::Process, text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    menu::{MenuRequest, Save},
    session::Session,
    Date,
};
use serde_json::{json, Value};
use std::time::Instant;

pub struct Late;

pub struct Long {
    /// The speed-ups checked against the game: `checked`; or not: `long`.
    pub checked: bool,
}

pub const LONG: Long = Long { checked: false };
pub const CHECKED: Long = Long { checked: true };

/// The days a year is let run.
const YEAR: u32 = 365;

impl Scenario for Late {
    fn name(&self) -> &'static str {
        "late"
    }

    fn about(&self) -> &'static str {
        "the late game: each late save loaded and observed, a year let run, the autosave written and loaded, a month more"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        45
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                name: "saves",
                default: "observer-1700.1.1,observer-1760.7.1,observer-1780.7.1,observer-1800.7.1,observer-1820.7.1",
                about: "the saves of the late game, by commas, from the developers' saves folder",
            },
            Parameter { name: "days", default: "365", about: "the days each save is let run before its autosave" },
        ]
    }

    fn saves(&self, parameters: &Parameters) -> Vec<String> {
        listed(&parameters.text("saves"))
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let days = parameters.number("days")?;
        let there: Vec<Save> = game.ask(&MenuRequest::Saves {})?;
        let saves = listed(&parameters.text("saves"));
        for save in &saves {
            game.check(there.iter().any(|s| &s.name == save), format!("no save {save} on this computer"))?;
        }
        *record = json!({ "stretches": [] });
        for save in &saves {
            let loaded: Session = game.ask(&MenuRequest::Load { save: save.clone() })?;
            if !loaded.observing {
                game.console("observe")?;
            }
            let session = game.session()?;
            game.check(session.observing, format!("the console's observe left {save}'s game played as {:?}", session.player))?;
            // The game ends on 1821.1.2: a late save runs no further than a
            // month before it, leaving the month after its autosave.
            let room = days_between(session.date, Date::from_ymd(1821, 1, 1).expect("a date")) - 31;
            let days = days.min(room.max(1) as u32);
            let stretch = Stretch::run(game, days, &format!("{save}: {days} days"))?;
            record["stretches"].as_array_mut().expect("stretches").push(stretch);
            let at = game.session()?.date;
            let again = game.autosave_and_load()?;
            game.check(again.date == at, format!("{save}'s autosave loaded on {}, written on {at}", again.date))?;
            let stretch = Stretch::run(game, 30, &format!("{save}: 30 days after its autosave"))?;
            record["stretches"].as_array_mut().expect("stretches").push(stretch);
            game.return_to_menu()?;
        }
        Ok(())
    }

    fn report(&self, record: &Value) -> Vec<String> {
        stretches_table("### The saves", record["stretches"].as_array().into_iter().flatten())
    }
}

impl Scenario for Long {
    fn name(&self) -> &'static str {
        if self.checked {
            "checked"
        } else {
            "long"
        }
    }

    fn about(&self) -> &'static str {
        if self.checked {
            "a game in observe mode from 1444 for some years with every speed-up checking its answers against the game's, none differing"
        } else {
            "a game in observe mode from 1444 to the end of the game, the autosave loaded every so many years, memory under a bound"
        }
    }

    fn build(&self, _: &Parameters) -> Build {
        if self.checked {
            Build::Switches
        } else {
            Build::Player
        }
    }

    fn options(&self, _: &Parameters) -> Option<String> {
        self.checked.then(|| "[test-switches]\nchecked = [\"all\"]\n".to_string())
    }

    fn minutes(&self) -> u32 {
        if self.checked {
            40
        } else {
            240
        }
    }

    fn parameters(&self) -> &'static [Parameter] {
        const LONG: [Parameter; 3] = [
            Parameter { name: "years", default: "376", about: "the years to run, from 1444.11.11; 376 is to the end of the game" },
            Parameter { name: "save_every", default: "50", about: "the years between two autosaves loaded in the game" },
            Parameter { name: "memory_mb", default: "8000", about: "the game's private bytes, in MB, it must stay under" },
        ];
        const CHECKED: [Parameter; 4] = [
            Parameter { name: "years", default: "20", about: "the years to run, from 1444.11.11" },
            Parameter { name: "save_every", default: "5", about: "the years between two autosaves loaded in the game" },
            Parameter { name: "memory_mb", default: "8000", about: "the game's private bytes, in MB, it must stay under" },
            Parameter {
                name: "may_differ",
                default: "army-searches",
                about: "the speed-ups whose differences have their reasons (ADR the-game-may-be-made-faster-with-reasoned-differences), by commas: counted, not failed",
            },
        ];
        if self.checked {
            &CHECKED
        } else {
            &LONG
        }
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let years = parameters.number("years")?;
        let save_every = parameters.number("save_every")?.max(1);
        let memory_mb = parameters.number("memory_mb")? as u64;
        let user = game.user()?;
        let mark = Mark::of(&user);
        let session: Session = game.ask(&MenuRequest::Start { country: None })?;
        game.check(session.observing, format!("menu.start in observe mode answered {session:?}"))?;
        *record = json!({ "began": session.date.to_string(), "years": [], "loads": [] });
        for year in 1..=years {
            let before = game.session()?.date;
            let stretch = Stretch::run(game, YEAR, &format!("year {year}"))?;
            let after = game.session()?.date;
            game.check(days_between(before, after) >= YEAR as i32, format!("year {year} ran from {before} to {after}"))?;
            let private = stretch["priv"].as_u64().unwrap_or(0);
            record["years"].as_array_mut().expect("years").push(stretch);
            game.check(private < memory_mb, format!("the game's private bytes are {private} MB on {after}, over {memory_mb} MB"))?;
            if year % save_every == 0 && year < years {
                let began = Instant::now();
                let again = game.autosave_and_load()?;
                game.check(again.date == after && again.observing, format!("the autosave of {after} loaded as {again:?}"))?;
                record["loads"].as_array_mut().expect("loads").push(json!({ "date": after.to_string(), "seconds": rounded(began.elapsed().as_secs_f64()) }));
            }
        }
        game.return_to_menu()?;
        if self.checked {
            let differences: Vec<String> = mark.since(&user.core_log()).into_iter().filter(|l| l.contains(" differs from the game's: ")).collect();
            let report = std::fs::read_to_string(user.report()).unwrap_or_default();
            let checks: Vec<String> = report
                .lines()
                .skip_while(|l| !l.starts_with("Speed-ups checked against the game"))
                .skip(1)
                .map(|l| l.trim().to_string())
                .take_while(|l| !l.is_empty())
                .collect();
            // A speed-up whose differences have their reasons may differ; the
            // others must not, in a single answer.
            let may_differ = listed(&parameters.text("may_differ"));
            let of = |line: &str| line.split("speed-up ").nth(1).and_then(|rest| rest.split(':').next()).unwrap_or("").to_string();
            let wrong: Vec<&String> = differences.iter().filter(|l| !may_differ.contains(&of(l))).collect();
            record["checks"] = json!(checks);
            record["may_differ"] = json!(may_differ);
            record["differences"] = json!(wrong.iter().take(20).collect::<Vec<_>>());
            game.check(!checks.is_empty(), "the startup report says nothing of the speed-ups checked")?;
            game.check(
                wrong.is_empty(),
                format!(
                    "{} answers of the speed-ups that must not differ differed from the game's; the first: {}",
                    wrong.len(),
                    wrong.first().map_or("", |l| l)
                ),
            )?;
        }
        Ok(())
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let years: Vec<&Value> = record["years"].as_array().into_iter().flatten().collect();
        let tens = years.iter().enumerate().filter(|(i, _)| i % 10 == 9 || *i + 1 == years.len()).map(|(_, y)| *y);
        let mut md = stretches_table(&format!("### The years from {}, each tenth", text(&record["began"])), tens);
        if let Some(slowest) = years.iter().min_by(|a, b| a["days_a_minute"].as_f64().unwrap_or(0.0).total_cmp(&b["days_a_minute"].as_f64().unwrap_or(0.0))) {
            md.extend([
                String::new(),
                format!("The slowest: {}, to {}, at {} days a minute.", text(&slowest["what"]), text(&slowest["date"]), text(&slowest["days_a_minute"])),
            ]);
        }
        let loads: Vec<String> =
            record["loads"].as_array().into_iter().flatten().map(|l| format!("{} ({} s)", text(&l["date"]), text(&l["seconds"]))).collect();
        if !loads.is_empty() {
            md.extend([String::new(), format!("The autosaves loaded in the game: {}.", loads.join(", "))]);
        }
        if let Some(checks) = record["checks"].as_array() {
            md.extend([String::new(), "### The speed-ups checked against the game".into(), String::new()]);
            md.extend(checks.iter().map(|c| format!("- {}", text(c))));
            let may: Vec<String> = record["may_differ"].as_array().into_iter().flatten().map(text).collect();
            if !may.is_empty() {
                md.extend([String::new(), format!("Differences counted, not failed, of {}, whose differences have their reasons.", may.join(", "))]);
            }
            for d in record["differences"].as_array().into_iter().flatten() {
                md.push(format!("- differed: `{}`", text(d)));
            }
        }
        md
    }
}

/// A stretch of days let run: what it was, the date after it, how many days
/// a minute it ran, and the game's memory after it.
struct Stretch;

impl Stretch {
    fn run(game: &mut Game, days: u32, what: &str) -> Played<Value> {
        let process = Process::find(game)?;
        let began = Instant::now();
        let before = game.session()?.date;
        let after: Date = game.run(days)?;
        let minutes = began.elapsed().as_secs_f64() / 60.0;
        let sample = process.sample().map_err(|stop| match stop {
            Stop::Crashed(_) => Stop::Crashed(format!("the game ended after {what}")),
            other => other,
        })?;
        let days_a_minute = rounded(f64::from(days_between(before, after)) / minutes.max(1e-6));
        println!("{what}: to {after}, {days_a_minute} days a minute, private {} MB", sample.private);
        Ok(json!({ "what": what, "date": after.to_string(), "days_a_minute": days_a_minute, "ws": sample.ws, "priv": sample.private }))
    }
}

fn stretches_table<'a>(title: &str, stretches: impl Iterator<Item = &'a Value>) -> Vec<String> {
    let mut md = vec![
        title.to_string(),
        String::new(),
        "| Stretch | To | Days a minute | Working set MB | Private MB |".into(),
        "| --- | --- | --- | --- | --- |".into(),
    ];
    for s in stretches {
        md.push(format!("| {} | {} | {} | {} | {} |", text(&s["what"]), text(&s["date"]), text(&s["days_a_minute"]), text(&s["ws"]), text(&s["priv"])));
    }
    md
}

/// Words joined with commas.
fn listed(text: &str) -> Vec<String> {
    text.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

fn rounded(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_long_game_s_report_gives_every_tenth_year_and_the_slowest() {
        let years: Vec<Value> = (1..=12)
            .map(|y| json!({ "what": format!("year {y}"), "date": format!("{}.11.11", 1444 + y), "days_a_minute": if y == 7 { 100.0 } else { 600.0 }, "ws": 3000, "priv": 3500 }))
            .collect();
        let record = json!({ "began": "1444.11.11", "years": years, "loads": [{ "date": "1449.11.11", "seconds": 40.0 }] });
        let md = LONG.report(&record).join("\n");
        assert!(md.contains("| year 10 | 1454.11.11 | 600.0 | 3000 | 3500 |") && md.contains("| year 12 |") && !md.contains("| year 3 |"), "{md}");
        assert!(md.contains("The slowest: year 7, to 1451.11.11, at 100.0 days a minute.") && md.contains("1449.11.11 (40.0 s)"), "{md}");
    }
}
