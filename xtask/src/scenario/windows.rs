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
        "native extension windows, replacing list rows, pop-up answers, panel closing, extension reload and session teardown; captures retained for visual inspection"
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
        record["completed_steps"] = json!(completed);
        record["extension_errors"] = json!(failures);
        record["native_errors"] = json!(native_errors);
        game.check(completed == (STEPS * 4) as usize, format!("the window probe completed {completed} steps, expected {}", STEPS * 4))?;
        game.check(failures.is_empty(), format!("window probe errors: {failures:?}"))?;
        game.check(native_errors.is_empty(), format!("native window or tooltip errors: {native_errors:?}"))?;
        Ok(())
    }
    fn report(&self, record: &Value) -> Vec<String> {
        vec![
            "### Native window courses".into(), String::new(),
            format!("Courses completed: {}. Steps answered: {}.", text(&record["courses"]), text(&record["completed_steps"])),
            format!("Native errors: {}. Extension errors: {}.", text(&record["native_errors"]), text(&record["extension_errors"])),
            String::new(),
            "Each course applies initial rows, replaces them, clears them and the tooltip, closes and reopens the window, opens another panel and returns to the first. It then leaves that window open for the next reload or session transition. Pop-up markers appear after twelve extension frames; their answers drive the next step.".into(),
            String::new(),
            format!("Captures in the claim's retained framework logs: {}.", text(&record["captures"])),
            "Captures require visual inspection; passing this scenario alone does not prove pixel placement, hover presentation, device-loss recovery or a native failure injected during construction.".into(),
        ]
    }
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
