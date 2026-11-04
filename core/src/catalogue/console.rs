//! The game console: a command run as the player would type it.
use crate::game::{self};
use eu4_harmony_remote::{
    console::{ConsoleAnswer, ConsoleRequest},
    Error,
};

layer! {
    /// What the command printed.
    fn console(command: &str) -> String = game::console::run;
}

pub fn run(request: ConsoleRequest, game: &mut (impl Layer + ?Sized)) -> Result<ConsoleAnswer, Error> {
    match request {
        ConsoleRequest::Run { command } => Ok(ConsoleAnswer::Run(game.console(&command)?)),
    }
}
