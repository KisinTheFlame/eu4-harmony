//! `extensions`: extensions that go wrong, in the real game, on the player
//! build (docs/plans/whole-game.md). Four test mods are in its test game's
//! playset (`mods/`): `qa-steady` counts the days and keeps its count as
//! save data, `qa-failing` raises errors, `qa-looping` loops without end,
//! `qa-hungry` runs into the memory limit and `qa-tasks` tries tasks, game
//! days, the script group and the reads in each session, writing a line for
//! each check.
//!
//! From the main menu it starts a new game as the country, lets days run,
//! writes the autosave and loads it in the game, reloads `qa-steady` again
//! and again while the time runs, lets days run again and returns. Then it
//! reads the extensions' logs: `qa-steady` counted every day and got its
//! count back after the load and each reload; `qa-failing` logged its error
//! and was not stopped; `qa-looping` was stopped by the time limit, then for
//! good; `qa-hungry` was refused memory and was not stopped; `qa-tasks`
//! passed each of its checks in both sessions, the task it left waiting at
//! the first one's end raising cancelled in the second, and failed none. The
//! date went on through all of it.
use super::{game::days_between, logs::Mark, text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    extensions::{ExtensionRequest, Reloaded},
    menu::MenuRequest,
    session::Session,
    CountryTag,
};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Instant};

pub struct Extensions;

/// The test mods, by their folders' names under `mods/`, which are their
/// ids too.
const MODS: [&str; 5] = ["qa-steady", "qa-failing", "qa-looping", "qa-hungry", "qa-tasks"];

/// What `qa-tasks` checks in each session, as its lines name them.
const TASK_CHECKS: [&str; 8] = ["wait", "all", "any", "cancel", "error", "async", "script", "reads"];

impl Scenario for Extensions {
    fn name(&self) -> &'static str {
        "extensions"
    }

    fn about(&self) -> &'static str {
        "extensions that go wrong, in the real game: errors, a loop the time limit stops, the memory limit, save data through a load and reloads"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        15
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter { name: "country", default: "FRA", about: "the country of the new game" },
            Parameter { name: "days", default: "120", about: "the days let run before the autosave, and again after the reloads" },
            Parameter { name: "reloads", default: "10", about: "how many times qa-steady is reloaded while the time runs" },
        ]
    }

    fn mods(&self, _: &Parameters) -> Vec<PathBuf> {
        MODS.iter().map(|m| crate::repo().join("xtask").join("src").join("scenario").join("mods").join(m)).collect()
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let country = parameters.text("country");
        let tag = CountryTag::parse(&country).ok_or_else(|| Stop::Failed(format!("no country tag {country:?}")))?;
        let days = parameters.number("days")?;
        let reloads = parameters.number("reloads")?;
        let user = game.user()?;
        let log = |id: &str| user.logs().join(format!("{id}.log"));
        let marks: Vec<Mark> = MODS.iter().map(|m| Mark::of_file(&log(m))).collect();

        let began: Session = game.ask(&MenuRequest::Start { country: Some(tag) })?;
        let ran = Instant::now();
        game.run(days)?;
        let loaded = game.autosave_and_load()?;
        let mut lost = Vec::new();
        for _ in 0..reloads {
            let reloaded: Reloaded = game.ask(&ExtensionRequest::Reload { id: MODS[0].into() })?;
            game.check(reloaded.session, "qa-steady reloaded with no session going on")?;
            lost.extend(reloaded.lost);
            game.run(3)?;
        }
        game.run(days)?;
        let end = game.session()?;
        let minutes = ran.elapsed().as_secs_f64() / 60.0;
        game.return_to_menu()?;

        let lines: Vec<Vec<String>> = MODS.iter().zip(&marks).map(|(m, mark)| mark.since(&log(m))).collect();
        let [steady, failing, looping, hungry, tasks] = [&lines[0], &lines[1], &lines[2], &lines[3], &lines[4]];
        let elapsed = days_between(began.date, end.date);
        // Each day qa-steady heard: its count and the date. The first day of
        // a session is heard at the session's own date, so the date of the
        // autosave is heard before it and again after its load.
        let heard: Vec<(i32, String)> = steady
            .iter()
            .filter_map(|l| {
                let mut words = l.strip_prefix("qa-steady day ")?.split(' ');
                Some((words.next()?.parse().ok()?, words.next()?.to_string()))
            })
            .collect();
        let counted_on = heard.iter().enumerate().all(|(i, (n, _))| *n == i as i32 + 1);
        let dates: std::collections::BTreeSet<&str> = heard.iter().map(|(_, d)| d.as_str()).collect();
        let last_day = heard.last().map(|(n, _)| *n);
        let saved_back = steady.iter().filter(|l| l.contains("qa-steady session: saved back")).count();
        let errors = failing.iter().filter(|l| l.contains("qa-failing: on purpose")).count();
        let wrong_calls = failing.iter().filter(|l| l.contains("province.get")).count();
        let time_limit = looping.iter().filter(|l| l.contains("more than 1 s without waiting")).count();
        let stopped_for_good = looping.iter().any(|l| l.starts_with("stopped:") && l.contains("ran past the time limit"));
        let refused = hungry.iter().filter(|l| l.contains("qa-hungry: false") && l.contains("memory")).count();
        // qa-tasks: each check passed in both sessions, the task left
        // waiting at the first one's end raised cancelled in the second, and
        // none failed.
        let passed = |name: &str| tasks.iter().filter(|l| l.as_str() == format!("qa-tasks ok {name}")).count();
        let short: Vec<&str> = TASK_CHECKS.iter().copied().filter(|c| passed(c) < 2).collect();
        let dropped = passed("dropped");
        let task_failures: Vec<&String> = tasks.iter().filter(|l| l.starts_with("qa-tasks FAIL")).collect();
        // Stopped for good; the time limit's own error says "stopped" too.
        let stopped = |lines: &[String]| lines.iter().any(|l| l.starts_with("stopped:"));
        *record = json!({
            "began": began.date.to_string(), "loaded": loaded.date.to_string(), "ended": end.date.to_string(),
            "days_a_minute": (f64::from(elapsed) / minutes.max(1e-6) * 10.0).round() / 10.0,
            "steady_last_day": last_day, "steady_dates": dates.len(), "steady_counted_on": counted_on, "saved_back": saved_back, "lost": lost,
            "failing_errors": errors, "failing_wrong_calls": wrong_calls, "failing_stopped": stopped(failing),
            "looping_time_limit": time_limit, "looping_stopped_for_good": stopped_for_good,
            "hungry_refused": refused, "hungry_stopped": stopped(hungry),
            "tasks_checks_passed": TASK_CHECKS.iter().map(|c| passed(c)).sum::<usize>(), "tasks_short": short, "tasks_dropped": dropped,
            "tasks_failed": task_failures, "tasks_stopped": stopped(tasks),
            "logs": MODS.iter().zip(&lines).map(|(m, l)| (m.to_string(), json!(l.iter().rev().take(5).rev().collect::<Vec<_>>()))).collect::<serde_json::Map<_, _>>(),
        });
        game.check(
            dates.len() as i32 == elapsed + 1 && counted_on,
            format!(
                "qa-steady heard {} dates of the {} from {} to {}, its count {}",
                dates.len(),
                elapsed + 1,
                began.date,
                end.date,
                if counted_on { "unbroken" } else { "broken" }
            ),
        )?;
        game.check(
            saved_back == 1 + reloads as usize,
            format!("qa-steady got its save data back {saved_back} times, not after the load and each of {reloads} reloads"),
        )?;
        game.check(lost.is_empty(), format!("a reload lost qa-steady's save data: {}", lost.join("; ")))?;
        game.check(
            errors >= 1 && !stopped(failing),
            format!("qa-failing logged its error {errors} times and was {}stopped", if stopped(failing) { "" } else { "not " }),
        )?;
        game.check(
            time_limit >= 1 && stopped_for_good,
            format!("qa-looping was stopped by the time limit {time_limit} times, and {}for good", if stopped_for_good { "" } else { "not " }),
        )?;
        game.check(
            refused >= 1 && !stopped(hungry),
            format!("qa-hungry was refused memory {refused} times and was {}stopped", if stopped(hungry) { "" } else { "not " }),
        )?;
        game.check(
            short.is_empty() && dropped == 1 && task_failures.is_empty() && !stopped(tasks),
            format!(
                "qa-tasks passed every check twice but {short:?}, the left task's cancel {dropped} times, failed {task_failures:?} and was {}stopped",
                if stopped(tasks) { "" } else { "not " }
            ),
        )?;
        Ok(())
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let row = |what: &str, key: &str| format!("| {what} | {} |", text(&record[key]));
        let mut md = vec![
            "### What the extensions came to".to_string(),
            String::new(),
            "| | |".into(),
            "| --- | --- |".into(),
            format!("| The game | from {} to {}, the autosave of {} loaded |", text(&record["began"]), text(&record["ended"]), text(&record["loaded"])),
            row("Game days a minute, all of it", "days_a_minute"),
            row("qa-steady's last day counted", "steady_last_day"),
            row("qa-steady's dates heard", "steady_dates"),
            row("qa-steady's count unbroken through the load and reloads", "steady_counted_on"),
            row("qa-steady's save data got back", "saved_back"),
            row("qa-failing's errors written in full", "failing_errors"),
            row("qa-failing's wrong calls written", "failing_wrong_calls"),
            row("qa-looping stopped by the time limit", "looping_time_limit"),
            row("qa-looping stopped for good", "looping_stopped_for_good"),
            row("qa-hungry refused memory", "hungry_refused"),
            row("qa-hungry stopped", "hungry_stopped"),
            row("qa-tasks' checks passed, both sessions", "tasks_checks_passed"),
            row("qa-tasks' checks not passed twice", "tasks_short"),
            row("qa-tasks' left task raised cancelled in the next session", "tasks_dropped"),
            row("qa-tasks' checks failed", "tasks_failed"),
            row("qa-tasks stopped", "tasks_stopped"),
        ];
        if let Some(logs) = record["logs"].as_object() {
            md.extend([String::new(), "The last lines of each log:".into(), String::new(), "```text".into()]);
            for (id, lines) in logs {
                md.extend(lines.as_array().into_iter().flatten().map(|l| format!("{id}: {}", text(l))));
            }
            md.push("```".into());
        }
        md
    }
}
