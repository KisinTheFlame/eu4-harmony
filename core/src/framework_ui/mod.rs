//! The framework UI (GLOSSARY, ADR framework-draws-its-own-ui): windows the
//! framework draws itself on top of the game's picture, with widgets of its
//! own: the notice window, the settings window and the mods window, and in the
//! main menu the corner buttons that open those two.
//!
//! [`frame`] runs once a frame on the main thread, just before the game
//! presents (`hook`); the window procedure ([`input`]) hands it the
//! mouse and keyboard on the window's thread. The two share it under a lock.
pub mod d3d9;
pub mod hook;
pub mod input;
pub mod mods_window;
pub mod text;
pub mod widgets;

use crate::settings::{self, Role, Settings};
use crate::speed::{self, Pace};
use crate::unicode::system_font::{self, Face, SystemFonts};
use crate::{lang, loc};
use eu4_harmony_loc::Language;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::Mutex;
use widgets::{Action, Command, Measure, Rect, Side, Widget, Window};

fn words(key: &str) -> String {
    loc::framework(lang::player(), key, &[])
}

fn words_with(key: &str, args: &[(&str, &str)]) -> String {
    loc::framework(lang::player(), key, args)
}

struct FrameworkUi {
    font: text::UiFont,
    backend: d3d9::Backend,
    /// The screen and the mouse in GUI units, `scale` pixels each.
    screen: (f32, f32),
    mouse: (f32, f32),
    scale: f32,
    /// The notice on screen, by its number.
    notice: Option<(u64, Window)>,
    settings: Option<Window>,
    mods: Option<Window>,
    /// The system fonts the settings window offers.
    system_fonts: Option<SystemFonts>,
    /// Families offered in the settings window, by English name, by their
    /// drop-down list.
    families: HashMap<&'static str, Vec<String>>,
    /// Weights offered in the settings window, by their drop-down list.
    weights: HashMap<&'static str, Vec<u16>>,
    /// The face each role showed when the settings window opened: one not
    /// chosen that the player leaves as it is goes on following the language.
    shown_faces: HashMap<Role, Face>,
}

/// A font role's drop-down lists in the settings window: its family's and
/// its weight's.
fn ids(role: Role) -> (&'static str, &'static str) {
    match role {
        Role::Body => ("body", "body_weight"),
        Role::Heading => ("heading", "heading_weight"),
        Role::Map => ("map", "map_weight"),
        Role::Console => ("console", "console_weight"),
    }
}

/// The weights offered for `face`'s family, their names, and which of them
/// draws `face`. A family not installed offers the weight chosen for it.
fn weight_options(fonts: Option<&SystemFonts>, face: &Face) -> (Vec<u16>, Vec<String>, usize) {
    let offered = fonts.map(|f| f.weights(&face.family)).unwrap_or_default();
    if offered.is_empty() {
        return (vec![face.weight], vec![system_font::weight_name(face.weight).to_string()], 0);
    }
    let drawn = fonts.and_then(|f| f.drawn(face)).map_or(face.weight, |d| d.weight);
    let selected = offered.iter().position(|(w, _)| *w == drawn).unwrap_or(0);
    let (weights, names) = offered.into_iter().unzip();
    (weights, names, selected)
}

// Its Direct3D objects are used only on the main thread; DirectWrite, which
// the window's thread reaches through the font, is free-threaded.
unsafe impl Send for FrameworkUi {}

static UI: Mutex<Option<FrameworkUi>> = Mutex::new(None);
static FAILED: AtomicBool = AtomicBool::new(false);
static FRAME: AtomicU64 = AtomicU64::new(0);
/// The device the game presented its last frame with, for [`still`].
static DEVICE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static MAIN_MENU_FRAME: AtomicU64 = AtomicU64::new(u64::MAX);
static MODAL: AtomicBool = AtomicBool::new(false);

/// The main menu drew this frame; its corner button shows while it does.
pub fn main_menu_frame() {
    MAIN_MENU_FRAME.store(FRAME.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn in_main_menu() -> bool {
    let menu = MAIN_MENU_FRAME.load(Ordering::Relaxed);
    menu != u64::MAX && FRAME.load(Ordering::Relaxed).saturating_sub(menu) <= 2
}

/// Whether a modal window (the settings window, the mods window) is open.
pub fn modal() -> bool {
    MODAL.load(Ordering::Relaxed)
}

/// What a corner button opens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opens {
    Settings,
    Mods,
}

/// The main menu's corner buttons, right to left: each one's place, label
/// and what it opens.
fn corner(screen: (f32, f32), measure: &dyn Measure) -> Vec<(Rect, String, Opens)> {
    let mut right = screen.0 - 16.0;
    [("EU4HARMONY_SETTINGS_BUTTON", Opens::Settings), ("EU4HARMONY_MODS_BUTTON", Opens::Mods)]
        .into_iter()
        .map(|(key, opens)| {
            let label = words(key);
            let w = measure.width(&label, widgets::TEXT_SIZE) + 24.0;
            right -= w;
            let rect = Rect { x: right, y: screen.1 - 44.0, w, h: 30.0 };
            right -= 8.0;
            (rect, label, opens)
        })
        .collect()
}

/// The settings window's width, and its side list's, which chooses among
/// its pages.
const SETTINGS_WIDTH: f32 = 600.0;
const SETTINGS_LIST: f32 = 168.0;

/// The choice bar's options for the pace at the fastest speed, in order.
const PACES: [Pace; 2] = [Pace::Smooth, Pace::Faster];

impl FrameworkUi {
    fn new() -> crate::Result<Self> {
        let face = SystemFonts::new().map(|fonts| settings::current().face(Role::Body, |f| fonts.has_family(f)))?;
        let font = text::UiFont::new(&face)?;
        Ok(Self {
            font,
            backend: d3d9::Backend::default(),
            screen: (0.0, 0.0),
            mouse: (-1.0, -1.0),
            scale: 1.0,
            notice: None,
            settings: None,
            mods: None,
            system_fonts: None,
            families: HashMap::new(),
            weights: HashMap::new(),
            shown_faces: HashMap::new(),
        })
    }

    fn open_settings(&mut self) {
        let next = settings::next();
        let fonts = SystemFonts::new().ok();
        let language = next.language();
        // Each role's face as the window shows it: in the weight that draws it.
        let faces: HashMap<Role, Face> = Role::ALL
            .into_iter()
            .map(|role| {
                let face = next.face(role, |f| fonts.as_ref().is_some_and(|fonts| fonts.has_family(f)));
                (role, fonts.as_ref().and_then(|fonts| fonts.drawn(&face)).unwrap_or(face))
            })
            .collect();
        // The installed families that draw `text`, by English name and as
        // shown in the language, and those shown that do not.
        let offered = |text: &str, roles: &[Role]| {
            let offered = fonts.as_ref().map(|f| f.families_with(text, language)).unwrap_or_default();
            let (mut families, mut shown): (Vec<String>, Vec<String>) = offered.into_iter().unzip();
            for family in roles.iter().flat_map(|role| next.chosen(*role).into_iter().chain([&faces[role]])).map(|face| &face.family) {
                if !families.contains(family) {
                    families.push(family.clone());
                    shown.push(family.clone());
                }
            }
            (families, shown)
        };
        // Fonts that write the language's name; the game console's needs
        // only Latin letters, as other fonts draw the rest.
        let (families, shown) = offered(language.native_name(), &[Role::Body, Role::Heading, Role::Map]);
        let (console_families, console_shown) = offered("A", &[Role::Console]);
        let index = |families: &[String], role: Role| families.iter().position(|f| *f == faces[&role].family).unwrap_or(0);
        let (body_weights, body_names, body_weight) = weight_options(fonts.as_ref(), &faces[&Role::Body]);
        let (heading_weights, heading_names, heading_weight) = weight_options(fonts.as_ref(), &faces[&Role::Heading]);
        let (map_weights, map_names, map_weight) = weight_options(fonts.as_ref(), &faces[&Role::Map]);
        let (console_weights, console_names, console_weight) = weight_options(fonts.as_ref(), &faces[&Role::Console]);
        let fonts_page = vec![
            Widget::Label(words("EU4HARMONY_SETTINGS_LANGUAGE")),
            Widget::Dropdown {
                id: "language",
                options: Language::ALL.iter().map(|l| l.native_name().to_string()).collect(),
                selected: Language::ALL.iter().position(|&l| l == language).unwrap_or(0),
            },
            Widget::Label(words("EU4HARMONY_SETTINGS_BODY_FONT")),
            Widget::Dropdown { id: "body", options: shown.clone(), selected: index(&families, Role::Body) },
            Widget::Label(words("EU4HARMONY_SETTINGS_BODY_WEIGHT")),
            Widget::Dropdown { id: "body_weight", options: body_names, selected: body_weight },
            Widget::Label(words("EU4HARMONY_SETTINGS_HEADING_FONT")),
            Widget::Dropdown { id: "heading", options: shown.clone(), selected: index(&families, Role::Heading) },
            Widget::Label(words("EU4HARMONY_SETTINGS_HEADING_WEIGHT")),
            Widget::Dropdown { id: "heading_weight", options: heading_names, selected: heading_weight },
            Widget::Label(words("EU4HARMONY_SETTINGS_MAP_FONT")),
            Widget::Dropdown { id: "map", options: shown, selected: index(&families, Role::Map) },
            Widget::Label(words("EU4HARMONY_SETTINGS_MAP_WEIGHT")),
            Widget::Dropdown { id: "map_weight", options: map_names, selected: map_weight },
            Widget::Toggle { id: "outline", label: words("EU4HARMONY_SETTINGS_MAP_OUTLINE"), on: next.map_outline },
            Widget::Label(words("EU4HARMONY_SETTINGS_CONSOLE_FONT")),
            Widget::Dropdown { id: "console", options: console_shown, selected: index(&console_families, Role::Console) },
            Widget::Label(words("EU4HARMONY_SETTINGS_CONSOLE_WEIGHT")),
            Widget::Dropdown { id: "console_weight", options: console_names, selected: console_weight },
            Widget::Label(words("EU4HARMONY_SETTINGS_RESTART")),
        ];
        let sessions_page = vec![
            Widget::Toggle { id: "return", label: words("EU4HARMONY_SETTINGS_RETURN_WITHOUT_RESTART"), on: next.return_without_restart },
            Widget::Toggle { id: "ironman_save", label: words("EU4HARMONY_SETTINGS_SAVE_IRONMAN_BEFORE_LOAD"), on: next.save_ironman_before_load },
            Widget::Label(words("EU4HARMONY_SETTINGS_UNDO")),
        ];
        let achievements_page = vec![
            Widget::Toggle { id: "outside_ironman", label: words("EU4HARMONY_SETTINGS_ACHIEVEMENTS_OUTSIDE_IRONMAN"), on: next.achievements_outside_ironman },
            Widget::Toggle {
                id: "changed_checksum",
                label: words("EU4HARMONY_SETTINGS_ACHIEVEMENTS_CHANGED_CHECKSUM"),
                on: next.achievements_with_changed_checksum,
            },
            Widget::Label(words("EU4HARMONY_SETTINGS_ACHIEVEMENTS")),
        ];
        let speed_page = vec![
            Widget::Explained { label: words("EU4HARMONY_SETTINGS_FASTEST_SPEED"), info: words("EU4HARMONY_SETTINGS_FASTEST_SPEED_INFO") },
            Widget::Choice {
                id: "pace",
                options: vec![words("EU4HARMONY_SETTINGS_PACE_SMOOTH"), words("EU4HARMONY_SETTINGS_PACE_FASTER")],
                selected: PACES.iter().position(|&p| p == next.fastest_speed).unwrap_or(0),
            },
        ];
        let remote_page = vec![
            Widget::Toggle { id: "remote_full_view", label: words("EU4HARMONY_SETTINGS_REMOTE_FULL_VIEW"), on: next.remote_full_view },
            Widget::Label(words("EU4HARMONY_SETTINGS_REMOTE_FULL_VIEW_INFO")),
        ];
        let footer = vec![
            Widget::Gap,
            Widget::Button { id: "save", label: words("EU4HARMONY_SETTINGS_SAVE") },
            Widget::Button { id: "cancel", label: words("EU4HARMONY_SETTINGS_CANCEL") },
        ];
        let pages = [
            "EU4HARMONY_SETTINGS_PAGE_LANGUAGE_FONTS",
            "EU4HARMONY_SETTINGS_PAGE_SESSIONS",
            "EU4HARMONY_SETTINGS_PAGE_ACHIEVEMENTS",
            "EU4HARMONY_SETTINGS_PAGE_GAME_SPEED",
            "EU4HARMONY_SETTINGS_PAGE_REMOTE_CONTROL",
        ];
        let side = Side::new("pages", pages.iter().map(|key| words(key)).collect(), SETTINGS_LIST);
        let mut window = Window::new(words("EU4HARMONY_SETTINGS_TITLE"), Vec::new(), SETTINGS_WIDTH)
            .with_side(side)
            .with_pages(vec![fonts_page, sessions_page, achievements_page, speed_page, remote_page])
            .with_footer(footer);
        window.center(&self.font, self.screen.0, self.screen.1);
        self.families = HashMap::from([("body", families.clone()), ("heading", families.clone()), ("map", families), ("console", console_families)]);
        self.weights = HashMap::from([
            ("body_weight", body_weights),
            ("heading_weight", heading_weights),
            ("map_weight", map_weights),
            ("console_weight", console_weights),
        ]);
        self.system_fonts = fonts;
        self.shown_faces = faces;
        self.settings = Some(window);
        MODAL.store(true, Ordering::Relaxed);
    }

    /// The player chose the family at `index` in the drop-down list `id`:
    /// its weight list becomes the new family's, keeping the weight chosen
    /// when it has it, else the one below.
    fn family_chosen(&mut self, id: &str, index: usize) {
        let weight_id = match id {
            "body" => "body_weight",
            "heading" => "heading_weight",
            "console" => "console_weight",
            _ => "map_weight",
        };
        let (Some(family), Some(window)) = (self.families.get(id).and_then(|f| f.get(index)), self.settings.as_mut()) else { return };
        let chosen = window.selected(weight_id).and_then(|i| self.weights.get(weight_id)?.get(i).copied()).unwrap_or(system_font::REGULAR);
        let (weights, names, selected) = weight_options(self.system_fonts.as_ref(), &Face::new(family.clone(), chosen));
        window.set_options(weight_id, names, selected);
        self.weights.insert(weight_id, weights);
    }

    fn close_settings(&mut self, save: bool) {
        if let (true, Some(w)) = (save, &self.settings) {
            let next = settings::next();
            let family = |id| w.selected(id).and_then(|i| self.families.get(id)?.get(i)).cloned();
            let weight = |id: &str| w.selected(id).and_then(|i| self.weights.get(id)?.get(i).copied());
            // A font left as it showed stays as it was: chosen, or following
            // the language.
            let font = |role: Role| {
                let shown = self.shown_faces.get(&role)?;
                let (family_id, weight_id) = ids(role);
                let face = Face::new(family(family_id).unwrap_or(shown.family.clone()), weight(weight_id).unwrap_or(shown.weight));
                if face == *shown {
                    next.chosen(role).cloned()
                } else {
                    Some(face)
                }
            };
            let chosen = Settings {
                language: w.selected("language").and_then(|i| Language::ALL.get(i).copied()).or(next.language),
                body_font: font(Role::Body),
                heading_font: font(Role::Heading),
                map_font: font(Role::Map),
                console_font: font(Role::Console),
                map_outline: w.is_on("outline").unwrap_or(next.map_outline),
                return_without_restart: w.is_on("return").unwrap_or(next.return_without_restart),
                save_ironman_before_load: w.is_on("ironman_save").unwrap_or(next.save_ironman_before_load),
                achievements_outside_ironman: w.is_on("outside_ironman").unwrap_or(next.achievements_outside_ironman),
                achievements_with_changed_checksum: w.is_on("changed_checksum").unwrap_or(next.achievements_with_changed_checksum),
                fastest_speed: w.selected("pace").and_then(|i| PACES.get(i).copied()).unwrap_or(next.fastest_speed),
                remote_full_view: w.is_on("remote_full_view").unwrap_or(next.remote_full_view),
            };
            match settings::save(chosen.clone()) {
                Ok(()) => log!("settings saved: {chosen:?}"),
                Err(e) => log!("settings not saved: {e}"),
            }
            speed::set_pace(settings::current().fastest_speed);
        }
        self.settings = None;
        self.system_fonts = None;
        self.shown_faces.clear();
        MODAL.store(false, Ordering::Relaxed);
    }

    fn open_mods(&mut self) {
        let mut window = mods_window::window(mods_window::cards(), &words_with, &loc::mod_text);
        window.center(&self.font, self.screen.0, self.screen.1);
        self.mods = Some(window);
        MODAL.store(true, Ordering::Relaxed);
    }

    /// The player chose the mod at `index` in the mods window's list.
    fn mod_chosen(&mut self, index: usize) {
        let (Some(card), Some(window)) = (mods_window::cards().get(index), self.mods.as_mut()) else { return };
        window.body = mods_window::details(card, &words_with, &loc::mod_text);
    }

    fn close_mods(&mut self) {
        self.mods = None;
        MODAL.store(false, Ordering::Relaxed);
    }

    /// Takes the notice to show from the queue and keeps its window: where
    /// the player moved it, while it shows the same notice.
    fn update_notice(&mut self) {
        let current = crate::notice::current();
        match (current, &mut self.notice) {
            (None, _) => self.notice = None,
            (Some(showing), Some((shown, window))) if *shown == showing.number => window.title = showing.title,
            (Some(showing), _) => {
                let mut window = Window::new(showing.title, vec![Widget::Label(showing.text)], 460.0);
                window.center(&self.font, self.screen.0, self.screen.1 * 0.6);
                self.notice = Some((showing.number, window));
            }
        }
    }

    fn event(&mut self, event: input::Event) -> bool {
        use input::Event::*;
        // The window procedure speaks pixels.
        let s = self.scale;
        let event = match event {
            Move(x, y) => Move(x / s, y / s),
            Down(x, y) => Down(x / s, y / s),
            Up(x, y) => Up(x / s, y / s),
            other => other,
        };
        if let Move(x, y) | Down(x, y) | Up(x, y) = event {
            self.mouse = (x, y);
        }
        if let Some(w) = self.mods.as_mut() {
            match Self::modal_event(w, &self.font, event) {
                Some(Action::Chose("mods", index)) => self.mod_chosen(index),
                Some(Action::Closed) => self.close_mods(),
                _ => {}
            }
            return true;
        }
        if let Some(w) = self.settings.as_mut() {
            match Self::modal_event(w, &self.font, event) {
                Some(Action::Chose(id @ ("body" | "heading" | "map" | "console"), index)) => self.family_chosen(id, index),
                Some(Action::Pressed("save")) => self.close_settings(true),
                Some(Action::Pressed(_)) | Some(Action::Closed) => self.close_settings(false),
                _ => {}
            }
            return true;
        }
        if let Some((_, w)) = self.notice.as_mut() {
            match event {
                Move(x, y) => w.mouse_move(x, y),
                Up(..) => w.mouse_up(),
                Down(x, y) if w.covers(&self.font, x, y) => {
                    if w.mouse_down(&self.font, x, y) == Some(Action::Closed) {
                        crate::notice::dismiss();
                        self.notice = None;
                    }
                    return true;
                }
                _ => {}
            }
        }
        if let Down(x, y) = event {
            let hit = in_main_menu().then(|| corner(self.screen, &self.font).into_iter().find(|(r, ..)| r.contains(x, y))).flatten();
            match hit.map(|(.., opens)| opens) {
                Some(Opens::Settings) => self.open_settings(),
                Some(Opens::Mods) => self.open_mods(),
                None => return false,
            }
            return true;
        }
        false
    }

    /// What the player did in a modal window `w`, which takes every event.
    fn modal_event(w: &mut Window, measure: &dyn Measure, event: input::Event) -> Option<Action> {
        use input::Event::*;
        match event {
            Move(x, y) => {
                w.mouse_move(x, y);
                None
            }
            Up(..) => {
                w.mouse_up();
                None
            }
            Down(x, y) if w.covers(measure, x, y) => w.mouse_down(measure, x, y),
            Down(..) => None,
            Wheel(notches) => {
                w.wheel(measure, notches);
                None
            }
            Escape => Some(Action::Closed),
        }
    }

    fn commands(&self) -> Vec<Command> {
        let mut out = Vec::new();
        if in_main_menu() && self.settings.is_none() && self.mods.is_none() {
            for (r, label, _) in corner(self.screen, &self.font) {
                let hover = r.contains(self.mouse.0, self.mouse.1);
                out.push(Command::Fill { rect: Rect { x: r.x - 1.0, y: r.y - 1.0, w: r.w + 2.0, h: r.h + 2.0 }, color: widgets::BORDER });
                out.push(Command::Fill { rect: r, color: if hover { widgets::HOVER } else { widgets::TITLE_BAR } });
                out.push(Command::Text { x: r.x + 12.0, y: r.y + 5.0, text: label, size: widgets::TEXT_SIZE, color: widgets::TEXT });
            }
        }
        if let Some((_, w)) = &self.notice {
            out.extend(w.draw(&self.font));
        }
        if let Some(w) = self.settings.as_ref().or(self.mods.as_ref()) {
            out.push(Command::Fill { rect: Rect { x: 0.0, y: 0.0, w: self.screen.0, h: self.screen.1 }, color: 0x80000000 });
            out.extend(w.draw(&self.font));
        }
        out
    }

    /// The still notice: `text` in a box in the middle of the screen, which
    /// is dimmed.
    fn still_commands(&self, text: String) -> Vec<Command> {
        let (w, h) = (self.font.width(&text, widgets::TEXT_SIZE) + 48.0, widgets::TEXT_SIZE + 32.0);
        let r = Rect { x: ((self.screen.0 - w) / 2.0).round(), y: ((self.screen.1 - h) / 2.0).round(), w, h };
        vec![
            Command::Fill { rect: Rect { x: 0.0, y: 0.0, w: self.screen.0, h: self.screen.1 }, color: 0x80000000 },
            Command::Fill { rect: Rect { x: r.x - 1.0, y: r.y - 1.0, w: r.w + 2.0, h: r.h + 2.0 }, color: widgets::BORDER },
            Command::Fill { rect: r, color: widgets::TITLE_BAR },
            Command::Text { x: r.x + 24.0, y: r.y + 14.0, text, size: widgets::TEXT_SIZE, color: widgets::TEXT },
        ]
    }
}

/// Shows the framework's words for `key` over the frame the game presented
/// last, and presents that at once: on the main thread, just before work
/// that keeps it from drawing for seconds, such as leaving a session. The
/// game's window shows it until the game presents its next frame.
pub fn still(key: &str) {
    if crate::thread_id() != crate::main_thread() || FAILED.load(Ordering::Relaxed) {
        return;
    }
    let device = DEVICE.load(Ordering::Relaxed);
    let mut guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    let (Some(ui), false) = (guard.as_mut(), device.is_null()) else { return };
    let commands = ui.still_commands(words(key));
    let shown = unsafe { ui.backend.draw(device, &ui.font, &commands, ui.scale) }.and_then(|()| unsafe { d3d9::Backend::present(device) });
    match shown {
        Ok(()) => log!("framework UI: still notice {key}"),
        Err(e) => log!("framework UI: no still notice {key}: {e}"),
    }
}

fn handle(event: input::Event) -> bool {
    match UI.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        Some(ui) => ui.event(event),
        None => false,
    }
}

/// Draws the framework UI on `device` (an `IDirect3DDevice9*`); called on the
/// main thread once a frame, after the game's scene has ended, just before it
/// presents.
///
/// # Safety
/// `device` is null or the game's live device.
pub unsafe fn frame(device: *mut c_void) {
    FRAME.fetch_add(1, Ordering::Relaxed);
    if FAILED.load(Ordering::Relaxed) || device.is_null() {
        return;
    }
    let mut guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        match FrameworkUi::new() {
            Ok(o) => *guard = Some(o),
            Err(e) => {
                // Expected on a system without the fonts: notices fall back to message boxes.
                log!("framework UI unavailable: {e}");
                FAILED.store(true, Ordering::Relaxed);
                crate::notice::fall_back("the framework UI is unavailable");
                return;
            }
        }
    }
    let ui = guard.as_mut().expect("just made");
    DEVICE.store(device, Ordering::Relaxed);
    input::attach(handle);
    // The framework UI is as large as the game's own, whatever its scale.
    let scale = crate::memory().and_then(crate::game::gui_scale).map_or(1.0, |(across, _)| across.clamp(0.5, 4.0));
    let pixels = unsafe { d3d9::Backend::screen(device) };
    ui.scale = scale;
    ui.font.set_scale(scale);
    ui.screen = (pixels.0 / scale, pixels.1 / scale);
    ui.update_notice();
    let commands = ui.commands();
    if let Err(e) = unsafe { ui.backend.draw(device, &ui.font, &commands, scale) } {
        log!("framework UI draw failed: {e}");
        ui.backend.release();
    }
}

/// The game is about to reset the device; what lives in its default pool goes.
pub fn device_reset() {
    if let Some(ui) = UI.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        ui.backend.release();
    }
}
