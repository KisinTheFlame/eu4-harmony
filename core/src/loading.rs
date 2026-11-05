//! A shorter startup and load, the game's data the same (docs/loading.md,
//! docs/research/loading.md).
//!
//! - `music`: what the game reads each music file whole for at startup, its
//!   channels, rate and length, kept from one start to the next.
//! - `lookups`: whether a file is in a folder of the game's or of the
//!   workshop's, answered from the folder's listing instead of asking
//!   Windows each time.
//! - `children`: a save's dated nodes added to their parent without walking
//!   its list for them first.
use crate::points::Point;
use std::path::Path;

pub mod children;
pub mod lookups;
pub mod music;

pub fn points() -> Vec<Point> {
    let mut points = music::points();
    points.extend(lookups::points());
    points.extend(children::points());
    points
}

/// Where the game and the player's files are, once known; before the game
/// reads its music.
pub fn set_folders(game_dir: &Path, user_dir: &Path) {
    music::set_folder(user_dir);
    lookups::set_roots(game_dir);
    children::set_checking();
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
