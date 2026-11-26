//! What the map's canal update draws outside any frame, in vanilla too:
//! `0x78f040` applies queued canal changes (`0x78fe60` during `0x7751b0`,
//! "Initializing savegame", and `0x78fb30` for the in-game canal effects).
//! Whenever the list is nonempty it looks up `"CANAL_CONSTRUCTION"` and hands
//! it to `0x10b3600(graphics, text, 0, 0)`, which emits D3D9 draw calls to the
//! back buffer without `BeginScene`, `EndScene` or `Present`. When a save with
//! canals is loaded from the main menu, the next loading screen draws its GUI
//! over that back buffer without clearing it (`0x11bb450`), leaving that text
//! in the middle of the loading screen. The out-of-scene draw is skipped.
use crate::native::points::Point;

/// In the map's canal update (`0x78f040`), the branch that skips the
/// out-of-scene status text when the change list is empty:
/// `cmp dword ptr [rsi+0x14], 0; je 0x78f0c9`.
const CANAL_STATUS_TEXT_BRANCH: usize = 0x78f084;

/// The repair as the world reset sees it ([`super::REPAIRS`]): it counts
/// nothing.
pub(super) const REPAIR: super::Repair = super::Repair { points, ..super::Repair::NONE };

pub(super) fn points() -> Vec<Point> {
    vec![
        // Take the existing skip edge unconditionally (`jmp 0x78f0c9`), leaving
        // the canal mesh, map-list and river-layer updates at `0x78f0c9` intact.
        Point::write(CANAL_STATUS_TEXT_BRANCH, "83 7e 14 00 74 3f", "canal update's out-of-scene status text branch", "83 7e 14 00 eb 3f"),
        Point::keep(0x78f099usize, "48 8d 15 b0 26 52 01 48 8d 4c 24 20 e8 f6 43 d2 00", "canal update's CANAL_CONSTRUCTION lookup"),
        Point::keep(0x78f0b1usize, "48 8d 54 24 20 48 8b cb e8 42 45 92 00", "canal update's direct text draw"),
    ]
}
