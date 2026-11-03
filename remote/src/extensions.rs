//! The mods' extensions, remote control only (ADR 0015): reloading one in
//! the running game (docs/extension-reload.md).
use serde::{Deserialize, Serialize};

group! {
    /// The mods' extensions: reloading one in the running game.
    "extension": ExtensionRequest, ExtensionAnswer;

    /// Reloads a mod's extension from its files, in the main menu or in a
    /// session: its Lua code and its texts. In a session the old code's
    /// save_data goes to the new code's on_session, as a loaded save's does;
    /// its waiting tasks are dropped, its map coloring is released and what
    /// it set on its interface elements is forgotten. Interface fragments
    /// are not reloaded. Rejected, the old code going on as it was, when the
    /// new code does not load or the manifest's id, permissions or
    /// api_version changed; not_found for a mod with no extension loaded.
    remote_anywhere Reload "reload" {
        /// The mod's id, as its manifest has it.
        id: String,
    } -> Reloaded, "session (a session was going on, so the new code's on_session ran) and lost (why the old code's save data did not reach it, or null)";
}

/// What a reload came to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Reloaded {
    pub session: bool,
    pub lost: Option<String>,
}
