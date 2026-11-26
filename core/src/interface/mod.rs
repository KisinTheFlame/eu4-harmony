//! Native game `.gui` interface integration: host-window element binding,
//! extension windows and pop-ups, window ordering, and late native tooltips.

pub(super) mod bound_element;
pub(crate) mod tooltips;
pub(crate) mod ui;
pub(crate) mod window_ordering;
pub(crate) mod windows;

use crate::base::Result;
use crate::native::memory::Memory;
use crate::native::points::Point;
use std::sync::OnceLock;

/// Callbacks wired by top-level `app` so `interface` never depends on
/// `lua`, `speed`, or `framework_ui`.
pub(crate) struct InterfaceHooks {
    pub(crate) mod_id: fn(usize) -> Option<String>,
    pub(crate) on_main_menu_frame: fn(usize, bool),
    pub(crate) framework_tooltip_at: fn() -> Option<Vec<u8>>,
}

static HOOKS: OnceLock<InterfaceHooks> = OnceLock::new();

pub(crate) fn set_hooks(hooks: InterfaceHooks) {
    let _ = HOOKS.set(hooks);
}

pub(super) fn mod_id(extension: usize) -> Option<String> {
    HOOKS.get().and_then(|h| (h.mod_id)(extension))
}

pub(super) fn in_game(m: &Memory) -> bool {
    crate::world::context(m).is_ok()
}

pub(super) fn modifiers_held() -> (bool, bool) {
    crate::render::input::modifiers_held()
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
    crate::world::context(m).ok().map(|ctx| String::from_utf8_lossy(&ctx.player.letters()).into_owned())
}

pub(super) fn resolve_shield_country(m: &Memory, named: Option<&str>) -> Result<Option<u64>> {
    let Ok(ctx) = crate::world::context(m) else {
        return Ok(None);
    };
    let letters = match named {
        Some(s) => s.to_string(),
        None => String::from_utf8_lossy(&ctx.player.letters()).into_owned(),
    };
    let bytes: [u8; 3] = letters.as_bytes().try_into().map_err(|_| format!("{letters:?} is not a tag"))?;
    let tag = crate::world::country_named(m, &ctx, eu4_harmony_remote::CountryTag(bytes)).map_err(|e| e.message)?;
    Ok(Some(tag.0))
}

pub(crate) fn points() -> Vec<Point> {
    let mut all = ui::points();
    all.extend(windows::points());
    all.extend(window_ordering::points());
    all.extend(tooltips::points());
    all
}
