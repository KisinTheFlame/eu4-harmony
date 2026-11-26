//! Native game `.gui` interface integration: host-window element binding,
//! extension windows and pop-ups, window ordering, and late native tooltips.

pub(crate) mod tooltips;
pub(crate) mod ui;
pub(crate) mod window_ordering;
pub(crate) mod windows;

use crate::base::Result;
use crate::native::memory::Memory;
use crate::native::points::Point;
use std::sync::OnceLock;

/// Callbacks wired by top-level `app` so `interface` never depends on
/// `world`, `capabilities`, `lua`, `speed`, or `framework_ui`.
pub(crate) struct InterfaceHooks {
    pub(crate) mod_id: fn(usize) -> Option<String>,
    pub(crate) in_game: fn(&Memory) -> bool,
    pub(crate) modifiers_held: fn() -> (bool, bool),
    pub(crate) on_main_menu_frame: fn(usize, bool),
    pub(crate) framework_tooltip_at: fn() -> Option<Vec<u8>>,
    pub(crate) player_letters: fn(&Memory) -> Option<String>,
    pub(crate) resolve_shield_country: fn(&Memory, Option<&str>) -> Result<Option<u64>>,
}

static HOOKS: OnceLock<InterfaceHooks> = OnceLock::new();

pub(crate) fn set_hooks(hooks: InterfaceHooks) {
    let _ = HOOKS.set(hooks);
}

pub(super) fn mod_id(extension: usize) -> Option<String> {
    HOOKS.get().and_then(|h| (h.mod_id)(extension))
}

pub(super) fn in_game(m: &Memory) -> bool {
    HOOKS.get().is_some_and(|h| (h.in_game)(m))
}

pub(super) fn modifiers_held() -> (bool, bool) {
    HOOKS.get().map_or((false, false), |h| (h.modifiers_held)())
}

pub(super) fn on_main_menu_frame(frontend: usize, show_corner: bool) {
    if let Some(h) = HOOKS.get() {
        (h.on_main_menu_frame)(frontend, show_corner);
    }
}

pub(super) fn framework_tooltip_at() -> Option<Vec<u8>> {
    HOOKS.get().and_then(|h| (h.framework_tooltip_at)())
}

pub(super) fn player_letters(m: &Memory) -> Option<String> {
    HOOKS.get().and_then(|h| (h.player_letters)(m))
}

pub(super) fn resolve_shield_country(m: &Memory, named: Option<&str>) -> Result<Option<u64>> {
    match HOOKS.get() {
        Some(h) => (h.resolve_shield_country)(m, named),
        None => Ok(None),
    }
}

pub(crate) fn points() -> Vec<Point> {
    let mut all = ui::points();
    all.extend(windows::points());
    all.extend(window_ordering::points());
    all.extend(tooltips::points());
    all
}
