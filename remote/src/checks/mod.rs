//! Shared refusals made by both the core and stand-ins outside the core (the
//! fake game of `eu4-harmony test`, the Lua host, and remote control tests),
//! returning the catalogue's own [`crate::Error`] directly
//! (ADR fold-checks-into-remote). Refusals no fake makes live with their
//! capability in the core (ADR refusals-live-with-their-capabilities).
pub mod armies;
pub mod countries;
pub mod provinces;
pub mod save_data;
pub mod script;
pub mod texts;
pub mod time;

/// What a shared check comes to: what it lets through, or a catalogue error.
pub type Checked<T = ()> = Result<T, crate::Error>;
