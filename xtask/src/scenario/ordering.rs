//! `window-ordering`: exercises native window activation, keyboard dispatch
//! order, pointer click activation without replay, and attached army-panel
//! elements (button + derived glow icon) while advancing time.
//!
//! Uses the two ordering fixtures under `xtask/src/scenario/mods/`:
//! 1. `qa-window-ordering`: opens `z_first`, a popup, and `a_last`, lets a text
//!    refresh and duplicate open run, then verifies two Escape presses and
//!    Return dispatch strictly in reverse open order.
//! 2. `qa-window-ordering-pointer`: clicks an exposed button on `first` behind
//!    `second` to verify single-fire activation and subsequent Escape closing
//!    `first` first, then selects an army so `single_unitpanel` opens with its
//!    attached button and glow icon and advances 20 days to cover derived-window
//!    classification on shared native hide methods.
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
    ArmyId,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Ordering;

const KEYBOARD_MOD: &str = "qa-window-ordering";
const POINTER_MOD: &str = "qa-window-ordering-pointer";
const CLEAN_SAVE: &str = "test-regular-1444";

fn mod_path(name: &str) -> PathBuf {
    crate::repo().join("xtask/src/scenario/mods").join(name)
}

impl Scenario for Ordering {
    fn name(&self) -> &'static str {
        "window-ordering"
    }

    fn about(&self) -> &'static str {
        "native window ordering: Escape/Return priority, pointer activation without replay, and army panel attached button+icon while advancing time"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        6
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn saves(&self, _: &Parameters) -> Vec<String> {
        vec![CLEAN_SAVE.into()]
    }

    fn mods(&self, _: &Parameters) -> Vec<PathBuf> {
        vec![mod_path(KEYBOARD_MOD)]
    }

    fn play(&self, game: &mut Game, _: &Parameters, record: &mut Value) -> Played<()> {
        let user = game.user()?;
        let core = Mark::of(&user);
        *record = json!({});

        // Phase 1: keyboard ordering and non-activating text refresh. Loading a
        // saved game avoids the new-game startup_screen dialog.
        let _: Session = game.ask(&MenuRequest::Load { save: CLEAN_SAVE.into() })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        answer_questions(game)?;
        let kb_mark = Mark::of_extension(&user, KEYBOARD_MOD);
        let reloaded: Reloaded = game.ask(&ExtensionRequest::Reload { id: KEYBOARD_MOD.into() })?;
        game.check(reloaded.session, "qa-window-ordering reloaded without an active session")?;
        wait_for_log(game, &kb_mark, KEYBOARD_MOD, "qa-window-ordering text and repeated open applied", Duration::from_secs(30))?;
        press_key(game, "escape")?;
        wait_for_log(game, &kb_mark, KEYBOARD_MOD, "qa-window-ordering last closed; first reopened", Duration::from_secs(20))?;
        press_key(game, "escape")?;
        wait_for_log(game, &kb_mark, KEYBOARD_MOD, "qa-window-ordering reopened first closed by Escape", Duration::from_secs(20))?;
        press_key(game, "enter")?;
        wait_for_log(game, &kb_mark, KEYBOARD_MOD, "qa-window-ordering keyboard passed", Duration::from_secs(20))?;
        let kb_lines = kb_mark.extension_messages(&user, KEYBOARD_MOD);
        let kb_failures = extension_failures(&kb_lines);
        record["keyboard"] = json!({ "passed": true, "errors": kb_failures });
        game.check(kb_failures.is_empty(), format!("qa-window-ordering errors: {kb_failures:?}"))?;
        game.return_to_menu()?;

        // Phase 2: pointer activation and army panel attached button+icon during time advance.
        game.mods(vec![mod_path(POINTER_MOD)]);
        game.restart()?;
        let ui_scale = read_setting(&user.path().join("settings.txt"), "game_ui_scale")
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|&s| s > 0.25 && s <= 4.0)
            .unwrap_or(1.0);
        let _: Session = game.ask(&MenuRequest::Load { save: CLEAN_SAVE.into() })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        answer_questions(game)?;
        let ptr_mark = Mark::of_extension(&user, POINTER_MOD);
        let reloaded: Reloaded = game.ask(&ExtensionRequest::Reload { id: POINTER_MOD.into() })?;
        game.check(reloaded.session, "qa-window-ordering-pointer reloaded without an active session")?;
        wait_for_log(game, &ptr_mark, POINTER_MOD, "qa-ordering-pointer ready", Duration::from_secs(20))?;
        // Wait briefly for the newly opened windows to render and enter hit lists.
        std::thread::sleep(Duration::from_millis(500));
        let _: Session = game.session()?;

        // `first` sits at top_left offset (100, 140), button `activate` at (24, 90)
        // with size (109, 31): GUI center is (178.5, 245.5), scaled by game_ui_scale.
        let click_x = (178.5 * ui_scale).round() as u32;
        let click_y = (245.5 * ui_scale).round() as u32;
        let () = game.ask(&InputRequest::Click { x: click_x, y: click_y, button: Some(Button::Left), count: None, modifiers: None, scale: None })?;
        wait_for_log(game, &ptr_mark, POINTER_MOD, "qa-ordering-pointer first action once", Duration::from_secs(20))?;
        press_key(game, "escape")?;
        wait_for_log(game, &ptr_mark, POINTER_MOD, "qa-ordering-pointer passed", Duration::from_secs(20))?;

        // Select an army so `single_unitpanel` opens with its attached button and
        // glow icon, then advance 20 days (covering the derived-window hide path).
        let selected = select_land_army(game)?;
        let before = game.session()?.date;
        let after = game.run(20)?;
        game.check(after.0 > before.0, format!("time did not advance with army panel open: {before} -> {after}"))?;
        let ptr_lines = ptr_mark.extension_messages(&user, POINTER_MOD);
        let ptr_failures = extension_failures(&ptr_lines);
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
        record["pointer"] = json!({
            "ui_scale": ui_scale,
            "click": [click_x, click_y],
            "passed": true,
            "selected_army": selected.0.to_string(),
            "advanced_from": before.to_string(),
            "advanced_to": after.to_string(),
            "errors": ptr_failures,
        });
        record["native_errors"] = json!(native_errors);
        game.check(ptr_failures.is_empty(), format!("qa-window-ordering-pointer errors: {ptr_failures:?}"))?;
        game.check(native_errors.is_empty(), format!("native window ordering errors: {native_errors:?}"))?;
        game.return_to_menu()
    }

    fn report(&self, record: &Value) -> Vec<String> {
        vec![
            "### Native window ordering".into(),
            String::new(),
            format!("Keyboard course passed: {}.", text(&record["keyboard"]["passed"])),
            format!(
                "Pointer course passed: {} (UI scale {}, click {}).",
                text(&record["pointer"]["passed"]),
                text(&record["pointer"]["ui_scale"]),
                text(&record["pointer"]["click"])
            ),
            format!(
                "Army panel attached controls held open through time advance: army {}, {} to {}.",
                text(&record["pointer"]["selected_army"]),
                text(&record["pointer"]["advanced_from"]),
                text(&record["pointer"]["advanced_to"])
            ),
            format!("Native errors: {}.", text(&record["native_errors"])),
        ]
    }
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
