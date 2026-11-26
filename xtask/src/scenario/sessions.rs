//! `sessions`: every way into and out of a session, round after round, on
//! the player build . Each round, from the main menu:
//! 1. loads the first save, lets days run, loads the second in the game, lets
//!    days run;
//! 2. returns; starts a new game as the country, lets days run, writes the
//!    console's autosave and loads it in the game, lets days run;
//! 3. returns; starts a game in observe mode, lets days run, puts the country
//!    in view and takes it out, returns;
//! 4. loads the ironman save, lets days run, loads the first save in the
//!    game, an in-game load in ironman, and returns;
//! 5. starts a new game, and while its time runs at speed 5, loads the first
//!    save in the game, starts a new game from that one and returns.
//!
//! Each step must answer as asked: every session a new number, its country
//! and date the save's or the new game's, and every return to the main menu
//! in this process, the in-process return.
use super::{
    game::{read_setting, set_setting},
    text, Build, Game, Parameter, Parameters, Played, Scenario, Stop,
};
use eu4_harmony_remote::{
    menu::{Menu, MenuRequest, Save},
    questions::{Question, QuestionRequest},
    screenshot::{Screenshot, ScreenshotRequest},
    session::{Session, SessionRequest},
    time::TimeRequest,
    CountryTag,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct Sessions;

/// The days each game in a round runs.
const DAYS: u32 = 10;

impl Scenario for Sessions {
    fn name(&self) -> &'static str {
        "sessions"
    }

    fn about(&self) -> &'static str {
        "every way into and out of a session, round after round: loads, in-game loads, new games, the autosave, observe mode, ironman and returns"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        40
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter { name: "rounds", default: "10", about: "rounds of every way in and out" },
            Parameter { name: "country", default: "FRA", about: "the country of each round's new game, and the one put in view in observe mode" },
            Parameter { name: "first_save", default: "test-regular-1444", about: "the save each round loads first" },
            Parameter { name: "second_save", default: "test-中文-1444", about: "the save each round loads from the first one's game" },
            Parameter { name: "ironman_save", default: "test-ironman-1444", about: "an ironman save, loaded each round" },
        ]
    }

    fn saves(&self, parameters: &Parameters) -> Vec<String> {
        ["first_save", "second_save", "ironman_save"].iter().map(|p| parameters.text(p)).collect()
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let rounds = parameters.number("rounds")?;
        let country = parameters.text("country");
        let tag = CountryTag::parse(&country).ok_or_else(|| Stop::Failed(format!("no country tag {country:?}")))?;
        let [first, second, ironman] = ["first_save", "second_save", "ironman_save"].map(|p| crate::saves::save_name(&parameters.text(p)));
        // Force uncompressed autosaves in this claim so every round's in-game
        // autosave load exercises the buffered local text save path alongside
        // the zipped and ironman saves.
        let user = game.user()?;
        game.end();
        set_setting(&user.path().join("settings.txt"), "compress_autosave", "no")?;
        game.restart()?;
        let there: Vec<Save> = game.ask(&MenuRequest::Saves {})?;
        for save in [&first, &second, &ironman] {
            game.check(there.iter().any(|s| &s.name == save), format!("no save {save} on this computer"))?;
        }
        *record = json!({ "rounds": [] });
        let mut steps = Steps { last: 0, process: game.ask::<Menu>(&MenuRequest::State {})?.process, record: Vec::new() };
        for round in 1..=rounds {
            let began = Instant::now();
            steps.load(game, &first)?;
            game.run(DAYS)?;
            steps.load(game, &second)?;
            game.run(DAYS)?;
            steps.back(game, "a return after an in-game load")?;

            steps.new_game(game, Some(tag))?;
            if round == 1 {
                // Verify country shield rendering right after returning to the
                // main menu and starting a new game (regression check for
                // flag-atlas texture reuse across world resets).
                let shield = verify_player_shield(game, &user)?;
                record["shield_check"] = json!({
                    "width": shield.width,
                    "height": shield.height,
                    "mean_luma": (shield.mean_luma * 10.0).round() / 10.0,
                    "non_black_ratio": (shield.non_black_ratio * 1000.0).round() / 1000.0,
                });
            }
            game.run(DAYS)?;
            let at = game.session()?;
            let loaded = steps.timed("the autosave and its load", |_| game.autosave_and_load())?;
            steps.session(game, &loaded, Some(tag), false, "the autosave's load")?;
            game.check(loaded.date == at.date, format!("the autosave loaded on {}, written on {}", loaded.date, at.date))?;
            game.run(DAYS)?;
            steps.back(game, "a return after the autosave's load")?;

            steps.new_game(game, None)?;
            game.run(DAYS)?;
            let viewed: Session = steps.timed("session.view", |_| game.ask(&SessionRequest::View { country: tag }))?;
            game.check(viewed.player == Some(tag) && viewed.observing, format!("session.view {country} answered {viewed:?}"))?;
            let left: Session = steps.timed("session.leave_view", |_| game.ask(&SessionRequest::LeaveView {}))?;
            game.check(left.player.is_none() && left.observing, format!("session.leave_view answered {left:?}"))?;
            game.run(DAYS)?;
            steps.back(game, "a return from observe mode")?;

            steps.load(game, &ironman)?;
            game.run(DAYS)?;
            steps.load(game, &first)?;
            steps.back(game, "a return after an in-game load in ironman")?;

            // Each way out again, asked while time runs.
            steps.new_game(game, Some(tag))?;
            steps.running(game)?;
            steps.load(game, &first)?;
            steps.running(game)?;
            steps.new_game(game, Some(tag))?;
            steps.running(game)?;
            steps.back(game, "a return asked while time ran")?;

            let seconds = began.elapsed().as_secs_f64();
            println!("round {round}: {seconds:.0} s");
            record["rounds"].as_array_mut().expect("rounds").push(json!({ "round": round, "seconds": seconds.round() }));
            record["steps"] = json!(steps.record);
        }
        Ok(())
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let mut md = vec!["### The rounds".to_string(), String::new(), "| Round | Seconds |".into(), "| --- | --- |".into()];
        for r in record["rounds"].as_array().into_iter().flatten() {
            md.push(format!("| {} | {} |", text(&r["round"]), text(&r["seconds"])));
        }
        let mut steps: Vec<&Value> = record["steps"].as_array().into_iter().flatten().collect();
        steps.sort_by(|a, b| b["seconds"].as_f64().unwrap_or(0.0).total_cmp(&a["seconds"].as_f64().unwrap_or(0.0)));
        md.extend([String::new(), "### The steps that took longest".into(), String::new(), "| Step | Seconds |".into(), "| --- | --- |".into()]);
        for s in steps.iter().take(10) {
            md.push(format!("| {} | {} |", text(&s["step"]), text(&s["seconds"])));
        }
        if record["shield_check"].is_object() {
            md.extend([
                String::new(),
                format!(
                    "Post-return player shield check ({}x{}): mean luma {}, non-black ratio {}.",
                    text(&record["shield_check"]["width"]),
                    text(&record["shield_check"]["height"]),
                    text(&record["shield_check"]["mean_luma"]),
                    text(&record["shield_check"]["non_black_ratio"]),
                ),
            ]);
        }
        md
    }
}

/// Summary statistics of the top-left player country shield interior.
#[derive(Clone, Copy, Debug)]
struct ShieldStats {
    width: u32,
    height: u32,
    mean_luma: f64,
    non_black_ratio: f64,
}

/// Captures a frame right after returning to the main menu and starting a new
/// game, then verifies the top-left player shield is rendered (not blank black).
fn verify_player_shield(game: &mut Game, user: &eu4_harmony_folders::UserFolder) -> Played<ShieldStats> {
    let () = game.ask(&TimeRequest::Pause {})?;
    std::thread::sleep(Duration::from_millis(300));
    let _: Session = game.session()?;
    let shot: Screenshot = game.ask(&ScreenshotRequest::Take { scale: Some(100) })?;
    let ui_scale =
        read_setting(&user.path().join("settings.txt"), "game_ui_scale").and_then(|s| s.parse::<f64>().ok()).filter(|&s| s > 0.25 && s <= 4.0).unwrap_or(1.0);
    let (width, height, bgr) = decode_png_bgr(&shot.image.bytes).map_err(|e| Stop::Failed(format!("decode shield screenshot: {e}")))?;
    let scale_x = ui_scale * (width as f64 / shot.frame_width.max(1) as f64);
    let scale_y = ui_scale * (height as f64 / shot.frame_height.max(1) as f64);
    let stats = inspect_player_shield(width, height, &bgr, scale_x, scale_y)
        .ok_or_else(|| Stop::Failed(format!("shield ROI out of bounds for {width}x{height} frame (ui_scale={ui_scale})")))?;
    game.check(
        stats.mean_luma >= 20.0 && stats.non_black_ratio >= 0.30,
        format!(
            "player shield appears blank after return-to-menu -> new game (mean_luma={:.1}, non_black_ratio={:.3})",
            stats.mean_luma, stats.non_black_ratio
        ),
    )?;
    Ok(stats)
}

/// Inspects the interior of EU4's top-left player shield (`topbar.gui`:
/// `[30..84] x [22..80]` in GUI units) in a 24-bit BGR image.
fn inspect_player_shield(width: u32, height: u32, bgr: &[u8], scale_x: f64, scale_y: f64) -> Option<ShieldStats> {
    let x0 = (30.0 * scale_x).round().max(0.0) as u32;
    let x1 = (84.0 * scale_x).round().min(width as f64) as u32;
    let y0 = (22.0 * scale_y).round().max(0.0) as u32;
    let y1 = (80.0 * scale_y).round().min(height as f64) as u32;
    if x1 <= x0 || y1 <= y0 || bgr.len() < (width as usize * height as usize * 3) {
        return None;
    }
    let mut count = 0u64;
    let mut luma_sum = 0.0f64;
    let mut non_black = 0u64;
    for y in y0..y1 {
        let row = y as usize * width as usize * 3;
        for x in x0..x1 {
            let at = row + x as usize * 3;
            let (b, g, r) = (bgr[at], bgr[at + 1], bgr[at + 2]);
            luma_sum += 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
            if r.max(g).max(b) >= 30 {
                non_black += 1;
            }
            count += 1;
        }
    }
    Some(ShieldStats { width, height, mean_luma: luma_sum / count as f64, non_black_ratio: non_black as f64 / count as f64 })
}

/// Decodes a PNG byte slice to `(width, height, 24-bit BGR pixels)` via WIC.
fn decode_png_bgr(png: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    let com_init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let result = (|| -> windows::core::Result<(u32, u32, Vec<u8>)> {
        unsafe {
            let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let stream = factory.CreateStream()?;
            stream.InitializeFromMemory(png)?;
            let decoder = factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
            let frame = decoder.GetFrame(0)?;
            let (mut width, mut height) = (0u32, 0u32);
            frame.GetSize(&mut width, &mut height)?;
            let rgb = factory.CreateFormatConverter()?;
            rgb.Initialize(&frame, &GUID_WICPixelFormat24bppBGR, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
            let mut pixels = vec![0u8; (width as usize) * (height as usize) * 3];
            rgb.CopyPixels(std::ptr::null(), width * 3, &mut pixels)?;
            Ok((width, height, pixels))
        }
    })();
    if com_init {
        unsafe { CoUninitialize() };
    }
    result.map_err(|e| e.to_string())
}

/// The steps of the rounds: the last session's number, the process every
/// return must stay in, and how long each step took.
struct Steps {
    last: u64,
    process: u64,
    record: Vec<Value>,
}

impl Steps {
    fn timed<T>(&mut self, step: &str, f: impl FnOnce(&mut Self) -> Played<T>) -> Played<T> {
        let began = Instant::now();
        let done = f(self)?;
        self.record.push(json!({ "step": step, "seconds": (began.elapsed().as_secs_f64() * 10.0).round() / 10.0 }));
        Ok(done)
    }

    /// Loads `save`, from the main menu or in the game, and checks the
    /// session it began against the save's header.
    fn load(&mut self, game: &mut Game, save: &str) -> Played<()> {
        let saves: Vec<Save> = game.ask(&MenuRequest::Saves {})?;
        let header = saves.into_iter().find(|s| s.name == save).ok_or_else(|| Stop::Failed(format!("the save {save} is gone")))?;
        let session: Session = self.timed(&format!("menu.load {save}"), |_| game.ask(&MenuRequest::Load { save: save.into() }))?;
        let what = format!("the load of {save}");
        self.session(game, &session, header.player, session.observing, &what)?;
        if let Some(date) = header.date {
            game.check(session.date == date, format!("{what} began on {}, the save's date {date}", session.date))?;
        }
        Ok(())
    }

    /// Starts a new game as `country`, or in observe mode with none.
    fn new_game(&mut self, game: &mut Game, country: Option<CountryTag>) -> Played<()> {
        let step = match country {
            Some(tag) => format!("menu.start {tag}"),
            None => "menu.start in observe mode".into(),
        };
        let session: Session = self.timed(&step, |_| game.ask(&MenuRequest::Start { country }))?;
        self.session(game, &session, country, country.is_none(), &step)
    }

    /// Checks a new session: a new number, the country and whether it
    /// observes as asked.
    fn session(&mut self, game: &Game, session: &Session, player: Option<CountryTag>, observing: bool, what: &str) -> Played<()> {
        game.check(session.number > self.last, format!("{what} answered session {}, after session {}", session.number, self.last))?;
        game.check(session.player == player && session.observing == observing, format!("{what} answered {session:?}, not {player:?} observing {observing}"))?;
        self.last = session.number;
        Ok(())
    }

    /// Lets time run at speed 5 for a few seconds, and leaves it running.
    fn running(&mut self, game: &mut Game) -> Played<()> {
        let questions: Vec<Question> = game.ask(&QuestionRequest::List {})?;
        for q in questions {
            if let Some(first) = q.options.first() {
                let () = game.ask(&QuestionRequest::Answer { question: q.id, option: first.option })?;
            }
        }
        let () = game.ask(&TimeRequest::SetSpeed { speed: 5 })?;
        let () = game.ask(&TimeRequest::Resume {})?;
        std::thread::sleep(std::time::Duration::from_secs(3));
        Ok(())
    }

    /// Returns to the main menu, which must come in this process.
    fn back(&mut self, game: &mut Game, what: &str) -> Played<()> {
        self.timed(what, |_| game.return_to_menu())?;
        let menu: Menu = game.ask(&MenuRequest::State {})?;
        game.check(menu.process == self.process, format!("{what} started the game again (process {} then {})", self.process, menu.process))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shield_inspector_distinguishes_blank_black_from_rendered_flag() {
        let (w, h) = (128u32, 128u32);
        let blank = vec![0u8; (w * h * 3) as usize];
        let blank_stats = inspect_player_shield(w, h, &blank, 1.0, 1.0).unwrap();
        assert!(blank_stats.mean_luma < 1.0 && blank_stats.non_black_ratio < 0.01);

        let mut colored = vec![0u8; (w * h * 3) as usize];
        for y in 22..80 {
            for x in 30..84 {
                let at = ((y * w + x) * 3) as usize;
                // France royal blue (BGR = 170, 50, 20)
                colored[at] = 170;
                colored[at + 1] = 50;
                colored[at + 2] = 20;
            }
        }
        let ok_stats = inspect_player_shield(w, h, &colored, 1.0, 1.0).unwrap();
        assert!(ok_stats.mean_luma >= 20.0 && ok_stats.non_black_ratio >= 0.99, "{ok_stats:?}");
    }
}
