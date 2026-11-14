//! Where the game keeps what the framework reads and writes inside its
//! objects. Each field, global and class that more than one module reads is
//! named once here, and no module names it again; a field one module alone
//! reads may be named in that module. A test
//! (`points::tests::every_field_is_named_in_one_file`) refuses a field named
//! in two files. The modules' points keep an instruction of the game's own
//! that uses it, so a build that moved one fails the startup check instead
//! of the framework reading the wrong place. The points say which numbers
//! they use, and a test checks that their bytes carry them.
//!
//! A game function is named once too: by the module that calls it for the
//! others (`game::power`'s spending, `game::modifiers`, `game_heap`), or
//! here, in a `functions` module, when a point here keeps its code. A test
//! (`points::tests::every_game_address_is_named_in_one_file`) refuses an
//! address of the game's named in two files.
pub mod application;
pub mod camera;
pub mod campaign;
pub mod console;
pub mod diplomacy;
pub mod gui;
pub mod session;
pub mod setup;
pub mod text;

/// The game's doubly linked lists: a header with the first and the last
/// node, their count (i32) and a byte set while the list is being walked,
/// then nodes of [`NODE_SIZE`] bytes with the item, the previous and the
/// next node, and a byte set on a node removed meanwhile.
pub mod list {
    pub const HEAD: usize = 0;
    pub const TAIL: usize = 8;
    pub const COUNT: usize = 0x10;
    pub const CHANGING: usize = 0x14;
    pub const SIZE: usize = 0x18;
    pub const NODE_SIZE: usize = 0x20;
    pub const ITEM: usize = 0;
    pub const PREVIOUS: usize = 8;
    pub const NEXT: usize = 0x10;
    pub const REMOVED: usize = 0x18;
}

/// The game's memory stream (vtables `0x1d92200` and `0x1dc0358`, whose write
/// is `0x171f290`): a line, a mode, whether it writes, a name, and the bytes
/// it holds. With no growing part it writes into the buffer at [`BUFFER`] of
/// [`CAPACITY`] bytes, up to [`AT`], keeping [`KEPT`], and sets [`FULL`]
/// instead of writing past its end; past the buffer the bytes go into the
/// growing part at [`GROWING`], a byte vector (begin, end, capacity).
/// [`OWNS_BUFFER`] and [`OWNS_GROWING`] say whether it lets its buffers go.
/// The save's loops write through it (`saving::parallel`, `saving::sum`), and
/// the local handler's post makes one over a vector of its own
/// (`game::commands`).
pub mod stream {
    pub const SIZE: usize = 0x58;
    pub const LINE: usize = 0x08;
    pub const MODE: usize = 0x10;
    pub const WRITING: usize = 0x14;
    pub const NAME: usize = 0x18;
    pub const GROWING: usize = 0x38;
    pub const BUFFER: usize = 0x40;
    pub const KEPT: usize = 0x48;
    pub const AT: usize = 0x4c;
    pub const CAPACITY: usize = 0x50;
    pub const FULL: usize = 0x54;
    pub const OWNS_BUFFER: usize = 0x55;
    pub const OWNS_GROWING: usize = 0x56;
}

/// A `std::map`'s node: its children and parent, whether it is the head (no
/// value), and the value from `+0x20`. The map keeps a pointer to its head,
/// whose parent is the root.
pub mod map {
    pub const LEFT: usize = 0x0;
    pub const PARENT: usize = 0x8;
    pub const RIGHT: usize = 0x10;
    pub const NIL: usize = 0x19;
    pub const VALUE: usize = 0x20;
    pub const NODE_SIZE: usize = 0x28;
}

use crate::points::Point;

pub fn points() -> Vec<Point> {
    let mut all = campaign::points();
    all.extend(gui::points());
    all.extend(text::points());
    all.extend(camera::points());
    all.extend(session::points());
    all.extend(setup::points());
    all.extend(application::points());
    all.extend(diplomacy::points());
    all.extend(console::points());
    all
}
