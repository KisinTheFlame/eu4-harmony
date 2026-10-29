//! The game console (GLOSSARY) and what running one of its commands answers
//! (docs/research/game-console.md).

/// The game console, a pointer in this global: its commands, then whether
/// the commands only developers may run are locked and whether the game is
/// ironman or multiplayer. Made when first asked for.
pub const GLOBAL: usize = 0x2357b58;

/// What running a command answers: whether the command succeeded (u8), then
/// the text the console window prints (a `std::string` of game text, its
/// lines in white when the command succeeded and in red when not).
pub mod result {
    pub const SUCCEEDED: usize = 0;
    pub const TEXT: usize = 8;
    pub const SIZE: usize = 0x28;
}

use crate::points::Point;

pub fn points() -> Vec<Point> {
    vec![
        // Running a command, filling in its result's text.
        Point::keep(0x17213d4usize, "48 89 44 24 48 c6 07 00 0f 10 44 24 48 0f 11 47 08", "game console run result").uses(&[result::TEXT]),
    ]
}
