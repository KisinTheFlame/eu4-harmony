//! Native game `.gui` interface integration: host-window element binding,
//! extension windows and pop-ups, window ordering, and late native tooltips.

pub(super) mod bound_element;
mod native_gui;
pub(crate) mod tooltips;
pub(crate) mod ui;
pub(crate) mod window_ordering;
pub(crate) mod windows;

use crate::{
    base::Result,
    native::{
        game_string,
        hooks::Call,
        memory::Memory,
        objects::gui::{classes, element, functions, main_menu, window},
        points::Point,
    },
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    OnceLock,
};

const BUTTON_DESTROY: usize = 0x10b5670;
/// Root window deleting destructor, reached from the GUI manager's sweep.
const WINDOW_DELETE: usize = 0x1546510;
const MENU_FRAME: usize = 0x10d2e80;

/// Sprite name for non-moveable framework window frames and extension windows
/// (all button states keep the panel's original colors).
pub(super) const FIXED_BACKGROUND_SPRITE: &str = "GFX_eu4harmony_fixed_window_background";

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

fn on_main_menu_frame(frontend: usize, show_corner: bool) {
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

/// Whether the main menu shows its first page ([`main_menu::PAGE`];
/// `0x10daf60` switches pages).
fn main_menu_page(m: &Memory, frontend: usize) -> Result<bool> {
    if m.ptr(frontend)? != m.at(classes::MAIN_MENU) {
        return Err("unexpected main menu object".into());
    }
    Ok(m.u32(frontend + main_menu::PAGE)? == 0)
}

/// The native main-menu multiplayer button while page 0 is alive, kept disabled
/// because Harmony currently supports single player only.
static MULTIPLAYER_BUTTON: AtomicUsize = AtomicUsize::new(0);

/// Tooltip text for the disabled main-menu multiplayer button when hovered.
/// Rendered by EU4's native `ToolTip_Font`, so it follows the startup session
/// language (`lang::player()`) whose font pages are loaded rather than `lang::ui()`.
pub(super) fn multiplayer_tooltip(target: usize) -> Option<Vec<u8>> {
    if target == 0 || MULTIPLAYER_BUTTON.load(Ordering::Relaxed) != target {
        return None;
    }
    let words = crate::base::loc::framework(crate::base::lang::player(), "EU4HARMONY_MULTIPLAYER_UNSUPPORTED", &[]);
    Some(crate::text::unicode::game_text::from_str(&words))
}

/// Finds `multi_player_button` inside page 0's `mainmenu_panel_bottom` and
/// disables it so the game's per-frame Steam/dialog refresh (`0x10eff90`,
/// `0x10f0860`) cannot re-enable it.
fn disable_multiplayer_button(m: &Memory, frontend: usize) -> Result<()> {
    let pages = m.ptr(frontend + main_menu::PAGES)?;
    if pages == 0 {
        return Err("main menu has no pages array".into());
    }
    let page0 = m.ptr(pages)?;
    if page0 == 0 || m.ptr(page0)? != m.at(classes::MAIN_MENU_FIRST_PAGE) {
        return Err("unexpected main menu first page object".into());
    }
    let mainmenu = m.ptr(page0 + main_menu::FIRST_PAGE_WINDOW)?;
    if mainmenu == 0 || m.ptr(mainmenu)? != m.at(classes::WINDOW) {
        return Err("unexpected main menu root window".into());
    }
    let begin = m.ptr(mainmenu + window::SUBWINDOWS)?;
    let end = m.ptr(mainmenu + window::SUBWINDOWS_END)?;
    if begin == 0 || end < begin || !(end - begin).is_multiple_of(window::SUBWINDOW_ENTRY) {
        return Err("unexpected main menu subwindows vector".into());
    }
    let count = ((end - begin) / window::SUBWINDOW_ENTRY).min(64);
    let mut panel = 0usize;
    for i in 0..count {
        let entry = begin + i * window::SUBWINDOW_ENTRY;
        if game_string::read(m, entry).is_ok_and(|name| name == b"mainmenu_panel_bottom") {
            panel = m.ptr(entry + window::SUBWINDOW_PTR)?;
            break;
        }
    }
    if panel == 0 || m.ptr(panel)? != m.at(classes::WINDOW) {
        return Err("mainmenu_panel_bottom subwindow not found".into());
    }
    let button = native_gui::lookup(m, panel, (window::FIND_BUTTON, functions::FIND_BUTTON), "multi_player_button")?;
    if button == 0 || m.ptr(button)? != m.at(classes::BUTTON) {
        return Err("multi_player_button not found".into());
    }
    if native_gui::element_name(m, button)? != b"multi_player_button" {
        return Err("multi_player_button lookup returned another element".into());
    }
    if m.vslot(button, element::ENABLE)? != m.at(native_gui::SET_ENABLED) {
        return Err("multi_player_button enable method changed".into());
    }
    native_gui::SET_ENABLED.call(m, [button, 0])?;
    MULTIPLAYER_BUTTON.store(button, Ordering::Relaxed);
    Ok(())
}

fn menu_after(call: &mut Call) {
    static PAGE_ERROR_LOGGED: AtomicBool = AtomicBool::new(false);
    static MP_ERROR_LOGGED: AtomicBool = AtomicBool::new(false);
    let m = call.memory();
    if call.faulted() || crate::base::thread_id() != crate::base::main_thread() {
        return;
    }
    crate::text::unicode::log_once();
    let show_corner = match main_menu_page(&m, call.arg(0)) {
        Ok(page) => page,
        // The corner button then shows on every page of the main menu.
        Err(e) => {
            if !PAGE_ERROR_LOGGED.swap(true, Ordering::Relaxed) {
                log!("ui: main menu page unknown: {e}");
            }
            true
        }
    };
    if show_corner {
        if let Err(e) = disable_multiplayer_button(&m, call.arg(0)) {
            if !MP_ERROR_LOGGED.swap(true, Ordering::Relaxed) {
                log!("ui: could not disable multiplayer button: {e}");
            }
        }
    }
    on_main_menu_frame(call.arg(0), show_corner);
}

fn button_destroy(call: &mut Call) {
    let button = call.arg(0);
    let _ = MULTIPLAYER_BUTTON.compare_exchange(button, 0, Ordering::Relaxed, Ordering::Relaxed);
    native_gui::drop_taking_right(button);
    ui::button_gone(call.memory(), button);
}

/// Every teardown path of a root window passes here first (notice-window.md §3).
fn window_kill(call: &mut Call) {
    window_teardown(call.memory(), call.arg(0));
}

/// Backstop for a root freed without passing the kill hook.
fn window_delete(call: &mut Call) {
    window_teardown(call.memory(), call.arg(0));
}

fn window_teardown(m: Memory, window: usize) {
    window_ordering::retire(window);
    ui::window_gone(m, window);
    windows::killed(&m, window);
}

pub(crate) fn points() -> Vec<Point> {
    let mut all = native_gui::points();
    all.extend([
        Point::observe(BUTTON_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8", "button destructor", 2, Some(button_destroy), None),
        Point::observe(
            native_gui::WINDOW_KILL,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 c6 81 f4 00 00 00 01",
            "window kill",
            1,
            Some(window_kill),
            None,
        ),
        Point::observe(
            WINDOW_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 0c 02 00 00 f6 c3 01 74 0d ba 80 0a 00 00",
            "window destructor",
            2,
            Some(window_delete),
            None,
        ),
        Point::observe(MENU_FRAME, "48 8b c4 48 89 58 10 55 56 57 41 54 41 55 41 56", "main menu frame", 2, None, Some(menu_after)),
        Point::keep(0x10db037usize, "41 8b 8e 00 09 00 00 49 8b 86 e8 08 00 00 48 8b 0c c8 48 8b 01 ff 50 18 41 89 be 00 09 00 00", "main menu page switch")
            .uses(&[main_menu::PAGE]),
    ]);
    all.extend(ui::points());
    all.extend(windows::points());
    all.extend(window_ordering::points());
    all.extend(tooltips::points());
    all
}
