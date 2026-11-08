//! The game console: a command run as the player would type it.
use super::{Outcome, Poll, Wait};
use crate::game::{self};
use eu4_harmony_remote::console::args;
use eu4_harmony_remote::console::{ConsoleAnswer, ConsoleRequest};
use eu4_harmony_remote::{Answer, Response};

layer! {
    run ConsoleRequest => ConsoleAnswer;
    /// What the command printed.
    fn console(command: &str) -> String = world game::console::run;
    Run { command } => game.console(command.as_str());
}

/// Runs the command now and answers at the next safe point: a command of
/// the game console may post the game's own commands, such as `observe`'s,
/// which the game carries out at its next frame, so that the answer comes
/// once they are carried out, and the session its client sends in next is
/// the one they leave, also when they ended the one it ran in.
pub fn run_and_answer_next(request: ConsoleRequest, game: &mut (impl Layer + ?Sized)) -> Outcome {
    Outcome::Later(Box::new(Ran(Some(run(request, game).map(Answer::Console)))))
}

/// A command run, its answer kept for the next safe point.
struct Ran(Option<Response>);

impl Wait for Ran {
    fn poll(&mut self, _: &mut Poll<'_>) -> Option<Response> {
        self.0.take()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        fake::{answer, quiet, Fake},
        Place, Request,
    };
    use super::*;
    use crate::remote::Engine;
    use std::time::Duration;

    #[test]
    fn a_command_is_answered_at_the_next_safe_point() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let reply = engine.submit(Request::Console(ConsoleRequest::Run { command: "observe".into() }));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert!(answer(&reply).is_null(), "answered at the safe point it ran at");
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert!(!answer(&reply).is_null());
    }
}
