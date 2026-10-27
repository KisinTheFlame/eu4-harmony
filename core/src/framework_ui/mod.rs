//! The framework UI (GLOSSARY, ADR 0013): windows the framework draws itself
//! on top of the game's picture, with widgets of its own: the notice window,
//! the settings window and, in the main menu, a corner button that opens it.
//!
//! [`frame`] runs once a frame on the main thread, just before the game
//! presents (`hook`); the window procedure ([`input`]) hands the overlay the
//! mouse and keyboard on the window's thread. The two share it under a lock.
pub mod d3d9;
pub mod hook;
pub mod input;
pub mod text;
pub mod widgets;

use crate::settings::{self, Settings, TextAppearance};
use crate::{lang, loc};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use widgets::{Action, Command, Measure, Rect, Widget, Window};

fn words(key: &str) -> String {
    loc::framework(lang::player(), key, &[])
}

struct Overlay {
    font: text::UiFont,
    backend: d3d9::Backend,
    /// The screen and the mouse in GUI units, `scale` pixels each.
    screen: (f32, f32),
    mouse: (f32, f32),
    scale: f32,
    /// The notice on screen, by its title and text.
    notice: Option<((String, String), Window)>,
    settings: Option<Window>,
    /// Families offered in the settings window, by English name.
    families: Vec<String>,
}

// Its Direct3D objects are used only on the main thread; DirectWrite, which
// the window's thread reaches through the font, is free-threaded.
unsafe impl Send for Overlay {}

static OVERLAY: Mutex<Option<Overlay>> = Mutex::new(None);
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

impl Overlay {
    fn new() -> crate::Result<Self> {
        let font = text::UiFont::new(&settings::current().body_font)?;
        Ok(Self { font, backend: d3d9::Backend::default(), screen: (0.0, 0.0), mouse: (-1.0, -1.0), scale: 1.0, notice: None, settings: None, families: Vec::new() })
    }

    fn open_settings(&mut self) {
        let next = settings::next();
        let fonts = crate::unicode::system_font::SystemFonts::new().ok();
        let offered = fonts.map(|f| f.families_with('中' as u32, lang::player() == eu4_harmony_loc::Language::SimpChinese)).unwrap_or_default();
        let mut families: Vec<String> = offered.iter().map(|(en, _)| en.clone()).collect();
        let mut shown: Vec<String> = offered.into_iter().map(|(_, s)| s).collect();
        for chosen in [&next.body_font, &next.map_font] {
            if !families.contains(chosen) {
                families.push(chosen.clone());
                shown.push(chosen.clone());
            }
        }
        let index = |family: &String| families.iter().position(|f| f == family).unwrap_or(0);
        let body = vec![
            Widget::Label(words("EU4HARMONY_SETTINGS_APPEARANCE")),
            Widget::Dropdown {
                id: "appearance",
                options: vec![words("EU4HARMONY_SETTINGS_BITMAP"), words("EU4HARMONY_SETTINGS_SYSTEM")],
                selected: usize::from(next.text_appearance == TextAppearance::System),
            },
            Widget::Label(words("EU4HARMONY_SETTINGS_BODY_FONT")),
            Widget::Dropdown { id: "body", options: shown.clone(), selected: index(&next.body_font) },
            Widget::Label(words("EU4HARMONY_SETTINGS_MAP_FONT")),
            Widget::Dropdown { id: "map", options: shown, selected: index(&next.map_font) },
            Widget::Label(words("EU4HARMONY_SETTINGS_RESTART")),
            Widget::Button { id: "save", label: words("EU4HARMONY_SETTINGS_SAVE") },
            Widget::Button { id: "cancel", label: words("EU4HARMONY_SETTINGS_CANCEL") },
        ];
        let mut window = Window::new(words("EU4HARMONY_SETTINGS_TITLE"), body, 420.0);
        window.center(&self.font, self.screen.0, self.screen.1);
        self.families = families;
        self.settings = Some(window);
        MODAL.store(true, Ordering::Relaxed);
    }

    fn close_settings(&mut self, save: bool) {
        if let (true, Some(w)) = (save, &self.settings) {
            let family = |id| w.selected(id).and_then(|i| self.families.get(i)).cloned();
            let chosen = Settings {
                text_appearance: if w.selected("appearance") == Some(1) { TextAppearance::System } else { TextAppearance::Bitmap },
                body_font: family("body").unwrap_or_else(|| settings::next().body_font),
                map_font: family("map").unwrap_or_else(|| settings::next().map_font),
            };
            match settings::save(chosen.clone()) {
                Ok(()) => log!("settings saved for the next start: {chosen:?}"),
                Err(e) => log!("settings not saved: {e}"),
            }
        }
        self.settings = None;
        MODAL.store(false, Ordering::Relaxed);
    }

    /// Takes the notice to show from the queue and keeps its window.
    fn update_notice(&mut self) {
        let current = crate::notice::current();
        match (current, &self.notice) {
            (None, _) => self.notice = None,
            (Some(words), Some((shown, _))) if *shown == words => {}
            (Some((title, text)), _) => {
                let mut window = Window::new(title.clone(), vec![Widget::Label(text.clone())], 460.0);
                window.center(&self.font, self.screen.0, self.screen.1 * 0.6);
                self.notice = Some(((title, text), window));
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
            Escape => Escape,
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
                Escape => Some(Action::Closed),
            };
            match action {
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
    match OVERLAY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        Some(overlay) => overlay.event(event),
        None => false,
    }
}

/// Draws the framework UI on `device` (an `IDirect3DDevice9*`); called on the
/// main thread once a frame, after the game's scene has ended, just before it
/// presents.
pub fn frame(device: *mut c_void) {
    FRAME.fetch_add(1, Ordering::Relaxed);
    if FAILED.load(Ordering::Relaxed) || device.is_null() {
        return;
    }
    let mut guard = OVERLAY.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        match Overlay::new() {
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
    let overlay = guard.as_mut().expect("just made");
    input::attach(handle);
    // The framework UI is as large as the game's own, whatever its scale.
    let scale = crate::memory().and_then(crate::game::gui_scale).map_or(1.0, |(across, _)| across.clamp(0.5, 4.0));
    let pixels = unsafe { d3d9::Backend::screen(device) };
    overlay.scale = scale;
    overlay.font.set_scale(scale);
    overlay.screen = (pixels.0 / scale, pixels.1 / scale);
    overlay.update_notice();
    let commands = overlay.commands();
    if let Err(e) = unsafe { overlay.backend.draw(device, &overlay.font, &commands, scale) } {
        log!("framework UI draw failed: {e}");
        overlay.backend.release();
    }
}

/// The game is about to reset the device; what lives in its default pool goes.
pub fn device_reset() {
    if let Some(overlay) = OVERLAY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        overlay.backend.release();
    }
}
