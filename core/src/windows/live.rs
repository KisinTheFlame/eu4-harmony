//! The game's side of extension windows and pop-ups: what makes the game's
//! windows show what extensions want (docs/research/extension-windows.md,
//! window-elements.md).
//!
//! The definitions [`super::gui`] writes are served as framework files
//! (`crate::physfs::serve`) and read with the game's own reread of an
//! interface file, which takes new names after startup: an extension
//! reload's windows are named afresh, and each pop-up has a name of its own.
//! A window is built with the game's CreateWindow, placed, shown and bound:
//! its buttons' clicks subscribed as the army panel's are (`crate::ui`), its
//! elements set as what the extension wants changes. The framework kills
//! it when it is to close, and hears when the game kills it itself, as the
//! game's `ui` observer of Kill tells [`killed`]; one still wanted open is
//! built again at the next frame.
//!
//! How each element is set and its clicks heard is `crate::bound_element`'s,
//! which the elements spliced into vanilla windows share.
//!
//! Locks: [`LIVE`] is held while the framework calls the game, so what the
//! game calls back (Kill, tooltips) takes it only with `try_lock`; clicks
//! are only queued, and taken up at the next frame.
//! [`super::WINDOWS`] is never held while the game is called.
#[path = "lifecycle.rs"]
mod lifecycle;

use super::{gui, type_name, with};
use crate::{
    bound_element::{self, BoundElement, Hears, Look, Shown},
    game::{self, Caller, WorldSession},
    game_string,
    hooks::Call,
    memory::Memory,
    objects::{
        campaign::{self, map_manager},
        gui::{classes, element, functions, list_box, manager, scene, window, window_gui},
    },
    points::{GameFunction, Point},
    ui::{self, Callback},
    unicode::game_text,
    Result,
};
use eu4_harmony_lua::windows::Wanted;
use eu4_harmony_lua::{
    dispatch::Click,
    layout::{self, Kind, Placed, Window},
    Popup, Property,
};
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

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
/// (docs/research/message-pop-up-actions.md §3).
const OPEN_VIEW: usize = 0x820990;
/// The idler's opening of the diplomacy view on a country `(idler, tag)`,
/// as a click on one of the game's shields does
/// (docs/research/message-pop-up-actions.md §3).
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
/// from it (docs/research/extension-windows.md §5).
const STRIP_SLOTS: usize = 8;
const STRIP_FIRST: (i32, i32) = (-360, -58);
const STRIP_STEP: i32 = 45;
const STRIP_BACKGROUND: (i32, i32) = (-2, 22);
/// How far each pop-up opened stands from the one before it.
const CASCADE: i32 = 24;

/// Set while the framework itself closes the game's big panels, to open a
/// window that is one: what the observer of that closing then ignores.
static OPENING: AtomicBool = AtomicBool::new(false);

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
}

/// An i64 place as the game passes one, `x | y << 32`.
fn pack((x, y): (i32, i32)) -> usize {
    (x as u32 as u64 | (y as u32 as u64) << 32) as usize
}

fn unpack(place: u64) -> (i32, i32) {
    (place as u32 as i32, (place >> 32) as u32 as i32)
}

/// Calls `f` with a game string holding `text`, destroyed after.
pub(crate) fn with_string<T>(m: &Memory, text: &[u8], f: impl FnOnce(usize) -> Result<T>) -> Result<T> {
    let mut string = game_string::empty();
    let at = string.as_mut_ptr() as usize;
    if !text.is_empty() {
        game_string::append_checked(m, at, text)?;
    }
    let result = f(at);
    game_string::destroy_checked(m, at)?;
    result
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
    crate::physfs::serve(path, game_text::from_str(&text));
    with_string(m, path.as_bytes(), |p| functions::READ_DEFINITIONS.call(m, [mgr, p]).map(|_| ()))
}

/// A window built from the definition `name`, hidden; checked to be one.
fn create(m: &Memory, mgr: usize, name: &str) -> Result<usize> {
    let root = with_string(m, name.as_bytes(), |n| functions::CREATE_WINDOW.call(m, [mgr, n]))?;
    if root == 0 {
        return Err(format!("the game built no window {name}"));
    }
    if m.ptr(root)? != m.at(classes::WINDOW) || m.vslot(root, window::KILL)? != m.at(ui::WINDOW_KILL) {
        return Err(format!("{name} is not a window of the kind the framework knows"));
    }
    Ok(root)
}

pub(crate) fn show_window(m: &Memory, root: usize, show: bool) -> Result<()> {
    let gui = root + window::GUI;
    let (slot, function) = if show { (window_gui::SHOW, functions::WINDOW_SHOW) } else { (window_gui::HIDE, functions::WINDOW_HIDE) };
    let at = m.vslot(gui, slot)?;
    if at != m.at(function) {
        return Err("a window's show or hide is not the one the framework knows".into());
    }
    m.call(at, &[gui]).map(|_| ())
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
            with_string(m, name.as_bytes(), |n| functions::FIND_LIST_BOX.call(m, [root, n]))?
        }
        Some(Kind::Text { .. }) => ui::lookup(m, root, (window::FIND_TEXT, functions::FIND_TEXT), name)?,
        Some(Kind::Icon { .. } | Kind::Progress { .. }) => ui::lookup(m, root, (window::FIND_ICON, functions::FIND_ICON), name)?,
        _ => ui::lookup(m, root, (window::FIND_BUTTON, functions::FIND_BUTTON), name)?,
    };
    if object == 0 {
        return Err(format!("{name} not found"));
    }
    if !matches!(kind, Some(Kind::List { .. })) && ui::element_name(m, object)? != name.as_bytes() {
        return Err(format!("the lookup of {name} found another element"));
    }
    Ok(object)
}

/// The element `object`, of `look`, hearing its clicks, subscribed.
fn listening(m: &Memory, object: usize, look: Look) -> Result<BoundElement> {
    let mut element = BoundElement::listening(m, object, look, Hears::Windows)?;
    element.link(m)?;
    Ok(element)
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

// ---------- applying what is wanted ----------

/// What a game widget shows at first, from its layout.
fn initial(placed: &Placed, words: &BTreeMap<String, String>) -> BTreeMap<&'static str, Property> {
    let tooltip = placed.tooltip.as_ref().map(|t| t.resolve(|key| words.get(key).cloned()));
    let words = |text: &Option<eu4_harmony_lua::Text>| super::words_of(words, text);
    let mut out = BTreeMap::new();
    let mut put = |p: Property| {
        out.insert(p.name(), p);
    };
    put(Property::Visible(placed.visible));
    if let Some(tooltip) = tooltip {
        put(Property::Tooltip(tooltip.into()));
    }
    match &placed.kind {
        Kind::Text { text, .. } => put(Property::Text(words(text).into())),
        Kind::Icon { frame, .. } => put(Property::Frame(*frame)),
        Kind::Button { frame, sprite: Some(_), .. } => put(Property::Frame(*frame)),
        Kind::Progress { value, .. } => put(Property::Value(*value)),
        Kind::Flag { country: Some(c), .. } => put(Property::Country(c.clone())),
        _ => {}
    }
    out
}

// ---------- the frame ----------

/// One extension window as a frame needs it.
struct WantWindow {
    extension: usize,
    /// Its definition's name, of this generation.
    type_name: String,
    layout: Window,
    words: BTreeMap<String, String>,
    wanted: Wanted,
}

/// What a frame needs of what extensions want, taken out of
/// [`super::WINDOWS`] so that the game is called without it.
struct Wants {
    generation: u64,
    windows: Vec<WantWindow>,
    definitions: Vec<String>,
    popups: Vec<(u32, Popup)>,
}

fn wants() -> Wants {
    with(|all| {
        let mut windows = Vec::new();
        let mut definitions = Vec::new();
        let none = BTreeMap::new();
        for d in all.wanted.declared() {
            let words = all.words.get(&d.extension).unwrap_or(&none);
            for w in &d.windows {
                let type_name = format!("{}_{}", type_name(&d.id, &w.name), all.generation);
                definitions.push(gui::window(&type_name, w, &|k| super::words_of(words, k)));
                let mut wanted = all.wanted.wanted(d.extension, &w.name).cloned().unwrap_or_default();
                let resolve = |property: &mut Property| {
                    if let Property::Text(content) | Property::Tooltip(content) = property {
                        *content = content.resolve(|key| crate::loc::mod_text(&d.id, key)).into();
                    }
                };
                for property in wanted.set.values_mut().flat_map(|p| p.values_mut()) {
                    resolve(property);
                }
                for (_, property) in wanted.rows.values_mut().flatten().flatten() {
                    resolve(property);
                }
                windows.push(WantWindow { extension: d.extension, type_name, layout: w.clone(), words: words.clone(), wanted });
            }
        }
        let popups = all.wanted.popups().iter().map(|p| (p.number, p.popup.clone())).collect();
        Wants { generation: all.generation, windows, definitions, popups }
    })
}

/// At every safe frame of a game, after the extensions: the game's windows
/// made to show what extensions want.
pub(crate) fn frame() {
    let Some(m) = crate::memory() else { return };
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    clicks(m);
    let wants = wants();
    with_live(|live| {
        if let Err(e) = lifecycle::frame(m, live, &wants) {
            live.report(e);
        }
    });
}

/// The clicks since the last frame, as what extensions hear and what opens,
/// closes and answers.
fn clicks(m: &Memory) {
    let clicked = bound_element::take_clicks(Hears::Windows);
    if clicked.is_empty() {
        return;
    }
    // Who each click is for, found under the lock, done after it.
    enum Was {
        Widget(usize, String, String, Option<u32>, bool, u32),
        Close(usize, String),
        Popup(u32, u32),
        Shield(u32, usize),
        Entry(usize, String),
    }
    let found: Vec<Was> = with_live(|live| {
        clicked
            .into_iter()
            .filter_map(|clicked| {
                let bound_element::Clicked { right, modifiers, .. } = clicked;
                for built in &live.built {
                    let Some(b) = built.elements.iter().find(|b| b.element.heard(&clicked)) else {
                        continue;
                    };
                    return match &built.key {
                        Key::Popup(n) => match gui::SHIELDS.iter().position(|s| *s == b.name) {
                            Some(side) => Some(Was::Shield(*n, side)),
                            None => gui::popup_button(&b.name).map(|button| Was::Popup(*n, button)),
                        },
                        Key::Window(e, w) if b.name == gui::CLOSE => Some(Was::Close(*e, w.clone())),
                        Key::Window(e, w) => Some(Was::Widget(*e, w.clone(), b.name.clone(), b.row.as_ref().map(|(_, r)| *r), right, modifiers)),
                    };
                }
                live.entries.iter().find(|en| en.element.heard(&clicked)).map(|en| Was::Entry(en.extension, en.window.clone()))
            })
            .collect()
    });
    for was in found {
        match was {
            Was::Widget(extension, window, widget, row, right, modifiers) => with(|all| {
                if all.clicks.len() < 64 {
                    let (shift, ctrl) = (modifiers & ui::MODIFIER_SHIFT != 0, modifiers & ui::MODIFIER_CTRL != 0);
                    all.clicks.push(Click { extension, button: format!("{window}.{widget}"), right, shift, ctrl, row });
                }
            }),
            Was::Close(extension, window) => with(|all| all.show(extension, &window, false)),
            Was::Popup(number, button) => {
                let _ = super::press_popup(number, button);
            }
            Was::Shield(number, side) => {
                if let Err(e) = open_diplomacy(m, number, side) {
                    log!("windows: pop-up {number}'s shield: {e}");
                }
            }
            Was::Entry(extension, window) => with(|all| {
                let seen = all.wanted.entry(extension, &window);
                all.wanted.hear_at_frame(seen);
            }),
        }
    }
}

/// The letters of the player's country, none in observe mode with no
/// country in view.
fn player_letters(m: &Memory) -> Option<String> {
    game::context(m).ok().map(|ctx| String::from_utf8_lossy(&ctx.player.letters()).into_owned())
}

/// Opens the diplomacy view on the country of a pop-up's shield, on the
/// `side` of [`gui::SHIELDS`], as a click on the game's own shields does.
fn open_diplomacy(m: &Memory, number: u32, side: usize) -> Result<()> {
    let named = with(|all| all.wanted.open_popup(number).map(|p| p.popup.shields[side].clone()));
    let Some(letters) = named.flatten().or_else(|| player_letters(m)) else { return Ok(()) };
    let session = WorldSession::open(Caller::Remote(game::As::Player)).map_err(|e| e.message)?;
    let bytes: [u8; 3] = letters.as_bytes().try_into().map_err(|_| format!("{letters:?} is not a tag"))?;
    let tag = game::countries::named(session.m, &session.world, eu4_harmony_remote::CountryTag(bytes)).map_err(|e| e.message)?;
    OPEN_DIPLOMACY.call(m, [idler(m)?, tag.0 as usize])?;
    Ok(())
}

/// The game is killing the window `root`: when it is one the framework
/// built, its subscriptions are let go while its elements are alive, and
/// one still wanted open is built again at the next frame. Ignored while
/// the framework kills it itself, holding the lock.
pub(crate) fn killed(m: &Memory, root: usize) {
    let Ok(mut guard) = LIVE.try_lock() else { return };
    let Some(live) = guard.as_mut() else { return };
    if let Err(e) = retire_entries(m, live, |entry| entry.strip == root) {
        live.report(format!("a killed entry strip could not let go of its buttons: {e}"));
    }
    if let Err(e) = lifecycle::killed(m, live, root) {
        live.report(format!("a window the game killed could not let go of its buttons: {e}"));
    }
}

/// The session ended: every window and pop-up is killed, and the state
/// forgets them.
pub(crate) fn session_ended() {
    crate::tooltips::clear();
    if let Some(m) = crate::memory() {
        with_live(|live| {
            if let Err(e) = retire_entries(m, live, |_| true) {
                live.report(format!("entry buttons at the session's end: {e}"));
            }
            if let Err(e) = lifecycle::session_ended(m, live) {
                live.report(format!("a window at the session's end: {e}"));
            }
        });
    }
    with(|all| {
        all.wanted.end_session();
        all.clicks.clear();
    });
}

// ---------- the game's big panels and tooltips ----------

/// The game closes its big panels, or opens one of its views: the extension
/// windows that are big panels close too, but when the framework itself
/// closes them to open one.
fn panels_closing(_: &mut Call) {
    if !OPENING.load(Ordering::Relaxed) {
        with(|all| {
            let closed = all.wanted.close_panels(None);
            all.wanted.hear_at_frame(closed);
        });
    }
}

/// Passes the actual hit to the shared native presentation adapter.
fn tooltip_after(call: &mut Call) {
    if !call.faulted() && crate::thread_id() == crate::main_thread() {
        crate::tooltips::hit(call.arg(2));
    }
}

pub(crate) fn managed_tooltip(element: usize) -> Option<Vec<u8>> {
    let guard = LIVE.try_lock().ok()?;
    let live = guard.as_ref()?;
    for built in &live.built {
        if let Some(bound) = built.elements.iter().find(|b| b.tooltip_object == element) {
            let row_hidden = bound
                .row
                .as_ref()
                .is_some_and(|(list, _)| built.elements.iter().any(|parent| parent.row.is_none() && &parent.name == list && !parent.element.wanted_visible()));
            return Some(if row_hidden { Vec::new() } else { bound.element.tooltip().unwrap_or_default().to_vec() });
        }
    }
    live.entries.iter().find(|e| e.element.object == element).map(|e| e.element.tooltip().unwrap_or_default().to_vec())
}

// ---------- entry buttons ----------

/// The element names of an entry button, by its mod's prefix: the button
/// and its background.
pub fn entry_names(id: &str, window: &str) -> (String, String) {
    let prefix = crate::splice::prefix(id);
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

/// After the game laid out its panel button strip: each entry button that
/// shows is placed in the next slot, left of the game's own.
fn strip_after(call: &mut Call) {
    if call.faulted() || crate::thread_id() != crate::main_thread() {
        return;
    }
    let m = call.memory();
    let idler = call.arg(0);
    let entries = with(|all| {
        let mut out = Vec::new();
        for d in all.wanted.declared() {
            for w in d.windows.iter().filter(|w| w.entry.is_some()) {
                let set = all
                    .wanted
                    .wanted(d.extension, &w.name)
                    .and_then(|w| w.set.get(layout::ENTRY))
                    .cloned()
                    .unwrap_or_else(|| BTreeMap::from([("visible", Property::Visible(false)), ("enabled", Property::Enabled(false))]));
                let set = entry_properties(set, |key| crate::loc::mod_text(&d.id, key));
                out.push((d.extension, d.id.clone(), w.name.clone(), set, w.entry.clone()));
            }
        }
        out
    });
    if entries.is_empty() {
        return;
    }
    let Ok(mut guard) = LIVE.try_lock() else { return };
    let live = guard.get_or_insert_with(Live::default);
    if let Err(e) = place_entries(&m, live, idler, &entries) {
        live.report(format!("entry buttons: {e}"));
    }
}

fn entry_properties(mut set: BTreeMap<&'static str, Property>, words: impl Fn(&str) -> Option<String>) -> BTreeMap<&'static str, Property> {
    let text = match set.get("tooltip") {
        Some(Property::Tooltip(text)) => text.resolve(words),
        _ => String::new(),
    };
    set.insert("tooltip", Property::Tooltip(text.into()));
    set
}

type EntryWant = (usize, String, String, BTreeMap<&'static str, Property>, Option<layout::Entry>);

fn place_entries(m: &Memory, live: &mut Live, idler: usize, entries: &[EntryWant]) -> Result<()> {
    let strip = m.ptr(idler + map_manager::PANEL_BUTTONS)?;
    if strip == 0 || m.ptr(strip)? != m.at(classes::WINDOW) {
        return Ok(());
    }
    // The game's own slots in use: the first shown.
    let mut used = 0;
    for k in 1..=STRIP_SLOTS {
        let button = ui::lookup(m, strip, (window::FIND_BUTTON, functions::FIND_BUTTON), &format!("f_button_{k}"))?;
        if button == 0 || m.u8(button + element::VISIBLE)? == 0 {
            break;
        }
        used = k;
    }
    let mut slot = used as i32;
    for (extension, id, window_name, set, entry) in entries {
        let Some(entry) = entry else { continue };
        let (button_name, background_name) = entry_names(id, window_name);
        let button = ui::lookup(m, strip, (window::FIND_BUTTON, functions::FIND_BUTTON), &button_name)?;
        if button == 0 || ui::element_name(m, button)? != button_name.as_bytes() || m.ptr(button)? != m.at(classes::BUTTON) {
            continue;
        }
        let background = ui::lookup(m, strip, (window::FIND_ICON, functions::FIND_ICON), &background_name)?;
        // Bound again whenever the game made the strip anew.
        let known = live.entries.iter().position(|e| e.extension == *extension && &e.window == window_name);
        let fresh = match known {
            Some(i) => live.entries[i].element.object != button || !live.entries[i].element.subscribed(m).unwrap_or(false),
            None => true,
        };
        if fresh {
            if let Some(i) = known {
                let old = live.entries.remove(i);
                live.retired.extend(old.element.retire());
            }
            let number = ui::lookup(m, strip, (window::FIND_TEXT, functions::FIND_TEXT), &format!("{button_name}_number"))?;
            if number == 0 || m.ptr(number)? != m.at(classes::TEXT_BOX) || ui::element_name(m, number)? != format!("{button_name}_number").as_bytes() {
                return Err(format!("the entry {button_name} has no independent number"));
            }
            live.entries.push(Entry {
                extension: *extension,
                window: window_name.clone(),
                strip,
                element: listening(m, button, Look::Button)?,
                background: BoundElement::new(background, Look::Icon),
                number,
                number_text: None,
            });
        }
        let visible = set.get("visible") != Some(&Property::Visible(false));
        let frame = match set.get("frame") {
            Some(Property::Frame(f)) => *f,
            _ => entry.frame,
        };
        let bound = live.entries.iter_mut().find(|e| e.element.object == button).unwrap();
        let mut shown = Shown::of(set);
        shown.visible = Some(visible);
        shown.enabled = Some(set.get("enabled") != Some(&Property::Enabled(false)));
        shown.frame = Some(frame);
        bound.element.apply(m, &shown)?;
        if background != 0 {
            bound.background.apply(m, &Shown { visible: Some(visible), ..Default::default() })?;
        }
        let value = match set.get("value") {
            Some(Property::Number(value)) => *value,
            _ => entry.value,
        };
        let text = entry.format.show(if visible { value } else { None });
        let number_at = (STRIP_FIRST.0 - STRIP_STEP * slot - 4, STRIP_FIRST.1 + 40);
        show_entry_number(m, bound, text, number_at)?;
        if !visible {
            continue;
        }
        let at = (STRIP_FIRST.0 - STRIP_STEP * slot, STRIP_FIRST.1);
        if m.vslot(button, element::PLACE)? == m.at(functions::BUTTON_PLACE.rva()) {
            functions::BUTTON_PLACE.call(m, [button, pack(at)])?;
        }
        let place = if background != 0 { m.vslot(background, element::PLACE)? } else { 0 };
        if background != 0 && functions::ICON_PLACES.iter().any(|&k| m.at(k) == place) {
            let mut resolved = 0u64;
            functions::PLACED.call(m, [button, &mut resolved as *mut u64 as usize])?;
            let (x, y) = unpack(resolved);
            m.call(place, &[background, pack((x + STRIP_BACKGROUND.0, y + STRIP_BACKGROUND.1))])?;
        }
        slot += 1;
    }
    Ok(())
}

fn retire_entries(m: &Memory, live: &mut Live, gone: impl Fn(&Entry) -> bool) -> Result<()> {
    let (old, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut live.entries).into_iter().partition(gone);
    live.entries = kept;
    let mut result = Ok(());
    for mut entry in old {
        result = result.and(entry.element.unlink(m));
        live.retired.extend(entry.element.retire());
    }
    result
}

fn show_entry_number(m: &Memory, entry: &mut Entry, text: String, at: (i32, i32)) -> Result<()> {
    let visible = !text.is_empty();
    if entry.number_text.as_ref() != Some(&text) {
        bound_element::write_text(m, entry.number, &game_text::from_str(&text))?;
        entry.number_text = Some(text);
    }
    if (m.u8(entry.number + element::VISIBLE)? != 0) != visible {
        let (slot, function) = if visible { (element::SHOW, TEXT_SHOW) } else { (element::HIDE, TEXT_HIDE) };
        if m.vslot(entry.number, slot)? != m.at(function.rva()) {
            return Err("entry number visibility method changed".into());
        }
        function.call(m, [entry.number])?;
    }
    if visible {
        if m.vslot(entry.number, TEXT_PLACE_SLOT)? != m.at(TEXT_PLACE.rva()) {
            return Err("entry number position method changed".into());
        }
        let position = [at.0, at.1];
        TEXT_PLACE.call(m, [entry.number, position.as_ptr() as usize])?;
    }
    Ok(())
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

    #[test]
    fn an_entry_localizes_its_default_tooltip_and_keeps_explicit_overrides() {
        let windows =
            layout::read(&serde_json::json!({ "tools": { "size": [200, 200], "entry": { "sprite": "GFX_tools", "tooltip": "ENTRY" } } }), &|_| true).unwrap();
        let mut state = eu4_harmony_lua::windows::Windows::default();
        state.declare(0, "tools", &windows);
        let initial = state.wanted(0, "tools").unwrap().set[layout::ENTRY].clone();
        for title in ["Convenience Tools", "便捷工具", "便利ツール", "편의 도구"] {
            let words = |_: &str| Some(title.to_string());
            let properties = entry_properties(initial.clone(), words);
            assert_eq!(properties.get("tooltip"), Some(&Property::Tooltip(title.into())));
            for explicit in ["Changed", ""] {
                let set = BTreeMap::from([("tooltip", Property::Tooltip(explicit.into()))]);
                assert_eq!(entry_properties(set.clone(), words), set);
            }
        }
        assert_eq!(
            entry_properties(BTreeMap::new(), |_| None).get("tooltip"),
            Some(&Property::Tooltip(String::new().into())),
            "forgetting state clears any old native cache value"
        );
    }
}
