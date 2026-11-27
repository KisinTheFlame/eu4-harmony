//! `crash-dumps`: triggers a controlled access violation through
//! `[test-switches] crash = true` on the dev build and verifies the
//! crash-capture pipeline end to end:
//! 1. The game exits with a non-zero crash code.
//! 2. `core.log` records the `crash:` exception line (`0xc0000005`) and call stack.
//! 3. The game writes a crash folder with `minidump.dmp` under `crashes\`.
//! 4. `dbgeng` can open that minidump and unwind a non-empty stack.
use super::{logs::Mark, text, Build, Game, Parameter, Parameters, Played, Scenario, Stop, CRASH_HANDLER, READING_A_DUMP};
use eu4_harmony_remote::{menu::MenuRequest, session::Session, CountryTag};
use serde_json::{json, Value};
use std::time::Duration;

pub struct CrashDumps;

impl Scenario for CrashDumps {
    fn name(&self) -> &'static str {
        "crash-dumps"
    }

    fn about(&self) -> &'static str {
        "controlled crash via test-switches: verifies crash log line, game minidump capture and readable dump stack"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Dev
    }

    fn options(&self, _: &Parameters) -> Option<String> {
        Some("[test-switches]\ncrash = true\n".into())
    }

    fn minutes(&self) -> u32 {
        1
    }

    fn expects_crash(&self) -> bool {
        true
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn play(&self, game: &mut Game, _: &Parameters, record: &mut Value) -> Played<()> {
        let user = game.user()?;
        let crashes_before = user.crash_folders().unwrap_or_default();
        let log_mark = Mark::of(&user);

        // Starting a new game reaches the first in-game frame where
        // `[test-switches] crash = true` executes a controlled access violation.
        match game.try_ask::<Session>(&MenuRequest::Start { country: Some(CountryTag(*b"FRA")) }, Duration::from_secs(120)) {
            Err(Stop::Crashed(_)) => {}
            Ok(_) => {
                // If `menu.start` answered just before the first frame crashed,
                // wait briefly for the process to exit.
                game.wait_gone(CRASH_HANDLER);
            }
            Err(other) => return Err(other),
        }
        game.wait_gone(CRASH_HANDLER);
        let ended = game.ended();
        game.end();

        let crash_folder = user.new_crash_folders(&crashes_before).pop();
        let dump = crash_folder.as_ref().map(|folder| folder.join("minidump.dmp")).filter(|path| path.is_file());
        let dump_bytes = dump.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
        let crash_lines: Vec<String> = log_mark.since(&user.core_log()).into_iter().filter(|l| l.contains("crash:")).collect();

        let symbols: Vec<_> = game.build_dir().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
        let stack = dump
            .as_deref()
            .map(|dump_path| {
                crate::crash_dump::read_tail(dump_path, &user.logs().join("crash-dumps-check.log"), &symbols, &[".ecxr", "kn 20"], READING_A_DUMP, 24)
            })
            .unwrap_or_default();

        let has_exception_log = crash_lines.iter().any(|l| l.contains("0xc0000005"));
        let stack_readable = stack.iter().any(|l| l.contains("eu4_harmony_core") || l.contains("eu4+"));
        *record = json!({
            "ended": ended.as_ref().map(|e| json!({ "exit_code": format!("0x{:x}", e.code), "said": e.describe() })).unwrap_or(Value::Null),
            "crash_folder": crash_folder.as_ref().map(|p| p.display().to_string()),
            "minidump": dump.as_ref().map(|p| p.display().to_string()),
            "dump_bytes": dump_bytes,
            "crash_log_lines": crash_lines,
            "stack": stack,
            "stack_readable": stack_readable,
        });

        // EU4's unhandled exception filter writes its crash folder and exits
        // with code 1; a bypass ends with the Windows exception code. Either
        // way, exit code 0 would mean a clean shutdown rather than a crash.
        game.check(ended.as_ref().is_some_and(|e| e.code != 0), format!("expected non-zero crash exit code, saw {ended:?}"))?;
        game.check(has_exception_log, format!("core.log missing 0xc0000005 crash record: {crash_lines:?}"))?;
        game.check(dump.is_some() && dump_bytes > 0, "no game minidump.dmp written in crashes\\")?;
        game.check(stack_readable, format!("minidump stack was not readable: {stack:?}"))?;
        Ok(())
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let mut md = vec![
            "### Controlled crash capture".into(),
            String::new(),
            format!("Process end: {}.", text(&record["ended"]["said"])),
            format!("Crash minidump: `{}` ({} bytes).", text(&record["minidump"]), text(&record["dump_bytes"])),
            format!("Stack readable by dbgeng: {}.", text(&record["stack_readable"])),
        ];
        let lines: Vec<String> = record["crash_log_lines"].as_array().into_iter().flatten().map(text).collect();
        if !lines.is_empty() {
            md.extend([String::new(), "Crash log records:".into(), String::new(), "```text".into()]);
            md.extend(lines);
            md.push("```".into());
        }
        md
    }
}
