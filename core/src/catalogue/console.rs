//! The game console: a command run as the player would type it.
use crate::game::{self};
use eu4_harmony_remote::console::{ConsoleAnswer, ConsoleRequest};
use eu4_harmony_remote::console::args;

layer! {
    run ConsoleRequest => ConsoleAnswer;
    /// What the command printed.
    fn console(command: &str) -> String = world game::console::run;
    Run { command } => game.console(command.as_str());
}
