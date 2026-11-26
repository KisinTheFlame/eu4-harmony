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
    pub const FILE_VTABLE: usize = 0x1dbfa98;
    pub const VTABLE: usize = 0x1dc0358;
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

    pub(super) fn points() -> Vec<crate::native::points::Point> {
        use crate::native::points::Point;
        vec![Point::keep(
            0x171ec8fusize,
            "48 8d 05 c2 16 6a 00 48 89 03 48 89 73 38 48 89 6b 40 4c 89 73 48 89 7b 50 c6 43 54 00 0f b6 84 24 80 00 00 00 88 43 55 0f b6 84 24 88 00 00 00 88 43 56",
            "memory stream made",
        ).uses(&[VTABLE, GROWING, BUFFER, KEPT, CAPACITY, FULL, OWNS_BUFFER, OWNS_GROWING])]
    }
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

use super::{memory::Memory, points::{GameFunction, Point}};

/// How many window pixels a GUI unit takes, (across, down); None before the
/// GUI exists (docs/game/interface/text-input.md §3).
pub(crate) fn gui_scale(m: &Memory) -> Option<(f32, f32)> {
    let gui = m.ptr(m.at(gui::globals::SCALE)).ok().filter(|&g| g != 0)?;
    let read = |at: usize| m.u32(gui + at).ok().map(f32::from_bits).filter(|s| s.is_finite() && *s > 0.0);
    Some((read(gui::scale::ACROSS)?, read(gui::scale::DOWN)?))
}

/// Reads the current game date (hours since 5000 BC) directly from the global
/// world pointer when available, for crash records without depending on `world`.
pub(crate) fn current_date(m: &Memory) -> Option<i32> {
    let world = m.ptr(m.at(campaign::globals::WORLD)).ok().filter(|&w| w != 0)?;
    let date = m.i32(world + campaign::world::DATE).ok()?;
    (date > 0).then_some(date)
}

/// Shared native army function RVAs used by repairs, speed-ups and capabilities.
pub(crate) mod army_rva {
    use super::GameFunction;
    pub(crate) const FIND_ID: GameFunction<1> = GameFunction::at(0x14cf730);
    pub(crate) const MORALE: GameFunction<3> = GameFunction::at(0xd4c060);
}

/// Shared native command-writer/token-table RVAs used by parallel saving and commands.
pub(crate) mod commands_rva {
    pub(crate) use super::stream::VTABLE as STREAM_VTABLE;
    pub(crate) const TOKENS_READY: usize = 0x2357b30;
    pub(crate) const TOKENS_LAST: usize = 0x2357b28;
    pub(crate) const TOKEN_NAMES: usize = 0x242b888;
    pub(crate) const WRITER_VTABLE: usize = 0x1dbf7d8;
}

/// Shared native save-writer constants and token representation.
pub(crate) mod saves_rva {
    use super::GameFunction;
    pub(crate) const WRITE_TOKEN: GameFunction<2> = GameFunction::at(0x1708870);
    pub(crate) const WRITE_CLOSE: GameFunction<1> = GameFunction::at(0x17087e0);
    pub(crate) const WRITER_DEPTH: usize = 0x8;
    pub(crate) const EQUALS: u32 = 1;
    pub(crate) const NUMBER: u32 = 0xc;
    pub(crate) const TOKEN_TEXT_LEN: usize = 512;

    #[repr(C)]
    pub(crate) struct GameToken {
        kind: u32,
        text: [u8; TOKEN_TEXT_LEN],
        rest: u32,
    }

    impl GameToken {
        pub(crate) fn new(kind: u32, text: &str) -> Self {
            let mut token = Self { kind, text: [0; TOKEN_TEXT_LEN], rest: 0 };
            token.text[..text.len()].copy_from_slice(text.as_bytes());
            token
        }

        /// A token of text the game keeps, its NUL included.
        pub(crate) fn from_bytes(kind: u32, text: &[u8]) -> Self {
            let mut token = Self { kind, text: [0; TOKEN_TEXT_LEN], rest: 0 };
            let n = text.len().min(TOKEN_TEXT_LEN - 1);
            token.text[..n].copy_from_slice(&text[..n]);
            token
        }
    }
}

/// Shared native map renderer RVAs used by drawing and map coloring.
pub(crate) mod map_rva {
    use super::GameFunction;
    pub(crate) const RENDERER: usize = 0x10a8c30;
    /// The in-game idler's map mode set (`drawing::deferred` defers it from a moved capital).
    pub(crate) const REFRESH: GameFunction<4> = GameFunction::at(0x821ff0);
}

/// Shared native calendar date RVAs.
pub(crate) mod dates_rva {
    /// `(&date)`: the date's year.
    pub(crate) const YEAR: usize = 0xcfd70;
}

pub fn points() -> Vec<Point> {
    let mut all = campaign::points();
    all.extend(stream::points());
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
