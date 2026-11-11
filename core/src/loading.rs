//! A shorter startup and load, the game's data the same (docs/plans/loading.md,
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
//! - `opens`: files opened, looked up and closed by several threads at once,
//!   instead of one at a time under two global locks.
//! - `listed`: every folder of the search path answered from its listing,
//!   which a watch of the folder keeps up to date.
//! - `transfer`: a save chosen in single player added to the file transfer
//!   without the deflated copy that only a multiplayer peer would read.
//! - `parameters`: a parameterized scripted trigger's or effect's use given
//!   its arguments in one pass, instead of the whole text made again for
//!   each parameter.
use std::path::Path;

pub mod children;
pub mod listed;
pub mod lookups;
pub mod music;
pub mod opens;
pub mod parameters;
pub mod transfer;

/// Where the game and the player's files are, once known; before the game
/// reads its music.
pub fn set_folders(game_dir: &Path, user_dir: &Path) {
    music::set_folder(user_dir);
    lookups::set_roots(game_dir);
}
