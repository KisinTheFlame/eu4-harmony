//! Repairs (GLOSSARY): the game's own defects the framework puts right, each
//! where the game goes wrong, so that they hold in vanilla's paths too
//! (docs/research/main-menu.md). One file each, which says what it puts
//! right and declares the repair as its `REPAIR`; [`REPAIRS`] holds them all.
//!
//! Here is what they share: letting the game's memory go ([`LettingGo`]),
//! what waits to be let go until the game can no longer use it
//! ([`waiting`]), and the moments of the sessions a repair acts on, which the
//! sessions tell (`crate::sessions`) and a repair declares: the main menu
//! shown, the world's clear, the world's reset, after which all say what they
//! did, the history read after it, and the first frame after it.
mod ai_draws;
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
mod waiting;

pub use session_interface::SessionInterface;

use crate::{memory::Memory, points::Point};

/// While it lives, `repair` lets the game's memory go on this thread: a
/// developer build's quarantine keeps what is freed then, while it is on
/// (docs/plans/leak-finding.md, M4). What the game frees later, as the GUI's sweep
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
/// menu or of the session it began: each repair lets go what had to wait
/// for it.
pub fn first_frame_after_reset(m: &Memory) {
    for repair in REPAIRS {
        if let Some(act) = repair.first_frame_after_reset {
            act(m);
        }
    }
}

/// A frame of the main menu ran (`crate::safe_point`): each repair acts on
/// it in turn.
pub fn main_menu_shown(m: &Memory) {
    for repair in REPAIRS {
        if let Some(act) = repair.main_menu_shown {
            act(m);
        }
    }
}

/// The world's own reset begins to clear the world, before any country is
/// cleared (`reset_leaks`): each repair acts on it in turn.
fn world_clearing(m: &Memory) {
    for repair in REPAIRS {
        if let Some(act) = repair.world_clearing {
            act(m);
        }
    }
}

/// After every world reset (`crate::sessions`), of the world `world`: what
/// waited for it to end is due, each repair acts on it, and all say what
/// they let go since the last.
pub fn world_reset(m: &Memory, world: usize) {
    waiting::Moment::Reset.passed();
    for repair in REPAIRS {
        if let Some(act) = repair.world_reset {
            act(m, world);
        }
    }
    log!("repair: since the last world reset, {}", since_reset());
}

/// After a world reset that read the game's history (`crate::sessions`):
/// each repair acts on it in turn.
pub fn after_history(m: &Memory) {
    for repair in REPAIRS {
        if let Some(act) = repair.after_history {
            act(m);
        }
    }
}

/// A repair as the sessions and startup see it, which its own file declares
/// as its `REPAIR`: its dependency points, what it says it let go since the
/// last world reset, taking its counts, if it counts any, and the moments of
/// the sessions it acts on.
struct Repair {
    points: fn() -> Vec<Point>,
    since_reset: Option<fn() -> String>,
    /// At every frame of the main menu.
    main_menu_shown: Option<fn(&Memory)>,
    /// As the world's own reset begins to clear the world, before any
    /// country is cleared.
    world_clearing: Option<fn(&Memory)>,
    /// After every world reset, of the world given.
    world_reset: Option<fn(&Memory, usize)>,
    /// After a world reset that read the game's history.
    after_history: Option<fn(&Memory)>,
    /// The first safe frame after a world reset.
    first_frame_after_reset: Option<fn(&Memory)>,
}

impl Repair {
    /// A repair with no points that counts nothing and acts at no moment, to
    /// declare one from.
    const NONE: Repair = Repair {
        points: Vec::new,
        since_reset: None,
        main_menu_shown: None,
        world_clearing: None,
        world_reset: None,
        after_history: None,
        first_frame_after_reset: None,
    };
}

/// Every repair, in the order they act at each moment.
const REPAIRS: &[Repair] = &[
    save_checksum::REPAIR,
    masks::REPAIR,
    coalitions::REPAIR,
    ai_draws::REPAIR,
    names::REPAIR,
    supporters::REPAIR,
    reset_leaks::REPAIR,
    modifiers::REPAIR,
    histories::REPAIR,
    history_lists::REPAIR,
    session_interface::REPAIR,
    interface_parts::REPAIR,
    panels::REPAIR,
    text_boxes::REPAIR,
    sprite_types::REPAIR,
    trade_nodes::REPAIR,
    estates::REPAIR,
    list_entries::REPAIR,
    new_start::REPAIR,
    pages::REPAIR,
];

/// What every repair that counts let go since the last world reset, one
/// after another; their counts start again.
fn since_reset() -> String {
    REPAIRS.iter().filter_map(|r| r.since_reset).map(|said| said()).collect::<Vec<_>>().join("; ")
}

pub fn points() -> Vec<Point> {
    REPAIRS.iter().flat_map(|r| (r.points)()).collect()
}

#[cfg(test)]
mod tests {
    /// Each repair's file is among [`super::REPAIRS`], which alone gives it
    /// its moments and its points: one left out would neither act nor be
    /// hooked.
    #[test]
    fn every_repair_is_among_the_repairs() {
        let text = include_str!("mod.rs");
        let repairs = &text[text.find("const REPAIRS").unwrap()..];
        let repairs = &repairs[..repairs.find("];").unwrap()];
        let modules: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("mod ").and_then(|r| r.strip_suffix(';'))).collect();
        assert!(modules.len() > 10, "the repairs' modules were not found");
        for module in modules.into_iter().filter(|&m| m != "waiting") {
            assert!(repairs.contains(&format!("{module}::REPAIR,")), "the repair {module} is not among REPAIRS");
        }
    }
}
