//! Questions: the game's events waiting for the player's answer. Answering
//! one is the player's choice (ADR 0015), made in its event window.
use super::Live;
use crate::game::{self, Caller, GResult};
use eu4_harmony_remote::{
    questions::{Question, QuestionAnswer, QuestionOption, QuestionRequest},
    Date, Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn questions(&mut self) -> GResult<Vec<game::questions::Question>>;
    fn answer_question(&mut self, question: u32, option: u32) -> GResult<()>;
}

impl Layer for Live {
    fn questions(&mut self) -> GResult<Vec<game::questions::Question>> {
        game::questions::questions(Caller::Remote)
    }

    fn answer_question(&mut self, question: u32, option: u32) -> GResult<()> {
        game::questions::answer(Caller::Remote, question, option)
    }
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
