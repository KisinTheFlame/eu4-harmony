//! A save chosen in single player added to the file transfer without its
//! deflated copy (docs/plans/game-speed.md, S11;
//! docs/research/save-transfer-copy.md).
//!
//! When the single player setup reads the save the player chose
//! (`0x1132ca0`), it hands the file to the game's file transfer manager,
//! whose add (`0xd7aad0`) deflates a file that is not a zip already at level
//! 9 into a copy of its own, on the main thread, for a multiplayer peer to
//! be sent. The load that follows reads the file as it was read, and only
//! the transfer's send, which a peer's request starts, reads the copy; a
//! single player game has no peer. The deflate was 2.5 s of a late plain
//! save of 69 MB, before its 3.1 s load.
//!
//! So in single player (the world's multiplayer kind 0) the add is asked for
//! no copy, as the game's own multiplayer host asks for none (`0xd84815`):
//! the file's description then holds the bytes as read and no copy, and the
//! load is the same call with the same bytes. Any other game keeps the
//! game's way.
use crate::called_by_game;
use crate::objects::campaign::{globals::WORLD, world::MULTIPLAYER};
use crate::points::{Function, Point};
use crate::speed::SpeedUp;

/// The transfer's add: `(manager, name, data, compress) -> index`.
const ADD: usize = 0xd7aad0;
type Add = extern "C" fn(usize, usize, usize, u8) -> u32;
static ORIGINAL_ADD: Function<Add> = Function::new();

/// Left to the game, every plain save chosen is deflated for no one.
/// There is nothing to check: what the change leaves out is a copy nothing
/// reads in single player.
pub const SPEED_UP: SpeedUp = SpeedUp { name: "transfer-copy", group: "loading", points, checkable: false };

fn points() -> Vec<Point> {
    vec![
        Point::replace(ADD, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 20", "file transfer's add", add as Add, &ORIGINAL_ADD),
        // `movzx r12d, r9b`: the add keeps `compress`; then, after the
        // checksum, `test r12b, r12b; je`: only with it does it deflate.
        Point::keep(ADD + 0x27, "45 0f b6 e1 4d 8b f8", "file transfer's add takes compress"),
        Point::keep(ADD + 0x19c, "45 84 e4 0f 84 23 04 00 00", "file transfer's add deflates with compress"),
        // The single player setup's read of the chosen save: `compress` is
        // whether the file is no zip (`test r12b, r12b; sete r9b`), then the
        // add.
        Point::keep(0x11337afusize, "45 84 e4 41 0f 94 c1 4c 8d 45 c8 49 8b 8d 48 2d 00 00 e8 0a 73 c4 ff", "setup adds the chosen save to the transfer"),
        // Entering the setup: the world, and whether it is single player.
        Point::keep(0x110e12eusize, "48 8b 1d 43 1d 23 01 48 8b 4e 08 48 8b 01 83 bb 7c 74 00 00 00", "setup's single player check")
            .uses(&[WORLD, MULTIPLAYER]),
    ]
}

#[called_by_game]
extern "C" fn add(manager: usize, name: usize, data: usize, compress: u8) -> u32 {
    let compress = if compress != 0 && single_player() { 0 } else { compress };
    ORIGINAL_ADD.get()(manager, name, data, compress)
}

/// Whether the world is a single player game's: its multiplayer kind 0.
fn single_player() -> bool {
    let m = crate::hooked_memory();
    match m.ptr(m.at(WORLD)) {
        Ok(world) if world != 0 => m.i32(world + MULTIPLAYER).is_ok_and(|kind| kind == 0),
        _ => false,
    }
}
