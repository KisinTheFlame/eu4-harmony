//! Questions: the game's events waiting for the player's answer. Answering
//! one is the player's choice (ADR 0015), made in its event window.
use crate::game::{self};
use eu4_harmony_remote::{
    questions::{Question, QuestionAnswer, QuestionOption, QuestionRequest},
    Date, Error,
};

layer! {
    fn questions() -> Vec<game::questions::Question> = game::questions::questions;
    fn answer_question(question: u32, option: u32) -> () = game::questions::answer;
}

pub fn run(request: QuestionRequest, game: &mut (impl Layer + ?Sized)) -> Result<QuestionAnswer, Error> {
    Ok(match request {
        QuestionRequest::List {} => QuestionAnswer::List(game.questions()?.into_iter().map(question).collect()),
        QuestionRequest::Answer { question, option } => QuestionAnswer::Answer(game.answer_question(question, option)?),
    })
}

fn question(q: game::questions::Question) -> Question {
    Question {
        id: q.id,
        event: q.event,
        title: q.title,
        description: q.description,
        deadline: Date(q.deadline),
        options: q.options.into_iter().map(|o| QuestionOption { option: o.index, text: o.text }).collect(),
    }
}
