//! Repairs (GLOSSARY): the game's own defects the framework puts right, each
//! where the game goes wrong, so that they hold in vanilla's paths too
//! (docs/research/main-menu.md). One file each:
//!
//! - [`reset_leaks`]: what a world reset leaks.
//! - [`modifiers`]: what a country's modifiers leak as they go or are made
//!   again.
//! - [`histories`]: what the records of the histories leave: the monarchs,
//!   heirs and queens of the countries' histories, the battles of the wars'.
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
//! - [`supporters`]: what a subject relation's end frees under its walk of
//!   the subject's supporters.
//!
//! Here is what they share: letting the game's memory go, and the world's
//! own reset and the first frame after it, which the sessions tell
//! (`crate::sessions`): after the reset several of them act and all say what
//! they did, and its first frame lets go what had to wait for it.
mod coalitions;
mod estates;
mod histories;
mod history_lists;
mod interface_parts;
mod list_entries;
mod masks;
mod modifiers;
mod names;
mod new_start;
mod pages;
mod panels;
mod reset_leaks;
mod save_checksum;
mod session_interface;
mod sprite_types;
mod supporters;
mod text_boxes;
mod trade_nodes;

pub use new_start::after_history;
pub use session_interface::SessionInterface;

use crate::{memory::Memory, points::Point};
use std::sync::atomic::Ordering;

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

/// The first safe frame after a world reset (`crate::sessions`), of the main
/// menu or of the session it began: lets go the persons the histories left
/// ([`histories::let_go`]) and gives back free pages
/// ([`pages::give_back_pages`]).
pub fn first_frame_after_reset(m: &Memory) {
    histories::let_go(m);
    pages::give_back_pages();
}

/// The main menu showed: the first time, the Empire's religions as the
/// definitions give them are kept ([`new_start`]).
pub fn main_menu_shown(m: &Memory) {
    new_start::main_menu_shown(m);
}

/// After every world reset (`crate::sessions`), of the world `world`: what it
/// leaves of the session is put back as a new start has it
/// ([`new_start::as_a_new_start`]). A load sets it again from its save.
pub fn world_reset(m: &Memory, world: usize) {
    histories::world_reset();
    match new_start::as_a_new_start(m, world, true) {
        Ok(found) if found.is_empty() => {}
        Ok(found) => log!("repair: the world reset left {found}; put back as a new start has them"),
        Err(e) => log!("repair: what the world reset leaves was not put back: {e}"),
    }
    log!(
            "repair: since the last world reset, {} mercenary companies' leaders, {} rebel factions, {} agents of the AI and {} sets of modifiers ({} of them once the modifiers that listed them went) and {} text boxes' animations were deleted with what owned them; {} sets of modifiers wait for the modifiers that list them; {:.1} MB of transparency masks were let go; {} monarchs, heirs and queens the histories left were deleted, {} kept as the game still had them; {} battles the wars' histories left were deleted; {} relations the diplomatic histories left were deleted, {} kept as the diplomacy was given them; {} records and blocks of records the countries' first histories listed alone were deleted, {} kept as their second histories held some of what they held; {} trade nodes the world's clear replaced were deleted; {} modifiers the estates it cleared left were deleted; {} entries the lists' kills left were deleted; {} windows the session's panels and the main menu's pages left were killed with them",
            reset_leaks::LEADERS.swap(0, Ordering::Relaxed),
            reset_leaks::FACTIONS.swap(0, Ordering::Relaxed),
            reset_leaks::AGENTS.swap(0, Ordering::Relaxed),
            modifiers::MODIFIER_SETS.swap(0, Ordering::Relaxed),
            modifiers::MODIFIER_SETS_WAITED.swap(0, Ordering::Relaxed),
            text_boxes::TEXT_BOX_ANIMATIONS.swap(0, Ordering::Relaxed),
            modifiers::MODIFIER_SETS_WAITING_COUNT.load(Ordering::Relaxed),
            masks::MASK_BYTES.swap(0, Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
            histories::PERSONS.swap(0, Ordering::Relaxed),
            histories::PERSONS_KEPT.swap(0, Ordering::Relaxed),
            histories::BATTLES.swap(0, Ordering::Relaxed),
            histories::RELATIONS.swap(0, Ordering::Relaxed),
            histories::RELATIONS_KEPT.swap(0, Ordering::Relaxed),
            history_lists::DELETED.swap(0, Ordering::Relaxed),
            history_lists::SHARED.swap(0, Ordering::Relaxed),
            trade_nodes::DELETED.swap(0, Ordering::Relaxed),
            estates::DELETED.swap(0, Ordering::Relaxed),
            list_entries::DELETED.swap(0, Ordering::Relaxed),
            panels::KILLED.swap(0, Ordering::Relaxed)
        );
}

pub fn points() -> Vec<Point> {
    let mut all = Vec::new();
    for points in [
        save_checksum::points,
        masks::points,
        coalitions::points,
        names::points,
        supporters::points,
        reset_leaks::points,
        modifiers::points,
        histories::points,
        history_lists::points,
        session_interface::points,
        interface_parts::points,
        panels::points,
        text_boxes::points,
        sprite_types::points,
        trade_nodes::points,
        estates::points,
        list_entries::points,
    ] {
        all.extend(points());
    }
    all
}
