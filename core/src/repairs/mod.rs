//! Repairs (GLOSSARY): the game's own defects the framework puts right, each
//! where the game goes wrong, so that they hold in vanilla's paths too
//! (docs/research/main-menu.md). One file each:
//!
//! - [`reset_leaks`]: what a world reset leaks.
//! - [`modifiers`]: what a country's modifiers leak as they go or are made
//!   again.
//! - [`monarchs`]: what a country's history leaks of its monarchs, heirs and
//!   queens.
//! - [`new_start`]: what a world reset leaves of a session.
//! - [`session_interface`]: what of the session's interface the game never
//!   lets go.
//! - [`interface_parts`]: what of its interface the game leaves as it lets a
//!   part of it go.
//! - [`text_boxes`]: what a text box leaves as it goes.
//! - [`sprite_types`]: what the game unloads while it is shown.
//! - [`pages`]: what the game's heap keeps of the sessions it let go.
//! - [`masks`]: what the textures' transparency masks keep.
//! - [`coalitions`]: what the game's AI changes on several worker threads at
//!   once without a lock.
//! - [`names`]: what the game's pick of a name from a list overflows.
//! - [`save_checksum`]: what the save checksum of a save loaded in a session
//!   reads.
//!
//! Here is what they share: letting the game's memory go, and the world's
//! own reset, after which several of them act and all say what they did.
mod coalitions;
mod interface_parts;
mod masks;
mod modifiers;
mod monarchs;
mod names;
mod new_start;
mod pages;
mod reset_leaks;
mod save_checksum;
mod session_interface;
mod sprite_types;
mod text_boxes;

pub use new_start::after_history;
pub use session_interface::SessionInterface;

use crate::{hooks::HookFrame, memory::Memory, points::Point};
use std::sync::atomic::Ordering;

/// `(world)`: the world's own reset, inside the game's and at startup.
const WORLD_RESET: usize = 0x749020;

/// While it lives, `repair` lets the game's memory go on this thread: a
/// developer build's quarantine keeps what is freed then, while it is on
/// (docs/leak-finding.md, M4). What the game frees later, as the GUI's sweep
/// frees a window a repair killed, is not kept.
struct LettingGo;

impl LettingGo {
    fn new(repair: &'static str) -> Self {
        #[cfg(feature = "dev-heap-census")]
        crate::census::quarantine::begin(repair);
        #[cfg(not(feature = "dev-heap-census"))]
        let _ = repair;
        Self
    }
}

impl Drop for LettingGo {
    fn drop(&mut self) {
        #[cfg(feature = "dev-heap-census")]
        crate::census::quarantine::end();
    }
}

/// A session's frame ran: the first after a world reset lets go the persons
/// the histories left ([`monarchs::let_go`]) and gives back free pages
/// ([`pages::give_back_pages`]).
pub fn session_frame() {
    monarchs::let_go(&crate::hooked_memory());
    pages::give_back_pages();
}

/// The main menu showed: the first time, the Empire's religions as the
/// definitions give them are kept ([`new_start`]); the first frame after a
/// world reset lets go the persons the histories left
/// ([`monarchs::let_go`]) and gives back free pages
/// ([`pages::give_back_pages`]).
pub fn main_menu_shown(m: &Memory) {
    monarchs::let_go(m);
    pages::give_back_pages();
    new_start::main_menu_shown(m);
}

/// After every world reset: what it leaves of the session is put back as a
/// new start has it ([`new_start::as_a_new_start`]). A load sets it again from its save.
extern "C" fn world_reset(frame: *mut HookFrame) {
    crate::guarded("world reset", || {
        pages::PAGES_TO_GIVE_BACK.store(true, Ordering::Relaxed);
        monarchs::world_reset();
        let m = crate::hooked_memory();
        let world = unsafe { (*frame).args[0] };
        match new_start::as_a_new_start(&m, world, true) {
            Ok(found) if found.is_empty() => {}
            Ok(found) => log!("repair: the world reset left {found}; put back as a new start has them"),
            Err(e) => log!("repair: what the world reset leaves was not put back: {e}"),
        }
        log!(
            "repair: since the last world reset, {} mercenary companies' leaders, {} rebel factions, {} agents of the AI and {} sets of modifiers ({} of them once the modifiers that listed them went) and {} text boxes' animations were deleted with what owned them; {} sets of modifiers wait for the modifiers that list them; {:.1} MB of transparency masks were let go; {} monarchs, heirs and queens the histories left were deleted, {} kept as the game still had them",
            reset_leaks::LEADERS.swap(0, Ordering::Relaxed),
            reset_leaks::FACTIONS.swap(0, Ordering::Relaxed),
            reset_leaks::AGENTS.swap(0, Ordering::Relaxed),
            modifiers::MODIFIER_SETS.swap(0, Ordering::Relaxed),
            modifiers::MODIFIER_SETS_WAITED.swap(0, Ordering::Relaxed),
            text_boxes::TEXT_BOX_ANIMATIONS.swap(0, Ordering::Relaxed),
            modifiers::MODIFIER_SETS_WAITING_COUNT.load(Ordering::Relaxed),
            masks::MASK_BYTES.swap(0, Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
            monarchs::PERSONS.swap(0, Ordering::Relaxed),
            monarchs::PERSONS_KEPT.swap(0, Ordering::Relaxed)
        );
        #[cfg(feature = "dev-heap-census")]
        crate::census::world_reset();
    });
}

pub fn points() -> Vec<Point> {
    let mut all = vec![Point::observe(
        WORLD_RESET,
        "48 89 5c 24 10 48 89 74 24 18 55 57 41 56 48 8b ec 48 83 ec 60 48 8b f1",
        "world's own reset",
        1,
        None,
        Some(world_reset),
    )];
    for points in [
        save_checksum::points,
        masks::points,
        coalitions::points,
        names::points,
        reset_leaks::points,
        modifiers::points,
        monarchs::points,
        session_interface::points,
        interface_parts::points,
        text_boxes::points,
        sprite_types::points,
    ] {
        all.extend(points());
    }
    all
}
