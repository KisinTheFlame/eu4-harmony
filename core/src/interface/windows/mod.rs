//! Extension windows and pop-ups (cli/skills/ui/windows.md,
//! ADR extension-windows-are-game-windows): windows of the game's own
//! interface, built from definitions the framework writes out of the
//! extensions' layouts ([`gui`]).
//!
//! What extensions want, and the rules it is held to, are the Lua host's
//! ([`eu4_harmony_lua::windows::Windows`]), which the fake game keeps too;
//! the core adds what only the game needs: the mods' words for the
//! definitions, the clicks heard from the game's buttons and the generation
//! of the definitions. At every safe frame of a game, [`live`] makes the
//! game's windows show it: it builds a window that is to be open and has
//! none, kills one that is to be closed, and sets what changed. So what an
//! extension sets holds while its window is closed, and across the game
//! building its interface again. Clicks, the windows the player opened or
//! closed and the pop-ups shown come back out of it as what extensions and
//! remote clients hear.
mod gui;
// FIXED_BACKGROUND_SPRITE now lives in `native::splice`.
mod live;

use eu4_harmony_lua::{
    dispatch::{Click, PopupPressed, WindowSeen},
    layout::Window,
    Popup, WindowChange,
};
use std::collections::BTreeMap;
use std::sync::Mutex;

pub(crate) use live::{frame, killed, managed_tooltip, session_ended, show_window, with_string};

#[derive(Default)]
struct Windows {
    /// What extensions want of their windows and pop-ups.
    wanted: eu4_harmony_lua::windows::Windows,
    images: BTreeMap<usize, Vec<eu4_harmony_lua::images::Image>>,
    /// The clicks on extension windows' buttons, heard at the next frame.
    clicks: Vec<Click>,
    /// Bumped whenever the declared windows change, so that their
    /// definitions are written and read again.
    generation: u64,
}

static WINDOWS: Mutex<Option<Windows>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Windows) -> T) -> T {
    let mut guard = WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Windows::default))
}

impl Windows {
    /// Opens or closes a window as the player does, heard at the next
    /// frame.
    fn show(&mut self, extension: usize, name: &str, open: bool) {
        let seen = self.wanted.show(extension, name, open);
        self.wanted.hear_at_frame(seen);
    }

    /// Closes everything of `extension`'s, unheard.
    fn forget(&mut self, extension: usize) {
        self.wanted.forget(extension);
        self.clicks.retain(|c| c.extension != extension);
    }

    /// Snapshots what the next safe frame needs without holding [`WINDOWS`]
    /// while calling the game. Definitions are rendered only when the native
    /// side has not yet read `self.generation`.
    fn snapshot(&self, last_read: Option<u64>, words_for: &impl Fn(&str) -> live::LookupWords) -> live::Wants {
        let need_definitions = last_read != Some(self.generation);
        let mut windows = Vec::new();
        let mut definitions = Vec::new();
        for d in self.wanted.declared() {
            let words = words_for(&d.id);
            for w in &d.windows {
                let type_name = format!("{}_{}", type_name(&d.id, &w.name), self.generation);
                if need_definitions {
                    definitions.push(gui::window(&type_name, w, &|text| text.as_ref().map(|t| t.resolve(&*words)).unwrap_or_default()));
                }
                let mut wanted = self.wanted.wanted(d.extension, &w.name).cloned().unwrap_or_default();
                let resolve = |property: &mut eu4_harmony_lua::Property| {
                    if let eu4_harmony_lua::Property::Text(content) | eu4_harmony_lua::Property::Tooltip(content) = property {
                        *content = content.resolve(&*words).into();
                    }
                };
                for property in wanted.set.values_mut().flat_map(|p| p.values_mut()) {
                    resolve(property);
                }
                for (_, property) in wanted.rows.values_mut().flatten().flatten() {
                    resolve(property);
                }
                windows.push(live::WantWindow { extension: d.extension, type_name, layout: w.clone(), words: words.clone(), wanted });
            }
        }
        let popups = self.wanted.popups().iter().map(|p| (p.number, p.popup.clone())).collect();
        let appearances = self.wanted.popups().iter().map(|p| (p.number, p.appearance)).collect();
        live::Wants { generation: self.generation, windows, definitions, popups, appearances }
    }

    /// Entry-button specifications for the panel button strip.
    fn entry_specs(&self, words_for: &impl Fn(&str, &str) -> Option<String>) -> Vec<live::EntrySpec> {
        let mut out = Vec::new();
        for d in self.wanted.declared() {
            for w in &d.windows {
                if let Some(resolved) = self.wanted.resolved_entry(d.extension, &w.name, |key| words_for(&d.id, key)) {
                    out.push(live::EntrySpec { extension: d.extension, id: d.id.clone(), window: w.name.clone(), resolved });
                }
            }
        }
        out
    }

    /// Applies classified native clicks to the desired state, returning any
    /// pop-up shield clicks `(popup_number, side)` that open diplomacy after
    /// locks are released.
    fn apply_clicks(&mut self, routed: Vec<live::RoutedClick>) -> Vec<(u32, usize)> {
        let mut shields = Vec::new();
        for click in routed {
            match click {
                live::RoutedClick::Widget { extension, window, widget, row, right, modifiers } => {
                    if self.wanted.check_press(extension, &window, &widget, row).is_ok() && self.clicks.len() < 64 {
                        let (shift, ctrl) = (modifiers & crate::interface::ui::MODIFIER_SHIFT != 0, modifiers & crate::interface::ui::MODIFIER_CTRL != 0);
                        self.clicks.push(Click { extension, button: format!("{window}.{widget}"), right, shift, ctrl, row });
                    }
                }
                live::RoutedClick::Close { extension, window } => self.show(extension, &window, false),
                live::RoutedClick::Popup { number, button } => {
                    let _ = self.wanted.press(number, button);
                }
                live::RoutedClick::Shield { number, side } => shields.push((number, side)),
                live::RoutedClick::Entry { extension, window } => {
                    let seen = self.wanted.entry(extension, &window);
                    self.wanted.hear_at_frame(seen);
                }
            }
        }
        shields
    }

    /// Records dragged window positions and unbuildable pop-ups reported by
    /// native reconciliation.
    fn apply_reconciled(&mut self, events: &live::ReconcileEvents) {
        for (extension, name, at) in &events.dragged {
            self.wanted.dragged(*extension, name, *at);
        }
        for &number in &events.lost_popups {
            self.wanted.lost_popup(number);
        }
    }

    /// Closes extension panels when the game closes its own big panels.
    fn close_panels(&mut self) {
        let closed = self.wanted.close_panels(None);
        self.wanted.hear_at_frame(closed);
    }

    /// Ends the session in desired state.
    fn end_session(&mut self) {
        self.wanted.end_session();
        self.clicks.clear();
    }
}

// ---------- what the Lua host and remote control call ----------

/// The windows the extension `extension` of the mod `id` declares, from its
/// main.lua; its earlier windows, at an extension reload, are closed.
pub fn declare(extension: usize, id: &str, windows: &[Window], images: &[eu4_harmony_lua::images::Image]) {
    with(|all| {
        all.forget(extension);
        all.wanted.declare(extension, id, windows);
        all.images.insert(extension, images.to_vec());
        all.generation += 1;
    });
}

/// Closes everything of a stopped extension's, which no longer hears.
pub fn forget(extension: usize) {
    with(|all| all.forget(extension));
}

/// A change an extension makes to its windows, checked by the Lua host.
pub fn change(extension: usize, change: WindowChange) -> Result<(), String> {
    let in_game = crate::session::sessions::going_on();
    with(|all| all.wanted.change(extension, change, in_game))
}

/// The tooltip content retained independently of a native binding.
pub fn tooltip(extension: usize, window: &str, widget: &str) -> Result<eu4_harmony_lua::Text, String> {
    with(|all| all.wanted.tooltip(extension, window, widget).ok_or_else(|| format!("no tooltip target {window}.{widget} declared by this mod")))
}

/// Shows a pop-up of `extension`'s, and gives its number.
pub fn popup(extension: usize, popup: Popup) -> Result<u32, String> {
    let in_game = crate::session::sessions::going_on();
    with(|all| all.wanted.popup(extension, popup, in_game))
}

/// Closes `extension`'s pop-up `number`; nothing when it is not open.
pub fn close_popup(extension: usize, number: u32) -> Result<(), String> {
    with(|all| all.wanted.close_popup(extension, number));
    Ok(())
}

/// The pop-ups open, oldest first, as `popup.list` gives them: each
/// button's words as they stand, left to right, the game's own OK in the
/// game's language.
pub fn popups() -> Vec<eu4_harmony_remote::popups::Popup> {
    let game_words = |key: &str| {
        crate::native::memory()
            .and_then(|m| crate::native::game_string::unformatted(m, key).ok())
            .filter(|w| !w.is_empty())
            .map(|w| crate::text::unicode::game_text::chars(&w).map(|(c, _)| c).collect())
            .unwrap_or_else(|| key.to_string())
    };
    let open: Vec<(u32, usize, Popup)> = with(|all| all.wanted.popups().iter().map(|p| (p.number, p.extension, p.popup.clone())).collect());
    open.into_iter()
        .map(|(number, extension, popup)| eu4_harmony_remote::popups::Popup {
            popup: number,
            mod_id: super::mod_id(extension).unwrap_or_default(),
            header: popup.header.clone(),
            title: popup.title.clone(),
            text: popup.text.clone(),
            buttons: popup.shown_buttons(&game_words(gui::OK)),
        })
        .collect()
}

/// Presses the button `button` (from 1, left to right as `popup.list`
/// gives them) of the pop-up `number`, as the player does; or why not.
pub fn press_popup(number: u32, button: u32) -> Result<(), String> {
    with(|all| all.wanted.press(number, button))
}

/// The pop-ups' buttons pressed since the last frame.
pub fn take_popups_pressed() -> Vec<PopupPressed> {
    with(|all| all.wanted.take_pressed())
}

/// The extension windows opened and closed since the last frame.
pub fn take_seen() -> Vec<WindowSeen> {
    with(|all| all.wanted.take_seen())
}

/// The clicks on extension windows' buttons since the last frame.
pub fn take_clicks() -> Vec<Click> {
    with(|all| std::mem::take(&mut all.clicks))
}

/// The pop-ups shown since the last frame: each one's number and its mod.
pub fn take_shown() -> Vec<(u32, String)> {
    let shown = with(|all| all.wanted.take_shown());
    shown.into_iter().map(|(number, extension)| (number, super::mod_id(extension).unwrap_or_default())).collect()
}

pub fn points() -> Vec<crate::native::points::Point> {
    live::points()
}

/// The framework files the definitions are read from, empty until
/// extensions declare windows (ADR framework-files-in-the-game-file-system).
pub fn files() -> Vec<crate::native::files::File> {
    let mut files: Vec<crate::native::files::File> = [live::WINDOWS_FILE, live::POPUPS_FILE]
        .into_iter()
        .map(|path| crate::native::files::File { path: path.into(), data: gui::file(&[]).into_bytes() })
        .collect();
    let mut sprites = gui::SPRITES.trim_end().strip_suffix('}').expect("the framework sprites close their block").to_string();
    with(|all| {
        for image in all.images.values().flatten() {
            sprites.push_str(&image.definition());
            files.push(crate::native::files::File { path: image.path.clone(), data: image.data.to_vec() });
        }
    });
    sprites.push_str("}\n");
    files.push(crate::native::files::File { path: SPRITES_FILE.into(), data: sprites.into_bytes() });
    files
}

/// The framework file of the sprites the definitions use.
const SPRITES_FILE: &str = "interface/eu4harmony_windows.gfx";

/// The panel button strip's file and window, where entry buttons are
/// spliced as the game reads it.
const STRIP_FILE: &str = "interface/menubar.gui";
const STRIP_WINDOW: &str = "floating_buttons";

/// Splices the entry buttons of the windows declared at startup into the
/// panel button strip; what could not be, as notes. A window whose entry
/// button an extension reload adds shows it from the next start.
pub fn splice_entries(plan: &mut crate::native::splice::Plan) -> Vec<String> {
    with(|all| {
        let mut text = String::new();
        for d in all.wanted.declared() {
            for w in &d.windows {
                if let Some(entry) = &w.entry {
                    text.push_str(&live::entry_fragment(&d.id, &w.name, entry));
                }
            }
        }
        if !text.is_empty() {
            plan.add(STRIP_FILE, crate::native::splice::Addition::Window { window: STRIP_WINDOW.into(), text });
        }
        Vec::new()
    })
}

/// The element names of a window's definition: its own, and its game
/// widgets' by their names, with the mod's prefix.
fn type_name(id: &str, window: &str) -> String {
    format!("{}{window}", crate::native::splice::prefix(id))
}
