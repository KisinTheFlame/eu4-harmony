//! Extension windows and pop-ups (docs/plans/extension-windows.md,
//! ADR extension-windows-are-game-windows): windows of the game's own
//! interface, built from definitions the framework writes out of the
//! extensions' layouts ([`gui`]).
//!
//! What extensions want is kept here, apart from the game's windows that
//! show it: which windows are open, what each game widget shows, the rows of
//! each list, the pop-ups open and their answers. At every safe frame of a
//! game, [`live`] makes the game's windows show it: it builds a window that
//! is to be open and has none, kills one that is to be closed, and sets what
//! changed. So what an extension sets holds while its window is closed, and
//! across the game building its interface again. Clicks, the windows the
//! player opened or closed and the pop-ups shown come back out of it as
//! what extensions and remote clients hear.
mod gui;
pub(crate) use gui::BACKGROUND_SPRITE;
mod live;

use eu4_harmony_lua::{
    dispatch::{Click, PopupPressed, WindowSeen},
    layout::Window,
    Popup, Property, WindowChange,
};
use std::collections::BTreeMap;
use std::sync::Mutex;

pub(crate) use live::{frame, killed, session_ended, show_window, with_string};

/// The most pop-ups open at once, past which a new one is refused.
const POPUPS: usize = 32;

/// One extension's windows, as its main.lua lays them out.
struct Declared {
    extension: usize,
    /// Its mod's id.
    id: String,
    windows: Vec<Window>,
    /// Each window's texts in the player language, by key, for its
    /// definition.
    words: BTreeMap<String, String>,
}

/// What an extension wants of one of its windows.
#[derive(Clone, Default, Debug, PartialEq)]
struct Wanted {
    open: bool,
    /// What it set on each game widget, by name, and on its entry button
    /// as [`layout::ENTRY`].
    set: BTreeMap<String, BTreeMap<&'static str, Property>>,
    /// Each list's rows.
    rows: BTreeMap<String, Vec<Vec<(String, Property)>>>,
    /// Where the player dragged it, for the rest of the session.
    dragged: Option<(i32, i32)>,
}

/// A pop-up open.
struct Open {
    number: u32,
    extension: usize,
    popup: Popup,
}

#[derive(Default)]
struct Windows {
    declared: Vec<Declared>,
    wanted: BTreeMap<(usize, String), Wanted>,
    popups: Vec<Open>,
    last_popup: u32,
    /// What extensions hear at the next frame.
    seen: Vec<WindowSeen>,
    clicks: Vec<Click>,
    /// The pop-ups' buttons pressed, whose actions run at the next frame.
    pressed: Vec<PopupPressed>,
    /// Pop-ups shown since the last frame, for the `popup` signal.
    shown: Vec<(u32, String)>,
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
    fn window(&self, extension: usize, name: &str) -> Option<&Window> {
        self.declared.iter().find(|d| d.extension == extension)?.windows.iter().find(|w| w.name == name)
    }

    /// Opens or closes a window, as its extension or the player does:
    /// opening one of the big panels closes every other extension's.
    fn show(&mut self, extension: usize, name: &str, open: bool) {
        let panel = self.window(extension, name).is_some_and(|w| w.panel);
        if open && panel {
            self.close_panels(Some((extension, name)));
        }
        let wanted = self.wanted.entry((extension, name.to_string())).or_default();
        if wanted.open != open {
            wanted.open = open;
            self.seen.push(WindowSeen { extension, window: name.to_string(), open });
        }
    }

    /// Closes every extension window that is a big panel, but `but`.
    fn close_panels(&mut self, but: Option<(usize, &str)>) {
        let panels: Vec<(usize, String)> = self
            .declared
            .iter()
            .flat_map(|d| d.windows.iter().filter(|w| w.panel).map(move |w| (d.extension, w.name.clone())))
            .filter(|(e, n)| but != Some((*e, n.as_str())))
            .collect();
        for (extension, name) in panels {
            if let Some(wanted) = self.wanted.get_mut(&(extension, name.clone())) {
                if wanted.open {
                    wanted.open = false;
                    self.seen.push(WindowSeen { extension, window: name, open: false });
                }
            }
        }
    }

    fn change(&mut self, extension: usize, change: WindowChange) {
        match change {
            WindowChange::Open(name) => self.show(extension, &name, true),
            WindowChange::Close(name) => self.show(extension, &name, false),
            WindowChange::Set { window, widget, value } => {
                self.wanted.entry((extension, window)).or_default().set.entry(widget).or_default().insert(value.name(), value);
            }
            WindowChange::Rows { window, widget, rows } => {
                self.wanted.entry((extension, window)).or_default().rows.insert(widget, rows);
            }
        }
    }

    /// A button of the pop-up `number` pressed, from 1: its extension runs
    /// the button's action at the next frame, which closes the pop-up or
    /// not. False when it is not open.
    fn press(&mut self, number: u32, button: u32) -> bool {
        let Some(open) = self.popups.iter().find(|p| p.number == number) else { return false };
        let pressed = PopupPressed { extension: open.extension, popup: number, button };
        // A button pressed twice before its action ran runs it once.
        if !self.pressed.contains(&pressed) {
            self.pressed.push(pressed);
        }
        true
    }

    /// Closes everything of `extension`'s, unheard: its windows and its
    /// pop-ups.
    fn forget(&mut self, extension: usize) {
        self.wanted.retain(|(e, _), _| *e != extension);
        self.popups.retain(|p| p.extension != extension);
        self.seen.retain(|s| s.extension != extension);
        self.clicks.retain(|c| c.extension != extension);
        self.pressed.retain(|p| p.extension != extension);
    }

    /// The session ended: every window and pop-up closes, unheard; where
    /// the player dragged them is forgotten.
    fn end_session(&mut self) {
        self.wanted.clear();
        self.popups.clear();
        self.seen.clear();
        self.clicks.clear();
        self.pressed.clear();
        self.shown.clear();
    }
}

// ---------- what the Lua host and remote control call ----------

/// The windows the extension `extension` of the mod `id` declares, from its
/// main.lua, with `words` putting the mod's keys into the player language;
/// its earlier windows, at an extension reload, are closed.
pub fn declare(extension: usize, id: &str, windows: &[Window], words: impl Fn(&str) -> Option<String>) {
    let mut keys = Vec::new();
    for w in windows {
        gui::keys(w, &mut keys);
    }
    let words = keys.into_iter().filter_map(|k| words(&k).map(|t| (k, t))).collect();
    with(|all| {
        all.forget(extension);
        all.declared.retain(|d| d.extension != extension);
        if !windows.is_empty() {
            all.declared.push(Declared { extension, id: id.to_string(), windows: windows.to_vec(), words });
        }
        all.generation += 1;
    });
}

/// Closes everything of a stopped extension's, which no longer hears.
pub fn forget(extension: usize) {
    with(|all| all.forget(extension));
}

/// A change an extension makes to its windows, checked by the Lua host.
pub fn change(extension: usize, change: WindowChange) -> Result<(), String> {
    if !crate::sessions::going_on() && matches!(change, WindowChange::Open(_)) {
        return Err("a window opens only in a game".into());
    }
    with(|all| all.change(extension, change));
    Ok(())
}

/// Shows a pop-up of `extension`'s, and gives its number.
pub fn popup(extension: usize, popup: Popup) -> Result<u32, String> {
    if !crate::sessions::going_on() {
        return Err("a pop-up is shown only in a game".into());
    }
    with(|all| {
        if all.popups.len() >= POPUPS {
            return Err(format!("{POPUPS} pop-ups are open already"));
        }
        all.last_popup += 1;
        let number = all.last_popup;
        let id = all.declared.iter().find(|d| d.extension == extension).map(|d| d.id.clone()).or_else(|| crate::lua::mod_id(extension)).unwrap_or_default();
        all.shown.push((number, id));
        all.popups.push(Open { number, extension, popup });
        Ok(number)
    })
}

/// Closes `extension`'s pop-up `number`; nothing when it is not open.
pub fn close_popup(extension: usize, number: u32) -> Result<(), String> {
    with(|all| all.popups.retain(|p| !(p.number == number && p.extension == extension)));
    Ok(())
}

/// Presses a button of a pop-up, from 1, left to right, as the player
/// does: its extension runs the button's action at the next frame. Nothing
/// when it is not open.
pub fn press(number: u32, button: u32) {
    with(|all| all.press(number, button));
}

/// The pop-ups open, oldest first, as `popup.list` gives them: each
/// button's words as they stand, left to right, the game's own OK in the
/// game's language.
pub fn popups() -> Vec<eu4_harmony_remote::popups::Popup> {
    let game_words = |key: &str| {
        crate::memory()
            .and_then(|m| crate::game_string::unformatted(m, key).ok())
            .filter(|w| !w.is_empty())
            .map(|w| crate::unicode::game_text::chars(&w).map(|(c, _)| c).collect())
            .unwrap_or_else(|| key.to_string())
    };
    let open: Vec<(u32, usize, Popup)> = with(|all| all.popups.iter().map(|p| (p.number, p.extension, p.popup.clone())).collect());
    open.into_iter()
        .map(|(number, extension, popup)| eu4_harmony_remote::popups::Popup {
            popup: number,
            mod_id: crate::lua::mod_id(extension).unwrap_or_default(),
            header: popup.header.clone(),
            title: popup.title.clone(),
            text: popup.text.clone(),
            buttons: if popup.buttons.is_empty() { vec![game_words(gui::OK)] } else { popup.buttons.clone() },
        })
        .collect()
}

/// Presses the button `button` (from 1, left to right as `popup.list`
/// gives them) of the pop-up `number`, as the player does; or why not.
pub fn press_popup(number: u32, button: u32) -> Result<(), String> {
    let count = with(|all| all.popups.iter().find(|p| p.number == number).map(|p| gui::shown(&p.popup).len()));
    let Some(count) = count else { return Err(format!("no pop-up {number} is open")) };
    if button == 0 || button as usize > count {
        return Err(format!("pop-up {number} has buttons 1 to {count}"));
    }
    press(number, button);
    Ok(())
}

/// The pop-ups' buttons pressed since the last frame.
pub fn take_popups_pressed() -> Vec<PopupPressed> {
    with(|all| std::mem::take(&mut all.pressed))
}

/// The extension windows opened and closed since the last frame.
pub fn take_seen() -> Vec<WindowSeen> {
    with(|all| std::mem::take(&mut all.seen))
}

/// The clicks on extension windows' buttons since the last frame.
pub fn take_clicks() -> Vec<Click> {
    with(|all| std::mem::take(&mut all.clicks))
}

/// The pop-ups shown since the last frame: each one's number and its mod.
pub fn take_shown() -> Vec<(u32, String)> {
    with(|all| std::mem::take(&mut all.shown))
}

pub fn points() -> Vec<crate::points::Point> {
    live::points()
}

/// The framework files the definitions are read from, empty until
/// extensions declare windows (ADR framework-files-in-the-game-file-system).
pub fn files() -> Vec<crate::files::File> {
    let mut files: Vec<crate::files::File> =
        [live::WINDOWS_FILE, live::POPUPS_FILE].into_iter().map(|path| crate::files::File { path: path.into(), data: gui::file(&[]).into_bytes() }).collect();
    files.push(crate::files::File { path: SPRITES_FILE.into(), data: gui::SPRITES.as_bytes().to_vec() });
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
pub fn splice_entries(plan: &mut crate::splice::Plan) -> Vec<String> {
    with(|all| {
        let mut text = String::new();
        for d in &all.declared {
            for w in &d.windows {
                if let Some(entry) = &w.entry {
                    text.push_str(&live::entry_fragment(&d.id, &w.name, entry));
                }
            }
        }
        if !text.is_empty() {
            plan.add(STRIP_FILE, crate::splice::Addition::Window { window: STRIP_WINDOW.into(), text });
        }
        Vec::new()
    })
}

/// A property as the game is to show it, for [`live`].
fn words_of(declared: &Declared, key: &Option<String>) -> String {
    key.as_ref().and_then(|k| declared.words.get(k)).cloned().unwrap_or_default()
}

/// The element names of a window's definition: its own, and its game
/// widgets' by their names, with the mod's prefix.
fn type_name(id: &str, window: &str) -> String {
    format!("{}{window}", crate::splice::prefix(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_lua::layout;
    use serde_json::json;

    fn declared() -> Windows {
        let layouts = json!({
            "reforms": { "size": [300, 200] },
            "electors": { "size": [300, 200] },
            "tracker": { "size": [200, 100], "panel": false },
        });
        let windows = layout::read(&layouts, &|_| true).unwrap();
        let mut all = Windows::default();
        all.declared.push(Declared { extension: 0, id: "a".into(), windows: windows.clone(), words: BTreeMap::new() });
        all.declared.push(Declared { extension: 1, id: "b".into(), windows, words: BTreeMap::new() });
        all
    }

    fn popup(buttons: Vec<String>) -> Popup {
        Popup { header: None, title: "T".into(), text: "".into(), buttons, shields: [None, None] }
    }

    fn seen(e: usize, w: &str, open: bool) -> WindowSeen {
        WindowSeen { extension: e, window: w.into(), open }
    }

    #[test]
    fn a_big_panel_closes_the_others_and_each_change_is_heard_once() {
        let mut all = declared();
        all.change(0, WindowChange::Open("tracker".into()));
        all.change(0, WindowChange::Open("reforms".into()));
        all.change(1, WindowChange::Open("electors".into()));
        all.change(1, WindowChange::Open("electors".into()));
        assert_eq!(all.seen, [seen(0, "tracker", true), seen(0, "reforms", true), seen(0, "reforms", false), seen(1, "electors", true)]);
        assert!(all.wanted[&(0, "tracker".into())].open, "not a big panel: it stays");
        all.seen.clear();
        // The game's own big panel opening closes them.
        all.close_panels(None);
        assert_eq!(all.seen, [seen(1, "electors", false)]);
    }

    #[test]
    fn what_is_set_is_kept_while_closed_and_forgotten_with_its_extension() {
        let mut all = declared();
        all.change(0, WindowChange::Set { window: "reforms".into(), widget: "status".into(), value: Property::Text("Ready".into()) });
        all.change(0, WindowChange::Set { window: "reforms".into(), widget: "status".into(), value: Property::Text("Done".into()) });
        assert_eq!(all.wanted[&(0, "reforms".into())].set["status"]["text"], Property::Text("Done".into()));
        all.popups.push(Open { number: 1, extension: 0, popup: popup(vec!["OK".into()]) });
        all.press(1, 1);
        all.forget(0);
        assert!(all.wanted.is_empty() && all.popups.is_empty() && all.pressed.is_empty());
    }

    #[test]
    fn a_pop_up_s_button_is_heard_by_its_extension_and_closes_nothing_by_itself() {
        let mut all = declared();
        all.popups.push(Open { number: 4, extension: 1, popup: popup(vec!["A".into(), "B".into()]) });
        assert!(all.press(4, 2));
        assert!(all.press(4, 2), "pressed twice before its action ran");
        assert_eq!(all.pressed, [PopupPressed { extension: 1, popup: 4, button: 2 }]);
        assert_eq!(all.popups.len(), 1, "its action closes it, or not");
        assert!(!all.press(5, 1), "none open");
    }
}
