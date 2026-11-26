//! A shorter startup and load, the game's data the same (docs/game/program/loading.md,
//! docs/game/program/loading.md). Each part is a speed-up of its own
//! (`crate::speed`).
//!
//! - `music`: what the game reads each music file whole for at startup, its
//!   channels, rate and length, kept from one start to the next.
//! - `lookups`: whether a file is in a folder of the game's or of the
//!   workshop's, answered from the folder's listing instead of asking
//!   Windows each time.
//! - `children`: dated history node lists (`0x7e7cd0`) maintained with an O(1)
//!   membership set and O(log K) date-ordered predecessor index.
//! - `province_shapes`: province pixel shape updates (`0xa402a0`) retain the
//!   last-span cache on existing-point hits so world resets stay O(1).
//! - `country_distances`: country distance rebuilds (`0x2d0b30`) precompute
//!   capital separation once per province, detect adjacency in O(|T|*deg), and
//!   prune planar distances with x-sorted wrapped coordinates.
//! - `flag_atlas`: flag cache validation (`0x10c5ff0`) cached and flag atlas
//!   page textures reused across in-game world resets.
//! - `token_lookup`: text lexer keyword lookups (`0x6e4880`) answered from a
//!   lock-free flat hash table instead of walking the ternary search tree.
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
pub mod country_distances;
pub mod distance_scratch;
pub mod dlc_verify;
pub mod flag_atlas;
mod folder;
pub mod listed;
pub mod loading_screen;
pub mod localisation;
pub mod lookups;
pub mod music;
pub mod opens;
pub mod parameters;
pub mod province_shapes;
pub mod save_input;
pub mod token_lookup;
pub mod transfer;
pub mod trigger_profile;

/// Where the game and the player's files are, once known; before the game
/// reads its music.
pub fn set_folders(game_dir: &Path, user_dir: &Path) {
    music::set_folder(user_dir);
    lookups::set_roots(game_dir);
}
