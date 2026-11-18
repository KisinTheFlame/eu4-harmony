//! The installed game: its folder, and whether Steam is ready to start it.
//! The game's process is the client's (`eu4_harmony_client::game`).
use crate::Result;
use std::path::PathBuf;

/// The game folder, as every program of EU4 Harmony finds it
/// ([`eu4_harmony_folders::game_dir`]).
pub fn dir() -> Result<PathBuf> {
    eu4_harmony_folders::game_dir(None)
}

/// Whether Steam runs with a user logged in ([`eu4_harmony_client::game::steam_ready`]).
pub fn steam_ready() -> Result<()> {
    eu4_harmony_client::game::steam_ready()
}
