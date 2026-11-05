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
    let player = s.player.letters();
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
