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
//! Locks: [`LIVE`] is held while the framework calls the game, so what the
//! game calls back (clicks, Kill, tooltips) takes it only with `try_lock`;
//! clicks are put in [`CLICKED`] alone and taken up at the next frame.
//! [`super::WINDOWS`] is never held while the game is called.
use super::{gui, type_name, with, Wanted};
use crate::called_by_game;
use crate::{
    game::{self, Caller, WorldSession},
    game_heap, game_string,
    hooks::Call,
    memory::Memory,
    objects::{
        campaign::{self, map_manager},
        gui::{classes, element, functions, globals, list_box, list_entry, manager, scene, window, window_gui},
    },
    points::{GameFunction, Point},
    ui::{self, Callback, Subscription},
    unicode::game_text,
    Result,
};
use eu4_harmony_lua::{
    dispatch::Click,
    layout::{self, Kind, Placed, Window},
    Popup, Property,
};
use std::collections::{BTreeMap, HashMap, HashSet};
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

/// The clicks the game reported since the last frame: the wrapper's
/// address, the button, whether it was a right click, and the keys held
/// down with it ([`ui::modifiers`]), read as it came, when they still are.
static CLICKED: Mutex<Vec<(usize, usize, bool, u32)>> = Mutex::new(Vec::new());

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
    object: usize,
    kind: Option<Kind>,
    subscription: Option<Subscription>,
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
    /// What was applied to each element, by its index in `elements`.
    applied: HashMap<usize, BTreeMap<&'static str, Property>>,
    /// The rows applied to each list.
    rows: BTreeMap<String, Vec<Vec<(String, Property)>>>,
}

/// An entry button on the panel button strip.
struct Entry {
    extension: usize,
    window: String,
    button: usize,
    subscription: Option<Subscription>,
}

#[derive(Default)]
struct Live {
    built: Vec<Built>,
    /// The generation of definitions the game has read.
    read: Option<u64>,
    /// The pop-ups whose definitions the game has read.
    popups_read: HashSet<u32>,
    entries: Vec<Entry>,
    /// The callbacks of subscriptions let go; the game may still hold
    /// pointers to them.
    retired: Vec<Callback>,
    /// What each bound element's tooltip says, game text, by the element.
    tooltips: HashMap<usize, Vec<u8>>,
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

#[called_by_game]
extern "system" fn left_click(context: usize, button: usize) {
    if let Ok(mut clicked) = CLICKED.lock() {
        if clicked.len() < 64 {
            clicked.push((context, button, false, ui::modifiers()));
        }
    }
}

#[called_by_game]
extern "system" fn right_click(context: usize, button: usize) {
    if let Ok(mut clicked) = CLICKED.lock() {
        if clicked.len() < 64 {
            clicked.push((context, button, true, ui::modifiers()));
        }
    }
}

fn subscribe(m: &Memory, object: usize) -> Result<Subscription> {
    let mut s = Subscription::new(m, object, left_click as *const () as usize, right_click as *const () as usize)?;
    s.link(m)?;
    Ok(s)
}

/// Binds the game widgets `widgets` of the window `root`, those of a row
/// with `row`, into `out`.
fn bind(m: &Memory, root: usize, widgets: &[Placed], row: Option<(String, u32)>, out: &mut Vec<Bound>) -> Result<()> {
    for (index, placed) in widgets.iter().enumerate() {
        let element = gui::element(placed, index);
        let object = find(m, root, Some(&placed.kind), &element)?;
        let subscription = match placed.kind {
            Kind::Button { .. } => Some(subscribe(m, object)?),
            _ => None,
        };
        let name = placed.name.clone().unwrap_or(element);
        out.push(Bound { name, object, kind: Some(placed.kind.clone()), subscription, row: row.clone() });
    }
    Ok(())
}

/// Lets go of a built window's subscriptions while its elements are alive.
fn unlink(m: &Memory, live: &mut Live, elements: &mut Vec<Bound>) -> Result<()> {
    let mut result = Ok(());
    for b in elements.drain(..) {
        live.tooltips.remove(&b.object);
        if let Some(mut s) = b.subscription {
            result = result.and(s.unlink(m));
            live.retired.push(s.wrapper);
        }
    }
    result
}

/// Kills a built window, its subscriptions let go first.
fn tear_down(m: &Memory, live: &mut Live, mut built: Built) -> Result<()> {
    unlink(m, live, &mut built.elements)?;
    show_window(m, built.root, false)?;
    // Checked to be the game's Kill as the window was built.
    let kill = m.vslot(built.root, window::KILL)?;
    m.call(kill, &[built.root]).map(|_| ())
}

// ---------- applying what is wanted ----------

/// What a game widget shows at first, from its layout.
fn initial(placed: &Placed, words: &dyn Fn(&Option<String>) -> String) -> BTreeMap<&'static str, Property> {
    let mut out = BTreeMap::new();
    let mut put = |p: Property| {
        out.insert(p.name(), p);
    };
    put(Property::Visible(placed.visible));
    if placed.tooltip.is_some() {
        put(Property::Tooltip(words(&placed.tooltip)));
    }
    match &placed.kind {
        Kind::Text { text, .. } => put(Property::Text(words(text))),
        Kind::Icon { frame, .. } => put(Property::Frame(*frame)),
        Kind::Button { frame, sprite: Some(_), .. } => put(Property::Frame(*frame)),
        Kind::Progress { value, .. } => put(Property::Value(*value)),
        Kind::Flag { country: Some(c), .. } => put(Property::Country(c.clone())),
        _ => {}
    }
    out
}

/// Calls the element's method in `slot` with `arg`, when it is one of
/// `known`.
fn method(m: &Memory, object: usize, slot: usize, known: &[usize], arg: usize) -> Result<()> {
    let at = m.vslot(object, slot)?;
    if !known.iter().any(|&k| m.at(k) == at) {
        return Err(format!("an element's method at {slot:#x} is not one the framework knows"));
    }
    m.call(at, &[object, arg]).map(|_| ())
}

/// Sets one property on a bound element.
fn apply_one(m: &Memory, live: &mut Live, b: &Bound, property: &Property, all: &BTreeMap<&'static str, Property>) -> Result<()> {
    let button = matches!(b.kind, Some(Kind::Button { .. } | Kind::Flag { .. }));
    let icon = matches!(b.kind, Some(Kind::Icon { .. } | Kind::Progress { .. }));
    match property {
        Property::Text(words) => {
            // A text box hides by showing nothing.
            let shown = all.get("visible") != Some(&Property::Visible(false));
            let words = if shown { game_text::from_str(words) } else { Vec::new() };
            write_text(m, b.object, &words)
        }
        Property::Visible(on) if matches!(b.kind, Some(Kind::Text { .. })) => {
            let words = match (on, all.get("text")) {
                (true, Some(Property::Text(w))) => game_text::from_str(w),
                _ => Vec::new(),
            };
            write_text(m, b.object, &words)
        }
        Property::Visible(on) if button || icon => {
            let (slot, known): (usize, &[usize]) = match (on, button) {
                (true, true) => (element::SHOW, &[ui::SHOW]),
                (false, true) => (element::HIDE, &[ui::HIDE]),
                (true, false) => (element::SHOW, &functions::ICON_SHOWS),
                (false, false) => (element::HIDE, &functions::ICON_HIDES),
            };
            let at = m.vslot(b.object, slot)?;
            if !known.iter().any(|&k| m.at(k) == at) {
                return Err("an element's show or hide is not one the framework knows".into());
            }
            m.call(at, &[b.object]).map(|_| ())
        }
        Property::Visible(_) => Ok(()),
        Property::Enabled(on) => method(m, b.object, element::ENABLE, &[ui::SET_ENABLED.rva()], *on as usize),
        Property::Frame(frame) => {
            let known: &[usize] = if button { &[functions::BUTTON_FRAME] } else { &functions::ICON_FRAMES };
            method(m, b.object, element::FRAME, known, *frame as usize)
        }
        Property::Value(value) => method(m, b.object, element::FRAME, &functions::ICON_FRAMES, (value.clamp(0.0, 1.0) * 100.0) as usize),
        Property::Country(letters) => set_country(m, b.object, letters),
        Property::Tooltip(words) => {
            live.tooltips.insert(b.object, game_text::from_str(words));
            Ok(())
        }
    }
}

fn write_text(m: &Memory, object: usize, bytes: &[u8]) -> Result<()> {
    let mut string = game_string::empty();
    if !bytes.is_empty() {
        game_string::append_checked(m, string.as_mut_ptr() as usize, bytes)?;
    }
    ui::TEXT_SET.call(m, [object, string.as_mut_ptr() as usize, 0])?;
    // The text box took the text; what is left is empty, still ours.
    game_string::destroy_checked(m, string.as_mut_ptr() as usize)
}

/// Gives a flag its country, which must be one of the game's: the game
/// takes the tag's index into its countries unchecked.
fn set_country(m: &Memory, object: usize, letters: &str) -> Result<()> {
    if letters.is_empty() {
        return Ok(());
    }
    let session = WorldSession::open(Caller::Remote(game::As::Player)).map_err(|e| e.message)?;
    let bytes: [u8; 3] = letters.as_bytes().try_into().map_err(|_| format!("{letters:?} is not a tag"))?;
    let tag = game::countries::named(session.m, &session.world, eu4_harmony_remote::CountryTag(bytes)).map_err(|e| e.message)?;
    let argument: [u64; 2] = [tag.0, 1];
    for (slot, function) in [(crate::objects::gui::button::SET_COUNTRY, functions::SET_COUNTRY.rva()), (element::INDEX, functions::SET_INDEX.rva())] {
        if m.vslot(object, slot)? != m.at(function) {
            return Err("a flag's method is not the one the framework knows".into());
        }
    }
    functions::SET_COUNTRY.call(m, [object, argument.as_ptr() as usize])?;
    functions::SET_INDEX.call(m, [object, tag.index() as i32 as u32 as usize])?;
    Ok(())
}

/// Applies what differs between `want` and what element `index` shows.
fn apply_element(m: &Memory, live: &mut Live, built: &mut Built, index: usize, want: &BTreeMap<&'static str, Property>) -> Result<()> {
    let applied = built.applied.entry(index).or_default().clone();
    for (name, property) in want {
        if applied.get(name) == Some(property) {
            continue;
        }
        apply_one(m, live, &built.elements[index], property, want)?;
        built.applied.entry(index).or_default().insert(name, property.clone());
    }
    Ok(())
}

/// Builds a list's rows again: every row deleted, and each added and set.
fn fill_rows(
    m: &Memory,
    live: &mut Live,
    built: &mut Built,
    list: &str,
    layout: &Window,
    generation_type: &str,
    rows: &[Vec<(String, Property)>],
) -> Result<()> {
    let Some(Placed { kind: Kind::List { row, .. }, .. }) = layout.widget(list) else { return Ok(()) };
    let Some(list_index) = built.elements.iter().position(|b| b.name == list && b.row.is_none()) else { return Ok(()) };
    let list_box = built.elements[list_index].object;
    // The rows' elements go with them: let go of them first.
    let (mut old, kept): (Vec<Bound>, Vec<Bound>) =
        std::mem::take(&mut built.elements).into_iter().partition(|b| b.row.as_ref().is_some_and(|(l, _)| l == list));
    built.elements = kept;
    built.applied.retain(|i, _| *i < built.elements.len());
    unlink(m, live, &mut old)?;
    for (slot, function) in [(list_box::CLEAR, functions::LIST_CLEAR.rva()), (list_box::ADD, functions::LIST_ADD.rva())] {
        if m.vslot(list_box, slot)? != m.at(function) {
            return Err("a list box's method is not the one the framework knows".into());
        }
    }
    functions::LIST_CLEAR.call(m, [list_box])?;
    let mgr = m.ptr(m.at(globals::MANAGER))?;
    let row_type = gui::row_type(generation_type, list);
    for (n, cells) in rows.iter().enumerate() {
        let entry = game_heap::new(m, list_entry::SIZE)?;
        with_string(m, row_type.as_bytes(), |t| functions::LIST_ENTRY.call(m, [entry, mgr, t]))?;
        let row_window = m.ptr(entry + list_entry::WINDOW)?;
        if row_window == 0 {
            return Err(format!("the game built no row {row_type}"));
        }
        functions::LIST_ADD.call(m, [list_box, entry, 1])?;
        let first = built.elements.len();
        bind(m, row_window, &row.widgets, Some((list.to_string(), n as u32 + 1)), &mut built.elements)?;
        let words = |_: &Option<String>| String::new();
        for (i, placed) in row.widgets.iter().enumerate() {
            let mut want = initial(placed, &words);
            for (name, property) in cells.iter().filter(|(name, _)| Some(name) == placed.name.as_ref()) {
                let _ = name;
                want.insert(property.name(), property.clone());
            }
            apply_element(m, live, built, first + i, &want)?;
        }
    }
    built.rows.insert(list.to_string(), rows.to_vec());
    Ok(())
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
        for d in &all.declared {
            for w in &d.windows {
                let type_name = format!("{}_{}", type_name(&d.id, &w.name), all.generation);
                definitions.push(gui::window(&type_name, w, &|k| super::words_of(d, k)));
                let wanted = all.wanted.get(&(d.extension, w.name.clone())).cloned().unwrap_or_default();
                windows.push(WantWindow { extension: d.extension, type_name, layout: w.clone(), words: d.words.clone(), wanted });
            }
        }
        let popups = all.popups.iter().map(|p| (p.number, p.popup.clone())).collect();
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
        if let Err(e) = sync(m, live, &wants) {
            live.report(e);
        }
    });
}

/// The clicks since the last frame, as what extensions hear and what opens,
/// closes and answers.
fn clicks(m: &Memory) {
    let clicked = std::mem::take(&mut *CLICKED.lock().unwrap_or_else(|e| e.into_inner()));
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
            .filter_map(|(context, button, right, modifiers)| {
                for built in &live.built {
                    let Some(b) = built.elements.iter().find(|b| b.object == button && b.subscription.as_ref().is_some_and(|s| s.context() == context)) else {
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
                live.entries
                    .iter()
                    .find(|en| en.button == button && en.subscription.as_ref().is_some_and(|s| s.context() == context))
                    .map(|en| Was::Entry(en.extension, en.window.clone()))
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
            Was::Popup(number, button) => super::press(number, button),
            Was::Shield(number, side) => {
                if let Err(e) = open_diplomacy(m, number, side) {
                    log!("windows: pop-up {number}'s shield: {e}");
                }
            }
            Was::Entry(extension, window) => with(|all| {
                let open = all.wanted.get(&(extension, window.clone())).is_some_and(|w| w.open);
                all.show(extension, &window, !open);
            }),
        }
    }
}

fn sync(m: &Memory, live: &mut Live, wants: &Wants) -> Result<()> {
    let mgr = m.ptr(m.at(globals::MANAGER))?;
    if mgr == 0 {
        return Ok(());
    }
    // Definitions of another generation: its windows go, the new are read.
    if live.read != Some(wants.generation) {
        let (old, popups): (Vec<Built>, Vec<Built>) = std::mem::take(&mut live.built).into_iter().partition(|b| matches!(b.key, Key::Window(..)));
        live.built = popups;
        for b in old {
            tear_down(m, live, b)?;
        }
        if !wants.definitions.is_empty() {
            read_definitions(m, mgr, WINDOWS_FILE, gui::file(&wants.definitions))?;
        }
        live.read = Some(wants.generation);
    }
    // What is no longer wanted goes.
    let open_popups: HashSet<u32> = wants.popups.iter().map(|(n, _)| *n).collect();
    let (gone, kept): (Vec<Built>, Vec<Built>) = std::mem::take(&mut live.built).into_iter().partition(|b| match &b.key {
        Key::Popup(n) => !open_popups.contains(n),
        Key::Window(e, w) => !wants.windows.iter().any(|ww| ww.extension == *e && &ww.layout.name == w && ww.wanted.open),
    });
    live.built = kept;
    for b in gone {
        tear_down(m, live, b)?;
    }
    let screen = screen(m, mgr)?;
    // Windows wanted open and not built are built; what is wanted applied.
    for want in &wants.windows {
        let key = Key::Window(want.extension, want.layout.name.clone());
        // A window the game would not build is not tried again until its
        // definitions are read anew.
        if want.wanted.open && !live.built.iter().any(|b| b.key == key) && !live.failed.contains(&want.type_name) {
            match build_window(m, mgr, live, key.clone(), want, screen) {
                Ok(built) => live.built.push(built),
                Err(e) => {
                    live.failed.insert(want.type_name.clone());
                    live.report(format!("{} not built: {e}", want.layout.name));
                }
            }
        }
        let Some(at) = live.built.iter().position(|b| b.key == key) else { continue };
        let mut built = live.built.remove(at);
        let result = apply_window(m, live, &mut built, want);
        // Where the player dragged it.
        if built.moveable {
            let now = unpack(m.u64(built.root + window::POSITION)?);
            if now != built.placed {
                built.placed = now;
                with(|all| {
                    if let Some(w) = all.wanted.get_mut(&(want.extension, want.layout.name.clone())) {
                        w.dragged = Some(now);
                    }
                });
            }
        }
        live.built.push(built);
        result?;
    }
    // Pop-ups not built yet: each read and built, cascading.
    for (number, popup) in &wants.popups {
        if live.built.iter().any(|b| b.key == Key::Popup(*number)) {
            continue;
        }
        let name = format!("eu4h_popup_{number}");
        if live.popups_read.insert(*number) {
            read_definitions(m, mgr, POPUPS_FILE, gui::file(&[gui::popup(&name, popup)]))?;
        }
        let shown = live.built.iter().filter(|b| matches!(b.key, Key::Popup(_))).count() as i32 % 8;
        match build_popup(m, mgr, *number, popup, &name, screen, shown) {
            Ok(built) => live.built.push(built),
            // Closed: the player cannot see it to press its buttons.
            Err(e) => {
                live.report(format!("pop-up {number} not built: {e}"));
                with(|all| all.popups.retain(|p| p.number != *number));
            }
        }
    }
    Ok(())
}

fn build_window(m: &Memory, mgr: usize, live: &mut Live, key: Key, want: &WantWindow, screen: (i32, i32)) -> Result<Built> {
    let layout = &want.layout;
    let root = create(m, mgr, &want.type_name)?;
    if layout.panel {
        let idler = idler(m)?;
        OPENING.store(true, Ordering::Relaxed);
        let closed = CLOSE_VIEWS.call(m, [idler]).and_then(|_| CLOSE_PANELS.call(m, [idler]));
        OPENING.store(false, Ordering::Relaxed);
        closed?;
    }
    let at = want.wanted.dragged.unwrap_or_else(|| place(layout, screen));
    set_position(m, root, at)?;
    let mut built = Built { key, root, placed: at, moveable: layout.moveable, elements: Vec::new(), applied: HashMap::new(), rows: BTreeMap::new() };
    let bound = (|| {
        if layout.close {
            let close = find(m, root, None, gui::CLOSE)?;
            built.elements.push(Bound { name: gui::CLOSE.into(), object: close, kind: None, subscription: Some(subscribe(m, close)?), row: None });
        }
        // Nothing on a window that does not move is pressed by a press on
        // its background.
        if !layout.moveable {
            ui::keep_still(m, root, gui::BACKGROUND)?;
        }
        bind(m, root, &layout.widgets, None, &mut built.elements)
    })();
    if let Err(e) = bound {
        let _ = tear_down(m, live, built);
        return Err(e);
    }
    show_window(m, root, true)?;
    Ok(built)
}

fn apply_window(m: &Memory, live: &mut Live, built: &mut Built, want: &WantWindow) -> Result<()> {
    let words = |k: &Option<String>| k.as_ref().and_then(|k| want.words.get(k)).cloned().unwrap_or_default();
    for (index, placed) in want.layout.widgets.iter().enumerate() {
        let name = placed.name.clone().unwrap_or_else(|| gui::element(placed, index));
        let Some(at) = built.elements.iter().position(|b| b.name == name && b.row.is_none()) else { continue };
        if matches!(placed.kind, Kind::List { .. }) {
            let rows = want.wanted.rows.get(&name).cloned().unwrap_or_default();
            if built.rows.get(&name) != Some(&rows) {
                fill_rows(m, live, built, &name, &want.layout, &want.type_name, &rows)?;
            }
            continue;
        }
        let mut wanted = initial(placed, &words);
        if let Some(set) = want.wanted.set.get(&name) {
            wanted.extend(set.iter().map(|(k, v)| (*k, v.clone())));
        }
        apply_element(m, live, built, at, &wanted)?;
    }
    Ok(())
}

fn build_popup(m: &Memory, mgr: usize, number: u32, popup: &Popup, name: &str, (sw, sh): (i32, i32), shown: i32) -> Result<Built> {
    let root = create(m, mgr, name)?;
    let x = (sw - gui::POPUP_WIDTH) / 2 + CASCADE * shown;
    let y = (sh - gui::popup_height(popup)) / 2 + CASCADE * shown;
    set_position(m, root, (x, y))?;
    let text = find(m, root, Some(&Kind::Text { text: None, font: layout::Font::Body, align: layout::Align::Start }), gui::TEXT)?;
    write_text(m, text, &game_text::from_str(&popup.text))?;
    // The shields: the countries named, the player's where none is, and
    // none to show in observe mode with no country in view. A click on one
    // opens the diplomacy view on its country, as on the game's.
    let mut elements = Vec::new();
    for (element, tag) in gui::SHIELDS.iter().zip(&popup.shields) {
        let Some(tag) = tag.clone().or_else(|| player_letters(m)) else { continue };
        let shield = find(m, root, None, element)?;
        if let Err(e) = set_country(m, shield, &tag) {
            log!("windows: pop-up {number}'s shield {tag}: {e}");
            continue;
        }
        elements.push(Bound { name: element.to_string(), object: shield, kind: None, subscription: Some(subscribe(m, shield)?), row: None });
    }
    for (name, _) in gui::shown(popup) {
        let object = find(m, root, None, &name)?;
        elements.push(Bound { name, object, kind: None, subscription: Some(subscribe(m, object)?), row: None });
    }
    show_window(m, root, true)?;
    Ok(Built { key: Key::Popup(number), root, placed: (x, y), moveable: true, elements, applied: HashMap::new(), rows: BTreeMap::new() })
}

/// The letters of the player's country, none in observe mode with no
/// country in view.
fn player_letters(m: &Memory) -> Option<String> {
    game::context(m).ok().map(|ctx| String::from_utf8_lossy(&ctx.player.letters()).into_owned())
}

/// Opens the diplomacy view on the country of a pop-up's shield, on the
/// `side` of [`gui::SHIELDS`], as a click on the game's own shields does.
fn open_diplomacy(m: &Memory, number: u32, side: usize) -> Result<()> {
    let named = with(|all| all.popups.iter().find(|p| p.number == number).map(|p| p.popup.shields[side].clone()));
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
    let Some(at) = live.built.iter().position(|b| b.root == root) else { return };
    let mut built = live.built.remove(at);
    if let Err(e) = unlink(m, live, &mut built.elements) {
        live.report(format!("a window the game killed could not let go of its buttons: {e}"));
    }
}

/// The session ended: every window and pop-up is killed, and the state
/// forgets them.
pub(crate) fn session_ended() {
    if let Some(m) = crate::memory() {
        with_live(|live| {
            for b in std::mem::take(&mut live.built) {
                if let Err(e) = tear_down(m, live, b) {
                    live.report(format!("a window at the session's end: {e}"));
                }
            }
        });
    }
    with(|all| all.end_session());
}

// ---------- the game's big panels and tooltips ----------

/// The game closes its big panels, or opens one of its views: the extension
/// windows that are big panels close too, but when the framework itself
/// closes them to open one.
fn panels_closing(_: &mut Call) {
    if !OPENING.load(Ordering::Relaxed) {
        with(|all| all.close_panels(None));
    }
}

/// Fills the tooltip of an extension window's element under the pointer.
fn tooltip_after(call: &mut Call) {
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    let m = call.memory();
    let (out, element) = (call.arg(1), call.arg(2));
    let Ok(guard) = LIVE.try_lock() else { return };
    let Some(text) = guard.as_ref().and_then(|l| l.tooltips.get(&element)).filter(|t| !t.is_empty()).cloned() else { return };
    drop(guard);
    let empty = |offset: usize| game_string::untouched(&m, out + offset);
    if let (Ok(true), Ok(true)) = (empty(0), empty(game_string::OBJECT)) {
        if let Err(e) = game_string::append_checked(&m, out, &text) {
            log!("windows: tooltip append failed: {e}");
        }
    }
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
         \t\tguiButtonType = {{ name = \"{button}\" position = {{ x = -360 y = -58 }} quadTextureSprite = \"{}\" frame = {} Orientation = \"LOWER_RIGHT\" clicksound = click }}\n",
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
        for d in &all.declared {
            for w in d.windows.iter().filter(|w| w.entry.is_some()) {
                let set = all.wanted.get(&(d.extension, w.name.clone())).and_then(|w| w.set.get(layout::ENTRY)).cloned().unwrap_or_default();
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
            Some(i) => live.entries[i].button != button || !live.entries[i].subscription.as_ref().is_some_and(|s| s.subscribed(m).unwrap_or(false)),
            None => true,
        };
        if fresh {
            if let Some(i) = known {
                let old = live.entries.remove(i);
                live.retired.extend(old.subscription.map(|s| s.wrapper));
            }
            live.entries.push(Entry { extension: *extension, window: window_name.clone(), button, subscription: Some(subscribe(m, button)?) });
        }
        let visible = set.get("visible") != Some(&Property::Visible(false));
        let frame = match set.get("frame") {
            Some(Property::Frame(f)) => *f,
            _ => entry.frame,
        };
        let tooltip = match set.get("tooltip") {
            Some(Property::Tooltip(t)) => Some(game_text::from_str(t)),
            _ => None,
        };
        if let Some(t) = tooltip {
            live.tooltips.insert(button, t);
        }
        let shown = m.u8(button + element::VISIBLE)? != 0;
        if visible != shown {
            let (slot_at, known) = if visible { (element::SHOW, ui::SHOW) } else { (element::HIDE, ui::HIDE) };
            let at = m.vslot(button, slot_at)?;
            if at == m.at(known) {
                m.call(at, &[button])?;
            }
            if background != 0 {
                let known: &[usize] = if visible { &functions::ICON_SHOWS } else { &functions::ICON_HIDES };
                let at = m.vslot(background, slot_at)?;
                if known.iter().any(|&k| m.at(k) == at) {
                    m.call(at, &[background])?;
                }
            }
        }
        if !visible {
            continue;
        }
        let frame_at = m.vslot(button, element::FRAME)?;
        if frame_at == m.at(functions::BUTTON_FRAME) {
            m.call(frame_at, &[button, frame as usize])?;
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

pub fn points() -> Vec<Point> {
    vec![
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
