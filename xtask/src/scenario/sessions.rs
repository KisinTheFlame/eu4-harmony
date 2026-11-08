//! `sessions`: every way into and out of a session, round after round, on
//! the player build (docs/plans/whole-game.md). Each round, from the main menu:
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
use super::{text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    menu::{Menu, MenuRequest, Save},
    questions::{Question, QuestionRequest},
    session::{Session, SessionRequest},
    time::TimeRequest,
    CountryTag,
};
use serde_json::{json, Value};
use std::time::Instant;

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
            Parameter { name: "first_save", default: "England-1", about: "the save each round loads first" },
            Parameter { name: "second_save", default: "英格兰1444_11_11", about: "the save each round loads from the first one's game" },
            Parameter { name: "ironman_save", default: "Castilla-5", about: "an ironman save, loaded each round" },
        ]
    }

    fn saves(&self, parameters: &Parameters) -> Vec<String> {
        ["first_save", "second_save", "ironman_save"].iter().map(|p| parameters.text(p)).collect()
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let rounds = parameters.number("rounds")?;
        let country = parameters.text("country");
        let tag = CountryTag::parse(&country).ok_or_else(|| Stop::Failed(format!("no country tag {country:?}")))?;
        let [first, second, ironman] = ["first_save", "second_save", "ironman_save"].map(|p| parameters.text(p));
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
        md
    }
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
