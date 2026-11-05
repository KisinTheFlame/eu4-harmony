//! A shorter startup and load, the game's data the same (docs/loading.md,
//! docs/research/loading.md).
//!
//! - `music`: what the game reads each music file whole for at startup, its
//!   channels, rate and length, kept from one start to the next.
use crate::points::Point;
use std::path::Path;

pub mod music;

pub fn points() -> Vec<Point> {
    music::points()
}

/// Where the game and the player's files are, once known; before the game
/// reads its music.
pub fn set_folders(game_dir: &Path, user_dir: &Path) {
    music::set_folder(user_dir);
}

/// A developer build's test switch `game-loads`: the startup and loads are
/// the game's own, as before, so that the two can be measured.
fn game_loads() -> bool {
    #[cfg(feature = "dev-test-switches")]
    return crate::dev::options().test_switches.game_loads;
    #[cfg(not(feature = "dev-test-switches"))]
    false
}

/// A developer build's test switch `check-loading`: what the framework
/// answers from what it keeps is checked against the game's own answer, the
/// game's kept, and a difference logged.
fn checking() -> bool {
    #[cfg(feature = "dev-test-switches")]
    return crate::dev::options().test_switches.check_loading;
    #[cfg(not(feature = "dev-test-switches"))]
    false
}
