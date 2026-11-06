//! The main menu, remote control only: where the game is, the saves, loading
//! one, starting a game, returning to the main menu.
//!
//! A load or a start answers once the game is ready, at its first frame,
//! with the session; it fails as soon as the main menu says the game did not
//! take it up, and a load fails when the game that came is not the save's.
//! Returning to the main menu starts `eu4.exe` again, so it is answered at
//! once; at the game's restart the engine answers every request `not_ready`
//! ([`ending`], `Engine::end`), and the new process takes the pipe over.
use super::{process, Convert, Live, Outcome, Poll, Wait};
use crate::game::{
    self,
    menu::{Expected, ExpectedProbe, Launch, Returned},
    GResult, Status,
};
use eu4_harmony_remote::{
    menu::{Menu, MenuAnswer, MenuRequest, Place, Save},
    session::Session,
    Answer, Code, CountryTag, Date, Error, Response,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn place(&mut self) -> GResult<game::menu::Place>;
    fn saves(&mut self) -> GResult<Vec<game::menu::Save>>;
    /// Asks for the save to be loaded and played; what its header says the
    /// game will be, asked once the game came.
    fn load(&mut self, save: &str) -> GResult<ExpectedProbe>;
    /// Asks for a new game as the country.
    fn start(&mut self, country: CountryTag) -> GResult<()>;
    /// What came of the load or start just asked for so far, asked again at
    /// each safe point until the game's first frame.
    fn launch(&mut self) -> Box<dyn FnMut() -> Launch + Send>;
    /// Asks for a return to the main menu: with an in-process return, what
    /// came of it so far, asked again at each safe point; None when the game
    /// starts again.
    fn return_to_main_menu(&mut self) -> GResult<Option<Box<dyn FnMut() -> Returned + Send>>>;
}

impl Layer for Live {
    fn place(&mut self) -> GResult<game::menu::Place> {
        game::menu::place(self.0)
    }

    fn saves(&mut self) -> GResult<Vec<game::menu::Save>> {
        game::menu::saves(self.0)
    }

    fn load(&mut self, save: &str) -> GResult<ExpectedProbe> {
        game::menu::load(self.0, save)
    }

    fn start(&mut self, country: CountryTag) -> GResult<()> {
        game::menu::start(self.0, country.0)
    }

    fn launch(&mut self) -> Box<dyn FnMut() -> Launch + Send> {
        let (caller, number) = (self.0, game::menu::last(self.0));
        Box::new(move || game::menu::launch(caller, number))
    }

    fn return_to_main_menu(&mut self) -> GResult<Option<Box<dyn FnMut() -> Returned + Send>>> {
        let caller = self.0;
        Ok(game::menu::return_to_main_menu(caller)?.map(|number| Box::new(move || game::menu::returned(caller, number)) as Box<_>))
    }
}

/// The group's slice is written above by hand, without `layer!`'s lines.
#[cfg(test)]
pub fn bindings() -> super::Bindings {
    super::Bindings::default()
}

/// What every request is answered once the game starts `eu4.exe` again to
/// return to the main menu, unless it has an answer of its own then.
pub fn ending() -> Error {
    Error::new(
        Code::NotReady,
        "the game is returning to the main menu: eu4.exe starts again, and the new process answers on the pipe once its main menu shows; ask again",
    )
}

pub fn run(request: MenuRequest, game: &mut (impl Layer + ?Sized)) -> Outcome {
    let done = |result: Result<MenuAnswer, Error>| Outcome::Done(result.map(Answer::Menu));
    match request {
        MenuRequest::State {} => done(game.place().map_err(Error::from).map(|place| MenuAnswer::State(Menu { process: process(), place: place_of(place) }))),
        MenuRequest::Saves {} => done(game.saves().map_err(Error::from).map(|saves| MenuAnswer::Saves(saves.into_iter().map(save).collect()))),
        MenuRequest::Load { save } => match game.load(&save) {
            Ok(expected) => Outcome::Later(Box::new(Starting { probe: game.launch(), answer: MenuAnswer::Load, expected: Some(expected) })),
            Err(fail) => done(Err(fail.into())),
        },
        MenuRequest::Start { country } => match game.start(country) {
            Ok(()) => Outcome::Later(Box::new(Starting { probe: game.launch(), answer: MenuAnswer::Start, expected: None })),
            Err(fail) => done(Err(fail.into())),
        },
        MenuRequest::Return {} => match game.return_to_main_menu() {
            Ok(None) => done(Ok(MenuAnswer::Return(Menu { process: process(), place: Place::Ending }))),
            Ok(Some(probe)) => Outcome::Later(Box::new(Returning { probe })),
            Err(fail) => done(Err(fail.into())),
        },
    }
}

/// An in-process return waiting for the main menu.
struct Returning {
    probe: Box<dyn FnMut() -> Returned + Send>,
}

impl Wait for Returning {
    fn poll(&mut self, _: &mut Poll<'_>) -> Option<Response> {
        match (self.probe)() {
            Returned::Waiting => None,
            Returned::Failed(why) => Some(Err(Error::new(Code::Rejected, why))),
            Returned::Done => Some(Ok(Answer::Menu(MenuAnswer::Return(Menu { process: process(), place: Place::MainMenu })))),
        }
    }
}

fn place_of(place: game::menu::Place) -> Place {
    match place {
        game::menu::Place::MainMenu => Place::MainMenu,
        game::menu::Place::Starting => Place::Starting,
        game::menu::Place::Game => Place::Game,
    }
}

fn save(s: game::menu::Save) -> Save {
    Save {
        name: s.name,
        file: s.file,
        bytes: s.bytes,
        modified: s.modified,
        player: s.player.map(CountryTag),
        date: s.date.map(Date),
        missing_mods: s.missing_mods,
    }
}

/// A load or start waiting for the game's first frame.
struct Starting {
    probe: Box<dyn FnMut() -> Launch + Send>,
    answer: fn(Session) -> MenuAnswer,
    /// For a load, the game its save's header says will come.
    expected: Option<ExpectedProbe>,
}

impl Wait for Starting {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response> {
        match (self.probe)() {
            Launch::Failed(why) => return Some(Err(Error::new(Code::Rejected, why))),
            // A start from a session leaves it first, at its next frame.
            Launch::Asked => return None,
            Launch::Waiting => {}
        }
        // The game came, but its frames have no game to play: no frame
        // after it would answer otherwise.
        if let Some(why) = at.unplayable() {
            return Some(Err(Error::new(
                Code::Rejected,
                format!("the game began, but the player cannot play it ({}): menu says game, and it goes on", why.message),
            )));
        }
        let game = at.game()?;
        match game.session() {
            Ok(s) => Some(match self.expected.as_mut().map(|probe| probe()).transpose() {
                Err(fail) => Err(fail.into()),
                Ok(expected) => match expected.and_then(|e| other_game(&e, &s)) {
                    Some(why) => Err(Error::new(Code::Rejected, why)),
                    None => Ok(Answer::Menu((self.answer)(s.convert()))),
                },
            }),
            Err(fail) if fail.status == Status::NotReady => None,
            Err(fail) => Some(Err(fail.into())),
        }
    }
}

/// Why the game that came after a load is not the save's, if it is not: the
/// game began another (a new game at the start date, say, when it could not
/// read the save).
fn other_game(expected: &Expected, s: &game::session::Session) -> Option<String> {
    let player = s.player.own().letters();
    let other_date = expected.date.is_some_and(|date| date != s.date);
    (player != expected.player || other_date).then(|| {
        let save = match expected.date {
            Some(date) => format!("of {} as {}", Date(date), CountryTag(expected.player)),
            None => format!("played as {}", CountryTag(expected.player)),
        };
        format!(
            "the game did not load the save: it began a game of {} as {}, where the save is {save}; that game is going on",
            Date(s.date),
            CountryTag(player)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        fake::{answer, ok, Fake, PLENTY},
        menu, process, Place,
    };
    use crate::game;
    use crate::remote::Engine;
    use eu4_harmony_remote::{events::EventsRequest, menu::MenuRequest, session::SessionRequest, texts::TextRequest, Code, CountryTag, Error, Request};
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn error(code: &str) -> impl Fn(&Value) -> bool + '_ {
        move |v: &Value| v["ok"] == false && v["error"]["code"] == code
    }

    fn remember(_: &str) {}

    #[test]
    fn a_load_or_start_answers_with_the_session_once_the_game_is_ready() {
        let engine = Engine::new(remember);
        let mut game = Fake::default();
        let menu = Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open")));
        let load = engine.submit(Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }));
        // Asked for in the main menu; the main menu's frames go by while the
        // game takes it up, then a save loads, with no safe point at all.
        engine.run(&mut game, &menu, PLENTY);
        engine.run(&mut game, &Place::Outside(None), PLENTY);
        assert!(answer(&load).is_null());
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(answer(&load), ok(json!({"number": 2, "player": "TUR", "observing": false, "date": "1444.11.11"})));
        assert_eq!(game.calls, ["load 奥斯曼1444_11_11", "session"]);

        // A load that comes to another game than its save's says so.
        let castile = engine.submit(Request::Menu(MenuRequest::Load { save: "Castilla-5".into() }));
        engine.run(&mut game, &menu, PLENTY);
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(
            answer(&castile),
            json!({"ok": false, "error": {"code": "rejected", "message": "the game did not load the save: it began a game of 1444.11.11 as TUR, where the save is of 1470.3.13 as CAS; that game is going on"}})
        );

        // What the game refuses is answered at once, or as soon as it says so.
        let missing = engine.submit(Request::Menu(MenuRequest::Load { save: "missing".into() }));
        let start = engine.submit(Request::Menu(MenuRequest::Start { country: CountryTag(*b"FRA") }));
        engine.run(&mut game, &menu, PLENTY);
        assert!(error("not_found")(&answer(&missing)));
        assert!(answer(&start).is_null());
        *game.launch.lock().unwrap() = game::menu::Launch::Failed("the single player setup did not take the country".into());
        engine.run(&mut game, &menu, PLENTY);
        assert_eq!(answer(&start), json!({"ok": false, "error": {"code": "rejected", "message": "the single player setup did not take the country"}}));
    }

    #[test]
    fn loading_starting_and_returning_wait_while_it_is_not_known_where_the_game_is() {
        // A frame the game is not ready in, between a game's frames: neither the
        // main menu nor a game is known. Where the game is answers; loading and
        // returning, which do one thing in the main menu and another in a game,
        // wait for a frame that knows which.
        let engine = Engine::new(remember);
        let mut game = Fake::default();
        let state = engine.submit(Request::Menu(MenuRequest::State {}));
        let load = engine.submit(Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }));
        let back = engine.submit(Request::Menu(MenuRequest::Return {}));
        engine.run(&mut game, &Place::Outside(None), PLENTY);
        assert_eq!(answer(&state)["ok"], true);
        assert!(answer(&load).is_null() && answer(&back).is_null());
        assert_eq!(game.calls, ["place"]);
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(game.calls, ["place", "load 奥斯曼1444_11_11", "return"]);
        assert_eq!(answer(&back), ok(json!({"process": process(), "place": "ending"})));
    }

    #[test]
    fn a_load_or_start_whose_game_cannot_be_played_says_so_at_its_first_frame() {
        let engine = Engine::new(remember);
        let mut game = Fake::default();
        let load = engine.submit(Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }));
        let start = engine.submit(Request::Menu(MenuRequest::Start { country: CountryTag(*b"FRA") }));
        engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open"))), PLENTY);
        // Every frame of the game that came fails: without this answer, they
        // would wait until the process ends.
        let unplayable = Place::Unplayable(Error::new(Code::NotInGame, "no controllable player"));
        let session = engine.submit(Request::Session(SessionRequest::Get {}));
        engine.run(&mut game, &unplayable, PLENTY);
        for got in [answer(&load), answer(&start)] {
            assert_eq!(
                got,
                json!({"ok": false, "error": {"code": "rejected", "message": "the game began, but the player cannot play it (no controllable player): menu says game, and it goes on"}})
            );
        }
        assert_eq!(answer(&session), json!({"ok": false, "error": {"code": "not_in_game", "message": "no controllable player"}}));
        assert_eq!(game.calls, ["load 奥斯曼1444_11_11", "start FRA"]);
    }

    #[test]
    fn a_load_is_compared_with_its_save_as_the_game_read_it() {
        let engine = Engine::new(remember);
        let mut game = Fake::default();
        let load = engine.submit(Request::Menu(MenuRequest::Load { save: "rewritten".into() }));
        engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open"))), PLENTY);
        // The load wrote the save before the game read it, as an in-game load
        // writes the ironman exit save of the session it leaves.
        *game.rewritten.lock().unwrap() = Some(game::menu::Expected { player: *b"TUR", date: Some(0x035D_7710) });
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(answer(&load), ok(json!({"number": 2, "player": "TUR", "observing": false, "date": "1444.11.11"})));
    }

    #[test]
    fn a_load_the_game_could_not_read_is_refused_though_a_game_comes() {
        let engine = Engine::new(remember);
        let mut game = Fake::default();
        let load = engine.submit(Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }));
        engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open"))), PLENTY);
        // The game's loader failed, and the game starts from what it read.
        *game.launch.lock().unwrap() = game::menu::Launch::Failed("the game could not read the save".into());
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(answer(&load), json!({"ok": false, "error": {"code": "rejected", "message": "the game could not read the save"}}));
        assert_eq!(game.calls, ["load 奥斯曼1444_11_11"]);
    }

    #[test]
    fn returning_to_the_main_menu_is_answered_before_the_process_ends() {
        let engine = Engine::new(remember);
        let mut game = Fake::default();
        let waiting =
            engine.submit(Request::Events(EventsRequest::Read { since: Some(0), wait_ms: Some(10_000), process: None, kinds: None, countries: None }));
        engine.run(&mut game, &Place::Game, PLENTY);
        let back = engine.submit(Request::Menu(MenuRequest::Return {}));
        let after = engine.submit(Request::Session(SessionRequest::Get {}));
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(answer(&back), ok(json!({"process": process(), "place": "ending"})));
        // Until the game starts eu4.exe again, the process answers as before.
        assert_eq!(answer(&after)["ok"], true);
        // At the restart, whoever asked for the return (the player too), what
        // waits or is queued is told the game starts again, at once, and so is
        // whatever comes later: no safe point comes any more.
        let queued = engine.submit(Request::Session(SessionRequest::Get {}));
        engine.end(menu::ending(), Duration::ZERO);
        let later = engine.submit(Request::Text(TextRequest::Language {}));
        for got in [answer(&waiting), answer(&queued), answer(&later)] {
            assert!(error("not_ready")(&got), "{got}");
            assert!(got["error"]["message"].as_str().unwrap().contains("returning to the main menu"), "{got}");
        }
        assert_eq!(game.calls, ["return", "session"]);
    }

    #[test]
    fn an_in_process_return_is_answered_once_the_main_menu_is_up() {
        let engine = Engine::new(remember);
        let returned = Arc::new(Mutex::new(game::menu::Returned::Waiting));
        let mut game = Fake { returning: Some(returned.clone()), ..Fake::default() };
        let back = engine.submit(Request::Menu(MenuRequest::Return {}));
        engine.run(&mut game, &Place::Game, PLENTY);
        assert!(back.wait(Duration::ZERO).is_none());
        // The main menu's safe points: the same process answers.
        *returned.lock().unwrap() = game::menu::Returned::Done;
        engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open"))), PLENTY);
        assert_eq!(answer(&back), ok(json!({"process": process(), "place": "main_menu"})));
        // One whose ironman exit save failed leaves the session going on.
        *returned.lock().unwrap() = game::menu::Returned::Waiting;
        let again = engine.submit(Request::Menu(MenuRequest::Return {}));
        engine.run(&mut game, &Place::Game, PLENTY);
        *returned.lock().unwrap() = game::menu::Returned::Failed("the game could not write the ironman exit save".into());
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(answer(&again), json!({"ok": false, "error": {"code": "rejected", "message": "the game could not write the ironman exit save"}}));
        assert_eq!(game.calls, ["return", "return"]);
    }

    #[test]
    fn a_return_needs_no_player() {
        // Where what needs a game is refused, the layer decides.
        let engine = Engine::new(remember);
        let mut game = Fake::default();
        let observing = Place::Outside(Some(Error::new(Code::NotInGame, "no player")));
        let session = engine.submit(Request::Session(SessionRequest::Get {}));
        let back = engine.submit(Request::Menu(MenuRequest::Return {}));
        engine.run(&mut game, &observing, PLENTY);
        assert!(error("not_in_game")(&answer(&session)));
        assert_eq!(answer(&back), ok(json!({"process": process(), "place": "ending"})));
        assert_eq!(game.calls, ["return"]);
    }
}
