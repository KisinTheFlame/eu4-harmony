//! Questions: the game's events waiting for the player's answer. Answering
//! one is the player's choice (ADR remote-control-uses-capabilities), made in
//! its event window.
use super::Convert;
use crate::game::{self};
use eu4_harmony_remote::{
    questions::{Question, QuestionAnswer, QuestionOption, QuestionRequest},
    Date,
};

layer! {
    run QuestionRequest => QuestionAnswer;
    List => fn questions() -> Vec<game::questions::Question> = game::questions::questions;
    Answer => fn answer_question(question: u32, option: u32) -> () = game::questions::answer;
}

impl Convert<Vec<Question>> for Vec<game::questions::Question> {
    fn convert(self) -> Vec<Question> {
        self.into_iter()
            .map(|q| Question {
                id: q.id,
                event: q.event,
                title: q.title,
                description: q.description,
                deadline: Date(q.deadline),
                options: q.options.into_iter().map(|o| QuestionOption { option: o.index, text: o.text }).collect(),
            })
            .collect()
    }
}
