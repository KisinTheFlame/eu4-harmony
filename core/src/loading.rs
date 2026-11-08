//! A shorter startup and load, the game's data the same (docs/loading.md,
//! docs/research/loading.md). Each part is a speed-up of its own
//! (`crate::speed`).
//!
//! - `music`: what the game reads each music file whole for at startup, its
//!   channels, rate and length, kept from one start to the next.
//! - `lookups`: whether a file is in a folder of the game's or of the
//!   workshop's, answered from the folder's listing instead of asking
//!   Windows each time.
//! - `children`: a save's dated nodes added to their parent without walking
//!   its list for them first.
use std::path::Path;

pub mod children;
pub mod lookups;
pub mod music;

/// Where the game and the player's files are, once known; before the game
/// reads its music.
pub fn set_folders(game_dir: &Path, user_dir: &Path) {
    music::set_folder(user_dir);
    lookups::set_roots(game_dir);
}
