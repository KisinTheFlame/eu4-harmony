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
use crate::layout::{Kind, Placed, Window};
use std::collections::BTreeMap;

/// The most pop-ups open at once, past which a new one is refused.
pub const POPUPS: usize = 32;

/// What an extension wants of one of its windows.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Wanted {
    pub open: bool,
    /// Order of the most recent real open request, shared with pop-ups.
    /// Native reconstruction and repeated open requests retain this value.
    pub appearance: u64,
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

/// Initial resolved properties of a placed game widget, given `words` for the
/// mod's localisation keys in the current player language.
pub fn placed_defaults(placed: &Placed, words: impl Fn(&str) -> Option<String>) -> BTreeMap<&'static str, Property> {
    let tooltip = placed.tooltip.as_ref().map(|t| t.resolve(&words));
    let mut out = BTreeMap::new();
    let mut put = |p: Property| {
        out.insert(p.name(), p);
    };
    put(Property::Visible(placed.visible));
    if let Some(tooltip) = tooltip {
        put(Property::Tooltip(tooltip.into()));
    }
    match &placed.kind {
        Kind::Text { text, .. } => put(Property::Text(text.as_ref().map(|t| t.resolve(&words)).unwrap_or_default().into())),
        Kind::Icon { frame, .. } | Kind::Button { frame, sprite: Some(_), .. } => put(Property::Frame(*frame)),
        Kind::Progress { value, .. } => put(Property::Value(*value)),
        Kind::Flag { country: Some(c), .. } => put(Property::Country(c.clone())),
        _ => {}
    }
    out
}

/// Resolved presentation state of a window's panel-strip entry button.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedEntry {
    pub sprite: String,
    pub visible: bool,
    pub enabled: bool,
    pub frame: u32,
    pub value: Option<f64>,
    /// Formatted independent number below the icon; empty when `value` is `None`.
    pub text: String,
    pub tooltip: String,
}

/// Resolved properties of one game widget, keyed by property name.
pub type ResolvedProperties = BTreeMap<&'static str, Property>;
/// Resolved properties of each named cell in one list row.
pub type ResolvedRow = BTreeMap<String, ResolvedProperties>;

/// Resolved presentation state of an extension window, merging layout defaults,
/// dynamic properties, list rows, and its entry button in the current language.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct ResolvedWindow {
    pub open: bool,
    pub widgets: BTreeMap<String, ResolvedProperties>,
    pub rows: BTreeMap<String, Vec<ResolvedRow>>,
    pub entry: Option<ResolvedEntry>,
}

/// A pop-up open.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenPopup {
    pub number: u32,
    /// Order of this opening among windows and pop-ups.
    pub appearance: u64,
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
    last_appearance: u64,
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
        if windows.is_empty() {
            self.declared.retain(|d| d.extension != extension);
            return;
        }
        let declared = Declared { extension, id: id.to_string(), windows: windows.to_vec() };
        match self.declared.iter().position(|d| d.extension == extension) {
            Some(at) => self.declared[at] = declared,
            None => self.declared.push(declared),
        }
        self.initialize_texts(extension);
    }

    /// Initial values are state, not a fallback applied again by a native
    /// binding. An explicitly cleared tooltip therefore stays empty.
    fn initialize_texts(&mut self, extension: usize) {
        let Some(declared) = self.declared.iter().find(|d| d.extension == extension) else { return };
        for window in &declared.windows {
            if window.entry.is_none() && window.widgets.iter().all(|p| p.name.is_none()) {
                continue;
            }
            let wanted = self.wanted.entry((extension, window.name.clone())).or_default();
            for placed in &window.widgets {
                if let Some(name) = &placed.name {
                    let properties = wanted.set.entry(name.clone()).or_default();
                    properties.insert("tooltip", Property::Tooltip(placed.tooltip.clone().unwrap_or_default()));
                    if let Kind::Text { text, .. } = &placed.kind {
                        properties.insert("text", Property::Text(text.clone().unwrap_or_default()));
                    }
                }
            }
            if let Some(entry) = &window.entry {
                let properties = wanted.set.entry(crate::layout::ENTRY.into()).or_default();
                for property in [
                    Property::Tooltip(entry.tooltip.clone().unwrap_or_default()),
                    Property::Visible(entry.visible),
                    Property::Enabled(entry.enabled),
                    Property::Number(entry.value),
                ] {
                    properties.insert(property.name(), property);
                }
            }
        }
    }

    /// Resolved presentation state of `extension`'s panel-strip entry button on
    /// `window`, if that window declares one.
    pub fn resolved_entry(&self, extension: usize, window: &str, words: impl Fn(&str) -> Option<String>) -> Option<ResolvedEntry> {
        let layout = self.window(extension, window)?;
        let entry = layout.entry.as_ref()?;
        let set = self.wanted(extension, window).and_then(|w| w.set.get(crate::layout::ENTRY));
        let visible = match set.and_then(|s| s.get("visible")) {
            Some(Property::Visible(on)) => *on,
            _ => entry.visible,
        };
        let enabled = match set.and_then(|s| s.get("enabled")) {
            Some(Property::Enabled(on)) => *on,
            _ => entry.enabled,
        };
        let frame = match set.and_then(|s| s.get("frame")) {
            Some(Property::Frame(f)) => *f,
            _ => entry.frame,
        };
        let value = match set.and_then(|s| s.get("value")) {
            Some(Property::Number(v)) => *v,
            _ => entry.value,
        };
        let tooltip = match set.and_then(|s| s.get("tooltip")) {
            Some(Property::Tooltip(t)) => t.resolve(&words),
            _ => entry.tooltip.as_ref().map(|t| t.resolve(&words)).unwrap_or_default(),
        };
        Some(ResolvedEntry { sprite: entry.sprite.clone(), visible, enabled, frame, value, text: entry.format.show(value), tooltip })
    }

    /// Resolved presentation state of `extension`'s declared window `name`.
    pub fn resolved_window(&self, extension: usize, name: &str, words: impl Fn(&str) -> Option<String>) -> Option<ResolvedWindow> {
        let layout = self.window(extension, name)?;
        let wanted = self.wanted(extension, name);
        let resolve_prop = |prop: &Property| match prop {
            Property::Text(t) => Property::Text(t.resolve(&words).into()),
            Property::Tooltip(t) => Property::Tooltip(t.resolve(&words).into()),
            other => other.clone(),
        };
        let mut widgets = BTreeMap::new();
        for placed in &layout.widgets {
            let Some(widget_name) = &placed.name else { continue };
            let mut props = placed_defaults(placed, &words);
            if matches!(placed.kind, Kind::Button { .. }) {
                props.insert("enabled", Property::Enabled(true));
            }
            if let Some(overrides) = wanted.and_then(|w| w.set.get(widget_name)) {
                for (k, v) in overrides {
                    props.insert(*k, resolve_prop(v));
                }
            }
            widgets.insert(widget_name.clone(), props);
        }
        let mut rows = BTreeMap::new();
        for placed in &layout.widgets {
            let (Some(list_name), Kind::List { row: row_layout, .. }) = (&placed.name, &placed.kind) else { continue };
            let Some(raw_rows) = wanted.and_then(|w| w.rows.get(list_name)) else { continue };
            let mut resolved_rows = Vec::with_capacity(raw_rows.len());
            for cells in raw_rows {
                let mut row_map = BTreeMap::new();
                for cell_placed in &row_layout.widgets {
                    let Some(cell_name) = &cell_placed.name else { continue };
                    let mut props = placed_defaults(cell_placed, &words);
                    if matches!(cell_placed.kind, Kind::Button { .. }) {
                        props.insert("enabled", Property::Enabled(true));
                    }
                    for (_, prop) in cells.iter().filter(|(n, _)| n == cell_name) {
                        props.insert(prop.name(), resolve_prop(prop));
                    }
                    row_map.insert(cell_name.clone(), props);
                }
                resolved_rows.push(row_map);
            }
            rows.insert(list_name.clone(), resolved_rows);
        }
        let entry = self.resolved_entry(extension, name, &words);
        Some(ResolvedWindow { open: wanted.is_some_and(|w| w.open), widgets, rows, entry })
    }

    /// Checks whether the player can press `widget` on `extension`'s `window`
    /// (or inside row `row`, 1-based), matching native click rules: the window
    /// must be open, any containing list must be visible, and the button itself
    /// must be visible and enabled.
    pub fn check_press(&self, extension: usize, window: &str, widget: &str, row: Option<u32>) -> Result<(), String> {
        let Some(layout) = self.window(extension, window) else {
            return Err(format!("the mod has no window {window:?}"));
        };
        if !self.is_open(extension, window) {
            return Err(format!("the mod's window {window:?} is not open"));
        }
        let wanted = self.wanted(extension, window);
        match row {
            None => {
                let Some(placed) = layout.widget(widget).filter(|w| matches!(w.kind, Kind::Button { .. })) else {
                    return Err(format!("the window {window:?} has no button {widget}"));
                };
                let overrides = wanted.and_then(|w| w.set.get(widget));
                let visible = match overrides.and_then(|s| s.get("visible")) {
                    Some(Property::Visible(on)) => *on,
                    _ => placed.visible,
                };
                if !visible {
                    return Err(format!("the button {window}.{widget} is hidden"));
                }
                let enabled = !matches!(overrides.and_then(|s| s.get("enabled")), Some(Property::Enabled(false)));
                if !enabled {
                    return Err(format!("the button {window}.{widget} is disabled"));
                }
                Ok(())
            }
            Some(n) => {
                let Some((list_placed, list_name, button_placed)) = layout.widgets.iter().find_map(|w| match &w.kind {
                    Kind::List { row, .. } => {
                        let btn = row.widget(widget).filter(|rw| matches!(rw.kind, Kind::Button { .. }))?;
                        Some((w, w.name.as_deref()?, btn))
                    }
                    _ => None,
                }) else {
                    return Err(format!("the window {window:?} has no button {widget} in a list's row"));
                };
                let list_visible = match wanted.and_then(|w| w.set.get(list_name)).and_then(|s| s.get("visible")) {
                    Some(Property::Visible(on)) => *on,
                    _ => list_placed.visible,
                };
                if !list_visible {
                    return Err(format!("the list {window}.{list_name} is hidden"));
                }
                let list_rows = wanted.and_then(|w| w.rows.get(list_name)).map(Vec::as_slice).unwrap_or(&[]);
                let Some(cells) = n.checked_sub(1).and_then(|i| list_rows.get(i as usize)) else {
                    return Err(format!("the list {list_name} has no row {n}"));
                };
                let mut visible = button_placed.visible;
                let mut enabled = true;
                for (_, prop) in cells.iter().filter(|(name, _)| name == widget) {
                    match prop {
                        Property::Visible(on) => visible = *on,
                        Property::Enabled(on) => enabled = *on,
                        _ => {}
                    }
                }
                if !visible {
                    return Err(format!("the button {window}.{widget} in row {n} is hidden"));
                }
                if !enabled {
                    return Err(format!("the button {window}.{widget} in row {n} is disabled"));
                }
                Ok(())
            }
        }
    }

    /// Presses `window`'s entry button from a test, refusing when the window
    /// has no entry button or when the entry button is hidden or disabled.
    pub fn press_entry(&mut self, extension: usize, name: &str) -> Result<Vec<WindowSeen>, String> {
        let Some(layout) = self.window(extension, name) else {
            return Err(format!("the mod has no window {name:?}"));
        };
        let Some(entry) = &layout.entry else {
            return Err(format!("the mod's window {name:?} has no entry button"));
        };
        let overrides = self.wanted(extension, name).and_then(|w| w.set.get(crate::layout::ENTRY));
        let visible = match overrides.and_then(|s| s.get("visible")) {
            Some(Property::Visible(on)) => *on,
            _ => entry.visible,
        };
        if !visible {
            return Err(format!("the entry button of {name:?} is hidden"));
        }
        let enabled = match overrides.and_then(|s| s.get("enabled")) {
            Some(Property::Enabled(on)) => *on,
            _ => entry.enabled,
        };
        if !enabled {
            return Err(format!("the entry button of {name:?} is disabled"));
        }
        Ok(self.show(extension, name, !self.is_open(extension, name)))
    }

    /// The current content, including an initial value, without depending on
    /// whether the native window exists.
    pub fn tooltip(&self, extension: usize, window: &str, widget: &str) -> Option<crate::Text> {
        match self.wanted(extension, window)?.set.get(widget)?.get("tooltip")? {
            Property::Tooltip(text) => Some(text.clone()),
            _ => None,
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

    /// A native entry click, also used by the fake game. Recheck current
    /// desired state so a queued click cannot outlive disabling or hiding it.
    pub fn entry(&mut self, extension: usize, name: &str) -> Vec<WindowSeen> {
        let Some(properties) = self.wanted(extension, name).and_then(|w| w.set.get(crate::layout::ENTRY)) else { return Vec::new() };
        if properties.get("visible") == Some(&Property::Visible(false)) || properties.get("enabled") == Some(&Property::Enabled(false)) {
            return Vec::new();
        }
        self.show(extension, name, !self.is_open(extension, name))
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
            if open {
                self.last_appearance = self.last_appearance.checked_add(1).expect("window appearance order exhausted");
                wanted.appearance = self.last_appearance;
            }
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
        self.last_appearance = self.last_appearance.checked_add(1).expect("window appearance order exhausted");
        self.popups.push(OpenPopup { number, appearance: self.last_appearance, extension, popup });
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
        let extensions: Vec<_> = self.declared.iter().map(|d| d.extension).collect();
        for extension in extensions {
            self.initialize_texts(extension);
        }
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
    fn appearance_tracks_real_opens_across_windows_and_popups() {
        let mut all = declared();
        open(&mut all, 1, "tracker");
        let first = all.wanted(1, "tracker").unwrap().appearance;
        let number = all.popup(0, popup(vec![]), true).unwrap();
        let middle = all.open_popup(number).unwrap().appearance;
        open(&mut all, 0, "tracker");
        let last = all.wanted(0, "tracker").unwrap().appearance;
        assert!(first < middle && middle < last);
        open(&mut all, 1, "tracker");
        assert_eq!(all.wanted(1, "tracker").unwrap().appearance, first);
        all.show(1, "tracker", false);
        open(&mut all, 1, "tracker");
        assert!(all.wanted(1, "tracker").unwrap().appearance > last);
        let retained = all.wanted(1, "tracker").unwrap().appearance;
        all.declare(2, "other", &[]);
        assert_eq!(all.wanted(1, "tracker").unwrap().appearance, retained);
    }

    #[test]
    fn hidden_or_disabled_entries_ignore_queued_clicks_and_reload_keeps_order() {
        let windows = layout::read(&json!({"tools":{"size":[200,200],"entry":{"sprite":"GFX_tools", "value":0}}}), &|_| true).unwrap();
        let mut all = Windows::default();
        all.declare(0, "a", &windows);
        all.declare(1, "b", &windows);
        all.declare(0, "a", &windows);
        assert_eq!(all.declared().iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(all.wanted(0, "tools").unwrap().set[crate::layout::ENTRY]["value"], Property::Number(Some(0.0)));
        let set = |value| WindowChange::Set { window: "tools".into(), widget: crate::layout::ENTRY.into(), value };
        all.change(0, set(Property::Enabled(false)), true).unwrap();
        assert!(all.entry(0, "tools").is_empty() && !all.is_open(0, "tools"));
        all.change(0, set(Property::Enabled(true)), true).unwrap();
        all.change(0, set(Property::Visible(false)), true).unwrap();
        assert!(all.entry(0, "tools").is_empty() && !all.is_open(0, "tools"));
        all.change(0, set(Property::Visible(true)), true).unwrap();
        assert_eq!(all.entry(0, "tools"), [seen(0, "tools", true)]);
        assert_eq!(all.entry(0, "tools"), [seen(0, "tools", false)]);
        all.forget(0);
        assert!(all.entry(0, "tools").is_empty(), "stopped extensions keep no clickable entry state");
    }

    #[test]
    fn tooltip_content_survives_close_and_clear_until_a_new_session_or_declaration() {
        let windows = layout::read(
            &json!({ "tools": {
                "size": [300, 200], "entry": {"sprite":"GFX_tools", "tooltip":{"key":"TITLE"}},
                "items": [{"type":"text", "name":"result", "size":[200,20], "tooltip":{"key":"COUNT", "values":{"N":"0"}}}]
            }}),
            &|_| true,
        )
        .unwrap();
        let mut all = Windows::default();
        all.declare(0, "tools", &windows);
        let initial = all.tooltip(0, "tools", "result").unwrap();
        assert_eq!(initial.resolve(|_| Some("Count: $N$".into())), "Count: 0");
        assert_eq!(initial.resolve(|_| Some("数量：$N$".into())), "数量：0");
        assert_eq!(all.tooltip(0, "tools", "entry").unwrap().localisation_key(), Some("TITLE"));
        let set = |text| WindowChange::Set { window: "tools".into(), widget: "result".into(), value: Property::Tooltip(text) };
        all.change(0, set("Replacement".into()), false).unwrap();
        all.show(0, "tools", true);
        all.show(0, "tools", false);
        assert_eq!(all.tooltip(0, "tools", "result"), Some("Replacement".into()));
        all.change(0, set(Default::default()), false).unwrap();
        all.show(0, "tools", true);
        assert_eq!(all.tooltip(0, "tools", "result"), Some(Default::default()), "opening never restores a cleared initial value");
        all.end_session();
        assert_eq!(all.tooltip(0, "tools", "result"), Some(initial.clone()));
        all.change(0, set("Before reload".into()), false).unwrap();
        all.declare(0, "tools", &windows);
        assert_eq!(all.tooltip(0, "tools", "result"), Some(initial));
        all.forget(0);
        assert_eq!(all.tooltip(0, "tools", "result"), None);
    }

    #[test]
    fn declared_text_and_tooltips_share_replacement_clear_and_session_rules() {
        let layouts = layout::read(
            &json!({"tools":{"size":[300,200],"items":[{
                "type":"text","name":"result","size":[200,20],
                "text":{"key":"COUNT","values":{"N":3}},"tooltip":{"key":"COUNT","values":{"N":3}}
            }]}}),
            &|key| key == "COUNT",
        )
        .unwrap();
        for name in ["text", "tooltip"] {
            let mut all = Windows::default();
            all.declare(0, "tools", &layouts);
            let initial = all.wanted(0, "tools").unwrap().set["result"][name].clone();
            let content = match &initial {
                Property::Text(text) | Property::Tooltip(text) => text,
                _ => unreachable!(),
            };
            assert_eq!(content.resolve(|_| Some("§YCount $N$§!\nReady".into())), "§YCount 3§!\nReady");
            assert_eq!(content.resolve(|_| Some("数量：$N$".into())), "数量：3");
            let property = |text| if name == "text" { Property::Text(text) } else { Property::Tooltip(text) };
            let change = |text| WindowChange::Set { window: "tools".into(), widget: "result".into(), value: property(text) };
            all.change(0, change("COUNT".into()), false).unwrap();
            assert_eq!(all.wanted(0, "tools").unwrap().set["result"][name], property("COUNT".into()));
            all.change(0, change(Default::default()), false).unwrap();
            all.show(0, "tools", true);
            all.show(0, "tools", false);
            all.show(0, "tools", true);
            assert_eq!(all.wanted(0, "tools").unwrap().set["result"][name], property(Default::default()));
            all.end_session();
            assert_eq!(all.wanted(0, "tools").unwrap().set["result"][name], initial);
        }
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
