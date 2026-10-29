//! The game console: a command run as the player would type it.
use super::Live;
use crate::game::{self, Caller, GResult};
use eu4_harmony_remote::{
    console::{ConsoleAnswer, ConsoleRequest},
    Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    /// What the command printed.
    fn console(&mut self, command: &str) -> GResult<String>;
}

impl Layer for Live {
    fn console(&mut self, command: &str) -> GResult<String> {
        game::console::run(Caller::Remote, command)
    }
}

pub fn run(request: ConsoleRequest, game: &mut (impl Layer + ?Sized)) -> Result<ConsoleAnswer, Error> {
    match request {
        ConsoleRequest::Run { command } => Ok(ConsoleAnswer::Run(game.console(&command)?)),
    }
}
