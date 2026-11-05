//! The game console: a command run as the player would type it.
use crate::game::{self};
use eu4_harmony_remote::console::{ConsoleAnswer, ConsoleRequest};

layer! {
    run ConsoleRequest => ConsoleAnswer;
    /// What the command printed.
    fn console(command: &str) -> String = game::console::run;
    Run { command } => game.console(command.as_str());
}
