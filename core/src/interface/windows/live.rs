//! The game's side of extension windows and pop-ups: what makes the game's
//! windows show what extensions want (docs/game/interface/extension-windows.md,
//! window-elements.md).
//!
//! The definitions [`super::gui`] writes are served as framework files
//! (`crate::native::physfs::serve`) and read with the game's own reread of an
//! interface file, which takes new names after startup: an extension
//! reload's windows are named afresh, and each pop-up has a name of its own.
//! A window is built with the game's CreateWindow, placed, shown and bound:
//! its buttons' clicks subscribed as the army panel's are (`crate::interface::ui`), its
//! elements set as what the extension wants changes. The framework kills
//! it when it is to close, and hears when the game kills it itself, as the
//! game's `ui` observer of Kill tells [`killed`]; one still wanted open is
//! built again at the next frame.
//!
//! How each element is set and its clicks heard is `crate::interface::bound_element`'s,
//! which the elements spliced into vanilla windows share.
//!
//! Locks: [`LIVE`] is held while the framework calls the game, so what the
//! game calls back (Kill, tooltips) takes it only with `try_lock`; clicks
//! are only queued, and taken up at the next frame.
//! [`super::WINDOWS`] is never held while the game is called.
#[path = "entries.rs"]
mod entries;
#[path = "lifecycle.rs"]
mod lifecycle;

use super::{gui, with};
use crate::{
    base::Result,
    interface::{
        bound_element::{self, BoundElement, ElementOps, Hears, Look},
        native_gui::{self, Callback},
    },
    native::hooks::Call,
    native::memory::Memory,
    native::objects::{
        campaign,
        gui::{classes, element, functions, list_box, manager, scene, window},
    },
    native::points::{GameFunction, Point},
    text::unicode::game_text,
};
use eu4_harmony_lua::windows::Wanted;
use eu4_harmony_lua::{
    layout::{self, Kind, Window},
    Popup, Property,
};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub(super) use entries::EntrySpec;
pub(super) use lifecycle::Events as ReconcileEvents;

/// The framework files the definitions are read from: extension windows'
/// and pop-ups', each served with what it is to hold when it is read.
pub const WINDOWS_FILE: &str = "interface/eu4harmony_windows.gui";
pub const POPUPS_FILE: &str = "interface/eu4harmony_popups.gui";

/// The idler's: close the views over the map, and every big panel.
const CLOSE_VIEWS: GameFunction<1> = GameFunction::at(0x818080);
const CLOSE_PANELS: GameFunction<1> = GameFunction::at(0x80c6e0);
/// The idler's opening of one of its views by number `(idler, view)`, which
/// closes the others in a loop of its own, not through [`CLOSE_PANELS`]: the
/// diplomacy view from a country's shield or the map
/// (docs/game/interface/message-pop-up-actions.md §3).
const OPEN_VIEW: usize = 0x820990;
/// The idler's opening of the diplomacy view on a country `(idler, tag)`,
/// as a click on one of the game's shields does
/// (docs/game/interface/message-pop-up-actions.md §3).
const OPEN_DIPLOMACY: GameFunction<2> = GameFunction::at(0x821420);
/// The idler's tooltip of an element under the pointer `(idler, out, element)`.
const TOOLTIP: usize = 0xb81870;
/// The idler's laying out of the panel button strip `(idler)`.
const STRIP_LAYOUT: usize = 0x80c9f0;
/// Independent instant-text methods (research/panel-entry-text.md).
const TEXT_PLACE: GameFunction<2> = GameFunction::at(0x152af90);
const TEXT_SHOW: GameFunction<1> = GameFunction::at(0x152ac90);
const TEXT_HIDE: GameFunction<1> = GameFunction::at(0x152ad10);
const TEXT_PLACE_SLOT: usize = 0x260;
const LIST_ELEMENT: usize = list_box::ELEMENT;

/// The strip's own slots, their names' stem, and the first one's place;
/// the slots are this far apart, and an entry button's background lies so
/// from it (docs/game/interface/extension-windows.md §5).
const STRIP_SLOTS: usize = 8;
const STRIP_FIRST: (i32, i32) = (-360, -58);
const STRIP_STEP: i32 = 45;
const STRIP_BACKGROUND: (i32, i32) = (-2, 22);
/// How far each pop-up opened stands from the one before it.
const CASCADE: i32 = 24;

/// Set while the framework itself closes the game's big panels, to open a
/// window that is one: what the observer of that closing then ignores.
static OPENING: AtomicBool = AtomicBool::new(false);
/// Set when `strip_after` could not take [`LIVE`] immediately, so the next
/// safe frame reconciles the panel button strip.
static STRIP_DIRTY: AtomicBool = AtomicBool::new(false);

/// What the framework built.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Key {
    Window(usize, String),
    Popup(u32),
}

/// An element of a built window the framework binds, with what it is.
struct Bound {
    /// The game widget's name, or the window's own element's
    /// ([`gui::CLOSE`], a pop-up's button).
    name: String,
    element: BoundElement,
    tooltip_object: usize,
    /// The list and the row, from 1, a game widget in a list's row is in.
    row: Option<(String, u32)>,
}

struct Built {
    key: Key,
    root: usize,
    /// Where the framework put it last, to tell a drag.
    placed: (i32, i32),
    moveable: bool,
    elements: Vec<Bound>,
    /// The rows applied to each list.
    rows: BTreeMap<String, Vec<Vec<(String, Property)>>>,
}

/// An entry button on the panel button strip.
struct Entry {
    extension: usize,
    window: String,
    strip: usize,
    element: BoundElement,
    background: BoundElement,
    number: usize,
    number_text: Option<String>,
}

#[derive(Default)]
struct Live {
    built: Vec<Built>,
    /// Roots whose cleanup has not entered native Kill; kept until it can
    /// be called or the game reports their destruction. An attempted Kill
    /// relinquishes the root even if it faults: the sweep may free it.
    closing: Vec<Built>,
    /// The generation of definitions the game has read.
    read: Option<u64>,
    /// The pop-ups whose definitions the game has read.
    popups_read: HashSet<u32>,
    entries: Vec<Entry>,
    /// The callbacks of subscriptions let go; the game may still hold
    /// pointers to them.
    retired: Vec<Callback>,
    /// The window definitions the game would not build, by name.
    failed: HashSet<String>,
    reported: HashSet<String>,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

fn with_live<T>(f: impl FnOnce(&mut Live) -> T) -> T {
    let mut guard = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Live::default))
}

impl Live {
    fn report(&mut self, message: String) {
        if self.reported.insert(message.clone()) {
            log!("windows: {message}");
        }
    }

    /// Classifies raw clicks against the currently bound extension windows,
    /// pop-ups and entry buttons without touching [`super::WINDOWS`].
    fn classify_clicks(&self, clicked: Vec<bound_element::Clicked>) -> Vec<RoutedClick> {
        clicked
            .into_iter()
            .filter_map(|clicked| {
                let bound_element::Clicked { right, modifiers, .. } = clicked;
                for built in &self.built {
                    let Some(b) = built.elements.iter().find(|b| b.element.heard(&clicked)) else {
                        continue;
                    };
                    return match &built.key {
                        Key::Popup(n) => match super::gui::SHIELDS.iter().position(|s| *s == b.name) {
                            Some(side) => Some(RoutedClick::Shield { number: *n, side }),
                            None => super::gui::popup_button(&b.name).map(|button| RoutedClick::Popup { number: *n, button }),
                        },
                        Key::Window(e, w) if b.name == super::gui::CLOSE => Some(RoutedClick::Close { extension: *e, window: w.clone() }),
                        Key::Window(e, w) => Some(RoutedClick::Widget {
                            extension: *e,
                            window: w.clone(),
                            widget: b.name.clone(),
                            row: b.row.as_ref().map(|(_, r)| *r),
                            right,
                            modifiers,
                        }),
                    };
                }
                self.entries.iter().find(|en| en.element.heard(&clicked)).map(|en| RoutedClick::Entry { extension: en.extension, window: en.window.clone() })
            })
            .collect()
    }

    /// Tooltip bytes for a managed element under the pointer, suppressing
    /// row-widget tooltips while their parent list widget is hidden.
    fn managed_tooltip(&self, element: usize) -> Option<Vec<u8>> {
        for built in &self.built {
            if let Some(bound) = built.elements.iter().find(|b| b.tooltip_object == element) {
                let row_hidden = bound.row.as_ref().is_some_and(|(list, _)| {
                    built.elements.iter().any(|parent| parent.row.is_none() && &parent.name == list && !parent.element.wanted_visible())
                });
                return Some(if row_hidden { Vec::new() } else { bound.element.tooltip().unwrap_or_default().to_vec() });
            }
        }
        self.entries.iter().find(|e| e.element.object == element).map(|e| e.element.tooltip().unwrap_or_default().to_vec())
    }
}

/// An i64 place as the game passes one, `x | y << 32`.
fn pack((x, y): (i32, i32)) -> usize {
    (x as u32 as u64 | (y as u32 as u64) << 32) as usize
}

fn unpack(place: u64) -> (i32, i32) {
    (place as u32 as i32, (place >> 32) as u32 as i32)
}

fn idler(m: &Memory) -> Result<usize> {
    let world = m.ptr(m.at(campaign::globals::WORLD))?;
    if world == 0 {
        return Err("no world".into());
    }
    let idler = m.ptr(world + campaign::world::MAP_MANAGER)?;
    if idler == 0 || m.ptr(idler)? != m.at(campaign::classes::MAP_MANAGER) {
        return Err("no map manager".into());
    }
    Ok(idler)
}

/// The screen's size in GUI units.
fn screen(m: &Memory, mgr: usize) -> Result<(i32, i32)> {
    let scene = m.ptr(mgr + manager::SCENE)?;
    Ok((m.i32(scene + scene::SIZE)?, m.i32(scene + scene::SIZE + 4)?))
}

/// Where a window of `size` stands on a screen of `screen`.
fn place(window: &Window, (sw, sh): (i32, i32)) -> (i32, i32) {
    let (fx, fy) = window.anchor.fraction();
    let x = (f64::from(sw - window.size.0) * fx) as i32 + window.offset.0;
    let y = (f64::from(sh - window.size.1) * fy) as i32 + window.offset.1;
    (x.clamp(0, (sw - window.size.0).max(0)), y.clamp(0, (sh - window.size.1).max(0)))
}

/// Reads the definitions file `path`, served with `text`, into the game.
fn read_definitions(m: &Memory, mgr: usize, path: &str, text: String) -> Result<()> {
    log!("windows: reading {} bytes of definitions from {path}", text.len());
    crate::native::physfs::serve(path, game_text::from_str(&text));
    native_gui::with_string(m, path.as_bytes(), |p| functions::READ_DEFINITIONS.call(m, [mgr, p]).map(|_| ()))
}

/// A window built from the definition `name`, hidden; checked to be one.
fn create(m: &Memory, mgr: usize, name: &str) -> Result<usize> {
    let root = native_gui::with_string(m, name.as_bytes(), |n| functions::CREATE_WINDOW.call(m, [mgr, n]))?;
    if root == 0 {
        return Err(format!("the game built no window {name}"));
    }
    if m.ptr(root)? != m.at(classes::WINDOW) || m.vslot(root, window::KILL)? != m.at(native_gui::WINDOW_KILL) {
        return Err(format!("{name} is not a window of the kind the framework knows"));
    }
    Ok(root)
}

fn set_position(m: &Memory, root: usize, at: (i32, i32)) -> Result<()> {
    functions::SET_POSITION.call(m, [root, pack(at)]).map(|_| ())
}

/// The element `name` of the window `root`, of the kind `kind` looks it up
/// by; checked to be the one of that name.
fn find(m: &Memory, root: usize, kind: Option<&Kind>, name: &str) -> Result<usize> {
    let object = match kind {
        Some(Kind::List { .. }) => {
            if m.vslot(root, window::FIND_LIST_BOX)? != m.at(functions::FIND_LIST_BOX) {
                return Err("a window's list box lookup is not the one the framework knows".into());
            }
            native_gui::with_string(m, name.as_bytes(), |n| functions::FIND_LIST_BOX.call(m, [root, n]))?
        }
        Some(Kind::Text { .. }) => native_gui::lookup(m, root, (window::FIND_TEXT, functions::FIND_TEXT), name)?,
        Some(Kind::Icon { .. } | Kind::Progress { .. }) => native_gui::lookup(m, root, (window::FIND_ICON, functions::FIND_ICON), name)?,
        _ => native_gui::lookup(m, root, (window::FIND_BUTTON, functions::FIND_BUTTON), name)?,
    };
    if object == 0 {
        return Err(format!("{name} not found"));
    }
    if !matches!(kind, Some(Kind::List { .. })) && native_gui::element_name(m, object)? != name.as_bytes() {
        return Err(format!("the lookup of {name} found another element"));
    }
    Ok(object)
}

/// How a game widget of `kind` is set.
fn look(kind: &Kind) -> Look {
    match kind {
        Kind::Text { .. } => Look::Text,
        Kind::Icon { .. } | Kind::Progress { .. } => Look::Icon,
        Kind::Button { .. } => Look::Button,
        Kind::Flag { .. } => Look::Flag,
        Kind::List { .. } => Look::List,
    }
}

// ---------- the frame ----------

pub(super) type LookupWords = std::sync::Arc<dyn Fn(&str) -> Option<String>>;

/// One extension window as a frame needs it.
pub(super) struct WantWindow {
    pub extension: usize,
    /// Its definition's name, of this generation.
    pub type_name: String,
    pub layout: Window,
    pub words: LookupWords,
    pub wanted: Wanted,
}

/// What a frame needs of what extensions want, taken out of
/// [`super::WINDOWS`] so that the game is called without it.
pub(super) struct Wants {
    pub generation: u64,
    pub windows: Vec<WantWindow>,
    pub definitions: Vec<String>,
    pub popups: Vec<(u32, Popup)>,
    /// Real pop-up appearance order, shared with Wanted::appearance.
    pub appearances: BTreeMap<u32, u64>,
}

/// A native click classified under [`LIVE`] and applied under [`super::WINDOWS`].
#[derive(Debug, PartialEq, Eq)]
pub(super) enum RoutedClick {
    Widget { extension: usize, window: String, widget: String, row: Option<u32>, right: bool, modifiers: u32 },
    Close { extension: usize, window: String },
    Popup { number: u32, button: u32 },
    Shield { number: u32, side: usize },
    Entry { extension: usize, window: String },
}

fn wants(last_read: Option<u64>) -> Wants {
    with(|all| {
        all.snapshot(last_read, &|id| {
            let owned = id.to_string();
            std::sync::Arc::new(move |key| crate::base::loc::mod_text(&owned, key))
        })
    })
}

/// At every safe frame of a game, after the extensions: the game's windows
/// made to show what extensions want.
pub(crate) fn frame() {
    let Some(m) = crate::native::memory() else { return };
    if crate::base::thread_id() != crate::base::main_thread() {
        return;
    }
    clicks(m);
    let last_read = with_live(|live| live.read);
    let wants = wants(last_read);
    let pending_entries = STRIP_DIRTY.swap(false, Ordering::AcqRel).then(entry_specs).unwrap_or_default();
    let events = with_live(|live| {
        let (events, result) = lifecycle::frame(m, live, &wants);
        if let Err(e) = result {
            live.report(e);
        }
        if !pending_entries.is_empty() {
            if let Ok(idler) = idler(m) {
                entries::sync(m, live, idler, &pending_entries);
            }
        }
        events
    });
    if !events.dragged.is_empty() || !events.lost_popups.is_empty() {
        with(|all| all.apply_reconciled(&events));
    }
}

/// The clicks since the last frame, as what extensions hear and what opens,
/// closes and answers.
fn clicks(m: &Memory) {
    let clicked = bound_element::take_clicks(Hears::Windows);
    if clicked.is_empty() {
        return;
    }
    let routed = with_live(|live| live.classify_clicks(clicked));
    let shields = with(|all| all.apply_clicks(routed));
    for (number, side) in shields {
        if let Err(e) = open_diplomacy(m, number, side) {
            log!("windows: pop-up {number}'s shield: {e}");
        }
    }
}

/// The letters of the player's country, none in observe mode with no
/// country in view.
fn player_letters(m: &Memory) -> Option<String> {
    super::super::player_letters(m)
}

/// Opens the diplomacy view on the country of a pop-up's shield, on the
/// `side` of [`gui::SHIELDS`], as a click on the game's own shields does.
fn open_diplomacy(m: &Memory, number: u32, side: usize) -> Result<()> {
    let named = with(|all| all.wanted.open_popup(number).map(|p| p.popup.shields[side].clone())).flatten();
    let Some(tag) = super::super::resolve_shield_country(m, named.as_deref())? else { return Ok(()) };
    OPEN_DIPLOMACY.call(m, [idler(m)?, tag as usize])?;
    Ok(())
}

/// The game is killing the window `root`: when it is one the framework
/// built, its subscriptions are let go while its elements are alive, and
/// one still wanted open is built again at the next frame. Ignored while
/// the framework kills it itself, holding the lock.
pub(crate) fn killed(m: &Memory, root: usize) {
    let Ok(mut guard) = LIVE.try_lock() else { return };
    let Some(live) = guard.as_mut() else { return };
    if let Err(e) = entries::retire(m, live, |entry| entry.strip == root) {
        live.report(format!("a killed entry strip could not let go of its buttons: {e}"));
    }
    if let Err(e) = lifecycle::killed(m, live, root) {
        live.report(format!("a window the game killed could not let go of its buttons: {e}"));
    }
}

/// The session ended: every window and pop-up is killed, and the state
/// forgets them.
pub(crate) fn session_ended() {
    crate::interface::tooltips::clear();
    STRIP_DIRTY.store(false, Ordering::Release);
    if let Some(m) = crate::native::memory() {
        with_live(|live| {
            if let Err(e) = entries::retire(m, live, |_| true) {
                live.report(format!("entry buttons at the session's end: {e}"));
            }
            if let Err(e) = lifecycle::session_ended(m, live) {
                live.report(format!("a window at the session's end: {e}"));
            }
        });
    }
    with(|all| all.end_session());
    crate::interface::window_ordering::clear();
}

// ---------- the game's big panels and tooltips ----------

/// The game closes its big panels, or opens one of its views: the extension
/// windows that are big panels close too, but when the framework itself
/// closes them to open one.
fn panels_closing(_: &mut Call) {
    if !OPENING.load(Ordering::Relaxed) {
        with(|all| all.close_panels());
    }
}

/// Passes the actual hit to the shared native presentation adapter.
fn tooltip_after(call: &mut Call) {
    if !call.faulted() && crate::base::thread_id() == crate::base::main_thread() {
        crate::interface::tooltips::hit(call.arg(2));
    }
}

pub(crate) fn managed_tooltip(element: usize) -> Option<Vec<u8>> {
    let guard = LIVE.try_lock().ok()?;
    guard.as_ref()?.managed_tooltip(element)
}

// ---------- entry buttons ----------

/// The element names of an entry button, by its mod's prefix: the button
/// and its background.
pub fn entry_names(id: &str, window: &str) -> (String, String) {
    let prefix = crate::native::splice::prefix(id);
    (format!("{prefix}entry_{window}"), format!("{prefix}entry_{window}_background"))
}

/// The `floating_buttons` text of an extension window's entry button.
pub fn entry_fragment(id: &str, window: &str, entry: &layout::Entry) -> String {
    let (button, background) = entry_names(id, window);
    format!(
        "\n\t\ticonType = {{ name = \"{background}\" spriteType = \"GFX_menubar_floating_bg\" position = {{ x = -362 y = -36 }} Orientation = \"LOWER_RIGHT\" }}\n\
         \t\tguiButtonType = {{ name = \"{button}\" position = {{ x = -360 y = -58 }} quadTextureSprite = \"{}\" frame = {} Orientation = \"LOWER_RIGHT\" clicksound = click }}\n\
         \t\tinstantTextBoxType = {{ name = \"{button}_number\" position = {{ x = -364 y = -18 }} font = \"vic_18\" maxWidth = 50 maxHeight = 32 format = centre Orientation = \"LOWER_RIGHT\" }}\n",
        entry.sprite.replace('"', ""),
        entry.frame
    )
}

fn entry_specs() -> Vec<entries::EntrySpec> {
    with(|all| all.entry_specs(&crate::base::loc::mod_text))
}

/// After the game laid out its panel button strip: each entry button that
/// shows is placed in the next slot, left of the game's own.
fn strip_after(call: &mut Call) {
    if call.faulted() || crate::base::thread_id() != crate::base::main_thread() {
        return;
    }
    let m = call.memory();
    let idler = call.arg(0);
    let specs = entry_specs();
    if specs.is_empty() {
        return;
    }
    let Ok(mut guard) = LIVE.try_lock() else {
        STRIP_DIRTY.store(true, Ordering::Release);
        return;
    };
    let live = guard.get_or_insert_with(Live::default);
    entries::sync(&m, live, idler, &specs);
}

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(list_box::SHOW, "48 83 e9 08 e9 b3 e3 ff ff", "list show visual interface"),
        Point::keep(list_box::HIDE, "48 83 e9 08 e9 fb e4 ff ff", "list hide visual interface"),
        Point::method(list_box::ELEMENT_CLASS, element::SHOW, list_box::SHOW, "list show method"),
        Point::method(list_box::ELEMENT_CLASS, element::HIDE, list_box::HIDE, "list hide method"),
        Point::keep(0x152e2cdusize, "49 89 46 08", "list visual element interface").uses(&[LIST_ELEMENT]),
        Point::keep(TEXT_PLACE, "40 53 48 83 ec 20 48 8b d9 48 8b 89 18 01 00 00", "entry number position"),
        Point::keep(TEXT_SHOW, "40 53 48 83 ec 20 80 b9 9c 01 00 00 00 48 8b d9", "entry number show"),
        Point::keep(TEXT_HIDE, "40 53 48 83 ec 20 c6 81 c5 00 00 00 00 48 8b d9", "entry number hide"),
        Point::method(classes::TEXT_BOX, TEXT_PLACE_SLOT, TEXT_PLACE.rva(), "entry number position method"),
        Point::method(classes::TEXT_BOX, element::SHOW, TEXT_SHOW.rva(), "entry number show method"),
        Point::method(classes::TEXT_BOX, element::HIDE, TEXT_HIDE.rva(), "entry number hide method"),
        Point::keep(CLOSE_VIEWS, "48 89 5c 24 10 57 48 83 ec 20 48 8b 99 80 1b 00 00 48 8b f9", "closing the views over the map"),
        Point::keep(OPEN_DIPLOMACY, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b 81 10 11", "opening the diplomacy view on a country"),
        Point::observe(CLOSE_PANELS, "48 89 5c 24 10 57 48 83 ec 20 48 8b 99 b8 10 00 00 48 8b f9", "closing the big panels", 1, Some(panels_closing), None),
        Point::observe(
            OPEN_VIEW,
            "40 53 55 57 48 83 ec 20 4c 89 7c 24 58 48 8b d9 44 8b fa e8",
            "opening one of the idler's views",
            2,
            Some(panels_closing),
            None,
        ),
        Point::observe(TOOLTIP, "48 89 5c 24 08 48 89 54 24 10 55 56 57 41 54 41", "the map manager's tooltip", 3, None, Some(tooltip_after)),
        Point::observe(
            STRIP_LAYOUT,
            "40 55 53 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 e1 48 81 ec f8 00 00",
            "laying out the panel button strip",
            1,
            None,
            Some(strip_after),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::bound_element::{Clicked, Shown};
    use eu4_harmony_lua::WindowChange;
    use serde_json::json;

    fn sample_windows() -> Vec<Window> {
        layout::read(
            &json!({
                "tools": {
                    "size": [400, 300],
                    "panel": true,
                    "entry": { "sprite": "GFX_entry", "tooltip": "ENTRY_TIP" },
                    "items": [
                        { "type": "button", "name": "press", "at": [5, 5], "size": [80, 20], "text": "PRESS", "tooltip": "PRESS_TIP" },
                        { "type": "list", "name": "results", "at": [5, 40], "size": [300, 120], "row": { "size": [280, 40], "items": [
                            { "type": "button", "name": "row_press", "size": [80, 20], "text": "ROW", "tooltip": "ROW_TIP" }
                        ]}}
                    ]
                }
            }),
            &|_| true,
        )
        .unwrap()
    }

    fn bound_button(name: &str, object: usize, row: Option<(&str, u32)>, tooltip: &[u8]) -> Bound {
        let mut element = BoundElement::test_listening(object, Look::Button);
        element.set_linked_for_test(true);
        element.record_for_test(&Shown { visible: Some(true), enabled: Some(true), tooltip: Some(tooltip.to_vec()), ..Default::default() });
        Bound { name: name.into(), element, tooltip_object: object, row: row.map(|(l, r)| (l.into(), r)) }
    }

    fn click_of(element: &BoundElement, right: bool, modifiers: u32) -> Clicked {
        Clicked { context: element.context_for_test().expect("listening"), button: element.object, right, modifiers }
    }

    #[test]
    fn snapshot_only_renders_gui_definitions_when_generation_changes() {
        let mut state = super::super::Windows::default();
        let layouts = sample_windows();
        state.wanted.declare(0, "alpha", &layouts);
        state.generation = 1;
        let words = |_: &str| -> LookupWords { std::sync::Arc::new(|key| Some(format!("L:{key}"))) };

        let fresh = state.snapshot(None, &words);
        assert_eq!(fresh.generation, 1);
        assert_eq!(fresh.definitions.len(), 1);
        assert!(fresh.definitions[0].contains("L:PRESS"));

        let steady = state.snapshot(Some(1), &words);
        assert_eq!(steady.generation, 1);
        assert!(steady.definitions.is_empty(), "steady-state frames must not reformat .gui definitions");

        state.generation = 2;
        let reloaded = state.snapshot(Some(1), &words);
        assert_eq!(reloaded.definitions.len(), 1);
    }

    #[test]
    fn clicks_classify_and_route_widgets_rows_close_popups_shields_and_entries() {
        let mut state = super::super::Windows::default();
        let layouts = sample_windows();
        state.wanted.declare(1, "alpha", &layouts);
        state.wanted.change(1, WindowChange::Open("tools".into()), true).unwrap();
        state
            .wanted
            .change(
                1,
                WindowChange::Rows { window: "tools".into(), widget: "results".into(), rows: vec![vec![("row_press".into(), Property::Enabled(true))]] },
                true,
            )
            .unwrap();
        let popup =
            Popup { header: None, title: "Title".into(), text: "Body".into(), buttons: vec!["Yes".into(), "No".into()], shields: [Some("FRA".into()), None] };
        let popup_button_2 = gui::shown(&popup)[1].0.clone();
        let popup_id = state.wanted.popup(1, popup, true).unwrap();

        let btn = bound_button("press", 101, None, b"Press");
        let row_btn = bound_button("row_press", 102, Some(("results", 1)), b"Row");
        let close_btn = bound_button(gui::CLOSE, 103, None, b"");
        let popup_btn = bound_button(&popup_button_2, 201, None, b"");
        let shield_btn = bound_button(gui::SHIELDS[0], 202, None, b"");
        let mut entry_el = BoundElement::test_listening(301, Look::Button);
        entry_el.set_linked_for_test(true);

        let raw = vec![
            click_of(&btn.element, true, native_gui::MODIFIER_SHIFT | native_gui::MODIFIER_CTRL),
            click_of(&row_btn.element, false, 0),
            click_of(&close_btn.element, false, 0),
            click_of(&popup_btn.element, false, 0),
            click_of(&shield_btn.element, false, 0),
            click_of(&entry_el, false, 0),
        ];

        let live = Live {
            built: vec![
                Built {
                    key: Key::Window(1, "tools".into()),
                    root: 10,
                    placed: (0, 0),
                    moveable: false,
                    elements: vec![btn, row_btn, close_btn],
                    rows: BTreeMap::new(),
                },
                Built { key: Key::Popup(popup_id), root: 20, placed: (0, 0), moveable: true, elements: vec![popup_btn, shield_btn], rows: BTreeMap::new() },
            ],
            entries: vec![Entry {
                extension: 1,
                window: "tools".into(),
                strip: 30,
                element: entry_el,
                background: BoundElement::new(302, Look::Icon),
                number: 303,
                number_text: None,
            }],
            ..Live::default()
        };

        let routed = live.classify_clicks(raw);
        assert_eq!(
            routed,
            vec![
                RoutedClick::Widget {
                    extension: 1,
                    window: "tools".into(),
                    widget: "press".into(),
                    row: None,
                    right: true,
                    modifiers: native_gui::MODIFIER_SHIFT | native_gui::MODIFIER_CTRL
                },
                RoutedClick::Widget { extension: 1, window: "tools".into(), widget: "row_press".into(), row: Some(1), right: false, modifiers: 0 },
                RoutedClick::Close { extension: 1, window: "tools".into() },
                RoutedClick::Popup { number: popup_id, button: 2 },
                RoutedClick::Shield { number: popup_id, side: 0 },
                RoutedClick::Entry { extension: 1, window: "tools".into() },
            ]
        );

        let shields = state.apply_clicks(routed);
        assert_eq!(shields, vec![(popup_id, 0)]);
        assert_eq!(state.clicks.len(), 2);
        assert_eq!(state.clicks[0].button, "tools.press");
        assert!(state.clicks[0].right && state.clicks[0].shift && state.clicks[0].ctrl);
        assert_eq!(state.clicks[1].button, "tools.row_press");
        assert_eq!(state.clicks[1].row, Some(1));
        assert_eq!(state.wanted.take_pressed().len(), 1);
        // Close closed the window, then Entry toggled it back open.
        assert!(state.wanted.wanted(1, "tools").unwrap().open);
    }

    #[test]
    fn managed_tooltip_suppresses_rows_of_hidden_lists_and_falls_back_to_entries() {
        let mut list_el = BoundElement::new(500, Look::List);
        list_el.record_for_test(&Shown { visible: Some(false), tooltip: Some(b"List".to_vec()), ..Default::default() });
        let row_btn = bound_button("row_press", 501, Some(("results", 1)), b"RowTip");
        let mut entry_el = BoundElement::new(600, Look::Button);
        entry_el.record_for_test(&Shown { visible: Some(true), tooltip: Some(b"EntryTip".to_vec()), ..Default::default() });

        let mut live = Live {
            built: vec![Built {
                key: Key::Window(1, "tools".into()),
                root: 10,
                placed: (0, 0),
                moveable: false,
                elements: vec![Bound { name: "results".into(), element: list_el, tooltip_object: 500 + list_box::ELEMENT, row: None }, row_btn],
                rows: BTreeMap::new(),
            }],
            entries: vec![Entry {
                extension: 1,
                window: "tools".into(),
                strip: 30,
                element: entry_el,
                background: BoundElement::new(601, Look::Icon),
                number: 602,
                number_text: None,
            }],
            ..Live::default()
        };

        assert_eq!(live.managed_tooltip(500 + list_box::ELEMENT), Some(Vec::new()), "hidden list hides its own tooltip");
        assert_eq!(live.managed_tooltip(501), Some(Vec::new()), "hidden parent list must suppress row tooltips");
        assert_eq!(live.managed_tooltip(600), Some(b"EntryTip".to_vec()));
        assert_eq!(live.managed_tooltip(999), None);

        // Showing the parent list restores both the list's and the row's tooltips.
        live.built[0].elements[0].element.record_for_test(&Shown { visible: Some(true), ..Default::default() });
        assert_eq!(live.managed_tooltip(500 + list_box::ELEMENT), Some(b"List".to_vec()));
        assert_eq!(live.managed_tooltip(501), Some(b"RowTip".to_vec()));
    }

    #[test]
    fn reconciled_events_update_dragged_position_and_drop_lost_popups_without_global_state() {
        let mut state = super::super::Windows::default();
        let layouts = sample_windows();
        state.wanted.declare(2, "alpha", &layouts);
        let popup_id = state
            .wanted
            .popup(2, Popup { header: None, title: "T".into(), text: "B".into(), buttons: vec!["OK".into()], shields: [None, None] }, true)
            .unwrap();

        let events = ReconcileEvents { dragged: vec![(2, "tools".into(), (42, 84))], lost_popups: vec![popup_id] };
        state.apply_reconciled(&events);

        assert_eq!(state.wanted.wanted(2, "tools").unwrap().dragged, Some((42, 84)));
        assert!(state.wanted.popups().is_empty());
    }
}
