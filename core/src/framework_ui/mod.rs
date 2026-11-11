//! The framework UI (GLOSSARY, ADR framework-draws-its-own-ui): windows the
//! framework draws itself on top of the game's picture, with widgets of its
//! own: the notice window, the settings window and the mods window, and in the
//! main menu the corner buttons that open those two.
//!
//! [`frame`] runs once a frame on the main thread, just before the game
//! presents (`hook`); the window procedure ([`input`]) hands it the
//! mouse and keyboard on the window's thread. The two share it under a lock.
pub mod d3d9;
pub mod fonts_page;
pub mod hook;
pub mod input;
pub mod mods_window;
pub mod text;
pub mod widgets;

use crate::settings::{self, Page, Role, Settings, Shows};
use crate::unicode::system_font::SystemFonts;
use crate::{lang, loc};
use fonts_page::{FontSystem, FontsPage};
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
    /// The settings window's language and fonts page, while it is open.
    fonts_page: Option<FontsPage<Option<SystemFonts>>>,
    mods: Option<Window>,
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

/// The widgets of the settings in [`settings::SETTINGS`] that `page` shows,
/// in their order there, showing the values `shown` has; each widget's id is
/// its setting's key.
fn setting_widgets(page: Page, shown: &Settings, words: &dyn Fn(&str) -> String) -> Vec<Widget> {
    let mut out = Vec::new();
    for setting in settings::SETTINGS.iter().filter(|s| s.page == page) {
        match setting.shows {
            Shows::Toggle { label, field } => out.push(Widget::Toggle { id: setting.key, label: words(label), on: (field.get)(shown) }),
        }
    }
    out
}

/// The words under `page`'s settings.
fn page_note(page: Page, words: &dyn Fn(&str) -> String) -> Option<Widget> {
    page.note().map(|key| Widget::Label(words(key)))
}

/// A page with nothing but settings from [`settings::SETTINGS`]: they, and
/// its note under them.
fn settings_page(page: Page, shown: &Settings, words: &dyn Fn(&str) -> String) -> Vec<Widget> {
    setting_widgets(page, shown, words).into_iter().chain(page_note(page, words)).collect()
}

/// Takes into `chosen` what the settings window `w` shows of the settings
/// in [`settings::SETTINGS`]; one it does not show keeps its value.
fn read_back(w: &Window, chosen: &mut Settings) {
    for setting in &settings::SETTINGS {
        match setting.shows {
            Shows::Toggle { field, .. } => {
                if let Some(on) = w.is_on(setting.key) {
                    (field.set)(chosen, on);
                }
            }
        }
    }
}

/// The settings window showing `next`, the settings the next start runs
/// with; its language and fonts page is `fonts`.
fn settings_window(next: &Settings, fonts: &FontsPage<impl FontSystem>, words: &dyn Fn(&str) -> String) -> Window {
    let pages = Page::ALL
        .into_iter()
        .map(|page| match page {
            Page::LanguageFonts => fonts.widgets(next, words),
            _ => settings_page(page, next, words),
        })
        .collect();
    let footer = vec![
        Widget::Gap,
        Widget::Button { id: "save", label: words("EU4HARMONY_SETTINGS_SAVE") },
        Widget::Button { id: "cancel", label: words("EU4HARMONY_SETTINGS_CANCEL") },
    ];
    let side = Side::new("pages", Page::ALL.iter().map(|page| words(page.title())).collect(), SETTINGS_LIST);
    Window::new(words("EU4HARMONY_SETTINGS_TITLE"), Vec::new(), SETTINGS_WIDTH).with_side(side).with_pages(pages).with_footer(footer)
}

/// The settings the settings window `w` saves: `next`, which it was opened
/// with, as `w` changed it.
fn chosen_settings(w: &Window, next: &Settings, fonts: &FontsPage<impl FontSystem>) -> Settings {
    let mut chosen = next.clone();
    fonts.read_back(w, &mut chosen);
    read_back(w, &mut chosen);
    chosen
}

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
            fonts_page: None,
            mods: None,
        })
    }

    fn open_settings(&mut self) {
        let next = settings::next();
        let fonts = FontsPage::new(SystemFonts::new().ok(), &next);
        let mut window = settings_window(&next, &fonts, &words);
        window.center(&self.font, self.screen.0, self.screen.1);
        self.settings = Some(window);
        self.fonts_page = Some(fonts);
        MODAL.store(true, Ordering::Relaxed);
    }

    /// The player chose the option at `index` in the settings window's
    /// drop-down list or choice bar `id`.
    fn settings_chose(&mut self, id: &str, index: usize) {
        if let (Some(fonts), Some(window)) = (self.fonts_page.as_mut(), self.settings.as_mut()) {
            fonts.chose(window, id, index);
        }
    }

    fn close_settings(&mut self, save: bool) {
        if let (true, Some(w), Some(fonts)) = (save, &self.settings, &self.fonts_page) {
            let chosen = chosen_settings(w, &settings::next(), fonts);
            match settings::save(chosen.clone()) {
                Ok(()) => log!("settings saved: {chosen:?}"),
                Err(e) => log!("settings not saved: {e}"),
            }
        }
        self.settings = None;
        self.fonts_page = None;
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
                Some(Action::Chose(id, index)) => self.settings_chose(id, index),
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

/// While set, the frames the main thread presents are held back, and the
/// game's window keeps showing what it showed last, such as a still notice.
static HOLDING: AtomicBool = AtomicBool::new(false);
static HELD: AtomicU64 = AtomicU64::new(0);

/// Holds back the frames the main thread presents from now on.
pub fn hold() {
    HELD.store(0, Ordering::Relaxed);
    HOLDING.store(true, Ordering::Relaxed);
}

/// Lets the frames the main thread presents through again; answers how many
/// were held back.
pub fn let_go() -> u64 {
    HOLDING.store(false, Ordering::Relaxed);
    HELD.swap(0, Ordering::Relaxed)
}

/// Whether frames are held back now, without counting one.
pub fn holding() -> bool {
    HOLDING.load(Ordering::Relaxed)
}

/// Whether the frame the main thread presents now is held back.
fn held() -> bool {
    let holding = HOLDING.load(Ordering::Relaxed);
    if holding {
        HELD.fetch_add(1, Ordering::Relaxed);
    }
    holding
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unicode::system_font::Face;
    use eu4_harmony_loc::Language;

    fn keys(key: &str) -> String {
        key.to_string()
    }

    /// The ids of the widgets that are read back.
    fn ids(widgets: &[Widget]) -> Vec<&'static str> {
        widgets
            .iter()
            .filter_map(|w| match w {
                Widget::Toggle { id, .. } | Widget::Choice { id, .. } | Widget::Dropdown { id, .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    /// The settings window's pages as far as [`settings::SETTINGS`] builds
    /// them: the language and fonts page without its fonts.
    fn pages(shown: &Settings) -> Vec<Vec<Widget>> {
        Page::ALL.into_iter().map(|page| settings_page(page, shown, &keys)).collect()
    }

    fn every_setting_changed() -> Settings {
        let mut changed = Settings::default();
        for setting in &settings::SETTINGS {
            setting.change(&mut changed);
        }
        changed
    }

    #[test]
    fn every_setting_shows_on_exactly_one_page() {
        let pages = pages(&Settings::default());
        for setting in &settings::SETTINGS {
            let on: Vec<Page> = Page::ALL.into_iter().zip(&pages).filter(|(_, widgets)| ids(widgets).contains(&setting.key)).map(|(page, _)| page).collect();
            assert_eq!(on, [setting.page], "{}", setting.key);
            assert_eq!(pages.iter().flat_map(|w| ids(w)).filter(|id| *id == setting.key).count(), 1, "{}", setting.key);
        }
        // A page's note comes after its settings.
        assert!(matches!(pages[1].last(), Some(Widget::Label(note)) if note == "EU4HARMONY_SETTINGS_UNDO"));
    }

    #[test]
    fn the_settings_window_reads_back_what_it_shows() {
        for (shown, before) in [(Settings::default(), every_setting_changed()), (every_setting_changed(), Settings::default())] {
            let side = Side::new("pages", Page::ALL.iter().map(|p| keys(p.title())).collect(), SETTINGS_LIST);
            let window = Window::new("", Vec::new(), SETTINGS_WIDTH).with_side(side).with_pages(pages(&shown));
            let mut read = before;
            read_back(&window, &mut read);
            assert_eq!(read, shown);
        }
        // A setting the window does not show keeps its value.
        let mut kept = every_setting_changed();
        read_back(&Window::new("", Vec::new(), SETTINGS_WIDTH), &mut kept);
        assert_eq!(kept, every_setting_changed());
    }

    #[test]
    fn the_settings_window_saved_unchanged_keeps_the_settings() {
        for language in Language::ALL {
            let following = Settings { language: Some(language), ..Settings::default() };
            // Chosen fonts: one in a weight its family lacks, and one not
            // installed.
            let chosen = Settings {
                body_font: Some(Face::new("Microsoft YaHei", 700)),
                heading_font: Some(Face::new("Segoe UI", 600)),
                map_font: Some(Face::new("Gone", 700)),
                console_font: Some(Face::new("Cascadia Mono", 600)),
                ..every_setting_changed()
            };
            let chosen = Settings { language: Some(language), ..chosen };
            for next in [following, chosen] {
                for fonts in [Some(fonts_page::tests::installed()), None] {
                    let page = FontsPage::new(fonts, &next);
                    let window = settings_window(&next, &page, &keys);
                    assert_eq!(chosen_settings(&window, &next, &page), next, "{language:?}");
                }
            }
        }
    }

    #[test]
    fn the_settings_window_saves_the_language_chosen() {
        let next = Settings { language: Some(Language::English), ..Settings::default() };
        let page = FontsPage::new(Some(fonts_page::tests::installed()), &next);
        let mut window = settings_window(&next, &page, &keys);
        let languages = Language::ALL.iter().map(|l| l.native_name().to_string()).collect();
        let korean = Language::ALL.iter().position(|&l| l == Language::Korean).unwrap();
        window.set_options("language", languages, korean);
        // Its fonts follow it at the next start.
        assert_eq!(chosen_settings(&window, &next, &page), Settings { language: Some(Language::Korean), ..next });
    }
}
