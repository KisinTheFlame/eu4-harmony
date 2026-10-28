//! The main menu, remote control only: where the game is, the saves, loading
//! one, starting a game, returning to the main menu.
//!
//! A load or a start answers once the game is ready, at its first frame,
//! with the session; it fails as soon as the game says it did not take it
//! up. Returning to the main menu starts `eu4.exe` again, so it is answered
//! before this process ends, and the engine answers every request after it
//! `not_ready` (`Outcome::Ends`): the new process takes the pipe over.
use super::{process, session, Live, Outcome, Poll, Wait};
use crate::game::{self, menu::Launch, Caller, GResult, Status};
use eu4_harmony_remote::{
    menu::{Menu, MenuAnswer, MenuRequest, Place, Save},
    session::Session,
    Answer, Code, CountryTag, Error, Response,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn place(&mut self) -> GResult<game::menu::Place>;
    fn saves(&mut self) -> GResult<Vec<game::menu::Save>>;
    /// Asks for the save to be loaded and played.
    fn load(&mut self, save: &str) -> GResult<()>;
    /// Asks for a new game as the country.
    fn start(&mut self, country: CountryTag) -> GResult<()>;
    /// What came of the last load or start so far, asked again at each safe
    /// point until the game's first frame.
    fn launch(&mut self) -> Box<dyn FnMut() -> Launch + Send>;
    fn return_to_main_menu(&mut self) -> GResult<()>;
}

impl Layer for Live {
    fn place(&mut self) -> GResult<game::menu::Place> {
        game::menu::place(Caller::Remote)
    }

    fn saves(&mut self) -> GResult<Vec<game::menu::Save>> {
        game::menu::saves(Caller::Remote)
    }

    fn load(&mut self, save: &str) -> GResult<()> {
        game::menu::load(Caller::Remote, save)
    }

    fn start(&mut self, country: CountryTag) -> GResult<()> {
        game::menu::start(Caller::Remote, country.0)
    }

    fn launch(&mut self) -> Box<dyn FnMut() -> Launch + Send> {
        Box::new(|| game::menu::launch(Caller::Remote))
    }

    fn return_to_main_menu(&mut self) -> GResult<()> {
        game::menu::return_to_main_menu(Caller::Remote)
    }
}

/// What every request is answered once the game is returning to the main
/// menu.
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
            Ok(()) => Outcome::Later(Box::new(Starting { probe: game.launch(), answer: MenuAnswer::Load })),
            Err(fail) => done(Err(fail.into())),
        },
        MenuRequest::Start { country } => match game.start(country) {
            Ok(()) => Outcome::Later(Box::new(Starting { probe: game.launch(), answer: MenuAnswer::Start })),
            Err(fail) => done(Err(fail.into())),
        },
        MenuRequest::Return {} => match game.return_to_main_menu() {
            Ok(()) => Outcome::Ends(Ok(Answer::Menu(MenuAnswer::Return(Menu { process: process(), place: Place::Ending }))), ending()),
            Err(fail) => done(Err(fail.into())),
        },
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
    Save { name: s.name, file: s.file, bytes: s.bytes, modified: s.modified }
}

/// A load or start waiting for the game's first frame.
struct Starting {
    probe: Box<dyn FnMut() -> Launch + Send>,
    answer: fn(Session) -> MenuAnswer,
}

impl Wait for Starting {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response> {
        if let Some(game) = at.game.as_deref_mut() {
            return match game.session() {
                Ok(s) => Some(Ok(Answer::Menu((self.answer)(session::answer(s))))),
                Err(fail) if fail.status == Status::NotReady => None,
                Err(fail) => Some(Err(fail.into())),
            };
        }
        match (self.probe)() {
            Launch::Waiting => None,
            Launch::Failed(why) => Some(Err(Error::new(Code::Rejected, why))),
        }
    }
}
