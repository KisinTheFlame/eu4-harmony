//! Questions (GLOSSARY): the game's own events that wait for the player's
//! answer, with the options the player may choose, and answering one as the
//! player does, by choosing an option in its event window.
use crate::Date;
use serde::{Deserialize, Serialize};

group! {
    /// The game's events waiting for the player's answer.
    "question": QuestionRequest, QuestionAnswer;

    /// The questions waiting for the player's answer: the game's events shown
    /// to the player in event windows, oldest first, with the options the
    /// player may choose. Texts are as the game shows them, in plain text: no
    /// color codes, icons as their names.
    game List "list" {} -> Vec<Question>, "the questions: id, event, title, description, deadline, and the options the player may choose (option, text)";
    /// Answers a question with one of its options, as the player does in its
    /// event window: the window closes and the game applies the option a
    /// moment later. Done means the game has the answer; the question is gone
    /// from the list at once.
    game Answer "answer" {
        /// The question's id, from question.list.
        question: u32,
        /// The option's number, from question.list.
        option: u32,
    } -> (), "nothing";
}

/// A question: one of the game's events waiting for the player's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    /// Names the question while it waits; a later question of the same event
    /// has another.
    pub id: u32,
    /// The event's id in the game's files, such as `flavor_fra.1`.
    pub event: String,
    pub title: String,
    pub description: String,
    /// The last day to answer; after it the game picks an option itself.
    pub deadline: Date,
    /// The options the player may choose, in the order the game shows them.
    pub options: Vec<QuestionOption>,
}

/// An option of a question.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    /// What question.answer takes; options the player may not choose are
    /// left out, so the numbers may skip.
    pub option: u32,
    pub text: String,
}
