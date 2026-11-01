//! The framework UI (GLOSSARY, ADR 0013): windows the framework draws itself
//! on top of the game's picture, with widgets of its own: the notice window,
//! the settings window and, in the main menu, a corner button that opens it.
//!
//! [`frame`] runs once a frame on the main thread, just before the game
//! presents (`hook`); the window procedure ([`input`]) hands it the
//! mouse and keyboard on the window's thread. The two share it under a lock.
pub mod d3d9;
pub mod hook;
pub mod input;
pub mod text;
pub mod widgets;

use crate::settings::{self, Settings};
use crate::unicode::system_font::{self, Face, SystemFonts};
use crate::{lang, loc};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use widgets::{Action, Command, Measure, Rect, Widget, Window};

fn words(key: &str) -> String {
    loc::framework(lang::player(), key, &[])
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
    /// The system fonts the settings window offers.
    system_fonts: Option<SystemFonts>,
    /// Families offered in the settings window, by English name.
    families: Vec<String>,
    /// Weights offered in the settings window, by their drop-down list.
    weights: HashMap<&'static str, Vec<u16>>,
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

/// Whether a modal window (the settings window) is open.
pub fn modal() -> bool {
    MODAL.load(Ordering::Relaxed)
}

fn corner(screen: (f32, f32), measure: &dyn Measure) -> Rect {
    let label = words("EU4HARMONY_SETTINGS_BUTTON");
    let w = measure.width(&label, widgets::TEXT_SIZE) + 24.0;
    Rect { x: screen.0 - w - 16.0, y: screen.1 - 44.0, w, h: 30.0 }
}

impl FrameworkUi {
    fn new() -> crate::Result<Self> {
        let font = text::UiFont::new(&settings::current().body_font)?;
        Ok(Self {
            font,
            backend: d3d9::Backend::default(),
            screen: (0.0, 0.0),
            mouse: (-1.0, -1.0),
            scale: 1.0,
            notice: None,
            settings: None,
            system_fonts: None,
            families: Vec::new(),
            weights: HashMap::new(),
        })
    }

    fn open_settings(&mut self) {
        let next = settings::next();
        let fonts = SystemFonts::new().ok();
        let offered = fonts.as_ref().map(|f| f.families_with('中' as u32, lang::player() == eu4_harmony_loc::Language::SimpChinese)).unwrap_or_default();
        let mut families: Vec<String> = offered.iter().map(|(en, _)| en.clone()).collect();
        let mut shown: Vec<String> = offered.into_iter().map(|(_, s)| s).collect();
        for chosen in [&next.body_font.family, &next.heading_font.family, &next.map_font.family] {
            if !families.contains(chosen) {
                families.push(chosen.clone());
                shown.push(chosen.clone());
            }
        }
        let index = |family: &String| families.iter().position(|f| f == family).unwrap_or(0);
        let (body_weights, body_names, body_weight) = weight_options(fonts.as_ref(), &next.body_font);
        let (heading_weights, heading_names, heading_weight) = weight_options(fonts.as_ref(), &next.heading_font);
        let (map_weights, map_names, map_weight) = weight_options(fonts.as_ref(), &next.map_font);
        let body = vec![
            Widget::Label(words("EU4HARMONY_SETTINGS_BODY_FONT")),
            Widget::Dropdown { id: "body", options: shown.clone(), selected: index(&next.body_font.family) },
            Widget::Label(words("EU4HARMONY_SETTINGS_BODY_WEIGHT")),
            Widget::Dropdown { id: "body_weight", options: body_names, selected: body_weight },
            Widget::Label(words("EU4HARMONY_SETTINGS_HEADING_FONT")),
            Widget::Dropdown { id: "heading", options: shown.clone(), selected: index(&next.heading_font.family) },
            Widget::Label(words("EU4HARMONY_SETTINGS_HEADING_WEIGHT")),
            Widget::Dropdown { id: "heading_weight", options: heading_names, selected: heading_weight },
            Widget::Label(words("EU4HARMONY_SETTINGS_MAP_FONT")),
            Widget::Dropdown { id: "map", options: shown, selected: index(&next.map_font.family) },
            Widget::Label(words("EU4HARMONY_SETTINGS_MAP_WEIGHT")),
            Widget::Dropdown { id: "map_weight", options: map_names, selected: map_weight },
            Widget::Label(words("EU4HARMONY_SETTINGS_MAP_OUTLINE")),
            Widget::Dropdown {
                id: "outline",
                options: vec![words("EU4HARMONY_SETTINGS_OUTLINE_FONT"), words("EU4HARMONY_SETTINGS_OUTLINE_NONE")],
                selected: usize::from(!next.map_outline),
            },
            Widget::Label(words("EU4HARMONY_SETTINGS_RESTART")),
            Widget::Button { id: "save", label: words("EU4HARMONY_SETTINGS_SAVE") },
            Widget::Button { id: "cancel", label: words("EU4HARMONY_SETTINGS_CANCEL") },
        ];
        let mut window = Window::new(words("EU4HARMONY_SETTINGS_TITLE"), body, 420.0);
        window.center(&self.font, self.screen.0, self.screen.1);
        self.families = families;
        self.weights = HashMap::from([("body_weight", body_weights), ("heading_weight", heading_weights), ("map_weight", map_weights)]);
        self.system_fonts = fonts;
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
            _ => "map_weight",
        };
        let (Some(family), Some(window)) = (self.families.get(index), self.settings.as_mut()) else { return };
        let chosen = window.selected(weight_id).and_then(|i| self.weights.get(weight_id)?.get(i).copied()).unwrap_or(system_font::REGULAR);
        let (weights, names, selected) = weight_options(self.system_fonts.as_ref(), &Face::new(family.clone(), chosen));
        window.set_options(weight_id, names, selected);
        self.weights.insert(weight_id, weights);
    }

    fn close_settings(&mut self, save: bool) {
        if let (true, Some(w)) = (save, &self.settings) {
            let next = settings::next();
            let family = |id| w.selected(id).and_then(|i| self.families.get(i)).cloned();
            let weight = |id: &str| w.selected(id).and_then(|i| self.weights.get(id)?.get(i).copied());
            let chosen = Settings {
                body_font: Face::new(family("body").unwrap_or(next.body_font.family), weight("body_weight").unwrap_or(next.body_font.weight)),
                heading_font: Face::new(family("heading").unwrap_or(next.heading_font.family), weight("heading_weight").unwrap_or(next.heading_font.weight)),
                map_font: Face::new(family("map").unwrap_or(next.map_font.family), weight("map_weight").unwrap_or(next.map_font.weight)),
                map_outline: w.selected("outline") != Some(1),
            };
            match settings::save(chosen.clone()) {
                Ok(()) => log!("settings saved for the next start: {chosen:?}"),
                Err(e) => log!("settings not saved: {e}"),
            }
        }
        self.settings = None;
        self.system_fonts = None;
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
        if let Some(w) = self.settings.as_mut() {
            let action = match event {
                Move(x, y) => {
                    w.mouse_move(x, y);
                    None
                }
                Up(..) => {
                    w.mouse_up();
                    None
                }
                Down(x, y) if w.covers(&self.font, x, y) => w.mouse_down(&self.font, x, y),
                Down(..) => None,
                Wheel(notches) => {
                    w.wheel(&self.font, notches);
                    None
                }
                Escape => Some(Action::Closed),
            };
            match action {
                Some(Action::Chose(id @ ("body" | "heading" | "map"), index)) => self.family_chosen(id, index),
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
            if in_main_menu() && corner(self.screen, &self.font).contains(x, y) {
                self.open_settings();
                return true;
            }
        }
        false
    }

    fn commands(&self) -> Vec<Command> {
        let mut out = Vec::new();
        if in_main_menu() && self.settings.is_none() {
            let r = corner(self.screen, &self.font);
            let hover = r.contains(self.mouse.0, self.mouse.1);
            out.push(Command::Fill { rect: Rect { x: r.x - 1.0, y: r.y - 1.0, w: r.w + 2.0, h: r.h + 2.0 }, color: widgets::BORDER });
            out.push(Command::Fill { rect: r, color: if hover { widgets::HOVER } else { widgets::TITLE_BAR } });
            out.push(Command::Text { x: r.x + 12.0, y: r.y + 5.0, text: words("EU4HARMONY_SETTINGS_BUTTON"), size: widgets::TEXT_SIZE, color: widgets::TEXT });
        }
        if let Some((_, w)) = &self.notice {
            out.extend(w.draw(&self.font));
        }
        if let Some(w) = &self.settings {
            out.push(Command::Fill { rect: Rect { x: 0.0, y: 0.0, w: self.screen.0, h: self.screen.1 }, color: 0x80000000 });
            out.extend(w.draw(&self.font));
        }
        out
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
