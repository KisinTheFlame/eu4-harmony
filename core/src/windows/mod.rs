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
mod live;

use eu4_harmony_lua::{
    dispatch::{Click, WindowSeen},
    layout::Window,
    Answered, Pending, Popup, Property, WindowChange,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub(crate) use live::{frame, killed, session_ended};

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

/// Where a pop-up's answer is put, and the pending answer that takes it.
type Slot = Arc<Mutex<Option<Answered>>>;

struct Taken(Slot);

impl Pending<Answered> for Taken {
    fn poll(&mut self) -> Option<Answered> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// A pop-up open.
struct Open {
    number: u32,
    extension: usize,
    popup: Popup,
    answer: Slot,
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
    /// Pop-ups shown since the last frame, for the `popup` event.
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

    /// Answers the pop-up `number` and closes it.
    fn answer(&mut self, number: u32, answer: Value) -> bool {
        let Some(at) = self.popups.iter().position(|p| p.number == number) else { return false };
        let open = self.popups.remove(at);
        *open.answer.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ok(answer));
        true
    }

    /// Closes everything of `extension`'s: its windows, unheard, and its
    /// pop-ups, answered `closed`.
    fn forget(&mut self, extension: usize) {
        self.wanted.retain(|(e, _), _| *e != extension);
        let closed: Vec<u32> = self.popups.iter().filter(|p| p.extension == extension).map(|p| p.number).collect();
        for number in closed {
            self.answer(number, Value::String("closed".into()));
        }
        self.seen.retain(|s| s.extension != extension);
        self.clicks.retain(|c| c.extension != extension);
    }

    /// The session ended: every window closes, unheard, and every pop-up
    /// answers `closed`; where the player dragged them is forgotten.
    fn end_session(&mut self) {
        self.wanted.clear();
        let all: Vec<u32> = self.popups.iter().map(|p| p.number).collect();
        for number in all {
            self.answer(number, Value::String("closed".into()));
        }
        self.seen.clear();
        self.clicks.clear();
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

/// Shows a pop-up of `extension`'s; its answer comes later.
pub fn popup(extension: usize, popup: Popup) -> Result<Box<dyn Pending<Answered>>, String> {
    if !crate::sessions::going_on() {
        return Err("a pop-up is shown only in a game".into());
    }
    with(|all| {
        if all.popups.len() >= POPUPS {
            return Err(format!("{POPUPS} pop-ups are open already"));
        }
        all.last_popup += 1;
        let number = all.last_popup;
        let answer = Slot::default();
        let id = all.declared.iter().find(|d| d.extension == extension).map(|d| d.id.clone()).or_else(|| crate::lua::mod_id(extension)).unwrap_or_default();
        all.shown.push((number, id));
        all.popups.push(Open { number, extension, popup, answer: answer.clone() });
        Ok(Box::new(Taken(answer)) as Box<dyn Pending<Answered>>)
    })
}

/// Closes `extension`'s open pop-ups of `id`, each answered `closed`.
pub fn close_popup(extension: usize, id: &str) -> Result<(), String> {
    with(|all| {
        let closed: Vec<u32> = all.popups.iter().filter(|p| p.extension == extension && p.popup.id.as_deref() == Some(id)).map(|p| p.number).collect();
        for number in closed {
            all.answer(number, Value::String("closed".into()));
        }
    });
    Ok(())
}

/// A button of a pop-up's pressed: its "Go to", or one of its others by
/// its number, from 1, the game's own OK the first when the mod gave none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pressed {
    Goto,
    Button(u32),
}

/// Presses a pop-up's button, as the player does: "Go to" turns the camera
/// to its province first. Its extension has the answer, `"goto"` or the
/// button's number, at the next frame. Nothing when it is not open.
pub fn press(number: u32, pressed: Pressed) {
    let province = with(|all| all.popups.iter().find(|p| p.number == number).and_then(|p| p.popup.goto));
    if let (Pressed::Goto, Some(province)) = (pressed, province) {
        let look = crate::game::WorldSession::open(crate::game::Caller::Remote).and_then(|session| {
            crate::game::camera::look_at(
                session,
                eu4_harmony_remote::camera::args::LookAt { province: Some(eu4_harmony_remote::ProvinceId(province)), x: None, y: None },
            )
        });
        if let Err(e) = look {
            log!("windows: pop-up {number}'s Go to province {province}: {}", e.message);
        }
    }
    let answer = match pressed {
        Pressed::Goto => Value::String("goto".into()),
        Pressed::Button(k) => Value::from(k),
    };
    with(|all| all.answer(number, answer));
}

/// The pop-ups open, oldest first, as `popup.list` gives them: each
/// button's words as they stand, left to right, the game's own Go to and
/// OK in the game's language.
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
            buttons: gui::shown(&popup)
                .into_iter()
                .map(|(name, words)| if name == gui::GOTO || (popup.buttons.is_empty() && words == "OK") { game_words(&words) } else { words })
                .collect(),
            province: popup.goto,
        })
        .collect()
}

/// Presses the button `button` (from 1, left to right as `popup.list`
/// gives them) of the pop-up `number`, as the player does; or why not.
pub fn press_popup(number: u32, button: u32) -> Result<(), String> {
    let shown = with(|all| all.popups.iter().find(|p| p.number == number).map(|p| gui::shown(&p.popup)));
    let Some(shown) = shown else { return Err(format!("no pop-up {number} is open")) };
    let Some((name, _)) = button.checked_sub(1).and_then(|i| shown.get(i as usize)) else {
        return Err(format!("pop-up {number} has buttons 1 to {}", shown.len()));
    };
    let pressed = match name.strip_prefix("eu4h_button_") {
        Some(k) => Pressed::Button(k.parse().unwrap_or(1)),
        None => Pressed::Goto,
    };
    press(number, pressed);
    Ok(())
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
        Popup { id: None, header: None, title: "T".into(), text: "".into(), buttons, goto: None, shields: [None, None] }
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
        let answer = Slot::default();
        all.popups.push(Open { number: 1, extension: 0, popup: popup(vec!["OK".into()]), answer: answer.clone() });
        all.forget(0);
        assert!(all.wanted.is_empty() && all.popups.is_empty());
        assert_eq!(*answer.lock().unwrap(), Some(Ok(json!("closed"))));
    }

    #[test]
    fn a_pop_up_is_answered_once() {
        let mut all = declared();
        let answer = Slot::default();
        all.popups.push(Open { number: 4, extension: 1, popup: popup(vec!["A".into(), "B".into()]), answer: answer.clone() });
        assert!(all.answer(4, json!(2)));
        assert!(!all.answer(4, json!(1)), "gone once answered");
        assert_eq!(Taken(answer).poll(), Some(Ok(json!(2))));
    }
}
