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

use crate::settings::{self, FrameRateLimit, Page, Role, Settings, Shows};
use crate::troubleshooting::local_dumps;
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

struct SettingsState<F: FontSystem> {
    window: Window,
    opened_next: Settings,
    fonts: FontsPage<F>,
    display_hz: u32,
}

/// Host seam for the framework UI shell: real game globals in production and
/// an in-process fake in unit tests.
trait UiHost {
    type Fonts: FontSystem;
    fn words(&self, key: &str, args: &[(&str, &str)]) -> String;
    fn mod_text(&self, id: &str, key: &str) -> Option<String>;
    fn current_settings(&self) -> Settings;
    fn next_settings(&self) -> Settings;
    fn save_settings(&mut self, settings: Settings) -> Result<(), String>;
    fn display_refresh_rate(&self) -> u32;
    fn font_system(&self) -> Self::Fonts;
    fn mod_cards(&self) -> &[mods_window::Card];
    fn notice_current(&self) -> Option<crate::notice::Showing>;
    fn dismiss_notice(&mut self);
    fn troubleshooting_state(&self) -> crate::troubleshooting::State;
    fn start_troubleshooting(&mut self);
    fn dumps_configured(&self) -> std::io::Result<bool>;
    fn start_enable_dumps(&mut self) -> std::io::Result<()>;
    fn poll_enable_dumps(&mut self) -> Option<std::io::Result<()>>;
    fn dump_pending(&self) -> bool;
}

struct LiveHost {
    dump_request: Option<std::thread::JoinHandle<std::io::Result<()>>>,
}

impl LiveHost {
    fn new() -> Self {
        Self { dump_request: None }
    }
}

impl UiHost for LiveHost {
    type Fonts = Option<SystemFonts>;

    fn words(&self, key: &str, args: &[(&str, &str)]) -> String {
        words_with(key, args)
    }

    fn mod_text(&self, id: &str, key: &str) -> Option<String> {
        loc::mod_text(id, key)
    }

    fn current_settings(&self) -> Settings {
        settings::current()
    }

    fn next_settings(&self) -> Settings {
        settings::next()
    }

    fn save_settings(&mut self, chosen: Settings) -> Result<(), String> {
        settings::save(chosen)
    }

    fn display_refresh_rate(&self) -> u32 {
        crate::drawing::display_refresh_rate()
    }

    fn font_system(&self) -> Self::Fonts {
        SystemFonts::new().ok()
    }

    fn mod_cards(&self) -> &[mods_window::Card] {
        mods_window::cards()
    }

    fn notice_current(&self) -> Option<crate::notice::Showing> {
        crate::notice::current()
    }

    fn dismiss_notice(&mut self) {
        crate::notice::dismiss();
    }

    fn troubleshooting_state(&self) -> crate::troubleshooting::State {
        crate::troubleshooting::state()
    }

    fn start_troubleshooting(&mut self) {
        crate::troubleshooting::start();
    }

    fn dumps_configured(&self) -> std::io::Result<bool> {
        local_dumps::configured()
    }

    fn start_enable_dumps(&mut self) -> std::io::Result<()> {
        let request = std::thread::Builder::new().name("harmony Windows dump setup".into()).spawn(local_dumps::enable)?;
        self.dump_request = Some(request);
        Ok(())
    }

    fn poll_enable_dumps(&mut self) -> Option<std::io::Result<()>> {
        if !self.dump_request.as_ref().is_some_and(|request| request.is_finished()) {
            return None;
        }
        let request = self.dump_request.take().expect("finished request");
        Some(request.join().unwrap_or_else(|_| Err(std::io::Error::other("dump setup thread stopped"))))
    }

    fn dump_pending(&self) -> bool {
        self.dump_request.is_some()
    }
}

/// Self-contained framework UI shell: owns screen/scale/mouse, main-menu
/// freshness, modal windows, settings edit snapshot, notice retention, and
/// unified stacking for drawing, input, and tooltips.
struct Shell<H: UiHost> {
    host: H,
    /// The screen and the mouse in GUI units, `scale` pixels each.
    screen: (f32, f32),
    mouse: (f32, f32),
    scale: f32,
    frame: u64,
    main_menu_frame: Option<u64>,
    /// The notice on screen, by its number.
    notice: Option<(u64, Window)>,
    settings: Option<SettingsState<H::Fonts>>,
    mods: Option<Window>,
    last_dump_outcome: Option<DumpStatus>,
}

struct FrameworkUi {
    font: text::UiFont,
    backend: d3d9::Backend,
    shell: Shell<LiveHost>,
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
fn corner(screen: (f32, f32), measure: &dyn Measure, words: &dyn Fn(&str) -> String) -> Vec<(Rect, String, Opens)> {
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
/// in their order there, showing the values `shown` has on a `display_hz`
/// display; each interactive widget's id is its setting's key.
fn setting_widgets(page: Page, shown: &Settings, display_hz: u32, words: &dyn Fn(&str) -> String) -> Vec<Widget> {
    let mut out = Vec::new();
    for setting in settings::SETTINGS.iter().filter(|s| s.page == page) {
        match setting.shows {
            Shows::Toggle { label, field } => out.push(Widget::Toggle { id: setting.key, label: words(label), on: (field.get)(shown) }),
            Shows::FrameRate { label, unlimited_tooltip, field } => {
                let (choices, selected) = settings::frame_rate_choices((field.get)(shown), display_hz);
                let options = choices
                    .iter()
                    .map(|choice| match choice {
                        FrameRateLimit::Display => display_hz.to_string(),
                        FrameRateLimit::Hz(r) => r.to_string(),
                        FrameRateLimit::Unlimited => "∞".to_string(),
                    })
                    .collect();
                let tooltips = choices
                    .iter()
                    .map(|choice| match choice {
                        FrameRateLimit::Unlimited => eu4_harmony_lua::Text::key(unlimited_tooltip.into()),
                        FrameRateLimit::Display | FrameRateLimit::Hz(_) => Default::default(),
                    })
                    .collect();
                out.push(Widget::Label(words(label)));
                out.push(Widget::Choice { id: setting.key, options, tooltips, selected });
            }
        }
    }
    out
}

/// A page with nothing but settings from [`settings::SETTINGS`].
fn settings_page(page: Page, shown: &Settings, display_hz: u32, words: &dyn Fn(&str) -> String) -> Vec<Widget> {
    setting_widgets(page, shown, display_hz, words)
}

/// Where problems with the framework are reported, as the About page names
/// it.
const ISSUES: &str = "https://github.com/KisinTheFlame/eu4-harmony/issues";

/// The button on the About page that packs the troubleshooting files.
const TROUBLESHOOTING: &str = "troubleshooting";

const ENABLE_DUMPS: &str = "enable_windows_dumps";

enum DumpStatus {
    Available,
    Pending,
    Configured,
    Cancelled,
    Failed(std::io::Error),
}

impl DumpStatus {
    fn from_check(result: std::io::Result<bool>) -> Self {
        match result {
            Ok(true) => Self::Configured,
            Ok(false) => Self::Available,
            Err(e) if e.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_CANCELLED as i32) => Self::Cancelled,
            Err(e) => Self::Failed(e),
        }
    }
}

/// The About page: the framework's version, where to report a problem, and
/// the button that packs the troubleshooting files, with the line after it
/// that says what it did.
fn about_page(
    dumps: &DumpStatus,
    troubleshooting: &crate::troubleshooting::State,
    words: &dyn Fn(&str) -> String,
    words_with: mods_window::Words,
) -> Vec<Widget> {
    let mut page = vec![
        Widget::Heading(words_with("EU4HARMONY_ABOUT_VERSION", &[("VERSION", env!("CARGO_PKG_VERSION"))])),
        Widget::Label(words_with("EU4HARMONY_ABOUT_ISSUES", &[("ISSUES", ISSUES)])),
        Widget::Gap,
        Widget::Button { id: TROUBLESHOOTING, label: words("EU4HARMONY_ABOUT_TROUBLESHOOTING") },
        Widget::Label(troubleshooting_line(troubleshooting, words_with)),
    ];
    match dumps {
        DumpStatus::Configured => page.push(Widget::Label(words("EU4HARMONY_DUMPS_CONFIGURED"))),
        DumpStatus::Pending => page.push(Widget::Label(words("EU4HARMONY_DUMPS_PENDING"))),
        state => {
            page.push(Widget::Button { id: ENABLE_DUMPS, label: words("EU4HARMONY_DUMPS_ENABLE") });
            match state {
                DumpStatus::Cancelled => page.push(Widget::Label(words("EU4HARMONY_DUMPS_CANCELLED"))),
                DumpStatus::Failed(e) => page.push(Widget::Label(words_with("EU4HARMONY_DUMPS_FAILED", &[("ERROR", &e.to_string())]))),
                _ => {}
            }
        }
    }
    page
}

/// What the troubleshooting files' packing has come to, as the line under
/// its button says it.
fn troubleshooting_line(state: &crate::troubleshooting::State, words_with: mods_window::Words) -> String {
    use crate::troubleshooting::State;
    match state {
        State::Idle => String::new(),
        State::Packing => words_with("EU4HARMONY_ABOUT_PACKING", &[]),
        State::Saved(file) => words_with("EU4HARMONY_ABOUT_SAVED", &[("FILE", file)]),
        State::Failed(why) => words_with("EU4HARMONY_ABOUT_NOT_SAVED", &[("ERROR", why)]),
    }
}

/// The place in the settings window's footer of the line that says a change
/// waits for the next start, just above the save button; empty while none
/// does, so that the buttons stay where they are.
const RESTART_LINE: usize = 1;

/// The line at [`RESTART_LINE`]: the words that say so while the settings
/// `w` shows, saved, would change what takes effect only at the next start
/// from what this session, running with `current`, has; nothing otherwise.
fn restart_line(
    w: &Window,
    current: &Settings,
    next: &Settings,
    fonts: &FontsPage<impl FontSystem>,
    display_hz: u32,
    words: &dyn Fn(&str) -> String,
) -> Widget {
    let waits = current.waits_for_restart(&chosen_settings(w, next, fonts, display_hz));
    Widget::Label(if waits { words("EU4HARMONY_SETTINGS_RESTART") } else { String::new() })
}

/// Takes into `chosen` what the settings window `w` shows of the settings
/// in [`settings::SETTINGS`]; one it does not show keeps its value.
fn read_back(w: &Window, next: &Settings, display_hz: u32, chosen: &mut Settings) {
    for setting in &settings::SETTINGS {
        match setting.shows {
            Shows::Toggle { field, .. } => {
                if let Some(on) = w.is_on(setting.key) {
                    (field.set)(chosen, on);
                }
            }
            Shows::FrameRate { field, .. } => {
                if let Some(index) = w.selected(setting.key) {
                    let before = (field.get)(next);
                    let (choices, initial) = settings::frame_rate_choices(before, display_hz);
                    if index == initial {
                        (field.set)(chosen, before);
                    } else if let Some(&choice) = choices.get(index) {
                        (field.set)(chosen, choice);
                    }
                }
            }
        }
    }
}

/// The settings window showing `next`, the settings the next start runs
/// with, to a session running with `current`; its language and fonts page is
/// `fonts`.
fn settings_window(
    current: &Settings,
    next: &Settings,
    fonts: &FontsPage<impl FontSystem>,
    display_hz: u32,
    dumps: &DumpStatus,
    troubleshooting: &crate::troubleshooting::State,
    words: &dyn Fn(&str) -> String,
    words_with: mods_window::Words,
) -> Window {
    let pages = Page::ALL
        .into_iter()
        .map(|page| match page {
            Page::LanguageFonts => fonts.widgets(next, words),
            Page::About => about_page(dumps, troubleshooting, words, words_with),
            _ => settings_page(page, next, display_hz, words),
        })
        .collect();
    let footer = vec![
        Widget::Gap,
        Widget::Label(String::new()),
        Widget::Button { id: "save", label: words("EU4HARMONY_SETTINGS_SAVE") }.with_tooltip(eu4_harmony_lua::Text::key("EU4HARMONY_SETTINGS_SAVE".into())),
        Widget::Button { id: "cancel", label: words("EU4HARMONY_SETTINGS_CANCEL") }
            .with_tooltip(eu4_harmony_lua::Text::key("EU4HARMONY_SETTINGS_CANCEL".into())),
    ];
    let side = Side::new("pages", Page::ALL.iter().map(|page| words(page.title())).collect(), SETTINGS_LIST);
    let mut window = Window::new(words("EU4HARMONY_SETTINGS_TITLE"), Vec::new(), SETTINGS_WIDTH).with_side(side).with_pages(pages).with_footer(footer);
    window.footer_mut()[RESTART_LINE] = restart_line(&window, current, next, fonts, display_hz, words);
    window
}

/// The settings the settings window `w` saves: `next`, which it was opened
/// with, as `w` changed it.
fn chosen_settings(w: &Window, next: &Settings, fonts: &FontsPage<impl FontSystem>, display_hz: u32) -> Settings {
    let mut chosen = next.clone();
    fonts.read_back(w, &mut chosen);
    read_back(w, next, display_hz, &mut chosen);
    chosen
}

/// Replaces the page at `index` in a multi-page window whether it is currently
/// active (`window.body`) or stored in `window.pages`.
fn replace_page(window: &mut Window, index: usize, widgets: Vec<Widget>) {
    if window.side.as_ref().is_some_and(|side| side.selected == index) {
        window.body = widgets;
    } else {
        window.pages[index] = widgets;
    }
}

impl<H: UiHost> Shell<H> {
    fn new(host: H) -> Self {
        Self {
            host,
            screen: (0.0, 0.0),
            mouse: (-1.0, -1.0),
            scale: 1.0,
            frame: 0,
            main_menu_frame: None,
            notice: None,
            settings: None,
            mods: None,
            last_dump_outcome: None,
        }
    }

    fn modal(&self) -> bool {
        self.mods.is_some() || self.settings.is_some()
    }

    /// Single source of truth for the topmost modal window, shared by drawing,
    /// event dispatch, and tooltip hit-testing.
    fn active_modal(&self) -> Option<&Window> {
        self.mods.as_ref().or(self.settings.as_ref().map(|s| &s.window))
    }

    fn in_main_menu(&self) -> bool {
        self.main_menu_frame.is_some_and(|menu| self.frame.saturating_sub(menu) <= 2)
    }

    fn sync_frame(&mut self, frame: u64, main_menu_frame: Option<u64>, pixels: (f32, f32), scale: f32) {
        self.frame = frame;
        self.main_menu_frame = main_menu_frame;
        self.scale = scale;
        self.screen = (pixels.0 / scale, pixels.1 / scale);
    }

    fn open_settings(&mut self, measure: &dyn Measure) {
        let next = self.host.next_settings();
        let current = self.host.current_settings();
        let fonts = FontsPage::new(self.host.font_system(), &next);
        let display_hz = self.host.display_refresh_rate();
        let dumps = if self.host.dump_pending() {
            DumpStatus::Pending
        } else if let Some(outcome) = self.last_dump_outcome.take() {
            outcome
        } else {
            DumpStatus::from_check(self.host.dumps_configured())
        };
        let troubleshooting = self.host.troubleshooting_state();
        let w = |key: &str| self.host.words(key, &[]);
        let ww = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
        let mut window = settings_window(&current, &next, &fonts, display_hz, &dumps, &troubleshooting, &w, &ww);
        window.center(measure, self.screen.0, self.screen.1);
        self.settings = Some(SettingsState { window, opened_next: next, fonts, display_hz });
    }

    fn show_dump_status(&mut self, status: &DumpStatus) {
        let index = Page::ALL.iter().position(|&p| p == Page::About).expect("About page");
        let troubleshooting = self.host.troubleshooting_state();
        let w = |key: &str| self.host.words(key, &[]);
        let ww = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
        let widgets = about_page(status, &troubleshooting, &w, &ww);
        if let Some(settings) = self.settings.as_mut() {
            replace_page(&mut settings.window, index, widgets);
        }
    }

    /// The player chose the option at `index` in the settings window's
    /// drop-down list or choice bar `id`.
    fn settings_chose(&mut self, id: &str, index: usize) {
        if let Some(settings) = self.settings.as_mut() {
            settings.fonts.chose(&mut settings.window, id, index);
        }
    }

    fn settings_changed(&mut self) {
        let current = self.host.current_settings();
        let w = |key: &str| self.host.words(key, &[]);
        if let Some(settings) = self.settings.as_mut() {
            settings.window.footer_mut()[RESTART_LINE] =
                restart_line(&settings.window, &current, &settings.opened_next, &settings.fonts, settings.display_hz, &w);
        }
    }

    fn close_settings(&mut self, save: bool) {
        if let Some(settings) = self.settings.take() {
            if save {
                let chosen = chosen_settings(&settings.window, &settings.opened_next, &settings.fonts, settings.display_hz);
                match self.host.save_settings(chosen.clone()) {
                    Ok(()) => log!("settings saved: {chosen:?}"),
                    Err(e) => log!("settings not saved: {e}"),
                }
            }
        }
    }

    fn open_mods(&mut self, measure: &dyn Measure) {
        let ww = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
        let mt = |id: &str, key: &str| self.host.mod_text(id, key);
        let mut window = mods_window::window(self.host.mod_cards(), &ww, &mt);
        window.center(measure, self.screen.0, self.screen.1);
        self.mods = Some(window);
    }

    fn mod_chosen(&mut self, index: usize) {
        let ww = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
        let mt = |id: &str, key: &str| self.host.mod_text(id, key);
        let Some(card) = self.host.mod_cards().get(index) else { return };
        let body = mods_window::details(card, &ww, &mt);
        if let Some(window) = self.mods.as_mut() {
            window.body = body;
        }
    }

    fn close_mods(&mut self) {
        self.mods = None;
    }

    /// Reconciles notices, async dump setup completion (even after Settings
    /// closed), and troubleshooting packing status once per frame.
    fn tick(&mut self, measure: &dyn Measure) {
        match (self.host.notice_current(), &mut self.notice) {
            (None, _) => self.notice = None,
            (Some(showing), Some((shown, window))) if *shown == showing.number => window.title = showing.title,
            (Some(showing), _) => {
                let mut window = Window::new(showing.title, vec![Widget::Label(showing.text)], 460.0);
                window.center(measure, self.screen.0, self.screen.1 * 0.6);
                self.notice = Some((showing.number, window));
            }
        }
        if let Some(result) = self.host.poll_enable_dumps() {
            if let Err(ref e) = result {
                log!("Windows dump setup: {e}");
            }
            let status = DumpStatus::from_check(result.map(|()| true));
            if self.settings.is_some() {
                self.show_dump_status(&status);
            } else {
                self.last_dump_outcome = Some(status);
            }
        }
        if self.settings.is_some() {
            let line = troubleshooting_line(&self.host.troubleshooting_state(), &|key, args| self.host.words(key, args));
            if let Some(settings) = self.settings.as_mut() {
                settings.window.set_label_after(TROUBLESHOOTING, line);
            }
        }
    }

    fn event(&mut self, measure: &dyn Measure, event: input::Event) -> bool {
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
            match Self::modal_event(w, measure, event) {
                Some(Action::Chose("mods", index)) => self.mod_chosen(index),
                Some(Action::Closed) => self.close_mods(),
                _ => {}
            }
            return true;
        }
        if let Some(settings) = self.settings.as_mut() {
            let action = Self::modal_event(&mut settings.window, measure, event);
            match action {
                Some(Action::Chose(id, index)) => {
                    self.settings_chose(id, index);
                    self.settings_changed();
                }
                Some(Action::Switched(..)) => self.settings_changed(),
                Some(Action::Pressed("save")) => self.close_settings(true),
                Some(Action::Pressed(ENABLE_DUMPS)) => {
                    if !self.host.dump_pending() {
                        let status = match self.host.start_enable_dumps() {
                            Ok(()) => DumpStatus::Pending,
                            Err(e) => DumpStatus::Failed(e),
                        };
                        self.show_dump_status(&status);
                    }
                }
                Some(Action::Pressed(TROUBLESHOOTING)) => {
                    self.host.start_troubleshooting();
                    let line = troubleshooting_line(&self.host.troubleshooting_state(), &|key, args| self.host.words(key, args));
                    if let Some(settings) = self.settings.as_mut() {
                        settings.window.set_label_after(TROUBLESHOOTING, line);
                    }
                }
                Some(Action::Pressed(_)) | Some(Action::Closed) => self.close_settings(false),
                None => {}
            }
            return true;
        }
        if let Some((_, w)) = self.notice.as_mut() {
            match event {
                Move(x, y) => w.mouse_move(x, y),
                Up(..) => w.mouse_up(),
                Down(x, y) if w.covers(measure, x, y) => {
                    if w.mouse_down(measure, x, y) == Some(Action::Closed) {
                        self.host.dismiss_notice();
                        self.notice = None;
                    }
                    return true;
                }
                _ => {}
            }
        }
        if let Down(x, y) = event {
            let w = |key: &str| self.host.words(key, &[]);
            let hit = self.in_main_menu().then(|| corner(self.screen, measure, &w).into_iter().find(|(r, ..)| r.contains(x, y))).flatten();
            match hit.map(|(.., opens)| opens) {
                Some(Opens::Settings) => self.open_settings(measure),
                Some(Opens::Mods) => self.open_mods(measure),
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

    fn commands(&self, measure: &dyn Measure) -> Vec<Command> {
        let mut out = Vec::new();
        if self.in_main_menu() && !self.modal() {
            let w = |key: &str| self.host.words(key, &[]);
            for (r, label, _) in corner(self.screen, measure, &w) {
                let hover = r.contains(self.mouse.0, self.mouse.1);
                out.push(Command::Fill { rect: Rect { x: r.x - 1.0, y: r.y - 1.0, w: r.w + 2.0, h: r.h + 2.0 }, color: widgets::BORDER });
                out.push(Command::Fill { rect: r, color: if hover { widgets::HOVER } else { widgets::TITLE_BAR } });
                out.push(Command::Text { x: r.x + 12.0, y: r.y + 5.0, text: label, size: widgets::TEXT_SIZE, color: widgets::TEXT });
            }
        }
        if let Some((_, w)) = &self.notice {
            out.extend(w.draw(measure));
        }
        if let Some(w) = self.active_modal() {
            out.push(Command::Fill { rect: Rect { x: 0.0, y: 0.0, w: self.screen.0, h: self.screen.1 }, color: 0x80000000 });
            out.extend(w.draw(measure));
        }
        out
    }

    /// Topmost managed tooltip content at the current pointer, matching the
    /// exact stacking order of [`Self::commands`] and [`Self::event`].
    fn tooltip_content(&self, measure: &dyn Measure) -> Option<eu4_harmony_lua::Text> {
        let (x, y) = self.mouse;
        if !(Rect { x: 0.0, y: 0.0, w: self.screen.0, h: self.screen.1 }).contains(x, y) {
            return None;
        }
        if let Some(window) = self.active_modal() {
            // The modal dimmer is itself a managed visual element with no hint.
            return Some(window.tooltip_at(measure, x, y).unwrap_or_default());
        }
        if let Some(content) = self.notice.as_ref().and_then(|(_, window)| window.tooltip_at(measure, x, y)) {
            return Some(content);
        }
        if self.in_main_menu() {
            let w = |key: &str| self.host.words(key, &[]);
            return corner(self.screen, measure, &w).into_iter().find(|(rect, ..)| rect.contains(x, y)).map(|(_, label, _)| label.into());
        }
        None
    }

    /// The still notice: `text` in a box in the middle of the screen, which
    /// is dimmed.
    fn still_commands(&self, measure: &dyn Measure, text: String) -> Vec<Command> {
        let (w, h) = (measure.width(&text, widgets::TEXT_SIZE) + 48.0, widgets::TEXT_SIZE + 32.0);
        let r = Rect { x: ((self.screen.0 - w) / 2.0).round(), y: ((self.screen.1 - h) / 2.0).round(), w, h };
        vec![
            Command::Fill { rect: Rect { x: 0.0, y: 0.0, w: self.screen.0, h: self.screen.1 }, color: 0x80000000 },
            Command::Fill { rect: Rect { x: r.x - 1.0, y: r.y - 1.0, w: r.w + 2.0, h: r.h + 2.0 }, color: widgets::BORDER },
            Command::Fill { rect: r, color: widgets::TITLE_BAR },
            Command::Text { x: r.x + 24.0, y: r.y + 14.0, text, size: widgets::TEXT_SIZE, color: widgets::TEXT },
        ]
    }
}

impl FrameworkUi {
    fn new() -> crate::Result<Self> {
        let face = SystemFonts::new().map(|fonts| settings::current().face(Role::Body, |f| fonts.has_family(f)))?;
        let font = text::UiFont::new(&face)?;
        Ok(Self { font, backend: d3d9::Backend::default(), shell: Shell::new(LiveHost::new()) })
    }
}

/// The actual self-drawn visual target, in the same units and stacking order
/// as drawing and input. Empty content belongs to the hit, too.
pub(crate) fn tooltip_at() -> Option<Vec<u8>> {
    if FAILED.load(Ordering::Relaxed) || HOLDING.load(Ordering::Relaxed) {
        return None;
    }
    let guard = UI.try_lock().ok()?;
    let ui = guard.as_ref()?;
    let content = ui.shell.tooltip_content(&ui.font)?;
    let text = content.resolve(|key| Some(crate::loc::framework(crate::lang::player(), key, &[])));
    Some(crate::unicode::game_text::from_str(&text))
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
    let commands = ui.shell.still_commands(&ui.font, words(key));
    let shown = unsafe { ui.backend.draw(device, &ui.font, &commands, ui.shell.scale) }.and_then(|()| unsafe { d3d9::Backend::present(device) });
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
    let mut guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    let Some(ui) = guard.as_mut() else { return false };
    let taken = ui.shell.event(&ui.font, event);
    MODAL.store(ui.shell.modal(), Ordering::Relaxed);
    taken
}

/// Draws the framework UI on `device` (an `IDirect3DDevice9*`); called on the
/// main thread once a frame, after the game's scene has ended, just before it
/// presents.
///
/// # Safety
/// `device` is null or the game's live device.
pub unsafe fn frame(device: *mut c_void) {
    let frame_number = FRAME.fetch_add(1, Ordering::Relaxed) + 1;
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
    let menu = MAIN_MENU_FRAME.load(Ordering::Relaxed);
    ui.font.set_scale(scale);
    ui.shell.sync_frame(frame_number, (menu != u64::MAX).then_some(menu), pixels, scale);
    ui.shell.tick(&ui.font);
    MODAL.store(ui.shell.modal(), Ordering::Relaxed);
    let commands = ui.shell.commands(&ui.font);
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

    fn keys_with(key: &str, args: &[(&str, &str)]) -> String {
        args.iter().fold(key.to_string(), |text, (name, value)| format!("{text} {name}={value}"))
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
    fn pages(shown: &Settings, display_hz: u32) -> Vec<Vec<Widget>> {
        Page::ALL.into_iter().map(|page| settings_page(page, shown, display_hz, &keys)).collect()
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
        let pages = pages(&Settings::default(), 60);
        for setting in &settings::SETTINGS {
            let on: Vec<Page> = Page::ALL.into_iter().zip(&pages).filter(|(_, widgets)| ids(widgets).contains(&setting.key)).map(|(page, _)| page).collect();
            assert_eq!(on, [setting.page], "{}", setting.key);
            assert_eq!(pages.iter().flat_map(|w| ids(w)).filter(|id| *id == setting.key).count(), 1, "{}", setting.key);
        }
        // Each page holds only its toggles or a setting's own label and choice bar.
        for (page, widgets) in Page::ALL.into_iter().zip(&pages) {
            if page == Page::Display {
                assert!(matches!(widgets.as_slice(), [Widget::Label(_), Widget::Choice { id: "frame_rate_limit", .. }]));
            } else {
                assert!(widgets.iter().all(|w| matches!(w, Widget::Toggle { .. })), "{page:?}");
            }
        }
    }

    #[test]
    fn the_settings_window_reads_back_what_it_shows() {
        for (shown, before) in [(Settings::default(), every_setting_changed()), (every_setting_changed(), Settings::default())] {
            let side = Side::new("pages", Page::ALL.iter().map(|p| keys(p.title())).collect(), SETTINGS_LIST);
            let window = Window::new("", Vec::new(), SETTINGS_WIDTH).with_side(side).with_pages(pages(&shown, 60));
            let mut read = before;
            read_back(&window, &shown, 60, &mut read);
            assert_eq!(read, shown);
        }
        // A setting the window does not show keeps its value.
        let mut kept = every_setting_changed();
        read_back(&Window::new("", Vec::new(), SETTINGS_WIDTH), &Settings::default(), 60, &mut kept);
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
                    let window = settings_window(&next, &next, &page, 180, &DumpStatus::Available, &crate::troubleshooting::State::Idle, &keys, &keys_with);
                    assert_eq!(chosen_settings(&window, &next, &page, 180), next, "{language:?}");
                }
            }
        }
    }

    #[test]
    fn the_settings_window_saves_the_language_chosen() {
        let next = Settings { language: Some(Language::English), ..Settings::default() };
        let page = FontsPage::new(Some(fonts_page::tests::installed()), &next);
        let mut window = settings_window(&next, &next, &page, 60, &DumpStatus::Available, &crate::troubleshooting::State::Idle, &keys, &keys_with);
        let languages = Language::ALL.iter().map(|l| l.native_name().to_string()).collect();
        let korean = Language::ALL.iter().position(|&l| l == Language::Korean).unwrap();
        window.set_options("language", languages, korean);
        // Its fonts follow it at the next start.
        assert_eq!(chosen_settings(&window, &next, &page, 60), Settings { language: Some(Language::Korean), ..next });
    }

    fn restart_shown(w: &mut Window) -> String {
        match &w.footer_mut()[RESTART_LINE] {
            Widget::Label(text) => text.clone(),
            _ => panic!("the restart line is a label"),
        }
    }

    #[test]
    fn the_restart_line_shows_while_a_change_waits_for_the_next_start() {
        let current = Settings { language: Some(Language::English), ..Settings::default() };
        let page = FontsPage::new(Some(fonts_page::tests::installed()), &current);
        let idle = crate::troubleshooting::State::Idle;
        // Nothing changed: no line, but its room, so the buttons stay put.
        let mut window = settings_window(&current, &current, &page, 60, &DumpStatus::Available, &idle, &keys, &keys_with);
        assert_eq!(restart_shown(&mut window), "");
        // A change that takes effect when saved needs no restart.
        let mut sound = current.clone();
        sound.music_loudness = !sound.music_loudness;
        let mut window = settings_window(&current, &sound, &page, 60, &DumpStatus::Available, &idle, &keys, &keys_with);
        assert_eq!(restart_shown(&mut window), "");
        // One that takes effect at the next start, saved before or chosen
        // now, does.
        let mut outline = current.clone();
        outline.map_outline = !outline.map_outline;
        let mut window = settings_window(
            &current,
            &outline,
            &FontsPage::new(Some(fonts_page::tests::installed()), &outline),
            60,
            &DumpStatus::Available,
            &idle,
            &keys,
            &keys_with,
        );
        assert_eq!(restart_shown(&mut window), "EU4HARMONY_SETTINGS_RESTART");
        let mut window = settings_window(&current, &current, &page, 60, &DumpStatus::Available, &idle, &keys, &keys_with);
        let languages = Language::ALL.iter().map(|l| l.native_name().to_string()).collect();
        let korean = Language::ALL.iter().position(|&l| l == Language::Korean).unwrap();
        window.set_options("language", languages, korean);
        window.footer_mut()[RESTART_LINE] = restart_line(&window, &current, &current, &page, 60, &keys);
        assert_eq!(restart_shown(&mut window), "EU4HARMONY_SETTINGS_RESTART");
    }

    #[test]
    fn the_display_page_follows_the_refresh_rate_until_another_is_chosen() {
        struct Mono;
        impl Measure for Mono {
            fn width(&self, text: &str, size: f32) -> f32 {
                text.chars().count() as f32 * size / 2.0
            }
            fn wrap(&self, text: &str, size: f32, width: f32) -> Vec<String> {
                let per_line = (width / (size / 2.0)) as usize;
                text.chars().collect::<Vec<_>>().chunks(per_line.max(1)).map(|c| c.iter().collect()).collect()
            }
        }
        let text_pos = |w: &Window, needle: &str| {
            w.draw(&Mono)
                .into_iter()
                .find_map(|c| match c {
                    Command::Text { x, y, text, .. } if text == needle => Some((x + 2.0, y + 8.0)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing text {needle:?}"))
        };

        let short_words = |key: &str| match key {
            "EU4HARMONY_SETTINGS_PAGE_DISPLAY" => "Display".to_string(),
            "EU4HARMONY_SETTINGS_FRAME_RATE_LIMIT" => "Frame rate limit".to_string(),
            _ => key.to_string(),
        };

        let current = Settings { language: Some(Language::English), ..Settings::default() };
        let page = FontsPage::new(Some(fonts_page::tests::installed()), &current);
        let mut window = settings_window(&current, &current, &page, 180, &DumpStatus::Available, &crate::troubleshooting::State::Idle, &short_words, &keys_with);
        window.center(&Mono, 1200.0, 900.0);
        assert_eq!(chosen_settings(&window, &current, &page, 180).frame_rate_limit, FrameRateLimit::Display);

        let (dx, dy) = text_pos(&window, "Display");
        assert_eq!(window.mouse_down(&Mono, dx, dy), Some(Action::Chose("pages", 3)));
        let _ = text_pos(&window, "Frame rate limit");
        let (hx, hy) = text_pos(&window, "180");
        let (fx, fy) = text_pos(&window, "60");
        let (ux, uy) = text_pos(&window, "∞");
        assert_eq!(window.tooltip_at(&Mono, fx, fy), Some(Default::default()));
        assert_eq!(window.tooltip_at(&Mono, ux, uy), Some(eu4_harmony_lua::Text::key("EU4HARMONY_SETTINGS_FRAME_RATE_UNLIMITED_TOOLTIP".into())));

        assert_eq!(window.mouse_down(&Mono, fx, fy), Some(Action::Chose("frame_rate_limit", 1)));
        window.footer_mut()[RESTART_LINE] = restart_line(&window, &current, &current, &page, 180, &short_words);
        assert_eq!(restart_shown(&mut window), "");
        assert_eq!(chosen_settings(&window, &current, &page, 180).frame_rate_limit, FrameRateLimit::Hz(60));

        assert_eq!(window.mouse_down(&Mono, ux, uy), Some(Action::Chose("frame_rate_limit", 8)));
        window.footer_mut()[RESTART_LINE] = restart_line(&window, &current, &current, &page, 180, &short_words);
        assert_eq!(restart_shown(&mut window), "");
        assert_eq!(chosen_settings(&window, &current, &page, 180).frame_rate_limit, FrameRateLimit::Unlimited);

        // Choosing the display's own rate again keeps it following the display.
        assert_eq!(window.mouse_down(&Mono, hx, hy), Some(Action::Chose("frame_rate_limit", 6)));
        assert_eq!(chosen_settings(&window, &current, &page, 180).frame_rate_limit, FrameRateLimit::Display);
    }

    #[test]
    fn the_dump_button_allows_retry_after_cancellation_or_failure() {
        let cancelled = std::io::Error::from_raw_os_error(windows_sys::Win32::Foundation::ERROR_CANCELLED as i32);
        let idle = crate::troubleshooting::State::Idle;
        for (state, clickable, feedback) in [
            (DumpStatus::from_check(Ok(false)), true, None),
            (DumpStatus::Pending, false, Some("EU4HARMONY_DUMPS_PENDING")),
            (DumpStatus::from_check(Ok(true)), false, Some("EU4HARMONY_DUMPS_CONFIGURED")),
            (DumpStatus::from_check(Err(cancelled)), true, Some("EU4HARMONY_DUMPS_CANCELLED")),
            (DumpStatus::from_check(Err(std::io::Error::other("denied"))), true, Some("EU4HARMONY_DUMPS_FAILED ERROR=denied")),
        ] {
            let page = about_page(&state, &idle, &keys, &keys_with);
            assert_eq!(page.iter().any(|w| matches!(w, Widget::Button { id: ENABLE_DUMPS, .. })), clickable);
            if let Some(expected) = feedback {
                assert!(matches!(page.last(), Some(Widget::Label(label)) if label == expected));
            }
        }
    }

    #[test]
    fn the_about_page_shows_the_version_and_what_the_troubleshooting_button_did() {
        let current = Settings::default();
        let page = FontsPage::new(Some(fonts_page::tests::installed()), &current);
        let mut window = settings_window(&current, &current, &page, 60, &DumpStatus::Available, &crate::troubleshooting::State::Idle, &keys, &keys_with);
        let about = Page::ALL.iter().position(|&p| p == Page::About).unwrap();
        let version = format!("EU4HARMONY_ABOUT_VERSION VERSION={}", env!("CARGO_PKG_VERSION"));
        assert!(matches!(&window.pages[about][0], Widget::Heading(text) if *text == version));
        window.set_label_after(TROUBLESHOOTING, troubleshooting_line(&crate::troubleshooting::State::Saved("a.zip".into()), &keys_with));
        let line = window.pages[about].iter().skip_while(|w| !matches!(w, Widget::Button { id: TROUBLESHOOTING, .. })).nth(1);
        assert!(matches!(line, Some(Widget::Label(text)) if text == "EU4HARMONY_ABOUT_SAVED FILE=a.zip"));
    }

    struct FixedMeasure;
    impl Measure for FixedMeasure {
        fn width(&self, text: &str, size: f32) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
        fn wrap(&self, text: &str, _: f32, _: f32) -> Vec<String> {
            vec![text.to_string()]
        }
    }

    struct FakeHost {
        current: Settings,
        next: Settings,
        saved: Vec<Settings>,
        notice: Option<crate::notice::Showing>,
        dismissed: usize,
        dumps_configured: bool,
        dump_pending: bool,
        dump_finished: Option<std::io::Result<()>>,
        troubleshooting: crate::troubleshooting::State,
        cards: Vec<mods_window::Card>,
    }

    impl Default for FakeHost {
        fn default() -> Self {
            Self {
                current: Settings::default(),
                next: Settings::default(),
                saved: Vec::new(),
                notice: None,
                dismissed: 0,
                dumps_configured: false,
                dump_pending: false,
                dump_finished: None,
                troubleshooting: crate::troubleshooting::State::Idle,
                cards: Vec::new(),
            }
        }
    }

    impl UiHost for FakeHost {
        type Fonts = Option<fonts_page::Table>;
        fn words(&self, key: &str, args: &[(&str, &str)]) -> String {
            keys_with(key, args)
        }
        fn mod_text(&self, _: &str, _: &str) -> Option<String> {
            None
        }
        fn current_settings(&self) -> Settings {
            self.current.clone()
        }
        fn next_settings(&self) -> Settings {
            self.next.clone()
        }
        fn save_settings(&mut self, chosen: Settings) -> Result<(), String> {
            self.next = chosen.clone();
            self.saved.push(chosen);
            Ok(())
        }
        fn display_refresh_rate(&self) -> u32 {
            60
        }
        fn font_system(&self) -> Self::Fonts {
            Some(fonts_page::tests::installed())
        }
        fn mod_cards(&self) -> &[mods_window::Card] {
            &self.cards
        }
        fn notice_current(&self) -> Option<crate::notice::Showing> {
            self.notice.clone()
        }
        fn dismiss_notice(&mut self) {
            self.dismissed += 1;
            self.notice = None;
        }
        fn troubleshooting_state(&self) -> crate::troubleshooting::State {
            self.troubleshooting.clone()
        }
        fn start_troubleshooting(&mut self) {
            self.troubleshooting = crate::troubleshooting::State::Packing;
        }
        fn dumps_configured(&self) -> std::io::Result<bool> {
            Ok(self.dumps_configured)
        }
        fn start_enable_dumps(&mut self) -> std::io::Result<()> {
            self.dump_pending = true;
            Ok(())
        }
        fn poll_enable_dumps(&mut self) -> Option<std::io::Result<()>> {
            let done = self.dump_finished.take()?;
            self.dump_pending = false;
            if done.is_ok() {
                self.dumps_configured = true;
            }
            Some(done)
        }
        fn dump_pending(&self) -> bool {
            self.dump_pending
        }
    }

    #[test]
    fn shell_corner_buttons_modal_stacking_and_main_menu_expiry_stay_consistent() {
        let mut shell = Shell::new(FakeHost::default());
        shell.sync_frame(10, Some(10), (1280.0, 720.0), 1.0);
        let buttons = corner(shell.screen, &FixedMeasure, &keys);
        let (settings_rect, _, _) = buttons.iter().find(|(_, _, o)| *o == Opens::Settings).unwrap();

        // Hovering the settings corner button reports its tooltip and clicking opens modal settings.
        let (cx, cy) = (settings_rect.x + 4.0, settings_rect.y + 4.0);
        assert!(!shell.event(&FixedMeasure, input::Event::Move(cx, cy)));
        assert_eq!(shell.tooltip_content(&FixedMeasure), Some("EU4HARMONY_SETTINGS_BUTTON".into()));
        assert!(shell.event(&FixedMeasure, input::Event::Down(cx, cy)));
        assert!(shell.modal());
        // Modal dimmer owns the corner position now and yields empty tooltip content.
        assert_eq!(shell.tooltip_content(&FixedMeasure), Some(eu4_harmony_lua::Text::default()));
        // Escape closes the modal and restores non-modal state.
        assert!(shell.event(&FixedMeasure, input::Event::Escape));
        assert!(!shell.modal());

        // Main-menu buttons expire once frame advances more than 2 frames past main_menu_frame.
        shell.sync_frame(13, Some(10), (1280.0, 720.0), 1.0);
        assert!(!shell.in_main_menu());
        assert_eq!(shell.tooltip_content(&FixedMeasure), None);
        assert!(!shell.event(&FixedMeasure, input::Event::Down(cx, cy)));
    }

    #[test]
    fn shell_reaps_async_dump_setup_after_settings_closes_and_retains_notice_position() {
        let mut host = FakeHost::default();
        host.notice = Some(crate::notice::Showing { number: 1, title: "First".into(), text: "Body".into() });
        let mut shell = Shell::new(host);
        shell.sync_frame(1, Some(1), (1280.0, 720.0), 1.0);
        shell.tick(&FixedMeasure);

        // Move notice window and update its title for the same notice number: position stays.
        let notice = &mut shell.notice.as_mut().unwrap().1;
        notice.x = 77.0;
        notice.y = 88.0;
        shell.host.notice = Some(crate::notice::Showing { number: 1, title: "Updated".into(), text: "Body".into() });
        shell.tick(&FixedMeasure);
        let notice = &shell.notice.as_ref().unwrap().1;
        assert_eq!((notice.x, notice.y), (77.0, 88.0));
        assert_eq!(notice.title, "Updated");

        // Start dump setup, close Settings before it finishes, then finish it on a later tick.
        shell.open_settings(&FixedMeasure);
        shell.host.start_enable_dumps().unwrap();
        shell.close_settings(false);
        shell.host.dump_finished = Some(Ok(()));
        shell.tick(&FixedMeasure);
        assert!(!shell.host.dump_pending());

        // Reopening Settings immediately shows Configured instead of Pending.
        shell.open_settings(&FixedMeasure);
        let about = Page::ALL.iter().position(|&p| p == Page::About).unwrap();
        let page = &shell.settings.as_ref().unwrap().window.pages[about];
        assert!(page.iter().any(|w| matches!(w, Widget::Label(l) if l == "EU4HARMONY_DUMPS_CONFIGURED")));
    }
}
