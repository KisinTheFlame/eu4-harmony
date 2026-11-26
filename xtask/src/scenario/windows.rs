//! `windows`: native extension window lifetime, virtual file mounting, and
//! window activation / input ordering in one scenario (two launches instead of
//! three):
//! 1. `qa-windows`: initial/replaced/cleared list rows, fixed panels, custom
//!    TGA panel entry, tooltip clearing, pop-up answers, reload, in-game load
//!    and session teardown, plus PhysFS virtual file mount and `error.log` checks.
//! 2. `qa-window-ordering`: keyboard reverse open-order Escape/Return dispatch,
//!    non-activating text refresh, pointer click activation without replay, and
//!    army panel (`single_unitpanel`) attached button + glow icon through 20 days.
use super::{game::read_setting, logs::Mark, text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    armies::{Army, ArmyKind, ArmyRequest},
    camera::{Camera, CameraRequest},
    extensions::{ExtensionRequest, Reloaded},
    input::{Button, InputRequest, Keys},
    menu::MenuRequest,
    popups::{Popup, PopupRequest},
    questions::{Question, QuestionRequest},
    screenshot::{Screenshot, ScreenshotRequest},
    session::Session,
    time::TimeRequest,
    ArmyId, CountryTag,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Windows;

const LIFETIME_MOD: &str = "qa-windows";
const ORDERING_MOD: &str = "qa-window-ordering";
const CLEAN_SAVE: &str = "test-regular-1444";
const STEPS: u32 = 7;

fn mod_path(name: &str) -> PathBuf {
    crate::repo().join("xtask/src/scenario/mods").join(name)
}

impl Scenario for Windows {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn about(&self) -> &'static str {
        "native extension windows, PhysFS asset mounts, reload/session teardown, Escape/Return and pointer ordering, and army panel attached controls"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        3
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn saves(&self, _: &Parameters) -> Vec<String> {
        vec![CLEAN_SAVE.into()]
    }

    fn mods(&self, _: &Parameters) -> Vec<PathBuf> {
        vec![mod_path(LIFETIME_MOD)]
    }

    fn play(&self, game: &mut Game, _: &Parameters, record: &mut Value) -> Played<()> {
        let user = game.user()?;
        let core = Mark::of(&user);
        *record = json!({"courses": [], "captures": [], "native_errors": []});

        // Phase 1: native window lifetime, panel entries, reload/load/session teardown,
        // PhysFS mounts and game error.log.
        let extension = Mark::of_extension(&user, LIFETIME_MOD);
        let _: Session = game.ask(&MenuRequest::Start { country: Some(CountryTag(*b"FRA")) })?;
        let load_error = Mark::default()
            .messages(&user.core_log(), eu4_harmony_logging::CORE)
            .into_iter()
            .find(|line| line.contains(&format!("extension \"{LIFETIME_MOD}\" not loaded:")));
        game.check(load_error.is_none(), load_error.unwrap_or_default())?;
        let _: () = game.ask(&TimeRequest::Pause {})?;
        course(game, "new-game", record)?;
        let reloaded: Reloaded = game.ask(&ExtensionRequest::Reload { id: LIFETIME_MOD.into() })?;
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

        let lines = extension.extension_messages(&user, LIFETIME_MOD);
        let completed = lines.iter().filter(|line| line.starts_with("qa-windows completed ")).count();
        let failures = extension_failures(&lines);
        let startup_lines = Mark::default().messages(&user.core_log(), eu4_harmony_logging::CORE);
        let (mount_count, framework_file_issues) = check_framework_files(&startup_lines);
        let error_log = std::fs::read_to_string(user.path().join("logs").join("error.log")).unwrap_or_default();
        let game_error_log_issues = game_error_log_issues(&error_log);
        record["completed_steps"] = json!(completed);
        record["extension_errors"] = json!(failures);
        record["mount_count"] = json!(mount_count);
        record["framework_file_issues"] = json!(framework_file_issues);
        record["game_error_log_issues"] = json!(game_error_log_issues);
        game.check(completed == (STEPS * 4) as usize, format!("the window probe completed {completed} steps, expected {}", STEPS * 4))?;
        game.check(failures.is_empty(), format!("window probe errors: {failures:?}"))?;
        game.check(framework_file_issues.is_empty(), format!("framework virtual file mount issues: {framework_file_issues:?}"))?;
        game.check(game_error_log_issues.is_empty(), format!("game error.log reported framework/extension asset errors: {game_error_log_issues:?}"))?;

        // Phase 2: combined keyboard + pointer ordering and army-panel attached controls
        // in one launch (loading a saved game avoids the new-game startup_screen dialog).
        game.mods(vec![mod_path(ORDERING_MOD)]);
        game.restart()?;
        let ui_scale = read_setting(&user.path().join("settings.txt"), "game_ui_scale")
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|&s| s > 0.25 && s <= 4.0)
            .unwrap_or(1.0);
        let _: Session = game.ask(&MenuRequest::Load { save: CLEAN_SAVE.into() })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        answer_questions(game)?;
        let ord_mark = Mark::of_extension(&user, ORDERING_MOD);
        let reloaded: Reloaded = game.ask(&ExtensionRequest::Reload { id: ORDERING_MOD.into() })?;
        game.check(reloaded.session, "qa-window-ordering reloaded without an active session")?;

        // 2a. Keyboard reverse open-order dispatch and non-activating text refresh.
        wait_for_log(game, &ord_mark, ORDERING_MOD, "qa-window-ordering text and repeated open applied", Duration::from_secs(30))?;
        press_key(game, "escape")?;
        wait_for_log(game, &ord_mark, ORDERING_MOD, "qa-window-ordering last closed; first reopened", Duration::from_secs(20))?;
        press_key(game, "escape")?;
        wait_for_log(game, &ord_mark, ORDERING_MOD, "qa-window-ordering reopened first closed by Escape", Duration::from_secs(20))?;
        press_key(game, "enter")?;
        wait_for_log(game, &ord_mark, ORDERING_MOD, "qa-window-ordering keyboard passed", Duration::from_secs(20))?;
        wait_for_log(game, &ord_mark, ORDERING_MOD, "qa-ordering-pointer ready", Duration::from_secs(20))?;

        // 2b. Pointer click activation without replay; wait briefly for hit lists.
        std::thread::sleep(Duration::from_millis(500));
        let _: Session = game.session()?;
        // `ptr_first` sits at top_left offset (100, 140), button `activate` at (24, 90)
        // with size (109, 31): GUI center is (178.5, 245.5), scaled by game_ui_scale.
        let click_x = (178.5 * ui_scale).round() as u32;
        let click_y = (245.5 * ui_scale).round() as u32;
        let () = game.ask(&InputRequest::Click { x: click_x, y: click_y, button: Some(Button::Left), count: None, modifiers: None, scale: None })?;
        wait_for_log(game, &ord_mark, ORDERING_MOD, "qa-ordering-pointer first action once", Duration::from_secs(20))?;
        press_key(game, "escape")?;
        wait_for_log(game, &ord_mark, ORDERING_MOD, "qa-ordering-pointer passed", Duration::from_secs(20))?;

        // 2c. Select an army so `single_unitpanel` opens with its attached button and
        // glow icon, then advance 20 days (covering the derived-window hide path).
        let selected = select_land_army(game)?;
        let before = game.session()?.date;
        let after = game.run(20)?;
        game.check(after.0 > before.0, format!("time did not advance with army panel open: {before} -> {after}"))?;

        let ord_lines = ord_mark.extension_messages(&user, ORDERING_MOD);
        let ord_failures = extension_failures(&ord_lines);
        let native_errors: Vec<String> = core
            .since(&user.core_log())
            .into_iter()
            .filter(|line| {
                (line.contains(" windows:") && !line.contains("windows: reading "))
                    || line.contains("window ordering:")
                    || line.contains("window activation:")
                    || line.contains("late native tooltip:")
            })
            .collect();
        record["ordering"] = json!({
            "keyboard_passed": true,
            "pointer_passed": true,
            "ui_scale": ui_scale,
            "click": [click_x, click_y],
            "selected_army": selected.0.to_string(),
            "advanced_from": before.to_string(),
            "advanced_to": after.to_string(),
            "errors": ord_failures,
        });
        record["native_errors"] = json!(native_errors);
        game.check(ord_failures.is_empty(), format!("qa-window-ordering errors: {ord_failures:?}"))?;
        game.check(native_errors.is_empty(), format!("native window or ordering errors: {native_errors:?}"))?;
        game.return_to_menu()
    }

    fn report(&self, record: &Value) -> Vec<String> {
        vec![
            "### Native window courses".into(),
            String::new(),
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
            format!(
                "Ordering: keyboard {}, pointer {} (UI scale {}, click {}), army panel attached controls held open through time advance (army {}, {} to {}).",
                text(&record["ordering"]["keyboard_passed"]),
                text(&record["ordering"]["pointer_passed"]),
                text(&record["ordering"]["ui_scale"]),
                text(&record["ordering"]["click"]),
                text(&record["ordering"]["selected_army"]),
                text(&record["ordering"]["advanced_from"]),
                text(&record["ordering"]["advanced_to"]),
            ),
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
            if let Some(popup) = popups.into_iter().find(|popup| popup.mod_id == LIFETIME_MOD && popup.title == title) {
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

fn press_key(game: &mut Game, name: &str) -> Played<()> {
    let keys = Keys::parse(name).map_err(Stop::Failed)?;
    game.ask(&InputRequest::Key { keys, repeat: None })
}

/// Answers any open country events so keyboard input reaches the fixture.
fn answer_questions(game: &mut Game) -> Played<()> {
    let questions: Vec<Question> = game.ask(&QuestionRequest::List {})?;
    for q in questions {
        if let Some(first) = q.options.first() {
            let () = game.ask(&QuestionRequest::Answer { question: q.id, option: first.option })?;
        }
    }
    let _: Session = game.session()?;
    Ok(())
}

fn wait_for_log(game: &mut Game, mark: &Mark, id: &str, needle: &str, timeout: Duration) -> Played<()> {
    let user = game.user()?;
    let deadline = Instant::now() + timeout;
    loop {
        let lines = mark.extension_messages(&user, id);
        if lines.iter().any(|l| l.contains(needle)) {
            return Ok(());
        }
        let errors = extension_failures(&lines);
        if !errors.is_empty() {
            return Err(Stop::Failed(format!("{id} failed while waiting for {needle:?}: {errors:?}")));
        }
        if Instant::now() >= deadline {
            if let Ok(shot) = game.ask::<Screenshot>(&ScreenshotRequest::Take { scale: Some(50) }) {
                let _ = std::fs::write(user.logs().join(format!("{id}-timeout.png")), &shot.image.bytes);
            }
            let popups: Vec<Popup> = game.ask(&PopupRequest::List {}).unwrap_or_default();
            let questions: Vec<Question> = game.ask(&QuestionRequest::List {}).unwrap_or_default();
            return Err(Stop::Failed(format!(
                "{id} did not log {needle:?} within {} s; popups={:?}; questions={:?}; last lines: {:?}",
                timeout.as_secs(),
                popups.iter().map(|p| (&p.mod_id, &p.title)).collect::<Vec<_>>(),
                questions.iter().map(|q| (&q.id, &q.title)).collect::<Vec<_>>(),
                lines.iter().rev().take(5).collect::<Vec<_>>()
            )));
        }
        let _: Session = game.session()?;
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn extension_failures(lines: &[String]) -> Vec<String> {
    lines.iter().filter(|l| l.contains("error:") || l.starts_with("stopped:") || l.contains("assertion failed")).cloned().collect()
}

/// Centers the camera on one of the player's land armies and box-selects it
/// around the viewport center, confirming selection through `army.selected`.
fn select_land_army(game: &mut Game) -> Played<ArmyId> {
    let armies: Vec<Army> = game.ask(&ArmyRequest::List {})?;
    let land: Vec<&Army> = armies.iter().filter(|a| a.kind == ArmyKind::Army).collect();
    game.check(!land.is_empty(), "the country has no land army to select")?;
    let shot: Screenshot = game.ask(&ScreenshotRequest::Take { scale: None })?;
    let (width, height) = png_dimensions(&shot.image.bytes).ok_or_else(|| Stop::Failed("could not read screenshot PNG dimensions".into()))?;
    let (cx, cy) = (width / 2, height / 2);

    for army in land.iter().filter(|a| a.province.is_some()).take(3) {
        let _: Camera = game.ask(&CameraRequest::LookAt { province: army.province, x: None, y: None, glide: Some(false) })?;
        let _: Camera = game.ask(&CameraRequest::Zoom { height: 220 })?;
        std::thread::sleep(Duration::from_millis(300));
        let _: Session = game.session()?;
        for half in [120u32, 240u32, 360u32] {
            let () = game.ask(&InputRequest::Drag {
                from_x: cx.saturating_sub(half),
                from_y: cy.saturating_sub(half),
                to_x: (cx + half).min(width.saturating_sub(1)),
                to_y: (cy + half).min(height.saturating_sub(1)),
                button: Some(Button::Left),
                modifiers: None,
                scale: None,
            })?;
            std::thread::sleep(Duration::from_millis(250));
            if let Ok(selected) = game.try_ask::<ArmyId>(&ArmyRequest::Selected {}, Duration::from_secs(15))? {
                return Ok(selected);
            }
        }
    }
    Err(Stop::Failed("could not select a land army to open single_unitpanel".into()))
}

/// Reads width and height from a PNG's IHDR chunk (bytes 16..24).
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (width > 0 && height > 0).then_some((width, height))
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

    #[test]
    fn png_dimensions_reads_ihdr_width_and_height() {
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&3840u32.to_be_bytes());
        png.extend_from_slice(&2160u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((3840, 2160)));
        assert_eq!(png_dimensions(b"not a png"), None);
    }
}
