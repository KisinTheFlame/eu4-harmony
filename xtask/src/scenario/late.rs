//! Late-game stability and speed-up equivalence checks:
//! - `long-1800`: plays `observer-1800.7.1` in observe mode to the vanilla/ironman
//!   end date (`1821.1.2`), reloads its autosave mid-run, verifies memory stays
//!   under a bound, and advances across the `1821.1.2` -> `1821.1.3` game-over
//!   boundary confirming that resuming is refused;
//! - `checked`: runs early (1444) and late (`observer-1800.7.1`) observe phases
//!   on the developer build with `[test-switches] checked = ["all"]`, requiring
//!   every checkable speed-up to check at least one answer with no unexpected
//!   differences from the game.
//!
//! Each records, for each stretch, how many game days a minute it ran and
//! the game's working set and private bytes after it.
use super::{game::days_between, logs::Mark, perf::Process, text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    menu::MenuRequest,
    questions::{Question, QuestionRequest},
    session::Session,
    time::{Stop as AdvanceStop, TimeRequest},
    Code, Date, Error,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct Long {
    /// The speed-ups checked against the game: `checked`; or late-to-end run: `long-1800`.
    pub checked: bool,
}

pub const LONG_1800: Long = Long { checked: false };
pub const CHECKED: Long = Long { checked: true };

/// Unmodded / ironman EU4 `END_DATE` (`defines.lua`, `docs/game/day/time.md`).
const END_DATE: (i32, u32, u32) = (1821, 1, 2);

fn end_date() -> Date {
    Date::from_ymd(END_DATE.0, END_DATE.1, END_DATE.2).expect("EU4 end date")
}

/// After which of a stretch's years, numbered from 1, the autosave is
/// written and loaded: every `save_every`, and once in a stretch too short
/// for that, after its middle year; never after its last.
fn loads_after(year: u32, years: u32, save_every: u32) -> bool {
    year < years && (year.is_multiple_of(save_every) || (years <= save_every && year == years.div_ceil(2)))
}

/// The checkable speed-ups that must check at least one answer during `checked`
/// (hidden, silent, early + late observe runs with autosaves). `music` is omitted
/// because silent hidden runs do not necessarily stream music tracks, and
/// `dlc-verify` is omitted because it only runs on a `dlc_signature` miss.
const REQUIRED_CHECKED: &[&str] = &[
    "lookups",
    "listed-folders",
    "script-parameters",
    "localisation",
    "trigger-profile",
    "parallel-saves",
    "army-searches",
    "modifier-sums",
    "country-distances",
    "flag-atlas",
    "token-lookup",
];

/// Parses `<name>: <checked> answer(s) checked, <differed> differing from the game's`.
fn parse_check_line(line: &str) -> Option<(String, u64, u64)> {
    let (name, rest) = line.split_once(':')?;
    let rest = rest.trim();
    let checked = rest.split_whitespace().next()?.parse().ok()?;
    let after_comma = rest.split_once(',')?.1.trim();
    let differed = after_comma.split_whitespace().next()?.parse().ok()?;
    Some((name.trim().to_string(), checked, differed))
}

/// The days a year is let run.
const YEAR: u32 = 365;

impl Scenario for Long {
    fn name(&self) -> &'static str {
        if self.checked {
            "checked"
        } else {
            "long-1800"
        }
    }

    fn about(&self) -> &'static str {
        if self.checked {
            "early (1444) and late (1800) observe runs with every checkable speed-up verified against the game, none differing"
        } else {
            "late game in observe mode from 1800 to the game end date (1821.1.2) and game-over boundary, with autosave reloads and memory bound"
        }
    }

    fn build(&self, _: &Parameters) -> Build {
        if self.checked {
            Build::Dev
        } else {
            Build::Player
        }
    }

    fn options(&self, _: &Parameters) -> Option<String> {
        Some(if self.checked { "[test-switches]\nchecked = [\"all\"]\n".to_string() } else { String::new() })
    }

    fn minutes(&self) -> u32 {
        if self.checked {
            4
        } else {
            15
        }
    }

    fn saves(&self, parameters: &Parameters) -> Vec<String> {
        if self.checked {
            let save = parameters.text("late_save");
            let late_years = parameters.text("late_years");
            if !save.is_empty() && late_years != "0" {
                vec![save]
            } else {
                Vec::new()
            }
        } else {
            vec![parameters.text("save")]
        }
    }

    fn parameters(&self) -> &'static [Parameter] {
        const LONG_1800_PARAMS: [Parameter; 3] = [
            Parameter { name: "save", default: "observer-1800.7.1", about: "the late save to play to the game's end date (1821.1.2)" },
            Parameter { name: "save_every", default: "10", about: "the years between two autosaves loaded in the game; at least one is" },
            Parameter { name: "memory_mb", default: "8000", about: "the game's private bytes, in MB, it must stay under" },
        ];
        const CHECKED_PARAMS: [Parameter; 6] = [
            Parameter { name: "years", default: "2", about: "early-game years to run from 1444.11.11" },
            Parameter { name: "late_save", default: "observer-1800.7.1", about: "late-game save to check after the early phase (empty to skip)" },
            Parameter { name: "late_years", default: "1", about: "late-game years to run on late_save" },
            Parameter { name: "save_every", default: "1", about: "years between autosaves loaded in the game" },
            Parameter { name: "memory_mb", default: "8000", about: "the game's private bytes, in MB, it must stay under" },
            Parameter {
                name: "may_differ",
                default: "army-searches",
                about: "the speed-ups whose differences have their reasons (ADR the-game-may-be-made-faster-with-reasoned-differences), by commas: counted, not failed",
            },
        ];
        if self.checked {
            &CHECKED_PARAMS
        } else {
            &LONG_1800_PARAMS
        }
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        if self.checked {
            play_checked(game, parameters, record)
        } else {
            play_long_1800(game, parameters, record)
        }
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let years: Vec<&Value> = record["years"].as_array().into_iter().flatten().collect();
        let shown: Vec<&Value> = if years.len() <= 12 {
            years.clone()
        } else {
            years.iter().enumerate().filter(|(i, _)| i % 5 == 4 || *i + 1 == years.len()).map(|(_, y)| *y).collect()
        };
        let mut md = stretches_table(&format!("### Stretches from {}", text(&record["began"])), shown.into_iter());
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
        if record["end_date_boundary"].is_object() {
            md.extend([
                String::new(),
                format!(
                    "End-date boundary: reached {}, resume refused after game over: {}.",
                    text(&record["end_date_boundary"]["reached"]),
                    text(&record["end_date_boundary"]["resume_refused"])
                ),
            ]);
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

fn play_long_1800(game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
    let save = crate::saves::save_name(&parameters.text("save"));
    let save_every = parameters.number("save_every")?.max(1);
    let memory_mb = parameters.number("memory_mb")? as u64;
    let end = end_date();

    game.ask::<Session>(&MenuRequest::Load { save })?;
    let session = game.observe_with_no_country()?;
    game.check(session.observing && session.player.is_none(), format!("the game began in observe mode with no country in view, answered {session:?}"))?;

    let lengths = stretches(days_between(session.date, end));
    *record = json!({ "began": session.date.to_string(), "years": [], "loads": [] });
    run_phase(game, "year", &lengths, save_every, memory_mb, record)?;

    // Advance one day past END_DATE (1821.1.2 -> 1821.1.3) to verify EU4's
    // end-of-game boundary and confirm resuming is refused once the game is over.
    let reached = advance_past_end_date(game, end)?;
    game.check(reached.0 > end.0, format!("expected to cross end date {end}, stopped at {reached}"))?;
    let resume = game.try_ask::<()>(&TimeRequest::Resume {}, Duration::from_secs(30))?;
    let refused = matches!(resume, Err(Error { code: Code::Rejected, .. }));
    record["end_date_boundary"] = json!({ "reached": reached.to_string(), "resume_refused": refused });
    game.check(refused, format!("expected time.resume to be refused after game over on {reached}, got {resume:?}"))?;

    game.return_to_menu()
}

fn play_checked(game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
    let early_years = parameters.number("years")?;
    let late_save = parameters.text("late_save");
    let late_years = parameters.number("late_years")?;
    let save_every = parameters.number("save_every")?.max(1);
    let memory_mb = parameters.number("memory_mb")? as u64;
    let user = game.user()?;
    let mark = Mark::of(&user);

    *record = json!({ "began": "1444.11.11", "years": [], "loads": [] });

    // Phase 1: early game (new game from 1444.11.11).
    if early_years > 0 {
        let session: Session = game.ask(&MenuRequest::Start { country: None })?;
        game.check(session.observing && session.player.is_none(), format!("the game began in observe mode with no country in view, answered {session:?}"))?;
        record["began"] = json!(session.date.to_string());
        let lengths = vec![YEAR; early_years as usize];
        run_phase(game, "early year", &lengths, save_every, memory_mb, record)?;
        game.return_to_menu()?;
    }

    // Phase 2: late game (observer-1800.7.1) to exercise large-map speed-ups
    // (country-distances, army-searches, modifier-sums, parallel-saves, token-lookup).
    if !late_save.is_empty() && late_years > 0 {
        let save_name = crate::saves::save_name(&late_save);
        game.ask::<Session>(&MenuRequest::Load { save: save_name })?;
        let session = game.observe_with_no_country()?;
        game.check(session.observing && session.player.is_none(), format!("late phase began in observe mode with no country in view, answered {session:?}"))?;
        if early_years == 0 {
            record["began"] = json!(session.date.to_string());
        }
        let lengths = vec![YEAR; late_years as usize];
        run_phase(game, "late year", &lengths, save_every, memory_mb, record)?;
        if late_years == 1 {
            // Ensure late-game autosave write + reload + post-reload days are checked too.
            let after = game.session()?.date;
            let began = Instant::now();
            let again = game.autosave_and_load()?;
            game.check(again.date == after && again.observing, format!("the late autosave of {after} loaded as {again:?}"))?;
            game.observe_with_no_country()?;
            record["loads"].as_array_mut().expect("loads").push(json!({ "date": after.to_string(), "seconds": rounded(began.elapsed().as_secs_f64()) }));
            let stretch = Stretch::run(game, 30, "late post-reload (30 days)")?;
            record["years"].as_array_mut().expect("years").push(stretch);
        }
        game.return_to_menu()?;
    }

    let differences: Vec<String> = mark.since(&user.core_log()).into_iter().filter(|l| l.contains(" differs from the game's: ")).collect();
    let report = std::fs::read_to_string(user.report()).unwrap_or_default();
    let checks: Vec<String> = report
        .lines()
        .skip_while(|l| !l.starts_with("Speed-ups checked against the game"))
        .skip(1)
        .map(|l| l.trim().to_string())
        .take_while(|l| !l.is_empty())
        .collect();
    // A speed-up whose differences have their reasons may differ; the others
    // must not, in a single answer, and every speed-up expected to run in this
    // campaign must check at least one answer.
    let may_differ = listed(&parameters.text("may_differ"));
    let of = |line: &str| line.split("speed-up ").nth(1).and_then(|rest| rest.split(':').next()).unwrap_or("").to_string();
    let wrong: Vec<&String> = differences.iter().filter(|l| !may_differ.contains(&of(l))).collect();
    let parsed: Vec<(String, u64, u64)> = checks.iter().filter_map(|line| parse_check_line(line)).collect();
    let unexercised: Vec<&str> =
        REQUIRED_CHECKED.iter().copied().filter(|name| parsed.iter().find(|(n, _, _)| n == name).is_none_or(|(_, checked, _)| *checked == 0)).collect();
    let reported_diffs: Vec<String> = parsed
        .iter()
        .filter(|(name, _, differed)| *differed > 0 && !may_differ.contains(name))
        .map(|(name, checked, differed)| format!("{name}: {differed} of {checked} differed"))
        .collect();
    record["checks"] = json!(checks);
    record["may_differ"] = json!(may_differ);
    record["unexercised"] = json!(unexercised);
    record["differences"] = json!(wrong.iter().take(20).collect::<Vec<_>>());
    game.check(!checks.is_empty(), "the startup report says nothing of the speed-ups checked")?;
    game.check(unexercised.is_empty(), format!("checkable speed-ups checked zero answers in this run: {}", unexercised.join(", ")))?;
    game.check(
        wrong.is_empty() && reported_diffs.is_empty(),
        format!(
            "{} answers of the speed-ups that must not differ differed from the game's; reported: {:?}; the first: {}",
            wrong.len(),
            reported_diffs,
            wrong.first().map_or("", |l| l)
        ),
    )
}

fn run_phase(game: &mut Game, label: &str, lengths: &[u32], save_every: u32, memory_mb: u64, record: &mut Value) -> Played<()> {
    let years = lengths.len() as u32;
    for (year, &days) in (1..=years).zip(lengths) {
        let before = game.session()?.date;
        let stretch = Stretch::run(game, days, &format!("{label} {year}"))?;
        let after = game.session()?.date;
        game.check(days_between(before, after) >= days as i32, format!("{label} {year} ran from {before} to {after}"))?;
        let private = stretch["priv"].as_u64().unwrap_or(0);
        record["years"].as_array_mut().expect("years").push(stretch);
        game.check(private < memory_mb, format!("the game's private bytes are {private} MB on {after}, over {memory_mb} MB"))?;
        if loads_after(year, years, save_every) {
            let began = Instant::now();
            let again = game.autosave_and_load()?;
            game.check(again.date == after && again.observing, format!("the autosave of {after} loaded as {again:?}"))?;
            game.observe_with_no_country()?;
            record["loads"].as_array_mut().expect("loads").push(json!({ "date": after.to_string(), "seconds": rounded(began.elapsed().as_secs_f64()) }));
        }
    }
    Ok(())
}

/// Advances across `end` (`1821.1.2`) into `1821.1.3`, answering any blocking
/// questions along the way and accepting the end-of-game pause once past `end`.
fn advance_past_end_date(game: &mut Game, end: Date) -> Played<Date> {
    for _ in 0..10 {
        let current = game.session()?.date;
        if current.0 > end.0 {
            return Ok(current);
        }
        let advanced = game.advance(1)?;
        if advanced.date.0 > end.0 {
            return Ok(advanced.date);
        }
        match advanced.stopped {
            AdvanceStop::Question => {
                let questions: Vec<Question> = game.ask(&QuestionRequest::List {})?;
                for q in questions {
                    if let Some(first) = q.options.first() {
                        let () = game.ask(&QuestionRequest::Answer { question: q.id, option: first.option })?;
                    }
                }
            }
            AdvanceStop::Date | AdvanceStop::Paused => {}
            AdvanceStop::Session | AdvanceStop::MainMenu => {
                return Err(Stop::Failed(format!("stopped unexpectedly at {} before crossing {end}: {}", advanced.date, advanced.reason)));
            }
        }
    }
    Ok(game.session()?.date)
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
    fn long_1800_targets_official_eu4_end_date_and_loads_mid_run() {
        let start = Date::from_ymd(1800, 7, 1).unwrap();
        let end = end_date();
        assert_eq!(end, Date::from_ymd(1821, 1, 2).unwrap());
        let lengths = stretches(days_between(start, end));
        assert_eq!(lengths.len(), 21);
        assert_eq!(lengths[0], 365);
        assert_eq!(*lengths.last().unwrap(), 185);
        let loads: Vec<u32> = (1..=lengths.len() as u32).filter(|&y| loads_after(y, lengths.len() as u32, 10)).collect();
        assert_eq!(loads, [10, 20]);
        assert_eq!(LONG_1800.saves(&Parameters::of(&LONG_1800, &[]).unwrap()), ["observer-1800.7.1"]);
    }

    #[test]
    fn checked_requires_late_save_by_default_and_skips_it_when_disabled() {
        let defaults = Parameters::of(&CHECKED, &[]).unwrap();
        assert_eq!(CHECKED.saves(&defaults), ["observer-1800.7.1"]);
        let no_late = Parameters::of(&CHECKED, &[("checked".into(), "late_save".into(), String::new())]).unwrap();
        assert!(CHECKED.saves(&no_late).is_empty());
        let zero_late = Parameters::of(&CHECKED, &[("checked".into(), "late_years".into(), "0".into())]).unwrap();
        assert!(CHECKED.saves(&zero_late).is_empty());
    }

    #[test]
    fn a_stretch_runs_in_years_and_loads_its_autosave_at_least_once() {
        assert_eq!(stretches(3 * 365 + 10), [365, 365, 365, 10]);
        assert_eq!(stretches(130), [130]);
        let loads = |years: u32, every: u32| (1..=years).filter(|&y| loads_after(y, years, every)).collect::<Vec<_>>();
        assert_eq!(loads(2, 1), [1]);
        assert_eq!(loads(20, 20), [10]);
        assert_eq!(loads(1, 10), Vec::<u32>::new());
    }

    #[test]
    fn parses_checked_speed_up_counts_from_report_lines() {
        assert_eq!(parse_check_line("localisation: 1234 answer(s) checked, 0 differing from the game's"), Some(("localisation".into(), 1234, 0)));
        assert_eq!(parse_check_line("army-searches: 2520 answer(s) checked, 3 differing from the game's"), Some(("army-searches".into(), 2520, 3)));
        assert_eq!(parse_check_line("unrelated"), None);
    }

    #[test]
    fn report_includes_slowest_loads_and_end_date_boundary() {
        let years: Vec<Value> = (1..=3)
            .map(|y| json!({ "what": format!("year {y}"), "date": format!("{}.7.1", 1800 + y), "days_a_minute": if y == 2 { 100.0 } else { 600.0 }, "ws": 3000, "priv": 3500 }))
            .collect();
        let record = json!({
            "began": "1800.7.1",
            "years": years,
            "loads": [{ "date": "1810.7.1", "seconds": 40.0 }],
            "end_date_boundary": { "reached": "1821.1.3", "resume_refused": true },
        });
        let md = LONG_1800.report(&record).join("\n");
        assert!(md.contains("| year 2 | 1802.7.1 | 100.0 | 3000 | 3500 |"), "{md}");
        assert!(md.contains("The slowest: year 2, to 1802.7.1, at 100.0 days a minute."), "{md}");
        assert!(md.contains("1810.7.1 (40.0 s)"), "{md}");
        assert!(md.contains("End-date boundary: reached 1821.1.3, resume refused after game over: true."), "{md}");
    }
}
