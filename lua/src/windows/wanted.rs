//! What extensions want of their windows and pop-ups, and the rules the game
//! holds them to, the same for the running game and the fake game: which
//! windows each extension declared, which are open and what each game widget
//! shows, the pop-ups open, and what extensions hear of them at the next
//! frame. The host makes its windows show it: the core the game's own, at
//! every safe frame of a game; the fake game reads it back for the tests.
//!
//! The rules: a window opens and a pop-up shows only while a session goes
//! on, which the host tells; opening one of the big panels closes every other
//! extension's; at most [`POPUPS`] pop-ups are open at once; a pop-up's
//! button pressed twice before its action ran runs it once; an extension
//! closes only its own pop-ups; and as a session ends every window and
//! pop-up closes, unheard.
use super::{Popup, Property, WindowChange};
use crate::dispatch::{PopupPressed, WindowSeen};
use crate::layout::Window;
use std::collections::BTreeMap;

/// The most pop-ups open at once, past which a new one is refused.
pub const POPUPS: usize = 32;

/// What an extension wants of one of its windows.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Wanted {
    pub open: bool,
    /// What it set on each game widget, by name, and on its entry button
    /// as [`crate::layout::ENTRY`].
    pub set: BTreeMap<String, BTreeMap<&'static str, Property>>,
    /// Each list's rows.
    pub rows: BTreeMap<String, Vec<Vec<(String, Property)>>>,
    /// Where the player dragged it, for the rest of the session.
    pub dragged: Option<(i32, i32)>,
}

/// One extension's windows, as its main.lua lays them out.
#[derive(Clone, Debug)]
pub struct Declared {
    pub extension: usize,
    /// Its mod's id.
    pub id: String,
    pub windows: Vec<Window>,
}

/// A pop-up open.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenPopup {
    pub number: u32,
    pub extension: usize,
    pub popup: Popup,
}

/// Every extension's windows and pop-ups.
#[derive(Default)]
pub struct Windows {
    declared: Vec<Declared>,
    wanted: BTreeMap<(usize, String), Wanted>,
    popups: Vec<OpenPopup>,
    last_popup: u32,
    /// What extensions hear at the next frame.
    seen: Vec<WindowSeen>,
    /// The pop-ups' buttons pressed, whose actions run at the next frame.
    pressed: Vec<PopupPressed>,
    /// The pop-ups shown since the last frame, each one's number and
    /// extension.
    shown: Vec<(u32, usize)>,
}

impl Windows {
    /// The windows the extension `extension` of the mod `id` declares, from
    /// its main.lua; its earlier windows and pop-ups, at an extension
    /// reload, are closed.
    pub fn declare(&mut self, extension: usize, id: &str, windows: &[Window]) {
        self.forget(extension);
        self.declared.retain(|d| d.extension != extension);
        if !windows.is_empty() {
            self.declared.push(Declared { extension, id: id.to_string(), windows: windows.to_vec() });
        }
    }

    /// The windows each extension declared, in the order they were.
    pub fn declared(&self) -> &[Declared] {
        &self.declared
    }

    fn window(&self, extension: usize, name: &str) -> Option<&Window> {
        self.declared.iter().find(|d| d.extension == extension)?.windows.iter().find(|w| w.name == name)
    }

    /// What `extension` wants of its window `name`, none before it set or
    /// opened anything of it.
    pub fn wanted(&self, extension: usize, name: &str) -> Option<&Wanted> {
        self.wanted.get(&(extension, name.to_string()))
    }

    /// Whether `extension`'s window `name` is to be open.
    pub fn is_open(&self, extension: usize, name: &str) -> bool {
        self.wanted(extension, name).is_some_and(|w| w.open)
    }

    /// The player dragged `extension`'s window `name` to `at`, where it
    /// stays for the rest of the session.
    pub fn dragged(&mut self, extension: usize, name: &str, at: (i32, i32)) {
        if let Some(w) = self.wanted.get_mut(&(extension, name.to_string())) {
            w.dragged = Some(at);
        }
    }

    /// Opens or closes a window, as the player does or as its extension
    /// does: opening one of the big panels closes every other extension's.
    /// What opened or closed, which the caller has heard; nothing is kept to
    /// hear.
    pub fn show(&mut self, extension: usize, name: &str, open: bool) -> Vec<WindowSeen> {
        let panel = self.window(extension, name).is_some_and(|w| w.panel);
        let mut changed = if open && panel { self.close_panels(Some((extension, name))) } else { Vec::new() };
        let wanted = self.wanted.entry((extension, name.to_string())).or_default();
        if wanted.open != open {
            wanted.open = open;
            changed.push(WindowSeen { extension, window: name.to_string(), open });
        }
        changed
    }

    /// Closes every extension window that is a big panel, but `but`; what
    /// closed, as [`Windows::show`] gives it.
    pub fn close_panels(&mut self, but: Option<(usize, &str)>) -> Vec<WindowSeen> {
        let panels: Vec<(usize, String)> = self
            .declared
            .iter()
            .flat_map(|d| d.windows.iter().filter(|w| w.panel).map(move |w| (d.extension, w.name.clone())))
            .filter(|(e, n)| but != Some((*e, n.as_str())))
            .collect();
        let mut closed = Vec::new();
        for (extension, name) in panels {
            if let Some(wanted) = self.wanted.get_mut(&(extension, name.clone())) {
                if wanted.open {
                    wanted.open = false;
                    closed.push(WindowSeen { extension, window: name, open: false });
                }
            }
        }
        closed
    }

    /// Keeps `seen` for extensions to hear at the next frame.
    pub fn hear_at_frame(&mut self, seen: Vec<WindowSeen>) {
        self.seen.extend(seen);
    }

    /// A change an extension makes to its windows, checked against their
    /// layouts already; what it opens or closes is heard at the next frame.
    /// A window opens only while a session goes on (`in_game`).
    pub fn change(&mut self, extension: usize, change: WindowChange, in_game: bool) -> Result<(), String> {
        match change {
            WindowChange::Open(_) if !in_game => return Err("a window opens only in a game".into()),
            WindowChange::Open(name) => {
                let seen = self.show(extension, &name, true);
                self.seen.extend(seen);
            }
            WindowChange::Close(name) => {
                let seen = self.show(extension, &name, false);
                self.seen.extend(seen);
            }
            WindowChange::Set { window, widget, value } => {
                self.wanted.entry((extension, window)).or_default().set.entry(widget).or_default().insert(value.name(), value);
            }
            WindowChange::Rows { window, widget, rows } => {
                self.wanted.entry((extension, window)).or_default().rows.insert(widget, rows);
            }
        }
        Ok(())
    }

    /// Shows a pop-up of `extension`'s, and gives its number: only while a
    /// session goes on (`in_game`), and while fewer than [`POPUPS`] are
    /// open.
    pub fn popup(&mut self, extension: usize, popup: Popup, in_game: bool) -> Result<u32, String> {
        if !in_game {
            return Err("a pop-up is shown only in a game".into());
        }
        if self.popups.len() >= POPUPS {
            return Err(format!("{POPUPS} pop-ups are open already"));
        }
        self.last_popup += 1;
        let number = self.last_popup;
        self.shown.push((number, extension));
        self.popups.push(OpenPopup { number, extension, popup });
        Ok(number)
    }

    /// The pop-ups open, oldest first.
    pub fn popups(&self) -> &[OpenPopup] {
        &self.popups
    }

    /// The pop-up `number`, if it is open.
    pub fn open_popup(&self, number: u32) -> Option<&OpenPopup> {
        self.popups.iter().find(|p| p.number == number)
    }

    /// Closes `extension`'s pop-up `number`; nothing when it is not open,
    /// or not its own.
    pub fn close_popup(&mut self, extension: usize, number: u32) {
        self.popups.retain(|p| !(p.number == number && p.extension == extension));
    }

    /// The pop-up `number` is gone, whoever's it was: the host could not
    /// show it.
    pub fn lost_popup(&mut self, number: u32) {
        self.popups.retain(|p| p.number != number);
    }

    /// Presses the button `button` of the pop-up `number`, from 1, left to
    /// right, as the player does: its extension runs the button's action at
    /// the next frame, which closes the pop-up or not; a button pressed
    /// twice before its action ran runs it once. Or why not.
    pub fn press(&mut self, number: u32, button: u32) -> Result<(), String> {
        let Some(open) = self.open_popup(number) else { return Err(format!("no pop-up {number} is open")) };
        let count = open.popup.buttons_shown();
        if button == 0 || button as usize > count {
            return Err(format!("pop-up {number} has buttons 1 to {count}"));
        }
        let pressed = PopupPressed { extension: open.extension, popup: number, button };
        if !self.pressed.contains(&pressed) {
            self.pressed.push(pressed);
        }
        Ok(())
    }

    /// Closes everything of `extension`'s, unheard: its windows and its
    /// pop-ups.
    pub fn forget(&mut self, extension: usize) {
        self.wanted.retain(|(e, _), _| *e != extension);
        self.popups.retain(|p| p.extension != extension);
        self.seen.retain(|s| s.extension != extension);
        self.pressed.retain(|p| p.extension != extension);
    }

    /// The session ended: every window and pop-up closes, unheard; where
    /// the player dragged them is forgotten.
    pub fn end_session(&mut self) {
        self.wanted.clear();
        self.popups.clear();
        self.seen.clear();
        self.pressed.clear();
        self.shown.clear();
    }

    /// The extension windows opened and closed since the last frame.
    pub fn take_seen(&mut self) -> Vec<WindowSeen> {
        std::mem::take(&mut self.seen)
    }

    /// The pop-ups' buttons pressed since the last frame.
    pub fn take_pressed(&mut self) -> Vec<PopupPressed> {
        std::mem::take(&mut self.pressed)
    }

    /// The pop-ups shown since the last frame: each one's number and
    /// extension.
    pub fn take_shown(&mut self) -> Vec<(u32, usize)> {
        std::mem::take(&mut self.shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout;
    use serde_json::json;

    fn declared() -> Windows {
        let layouts = json!({
            "reforms": { "size": [300, 200] },
            "electors": { "size": [300, 200] },
            "tracker": { "size": [200, 100], "panel": false },
        });
        let windows = layout::read(&layouts, &|_| true).unwrap();
        let mut all = Windows::default();
        all.declare(0, "a", &windows);
        all.declare(1, "b", &windows);
        all
    }

    fn popup(buttons: Vec<String>) -> Popup {
        Popup { header: None, title: "T".into(), text: "".into(), buttons, shields: [None, None] }
    }

    fn seen(e: usize, w: &str, open: bool) -> WindowSeen {
        WindowSeen { extension: e, window: w.into(), open }
    }

    fn open(all: &mut Windows, e: usize, w: &str) {
        all.change(e, WindowChange::Open(w.into()), true).unwrap();
    }

    #[test]
    fn a_big_panel_closes_the_others_and_each_change_is_heard_once() {
        let mut all = declared();
        open(&mut all, 0, "tracker");
        open(&mut all, 0, "reforms");
        open(&mut all, 1, "electors");
        open(&mut all, 1, "electors");
        assert_eq!(all.take_seen(), [seen(0, "tracker", true), seen(0, "reforms", true), seen(0, "reforms", false), seen(1, "electors", true)]);
        assert!(all.is_open(0, "tracker"), "not a big panel: it stays");
        // The game's own big panel opening closes them.
        assert_eq!(all.close_panels(None), [seen(1, "electors", false)]);
        assert!(all.take_seen().is_empty(), "what the caller is given is not kept to hear");
    }

    #[test]
    fn what_is_set_is_kept_while_closed_and_forgotten_with_its_extension() {
        let mut all = declared();
        let set = |text: &str| WindowChange::Set { window: "reforms".into(), widget: "status".into(), value: Property::Text(text.into()) };
        all.change(0, set("Ready"), false).unwrap();
        all.change(0, set("Done"), true).unwrap();
        assert_eq!(all.wanted(0, "reforms").unwrap().set["status"]["text"], Property::Text("Done".into()));
        all.popup(0, popup(vec!["OK".into()]), true).unwrap();
        all.press(1, 1).unwrap();
        all.forget(0);
        assert!(all.wanted(0, "reforms").is_none() && all.popups().is_empty() && all.take_pressed().is_empty());
        assert_eq!(all.take_shown(), [(1, 0)], "that it was shown is still told");
    }

    #[test]
    fn a_pop_up_s_button_is_heard_by_its_extension_and_closes_nothing_by_itself() {
        let mut all = declared();
        let number = all.popup(1, popup(vec!["A".into(), "B".into()]), true).unwrap();
        assert_eq!(all.press(number, 2), Ok(()));
        assert_eq!(all.press(number, 2), Ok(()), "pressed twice before its action ran");
        assert_eq!(all.take_pressed(), [PopupPressed { extension: 1, popup: number, button: 2 }]);
        assert_eq!(all.popups().len(), 1, "its action closes it, or not");
        assert_eq!(all.press(number + 1, 1), Err(format!("no pop-up {} is open", number + 1)));
        assert_eq!(all.press(number, 3), Err(format!("pop-up {number} has buttons 1 to 2")));
        assert_eq!(all.press(number, 0), Err(format!("pop-up {number} has buttons 1 to 2")));
    }

    #[test]
    fn a_window_opens_and_a_pop_up_shows_only_in_a_game() {
        let mut all = declared();
        assert_eq!(all.change(0, WindowChange::Open("reforms".into()), false), Err("a window opens only in a game".into()));
        assert!(all.change(0, WindowChange::Close("reforms".into()), false).is_ok(), "closing is always allowed");
        assert_eq!(all.popup(0, popup(Vec::new()), false), Err("a pop-up is shown only in a game".into()));
        assert!(!all.is_open(0, "reforms") && all.popups().is_empty());
    }

    #[test]
    fn at_most_so_many_pop_ups_are_open_and_the_game_s_own_ok_is_one_button() {
        let mut all = declared();
        for _ in 0..POPUPS {
            all.popup(0, popup(Vec::new()), true).unwrap();
        }
        assert_eq!(all.popup(1, popup(Vec::new()), true), Err(format!("{POPUPS} pop-ups are open already")));
        assert_eq!(all.press(1, 1), Ok(()), "the game's own OK");
        assert!(all.press(1, 2).is_err());
        assert_eq!(all.take_shown().len(), POPUPS);
    }

    #[test]
    fn an_extension_closes_only_its_own_pop_ups_and_a_session_s_end_closes_all() {
        let mut all = declared();
        let a = all.popup(0, popup(Vec::new()), true).unwrap();
        let b = all.popup(1, popup(Vec::new()), true).unwrap();
        all.close_popup(1, a);
        assert_eq!(all.popups().len(), 2, "not its own");
        all.close_popup(1, b);
        assert_eq!(all.popups().iter().map(|p| p.number).collect::<Vec<_>>(), [a]);
        open(&mut all, 0, "tracker");
        all.dragged(0, "tracker", (5, 6));
        all.end_session();
        assert!(all.popups().is_empty() && !all.is_open(0, "tracker") && all.wanted(0, "tracker").is_none() && all.take_seen().is_empty());
        assert_eq!(all.declared().len(), 2, "what was declared stays");
    }

    #[test]
    fn a_reload_closes_the_extension_s_windows_and_declares_them_anew() {
        let mut all = declared();
        open(&mut all, 0, "tracker");
        all.declare(0, "a", &[]);
        assert!(!all.is_open(0, "tracker"));
        assert_eq!(all.declared().iter().map(|d| d.extension).collect::<Vec<_>>(), [1]);
    }
}
