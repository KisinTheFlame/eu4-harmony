//! The checks the framework makes of a capability's call itself, before it
//! asks the game or besides the game's own checks, each with the status and
//! the words it refuses in. The core makes them; the fakes that stand in for
//! the core in tests, the SDK's `FakeGame` and the remote control engine's,
//! make the same ones through here, so that a fake refuses what the core
//! refuses, in the core's words. A refusal whose reasons the game gives is
//! not here: only the game has those.
//!
//! One module per group of capabilities, as in the core's capability layer,
//! and the game's calendar ([`Date`]), which the refusals, remote control's
//! JSON and the core's own wording write dates in.
pub mod armies;
pub mod camera;
pub mod console;
mod date;
pub mod diplomacy;
pub mod offers;
pub mod provinces;
pub mod questions;
pub mod recruiting;
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
