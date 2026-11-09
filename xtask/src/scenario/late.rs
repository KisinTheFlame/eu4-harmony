//! Games let run long, on the player build unless said (docs/plans/whole-game.md):
//! - `late`: each of the late game's saves loaded, observed, a year let run,
//!   the autosave written and loaded, a month more, and a return;
//! - `long`: a game in observe mode from 1444 to the end of the game, a
//!   year at a time, the autosave written and loaded every so many years,
//!   the game's memory kept under a bound;
//! - `long-1444` to `long-1820`: the same game in stretches that play side by
//!   side, each from a new game in 1444 or from one of the developers' saves
//!   of that game (`late`'s), to the date of the next, with the same checks
//!   and an autosave loaded in each;
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
    /// The stretch of the game it plays, for a segment; the whole game for
    /// `long` and `checked`.
    pub segment: Option<Segment>,
}

/// A stretch of the long game: its name, the developers' save it begins
/// from (a new game in 1444 for none), the date it runs to, and about how
/// many minutes it plays.
pub struct Segment {
    pub name: &'static str,
    pub from: Option<&'static str>,
    pub to: (i32, u32, u32),
    pub minutes: u32,
}

pub const LONG: Long = Long { checked: false, segment: None };
pub const CHECKED: Long = Long { checked: true, segment: None };

/// The long game in stretches, which together play what `long` plays: from
/// 1444.11.11 to 1820.11.11, `long`'s 376 years.
pub const SEGMENTS: [Long; 7] = [
    segment("long-1444", None, (1600, 1, 1), 60),
    segment("long-1600", Some("observer-1600.1.1"), (1700, 1, 1), 50),
    segment("long-1700", Some("observer-1700.1.1"), (1760, 7, 1), 40),
    segment("long-1760", Some("observer-1760.7.1"), (1780, 7, 1), 20),
    segment("long-1780", Some("observer-1780.7.1"), (1800, 7, 1), 20),
    segment("long-1800", Some("observer-1800.7.1"), (1820, 7, 1), 20),
    segment("long-1820", Some("observer-1820.7.1"), (1820, 11, 11), 5),
];

const fn segment(name: &'static str, from: Option<&'static str>, to: (i32, u32, u32), minutes: u32) -> Long {
    Long { checked: false, segment: Some(Segment { name, from, to, minutes }) }
}

impl Segment {
    fn end(&self) -> Date {
        Date::from_ymd(self.to.0, self.to.1, self.to.2).expect("a segment's date")
    }
}

/// After which of a stretch's years, numbered from 1, the autosave is
/// written and loaded: every `save_every`, and once in a stretch too short
/// for that, after its middle year; never after its last.
fn loads_after(year: u32, years: u32, save_every: u32) -> bool {
    year < years && (year.is_multiple_of(save_every) || (years <= save_every && year == years.div_ceil(2)))
}

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
        match &self.segment {
            Some(segment) => segment.name,
            None if self.checked => "checked",
            None => "long",
        }
    }

    fn about(&self) -> &'static str {
        if self.segment.is_some() {
            "a stretch of the long game in observe mode, from 1444 or a developers' save to the next, the autosave loaded in it, memory under a bound"
        } else if self.checked {
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
        match &self.segment {
            Some(segment) => segment.minutes,
            None if self.checked => 40,
            None => 240,
        }
    }

    fn saves(&self, _: &Parameters) -> Vec<String> {
        self.segment.as_ref().and_then(|s| s.from).map(String::from).into_iter().collect()
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
        const SEGMENT: [Parameter; 2] = [
            Parameter { name: "save_every", default: "20", about: "the years between two autosaves loaded in the game; at least one is" },
            Parameter { name: "memory_mb", default: "8000", about: "the game's private bytes, in MB, it must stay under" },
        ];
        match (&self.segment, self.checked) {
            (Some(_), _) => &SEGMENT,
            (None, true) => &CHECKED,
            (None, false) => &LONG,
        }
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let save_every = parameters.number("save_every")?.max(1);
        let memory_mb = parameters.number("memory_mb")? as u64;
        let user = game.user()?;
        let mark = Mark::of(&user);
        let session = match self.segment.as_ref().and_then(|s| s.from) {
            Some(save) => {
                let loaded: Session = game.ask(&MenuRequest::Load { save: save.into() })?;
                if !loaded.observing {
                    game.console("observe")?;
                }
                game.session()?
            }
            None => game.ask(&MenuRequest::Start { country: None })?,
        };
        game.check(session.observing, format!("the game began in observe mode, answered {session:?}"))?;
        // A year at a time; a segment's last stretch ends on its date.
        let lengths: Vec<u32> = match &self.segment {
            Some(segment) => stretches(days_between(session.date, segment.end())),
            None => vec![YEAR; parameters.number("years")? as usize],
        };
        let years = lengths.len() as u32;
        *record = json!({ "began": session.date.to_string(), "years": [], "loads": [] });
        for (year, days) in (1..=years).zip(lengths) {
            let before = game.session()?.date;
            let stretch = Stretch::run(game, days, &format!("year {year}"))?;
            let after = game.session()?.date;
            game.check(days_between(before, after) >= days as i32, format!("year {year} ran from {before} to {after}"))?;
            let private = stretch["priv"].as_u64().unwrap_or(0);
            record["years"].as_array_mut().expect("years").push(stretch);
            game.check(private < memory_mb, format!("the game's private bytes are {private} MB on {after}, over {memory_mb} MB"))?;
            if loads_after(year, years, save_every) {
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

/// A stretch of `days` in years and the days left after them.
fn stretches(days: i32) -> Vec<u32> {
    let days = days.max(0) as u32;
    let mut lengths = vec![YEAR; (days / YEAR) as usize];
    if !days.is_multiple_of(YEAR) {
        lengths.push(days % YEAR);
    }
    lengths
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

    #[test]
    fn the_segments_play_long_s_years_from_save_to_save() {
        let names: Vec<&str> = SEGMENTS.iter().map(|s| s.name()).collect();
        assert_eq!(names, ["long-1444", "long-1600", "long-1700", "long-1760", "long-1780", "long-1800", "long-1820"]);
        // Each begins where the one before ends, the first on a new game.
        assert!(SEGMENTS[0].segment.as_ref().unwrap().from.is_none());
        for pair in SEGMENTS.windows(2) {
            let (a, b) = (pair[0].segment.as_ref().unwrap(), pair[1].segment.as_ref().unwrap());
            let save = format!("observer-{}.{}.{}", a.to.0, a.to.1, a.to.2);
            assert_eq!(b.from, Some(save.as_str()), "{}", b.name);
        }
        // The last ends where long does, 376 years after 1444.11.11.
        assert_eq!(SEGMENTS[6].segment.as_ref().unwrap().to, (1820, 11, 11));
        assert_eq!(SEGMENTS[1].saves(&Parameters::of(&SEGMENTS[1], &[]).unwrap()), ["observer-1600.1.1"]);
        assert!(SEGMENTS[0].saves(&Parameters::of(&SEGMENTS[0], &[]).unwrap()).is_empty());
    }

    #[test]
    fn a_stretch_runs_in_years_and_loads_its_autosave_at_least_once() {
        assert_eq!(stretches(3 * 365 + 10), [365, 365, 365, 10]);
        assert_eq!(stretches(130), [130]);
        // 20 years, every 20: once, after the middle; 100 years: after 20,
        // 40, 60 and 80; one year: never.
        let loads = |years: u32, every: u32| (1..=years).filter(|&y| loads_after(y, years, every)).collect::<Vec<_>>();
        assert_eq!(loads(20, 20), [10]);
        assert_eq!(loads(100, 20), [20, 40, 60, 80]);
        assert_eq!(loads(1, 20), Vec::<u32>::new());
        assert_eq!(loads(376, 50), [50, 100, 150, 200, 250, 300, 350]);
    }
}
