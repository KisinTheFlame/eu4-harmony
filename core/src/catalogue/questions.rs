//! Questions: the game's events waiting for the player's answer. Answering
//! one is the player's choice (ADR remote-control-uses-capabilities), made in
//! its event window.
use eu4_harmony_remote::questions::args;
use eu4_harmony_remote::questions::{Question, QuestionAnswer, QuestionRequest};

layer! {
    run QuestionRequest => QuestionAnswer;
    List => fn questions -> Vec<Question> = crate::game::questions::questions;
    Answer => fn answer_question -> () = crate::game::questions::answer;
}
