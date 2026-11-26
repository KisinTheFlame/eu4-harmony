//! A native window's course through rows, closing, reloading and sessions.
//! Desired-state readback is not native evidence: each step waits for frames,
//! captures selected rendered frames, and rejects the native window errors.
use super::{logs::Mark, text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    extensions::{ExtensionRequest, Reloaded},
    menu::MenuRequest,
    popups::{Popup, PopupRequest},
    screenshot::{Screenshot, ScreenshotRequest},
    session::Session,
    time::TimeRequest,
    CountryTag,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Windows;
const MOD: &str = "qa-windows";
const STEPS: u32 = 7;

impl Scenario for Windows {
    fn name(&self) -> &'static str {
        "windows"
    }
    fn about(&self) -> &'static str {
        "native extension windows, panel entries, fixed panels, replacing list rows, pop-up answers, panel closing, extension reload and session teardown; captures retained for visual inspection"
    }
    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }
    fn minutes(&self) -> u32 {
        5
    }
    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }
    fn mods(&self, _: &Parameters) -> Vec<PathBuf> {
        vec![crate::repo().join("xtask/src/scenario/mods").join(MOD)]
    }
    fn play(&self, game: &mut Game, _: &Parameters, record: &mut Value) -> Played<()> {
        let user = game.user()?;
        let core = Mark::of(&user);
        let load_error = Mark::default()
            .messages(&user.core_log(), eu4_harmony_logging::CORE)
            .into_iter()
            .find(|line| line.contains(&format!("extension \"{MOD}\" not loaded:")));
        game.check(load_error.is_none(), load_error.unwrap_or_default())?;
        let extension = Mark::of_extension(&user, MOD);
        let _: Session = game.ask(&MenuRequest::Start { country: Some(CountryTag(*b"FRA")) })?;
        let _: () = game.ask(&TimeRequest::Pause {})?;
        *record = json!({"courses": [], "captures": [], "native_errors": []});
        course(game, "new-game", record)?;
        let reloaded: Reloaded = game.ask(&ExtensionRequest::Reload { id: MOD.into() })?;
        game.check(reloaded.session && reloaded.lost.is_none(), format!("the window probe did not reload with its saved data: {:?}", reloaded.lost))?;
        course(game, "extension-reload", record)?;
        game.autosave_and_load()?;
        let _: () = game.ask(&TimeRequest::Pause {})?;
        course(game, "in-game-load", record)?;
        // Leave a window open when the session ends; a new session must build
        // its own objects rather than keep handles from the previous world.
        game.return_to_menu()?;
        let _: Session = game.ask(&MenuRequest::Start { country: Some(CountryTag(*b"FRA")) })?;
        let _: () = game.ask(&TimeRequest::Pause {})?;
        course(game, "next-session", record)?;
        game.return_to_menu()?;
        let lines = extension.extension_messages(&user, MOD);
        let completed = lines.iter().filter(|line| line.starts_with("qa-windows completed ")).count();
        let failures: Vec<_> =
            lines.iter().filter(|line| line.contains("error:") || line.starts_with("stopped:") || line.contains("assertion failed")).collect();
        let native_errors: Vec<_> = core
            .since(&user.core_log())
            .into_iter()
            .filter(|line| line.contains(" windows:") && !line.contains("windows: reading ") || line.contains("late native tooltip:"))
            .collect();
        let startup_lines = Mark::default().messages(&user.core_log(), eu4_harmony_logging::CORE);
        let (mount_count, framework_file_issues) = check_framework_files(&startup_lines);
        let error_log = std::fs::read_to_string(user.path().join("logs").join("error.log")).unwrap_or_default();
        let game_error_log_issues = game_error_log_issues(&error_log);
        record["completed_steps"] = json!(completed);
        record["extension_errors"] = json!(failures);
        record["native_errors"] = json!(native_errors);
        record["mount_count"] = json!(mount_count);
        record["framework_file_issues"] = json!(framework_file_issues);
        record["game_error_log_issues"] = json!(game_error_log_issues);
        game.check(completed == (STEPS * 4) as usize, format!("the window probe completed {completed} steps, expected {}", STEPS * 4))?;
        game.check(failures.is_empty(), format!("window probe errors: {failures:?}"))?;
        game.check(native_errors.is_empty(), format!("native window or tooltip errors: {native_errors:?}"))?;
        game.check(framework_file_issues.is_empty(), format!("framework virtual file mount issues: {framework_file_issues:?}"))?;
        game.check(game_error_log_issues.is_empty(), format!("game error.log reported framework/extension asset errors: {game_error_log_issues:?}"))?;
        Ok(())
    }
    fn report(&self, record: &Value) -> Vec<String> {
        vec![
            "### Native window courses".into(), String::new(),
            format!("Courses completed: {}. Steps answered: {}.", text(&record["courses"]), text(&record["completed_steps"])),
            format!(
                "Native errors: {}. Extension errors: {}. Framework file mounts: {} (issues: {}). Game error.log issues: {}.",
                text(&record["native_errors"]),
                text(&record["extension_errors"]),
                text(&record["mount_count"]),
                text(&record["framework_file_issues"]),
                text(&record["game_error_log_issues"]),
            ),
            String::new(),
            "Each course applies initial rows, replaces them, clears them and the tooltip, closes and reopens the window, opens another panel and returns to the first. It then leaves that window open for the next reload or session transition. Pop-up markers appear after twelve extension frames; their answers drive the next step.".into(),
            String::new(),
            format!("Captures in the claim's retained framework logs: {}.", text(&record["captures"])),
            "Captures require visual inspection; passing this scenario alone does not prove pixel placement, hover presentation, device-loss recovery or a native failure injected during construction.".into(),
        ]
    }
}

const REQUIRED_MOUNTED_FILES: &[&str] =
    &["interface/eu4harmony_windows.gui", "interface/eu4harmony_windows.gfx", "gfx/interface/eu4harmony/qa-windows/probe_entry.tga"];

/// Verifies that the final `PHYSFS_init` mount included the framework window
/// definitions plus `qa-windows`'s entry image, and that the game actually read
/// `interface/eu4harmony_windows.gui`.
fn check_framework_files(core_messages: &[String]) -> (usize, Vec<String>) {
    let mounts: Vec<&String> = core_messages.iter().filter(|line| line.starts_with("framework files: mounted ")).collect();
    let mut issues = Vec::new();
    match mounts.last() {
        None => issues.push("core.log recorded no `framework files: mounted` line".into()),
        Some(last_mount) => {
            for file in REQUIRED_MOUNTED_FILES {
                if !last_mount.contains(file) {
                    issues.push(format!("final framework files mount omitted {file:?}: {last_mount}"));
                }
            }
        }
    }
    let served_gui = core_messages.iter().any(|line| line.contains("served \"interface/eu4harmony_windows.gui\""));
    if !served_gui {
        issues.push("core.log did not record serving `interface/eu4harmony_windows.gui`".into());
    }
    (mounts.len(), issues)
}

/// Returns any lines in EU4's `logs/error.log` referencing framework or
/// `qa-windows` assets (e.g. missing sprites or unmounted `.gui`/`.gfx` files).
fn game_error_log_issues(error_log: &str) -> Vec<String> {
    error_log
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("eu4harmony") || lower.contains("qa-windows") || lower.contains("qa_entry")
        })
        .map(|line| line.trim().to_string())
        .collect()
}

fn course(game: &mut Game, name: &str, record: &mut Value) -> Played<()> {
    for step in 1..=STEPS {
        let title = format!("QA windows step {step}");
        let deadline = Instant::now() + Duration::from_secs(45);
        let popup = loop {
            let popups: Vec<Popup> = game.ask_within(&PopupRequest::List {}, Duration::from_secs(15))?;
            if let Some(popup) = popups.into_iter().find(|popup| popup.mod_id == MOD && popup.title == title) {
                break popup;
            }
            game.check(Instant::now() < deadline, format!("{name}: the window probe did not reach step {step}"))?;
            std::thread::sleep(Duration::from_millis(100));
        };
        if matches!(step, 2 | 5) {
            let shot: Screenshot = game.ask(&ScreenshotRequest::Take { scale: Some(50) })?;
            let file = format!("qa-windows-{name}-{step}.png");
            std::fs::write(game.user()?.logs().join(&file), &shot.image.bytes).map_err(|e| Stop::Failed(format!("write {file}: {e}")))?;
            record["captures"].as_array_mut().expect("captures declared").push(json!(file));
        }
        let _: () = game.ask(&PopupRequest::Answer { popup: popup.popup, button: 1 })?;
    }
    // The answer queues its action; another request runs at a later safe
    // point, after the final marker's action and the native synchronization.
    let _: Session = game.session()?;
    record["courses"].as_array_mut().expect("courses declared").push(json!(name));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framework_file_check_requires_complete_final_mount_and_gui_read() {
        let ok = vec![
            "framework files: mounted 4096 bytes: [\"interface/eu4harmony_windows.gui\", \"interface/eu4harmony_windows.gfx\", \"gfx/interface/eu4harmony/qa-windows/probe_entry.tga\"]".to_string(),
            "framework files: mounted 4096 bytes: [\"interface/eu4harmony_windows.gui\", \"interface/eu4harmony_windows.gfx\", \"gfx/interface/eu4harmony/qa-windows/probe_entry.tga\"]".to_string(),
            "served \"interface/eu4harmony_windows.gui\": 2458 bytes".to_string(),
        ];
        assert_eq!(check_framework_files(&ok), (2, Vec::new()));

        let missing_icon = vec![
            "framework files: mounted 1024 bytes: [\"interface/eu4harmony_windows.gui\", \"interface/eu4harmony_windows.gfx\"]".to_string(),
            "served \"interface/eu4harmony_windows.gui\": 2458 bytes".to_string(),
        ];
        let (mounts, issues) = check_framework_files(&missing_icon);
        assert_eq!(mounts, 1);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("probe_entry.tga"), "{issues:?}");

        let never_served = vec![
            "framework files: mounted 4096 bytes: [\"interface/eu4harmony_windows.gui\", \"interface/eu4harmony_windows.gfx\", \"gfx/interface/eu4harmony/qa-windows/probe_entry.tga\"]".to_string(),
        ];
        let (_, issues) = check_framework_files(&never_served);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("interface/eu4harmony_windows.gui"), "{issues:?}");
    }

    #[test]
    fn game_error_log_flags_missing_framework_sprites_and_gui_files() {
        let log = "\
[graphics.cpp:123]: Unrelated vanilla warning\n\
[gui.cpp:45]: Could not open file: interface/eu4harmony_windows.gui, error: not found\n\
[sprite.cpp:88]: Could not find sprite type [GFX_eu4harmony_qa-windows_qa_entry]\n";
        let issues = game_error_log_issues(log);
        assert_eq!(issues.len(), 2);
        assert!(issues[0].contains("eu4harmony_windows.gui"));
        assert!(issues[1].contains("qa-windows"));
    }
}
