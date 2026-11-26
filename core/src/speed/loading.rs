//! A shorter startup and load, the game's data the same (docs/game/program/loading.md,
//! docs/game/program/loading.md). Each part is a speed-up of its own
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
//! - `save_input`: a local text save buffered once for the in-game load,
//!   using the game's memory stream while its original file stays open.
//! - `distance_scratch`: country distance rebuilds omit target lists when
//!   there are no source provinces, and otherwise reuse their allocation.
//! - `parameters`: a parameterized scripted trigger's or effect's use given
//!   its arguments in one pass, instead of the whole text made again for
//!   each parameter.
//! - `localisation`: localisation loaded once at startup instead of twice,
//!   its replacement provenance lookups indexed, and its duplicate check
//!   answered without discarded formatting when `text.log` is off.
//! - `trigger_profile`: the triggers' optimization profile matched through a
//!   compact interned index instead of allocating a temporary hash table at
//!   every tree node.
//! - `loading_screen`: redundant loading-screen redraws (`0x11bb4f0`) throttled
//!   so rapid progress updates do not stall on repeated DWM/vsync presents.
//! - `dlc_verify`: when `dlc_signature` misses, skips archive MD5 scans for
//!   DLCs that fail the store ownership check and hashes owned archives live in
//!   parallel across worker threads.
//!
//! `folder` is shared by `lookups` and `listed`: a folder's listing and the
//! names Windows reads as they are written, written once.
use std::path::Path;

pub mod children;
pub mod distance_scratch;
pub mod dlc_verify;
mod folder;
pub mod listed;
pub mod loading_screen;
pub mod localisation;
pub mod lookups;
pub mod music;
pub mod opens;
pub mod parameters;
pub mod save_input;
pub mod transfer;
pub mod trigger_profile;

/// Where the game and the player's files are, once known; before the game
/// reads its music.
pub fn set_folders(game_dir: &Path, user_dir: &Path) {
    music::set_folder(user_dir);
    lookups::set_roots(game_dir);
}
