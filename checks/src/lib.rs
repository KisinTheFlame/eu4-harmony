//! The checks the framework makes of a capability's call itself that a
//! stand-in for the core makes too, each with the status and the words it
//! refuses in: the core makes them, and the fakes that stand in for it
//! outside the core, the fake game of `eu4-harmony test`, the Lua host's own
//! and remote control's, make the same ones through here, so that a fake
//! refuses what the core refuses, in the core's words
//! (ADR refusals-live-with-their-capabilities). A refusal no fake makes lives
//! with its capability in the core.
//!
//! One module per group of capabilities, as in the core's capability layer;
//! the Lua form of answers ([`form`]); and the game's calendar ([`Date`]),
//! which the refusals, remote control's JSON and the core's own wording write
//! dates in.
pub mod armies;
pub mod countries;
mod date;
pub mod form;
pub mod provinces;
pub mod save_data;
pub mod script;
pub mod texts;
pub mod time;

pub use date::Date;

/// How a check refuses, as the core's status for it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The game state cannot be read right now; a later frame may succeed.
    NotReady,
    NotFound,
    /// A command was refused; the message says why.
    Rejected,
    InvalidArgument,
}

/// A check's refusal: how it refuses, and its words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub status: Status,
    pub message: String,
}

impl Refusal {
    pub(crate) fn new(status: Status, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
}

/// What a check comes to: what it lets through, or its refusal.
pub type Checked<T = ()> = Result<T, Refusal>;
