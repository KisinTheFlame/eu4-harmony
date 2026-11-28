//! The framework UI (GLOSSARY, ADR framework-draws-its-own-ui): windows the
//! framework draws itself on top of the game's picture, with widgets of its
//! own: the notice window, the settings window, the restart prompt and the mods
//! window, and in the main menu and the in-game menu the corner bar that opens
//! them.
//!
//! [`frame`] runs once a frame on the main thread or the drawing thread, just
//! before the game presents (`hook`); the window procedure ([`input`]) hands
//! it the mouse and keyboard on the window's thread. The two share it under a
//! lock.
mod d3d9;
pub mod fonts_page;
pub mod mods_window;
pub mod text;
pub mod widgets;

use crate::base::settings::{self, FrameRateLimit, GameSpeed, Page, Settings, Shows};
use crate::base::troubleshooting::local_dumps;
use crate::render::window_input as input;
use crate::text::unicode::system_font::SystemFonts;
use crate::{base::lang, base::loc};
use eu4_harmony_loc::Language;
use fonts_page::{FontSystem, FontsPage};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use widgets::{Action, ButtonStyle, Command, Measure, Rect, RowButton, Side, SliderFormat, Widget, Window};

fn words(key: &str) -> String {
    loc::framework(lang::ui(), key, &[])
}

fn words_with(key: &str, args: &[(&str, &str)]) -> String {
    loc::framework(lang::ui(), key, args)
}

struct SettingsState<F: FontSystem> {
    window: Window,
    opened_next: Settings,
    opened_draft: Settings,
    fonts: FontsPage<F>,
    preview_language: Option<Language>,
    display_hz: u32,
    in_game: bool,
}

struct ModsState {
    window: Window,
    in_game: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CornerContext {
    None,
    MainMenu,
    InGameMenu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DumpAction {
    Enable,
    Disable,
}

/// Host seam for the framework UI shell: real game globals in production and
/// an in-process fake in unit tests.
trait UiHost {
    type Fonts: FontSystem;
    fn ui_language(&self) -> Language;
    fn words(&self, key: &str, args: &[(&str, &str)]) -> String;
    fn words_in(&self, language: Language, key: &str, args: &[(&str, &str)]) -> String;
    fn mod_text(&self, id: &str, key: &str) -> Option<String>;
    fn current_settings(&self) -> Settings;
    fn next_settings(&self) -> Settings;
    fn save_settings(&mut self, settings: Settings, in_game: bool) -> Result<(), String>;
    fn display_refresh_rate(&self) -> u32;
    fn font_system(&self) -> Self::Fonts;
    fn mod_cards(&self) -> &[mods_window::Card];
    fn notice_current(&self) -> Option<crate::base::notice::Showing>;
    fn dismiss_notice(&mut self);
    fn troubleshooting_state(&self) -> crate::base::troubleshooting::State;
    fn start_troubleshooting(&mut self);
    fn dumps_status(&self) -> std::io::Result<local_dumps::Configuration>;
    fn start_enable_dumps(&mut self) -> std::io::Result<()>;
    fn start_disable_dumps(&mut self) -> std::io::Result<()>;
    fn poll_dump_op(&mut self) -> Option<(DumpAction, std::io::Result<()>)>;
    fn dump_pending(&self) -> Option<DumpAction>;
    fn open_url(&mut self, url: &str) -> Result<(), String>;
    fn open_logs_folder(&mut self) -> Result<(), String>;
    fn request_restart(&mut self) -> Result<(), String>;
}

struct LiveHost {
    dump_request: Option<(DumpAction, std::thread::JoinHandle<std::io::Result<()>>)>,
}

impl LiveHost {
    fn new() -> Self {
        Self { dump_request: None }
    }
}

fn wide_nul(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_url_in_browser(url: &str) -> Result<(), String> {
    let url = url.to_string();
    std::thread::Builder::new()
        .name("harmony-open-url".into())
        .spawn(move || {
            use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
            use windows_sys::Win32::UI::Shell::ShellExecuteW;
            use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
            let verb = wide_nul("open");
            let target = wide_nul(&url);
            let com = unsafe { CoInitializeEx(std::ptr::null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) };
            let result =
                unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), target.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) } as isize;
            if com >= 0 {
                unsafe { CoUninitialize() };
            }
            if result <= 32 {
                log!("framework UI: failed to open {url}: ShellExecuteW returned {result}");
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

impl UiHost for LiveHost {
    type Fonts = Option<SystemFonts>;

    fn ui_language(&self) -> Language {
        lang::ui()
    }

    fn words(&self, key: &str, args: &[(&str, &str)]) -> String {
        words_with(key, args)
    }

    fn words_in(&self, language: Language, key: &str, args: &[(&str, &str)]) -> String {
        loc::framework(language, key, args)
    }

    fn mod_text(&self, id: &str, key: &str) -> Option<String> {
        loc::mod_text_in(lang::ui(), id, key)
    }

    fn current_settings(&self) -> Settings {
        settings::current()
    }

    fn next_settings(&self) -> Settings {
        settings::next()
    }

    fn save_settings(&mut self, chosen: Settings, in_game: bool) -> Result<(), String> {
        settings::save(chosen, in_game)
    }

    fn display_refresh_rate(&self) -> u32 {
        crate::render::drawing::display_refresh_rate()
    }

    fn font_system(&self) -> Self::Fonts {
        SystemFonts::new().ok()
    }

    fn mod_cards(&self) -> &[mods_window::Card] {
        mods_window::cards()
    }

    fn notice_current(&self) -> Option<crate::base::notice::Showing> {
        crate::base::notice::current()
    }

    fn dismiss_notice(&mut self) {
        crate::base::notice::dismiss();
    }

    fn troubleshooting_state(&self) -> crate::base::troubleshooting::State {
        crate::base::troubleshooting::state()
    }

    fn start_troubleshooting(&mut self) {
        crate::base::troubleshooting::start();
    }

    fn dumps_status(&self) -> std::io::Result<local_dumps::Configuration> {
        local_dumps::status()
    }

    fn start_enable_dumps(&mut self) -> std::io::Result<()> {
        let request = std::thread::Builder::new().name("harmony Windows dump setup".into()).spawn(|| local_dumps::enable().map(|_| ()))?;
        self.dump_request = Some((DumpAction::Enable, request));
        Ok(())
    }

    fn start_disable_dumps(&mut self) -> std::io::Result<()> {
        let request = std::thread::Builder::new().name("harmony Windows dump cleanup".into()).spawn(|| local_dumps::disable().map(|_| ()))?;
        self.dump_request = Some((DumpAction::Disable, request));
        Ok(())
    }

    fn poll_dump_op(&mut self) -> Option<(DumpAction, std::io::Result<()>)> {
        if !self.dump_request.as_ref().is_some_and(|(_, request)| request.is_finished()) {
            return None;
        }
        let (action, request) = self.dump_request.take().expect("finished request");
        let outcome = request.join().unwrap_or_else(|_| Err(std::io::Error::other("dump worker thread stopped")));
        Some((action, outcome))
    }

    fn dump_pending(&self) -> Option<DumpAction> {
        self.dump_request.as_ref().map(|(action, _)| *action)
    }

    fn open_url(&mut self, url: &str) -> Result<(), String> {
        open_url_in_browser(url)
    }

    fn open_logs_folder(&mut self) -> Result<(), String> {
        let game = crate::base::game_dir().ok_or("the game folder is not known")?;
        let user = crate::base::paths::user_dir(game)?;
        let logs = eu4_harmony_folders::UserFolder::new(user).logs();
        let _ = std::fs::create_dir_all(&logs);
        open_url_in_browser(&logs.to_string_lossy())
    }

    fn request_restart(&mut self) -> Result<(), String> {
        crate::session::transition::request_restart()
    }
}

/// Self-contained framework UI shell: owns screen/scale/mouse, menu
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
    in_game_menu: bool,
    /// The notice on screen, by its number.
    notice: Option<(u64, Window)>,
    settings: Option<SettingsState<H::Fonts>>,
    mods: Option<ModsState>,
    restart_prompt: Option<Window>,
    discard_prompt: Option<Window>,
    dumps_status: DumpStatus,
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
static MAIN_MENU_FRAME: AtomicU64 = AtomicU64::new(u64::MAX);
static MODAL: AtomicBool = AtomicBool::new(false);

/// The main menu drew this frame; its corner bar shows while it does.
pub fn main_menu_frame() {
    MAIN_MENU_FRAME.store(FRAME.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// Whether a modal window (settings, mods, or restart confirmation) is open.
pub fn modal() -> bool {
    MODAL.load(Ordering::Relaxed)
}

/// What a corner bar item opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Opens {
    About,
    Mods,
    Settings,
}

/// One segment inside the unified Scheme A bottom-right corner bar.
struct CornerItem {
    rect: Rect,
    label: String,
    opens: Opens,
    is_brand: bool,
}

const CORNER_TEXT_SIZE: f32 = 18.0;

/// Formats a version badge with its optional localized city codename and beta
/// channel label (for example `v1.0.0 "Constantinople"` or
/// `v1.0.0「君士坦丁堡」内测版`).
fn format_version_badge(version: &str, codename: Option<&str>, channel: eu4_harmony_folders::InstallChannel, words_with: mods_window::Words) -> String {
    let base = format!("v{version}");
    let with_codename = match codename.filter(|c| !c.trim().is_empty()) {
        Some(name) => {
            let key = format!("EU4HARMONY_CODENAME_{}", name.trim().to_ascii_uppercase().replace([' ', '-'], "_"));
            let translated = words_with(&key, &[]);
            let shown = if translated == key { name.trim() } else { translated.as_str() };
            words_with("EU4HARMONY_VERSION_CODENAME_FORMAT", &[("VERSION", &base), ("CODENAME", shown)])
        }
        None => base,
    };
    if channel == eu4_harmony_folders::InstallChannel::Beta {
        format!("{with_codename} {}", words_with("EU4HARMONY_CHANNEL_BETA", &[]))
    } else {
        with_codename
    }
}

/// The active build's version badge for the corner bar and About page.
fn active_version_badge(words_with: mods_window::Words) -> String {
    format_version_badge(env!("CARGO_PKG_VERSION"), eu4_harmony_folders::VERSION_CODENAME, crate::base::paths::install_channel(), words_with)
}

/// The unified corner bar at the bottom right of the main menu and in-game
/// menu (`Harmony vX.Y.Z ["Codename"] [Beta] | Mods | Settings`), ordered
/// left-to-right.
fn corner(context: CornerContext, screen: (f32, f32), measure: &dyn Measure, words_with: mods_window::Words) -> Vec<CornerItem> {
    if context == CornerContext::None {
        return Vec::new();
    }
    let version = format!("Harmony {}", active_version_badge(words_with));
    let entries = vec![
        (version, Opens::About, true),
        (words_with("EU4HARMONY_MODS_BUTTON", &[]), Opens::Mods, false),
        (words_with("EU4HARMONY_SETTINGS_BUTTON", &[]), Opens::Settings, false),
    ];

    let pad_x = 16.0;
    let brand_extra = 14.0;
    let h = 38.0;
    let y = screen.1 - 54.0;
    let widths: Vec<f32> = entries
        .iter()
        .map(|(label, _, is_brand)| (measure.width(label, CORNER_TEXT_SIZE) + 2.0 * pad_x + if *is_brand { brand_extra } else { 0.0 }).round())
        .collect();
    let total_w: f32 = widths.iter().sum();
    let mut x = (screen.0 - 18.0 - total_w).max(8.0).round();
    entries
        .into_iter()
        .zip(widths)
        .map(|((label, opens, is_brand), w)| {
            let rect = Rect { x, y, w, h };
            x += w;
            CornerItem { rect, label, opens, is_brand }
        })
        .collect()
}

/// Renders a Scheme A floating tooltip card near the cursor, flipping left or
/// upward to remain within `screen`.
fn draw_tooltip(out: &mut Vec<Command>, measure: &dyn Measure, screen: (f32, f32), mouse: (f32, f32), text: &str) {
    const TIP_SIZE: f32 = 14.5;
    const TIP_LINE: f32 = TIP_SIZE + 5.5;
    const PAD_X: f32 = 12.0;
    const PAD_Y: f32 = 9.0;
    const MAX_W: f32 = 340.0;
    const MARGIN: f32 = 8.0;

    let wrap_w = (MAX_W - 2.0 * PAD_X).min((screen.0 - 2.0 * MARGIN - 2.0 * PAD_X).max(60.0));
    let mut lines = Vec::new();
    for para in text.split('\n') {
        if para.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(measure.wrap(para, TIP_SIZE, wrap_w));
        }
    }
    if lines.is_empty() {
        return;
    }
    let widest = lines.iter().map(|line| measure.width(line, TIP_SIZE)).fold(0.0_f32, f32::max);
    let w = (widest + 2.0 * PAD_X).ceil();
    let h = (lines.len() as f32 * TIP_LINE + 2.0 * PAD_Y - 4.0).ceil();

    let mut x = mouse.0 + 14.0;
    if x + w > screen.0 - MARGIN {
        x = mouse.0 - w - 10.0;
    }
    x = x.clamp(MARGIN, (screen.0 - w - MARGIN).max(MARGIN)).round();

    let mut y = mouse.1 + 18.0;
    if y + h > screen.1 - MARGIN {
        y = mouse.1 - h - 10.0;
    }
    y = y.clamp(MARGIN, (screen.1 - h - MARGIN).max(MARGIN)).round();

    let rect = Rect { x, y, w, h };
    out.push(Command::RoundFill { rect: Rect { x: x + 1.0, y: y + 2.0, w, h }, color: 0x55000000, radius: 6.0 });
    out.push(Command::RoundFill { rect, color: widgets::LIST, radius: 5.0 });
    out.push(Command::RoundStroke { rect, color: widgets::BORDER_GOLD_SOFT, radius: 5.0, thickness: 1.0 });
    for (n, line) in lines.into_iter().enumerate() {
        out.push(Command::text(x + PAD_X, y + PAD_Y + n as f32 * TIP_LINE - 1.0, line, TIP_SIZE, widgets::TEXT));
    }
}

/// The settings window's width, and its side list's, which chooses among
/// its pages.
const SETTINGS_WIDTH: f32 = 800.0;
const SETTINGS_LIST: f32 = 204.0;

/// Tooltip and footer key for settings that cannot be changed during a game.
const MAIN_MENU_ONLY_KEY: &str = "EU4HARMONY_SETTINGS_MAIN_MENU_ONLY";

fn main_menu_only_tooltip() -> eu4_harmony_lua::Text {
    eu4_harmony_lua::Text::key(MAIN_MENU_ONLY_KEY.into())
}

fn disable_widget(widget: Widget) -> Widget {
    if matches!(widget, Widget::Gap | Widget::Divider | Widget::PageHeader(_)) {
        widget
    } else {
        widget.with_enabled(false).with_tooltip(main_menu_only_tooltip())
    }
}

fn disable_main_menu_only(widgets: Vec<Widget>) -> Vec<Widget> {
    widgets.into_iter().map(disable_widget).collect()
}

/// The widgets of the settings in [`settings::SETTINGS`] that `page` shows,
/// in their order there, showing the values `shown` has on a `display_hz`
/// display (with frame-rate choice options anchored to `options_base` so
/// rebuilding preserves choice indices for [`read_back`]); each interactive
/// widget's id is its setting's key. When `in_game` is true, settings that
/// cannot be modified during a session are disabled.
fn setting_widgets(page: Page, shown: &Settings, options_base: &Settings, display_hz: u32, in_game: bool, words_with: mods_window::Words) -> Vec<Widget> {
    let words = |key: &str| words_with(key, &[]);
    let mut out = Vec::new();
    for (idx, setting) in settings::SETTINGS.iter().filter(|s| s.page == page).enumerate() {
        if idx > 0 {
            out.push(Widget::Divider);
        }
        let enabled = !in_game || setting.editable_in_game();
        let row = match setting.shows {
            Shows::Toggle { label, field } => Widget::Toggle { id: setting.key, label: words(label), on: (field.get)(shown) },
            Shows::FrameRate { label, unlimited_tooltip, field } => {
                let base_limit = (field.get)(options_base);
                let (choices, initial) = settings::frame_rate_choices(base_limit, display_hz);
                let current_limit = (field.get)(shown);
                let display = if settings::FRAME_RATE_BOUNDS.contains(&display_hz) { display_hz } else { 60 };
                let selected = if current_limit == base_limit {
                    initial
                } else {
                    match current_limit {
                        FrameRateLimit::Display => choices.iter().position(|&c| c == FrameRateLimit::Hz(display)).unwrap_or(initial),
                        FrameRateLimit::Unlimited => choices.len() - 1,
                        FrameRateLimit::Hz(r) => choices.iter().position(|&c| c == FrameRateLimit::Hz(r)).unwrap_or(initial),
                    }
                };
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
                        FrameRateLimit::Unlimited if enabled => eu4_harmony_lua::Text::key(unlimited_tooltip.into()),
                        _ => Default::default(),
                    })
                    .collect();
                Widget::Labeled { label: words(label), status: None, control: Box::new(Widget::Choice { id: setting.key, options, tooltips, selected }) }
            }
            Shows::Slider { label, min, max, format, field, .. } => Widget::Labeled {
                label: words(label),
                status: None,
                control: Box::new(Widget::Slider { id: setting.key, min, max, value: (field.get)(shown).clamp(min, max), format: SliderFormat::Fn(format) }),
            },
            Shows::GameSpeed { label, field } => {
                let current = (field.get)(shown);
                let index = current.slider_index();
                let mut labels: Vec<String> = settings::GAME_SPEED_PRESETS
                    .iter()
                    .map(|&rate| {
                        let text = GameSpeed::format_rate(rate);
                        words_with("EU4HARMONY_SETTINGS_GAME_SPEED_RATE", &[("RATE", &text)])
                    })
                    .collect();
                labels.push(words("EU4HARMONY_SETTINGS_GAME_SPEED_UNLIMITED"));
                if !current.is_preset() {
                    if let Some(rate) = current.rate() {
                        let text = GameSpeed::format_rate(rate);
                        if let Some(slot) = labels.get_mut(index as usize) {
                            *slot = words_with("EU4HARMONY_SETTINGS_GAME_SPEED_RATE", &[("RATE", &text)]);
                        }
                    }
                }
                Widget::Labeled {
                    label: words(label),
                    status: None,
                    control: Box::new(Widget::Slider {
                        id: setting.key,
                        min: 0,
                        max: settings::GAME_SPEED_PRESETS.len() as u32,
                        value: index,
                        format: SliderFormat::Labels(labels),
                    }),
                }
            }
        };
        out.push(if enabled { row } else { disable_widget(row) });
    }
    out
}

/// A page with nothing but settings from [`settings::SETTINGS`].
fn settings_page(page: Page, shown: &Settings, options_base: &Settings, display_hz: u32, in_game: bool, words_with: mods_window::Words) -> Vec<Widget> {
    let mut out = vec![Widget::PageHeader(words_with(page.title(), &[]))];
    out.extend(setting_widgets(page, shown, options_base, display_hz, in_game, words_with));
    out
}

/// Where problems with the framework are reported, as the About page names it.
const ISSUES: &str = "https://github.com/KisinTheFlame/eu4-harmony/issues";

/// Interactive widget IDs on the Advanced/About pages and settings/confirmation dialogs.
const OPEN_ISSUES: &str = "open_issues";
const OPEN_LOGS: &str = "open_logs";
const TROUBLESHOOTING: &str = "troubleshooting";
const ENABLE_DUMPS: &str = "enable_windows_dumps";
const DISABLE_DUMPS: &str = "disable_windows_dumps";
const RESET_PAGE: &str = "reset_page";
const SAVE: &str = "save";
const CANCEL: &str = "cancel";
const RESTART_NOW: &str = "restart_now";
const RESTART_LATER: &str = "restart_later";
const DISCARD_CONFIRM: &str = "discard_confirm";
const DISCARD_CANCEL: &str = "discard_cancel";

#[derive(Clone, Debug, PartialEq, Eq)]
enum DumpStatus {
    NotConfigured,
    Harmony,
    External,
    Pending,
    Cancelled(DumpAction),
    Failed(DumpAction, String),
}

impl DumpStatus {
    fn from_status(result: std::io::Result<local_dumps::Configuration>) -> Self {
        match result {
            Ok(local_dumps::Configuration::NotConfigured) => Self::NotConfigured,
            Ok(local_dumps::Configuration::Harmony) => Self::Harmony,
            Ok(local_dumps::Configuration::External) => Self::External,
            Err(e) if e.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_CANCELLED as i32) => Self::Cancelled(DumpAction::Enable),
            Err(e) => Self::Failed(DumpAction::Enable, e.to_string()),
        }
    }

    fn from_op(action: DumpAction, result: std::io::Result<()>) -> Self {
        match result {
            Ok(()) => match action {
                DumpAction::Enable => Self::Harmony,
                DumpAction::Disable => Self::NotConfigured,
            },
            Err(e) if e.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_CANCELLED as i32) => Self::Cancelled(action),
            Err(e) => Self::Failed(action, e.to_string()),
        }
    }
}

/// The Advanced page: opt-in remote control toggle (main-menu only), log
/// folder shortcut, and Windows crash dump management.
fn advanced_page(
    shown: &Settings,
    display_hz: u32,
    dumps: &DumpStatus,
    in_game: bool,
    words: &dyn Fn(&str) -> String,
    words_with: mods_window::Words,
) -> Vec<Widget> {
    let mut page = vec![Widget::PageHeader(words("EU4HARMONY_SETTINGS_PAGE_ADVANCED"))];
    page.extend(setting_widgets(Page::Advanced, shown, shown, display_hz, in_game, words_with));
    page.push(Widget::Gap);
    page.push(Widget::Divider);
    page.push(Widget::Gap);
    page.push(Widget::Button { id: OPEN_LOGS, label: words("EU4HARMONY_ADVANCED_OPEN_LOGS"), style: ButtonStyle::Normal });
    page.push(Widget::Gap);
    page.push(Widget::Divider);
    page.push(Widget::Gap);
    let enable_button = || Widget::Button { id: ENABLE_DUMPS, label: words("EU4HARMONY_DUMPS_ENABLE"), style: ButtonStyle::Normal };
    let disable_button = || Widget::Button { id: DISABLE_DUMPS, label: words("EU4HARMONY_DUMPS_DISABLE"), style: ButtonStyle::Danger };
    match dumps {
        DumpStatus::NotConfigured => {
            page.push(enable_button());
        }
        DumpStatus::Harmony => {
            page.push(Widget::Label(words("EU4HARMONY_DUMPS_CONFIGURED")));
            page.push(disable_button());
        }
        DumpStatus::External => {
            page.push(Widget::Label(words("EU4HARMONY_DUMPS_EXTERNAL")));
        }
        DumpStatus::Pending => {
            page.push(Widget::Label(words("EU4HARMONY_DUMPS_PENDING")));
        }
        DumpStatus::Cancelled(DumpAction::Enable) => {
            page.push(enable_button());
            page.push(Widget::Label(words("EU4HARMONY_DUMPS_CANCELLED")));
        }
        DumpStatus::Cancelled(DumpAction::Disable) => {
            page.push(Widget::Label(words("EU4HARMONY_DUMPS_CONFIGURED")));
            page.push(disable_button());
            page.push(Widget::Label(words("EU4HARMONY_DUMPS_CANCELLED")));
        }
        DumpStatus::Failed(DumpAction::Enable, e) => {
            page.push(enable_button());
            page.push(Widget::Label(words_with("EU4HARMONY_DUMPS_FAILED", &[("ERROR", e)])));
        }
        DumpStatus::Failed(DumpAction::Disable, e) => {
            page.push(Widget::Label(words("EU4HARMONY_DUMPS_CONFIGURED")));
            page.push(disable_button());
            page.push(Widget::Label(words_with("EU4HARMONY_DUMPS_DISABLE_FAILED", &[("ERROR", e)])));
        }
    }
    page
}

/// The About page: brand hero card with version badge and clickable issue
/// tracker link, compact troubleshooting export button, and privacy note.
fn about_page(troubleshooting: &crate::base::troubleshooting::State, words: &dyn Fn(&str) -> String, words_with: mods_window::Words) -> Vec<Widget> {
    vec![
        Widget::PageHeader(words("EU4HARMONY_SETTINGS_PAGE_ABOUT")),
        Widget::AboutHero {
            name: "Harmony".into(),
            version: active_version_badge(words_with),
            link_id: OPEN_ISSUES,
            link_label: words_with("EU4HARMONY_ABOUT_ISSUES", &[("ISSUES", ISSUES)]),
        },
        Widget::Button { id: TROUBLESHOOTING, label: words("EU4HARMONY_ABOUT_TROUBLESHOOTING"), style: ButtonStyle::Normal },
        Widget::Label(troubleshooting_line(troubleshooting, words_with)),
        Widget::Gap,
        Widget::Label(words("EU4HARMONY_ABOUT_TROUBLESHOOTING_PRIVACY")),
    ]
}

/// What the troubleshooting files' packing has come to, as the line under
/// its button says it.
fn troubleshooting_line(state: &crate::base::troubleshooting::State, words_with: mods_window::Words) -> String {
    use crate::base::troubleshooting::State;
    match state {
        State::Idle => String::new(),
        State::Packing => words_with("EU4HARMONY_ABOUT_PACKING", &[]),
        State::Saved(file) => words_with("EU4HARMONY_ABOUT_SAVED", &[("FILE", file)]),
        State::Failed(why) => words_with("EU4HARMONY_ABOUT_NOT_SAVED", &[("ERROR", why)]),
    }
}

/// Footer slot index holding the status/restart/error notice above the action bar.
const RESTART_LINE: usize = 0;

/// Whether the active page in `w` can be reset to defaults right now.
fn page_resettable(w: &Window, in_game: bool) -> bool {
    let Some(page) = w.side.as_ref().and_then(|s| Page::ALL.get(s.selected)).copied() else { return false };
    page != Page::About && (!in_game || page.editable_in_game())
}

/// The line at [`RESTART_LINE`]: in the main menu, says when a change waits
/// for the next start; in a game, says when the viewed page can only be
/// changed from the main menu; empty otherwise so the buttons stay put.
fn footer_notice(
    w: &Window,
    current: &Settings,
    next: &Settings,
    fonts: &FontsPage<impl FontSystem>,
    display_hz: u32,
    in_game: bool,
    words: &dyn Fn(&str) -> String,
) -> Widget {
    if in_game {
        let locked = w.side.as_ref().and_then(|s| Page::ALL.get(s.selected)).is_some_and(|p| !p.editable_in_game());
        return Widget::Label(if locked { words(MAIN_MENU_ONLY_KEY) } else { String::new() });
    }
    let waits = current.waits_for_restart(&chosen_settings(w, next, fonts, display_hz, false));
    Widget::Label(if waits { words("EU4HARMONY_SETTINGS_RESTART") } else { String::new() })
}

/// Takes into `chosen` what the settings window `w` shows of the settings
/// in [`settings::SETTINGS`]; one it does not show (or that is disabled in
/// a game) keeps its value.
fn read_back(w: &Window, next: &Settings, display_hz: u32, in_game: bool, chosen: &mut Settings) {
    for setting in &settings::SETTINGS {
        if in_game && !setting.editable_in_game() {
            continue;
        }
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
            Shows::Slider { field, .. } => {
                if let Some(value) = w.value(setting.key) {
                    (field.set)(chosen, value);
                }
            }
            Shows::GameSpeed { field, .. } => {
                if let Some(index) = w.value(setting.key) {
                    let before = (field.get)(next);
                    if index == before.slider_index() {
                        (field.set)(chosen, before);
                    } else {
                        (field.set)(chosen, GameSpeed::from_slider_index(index));
                    }
                }
            }
        }
    }
}

/// The settings window showing `draft` (the values currently displayed, anchored
/// to `opened_next` for frame-rate choice options and read-back), to a session
/// running with `current`; its language and fonts page is `fonts`. When `in_game`
/// is true, pages unavailable during a session are disabled and the first
/// editable page is selected initially unless `initial_page` requests another.
#[expect(clippy::too_many_arguments, reason = "builds the settings window from the current snapshot, draft values, host status and wording seam")]
fn settings_window(
    current: &Settings,
    opened_next: &Settings,
    draft: &Settings,
    fonts: &FontsPage<impl FontSystem>,
    display_hz: u32,
    dumps: &DumpStatus,
    troubleshooting: &crate::base::troubleshooting::State,
    in_game: bool,
    initial_page: Option<Page>,
    words_with: mods_window::Words,
    words_in: &dyn Fn(Language, &str) -> String,
) -> Window {
    let words = |key: &str| words_with(key, &[]);
    let pages = Page::ALL
        .into_iter()
        .map(|page| match page {
            Page::LanguageFonts => {
                let widgets = fonts.widgets(draft, &words, words_in);
                if in_game {
                    disable_main_menu_only(widgets)
                } else {
                    widgets
                }
            }
            Page::Advanced => advanced_page(draft, display_hz, dumps, in_game, &words, words_with),
            Page::About => about_page(troubleshooting, &words, words_with),
            _ => settings_page(page, draft, opened_next, display_hz, in_game, words_with),
        })
        .collect();
    let footer = vec![
        Widget::Label(String::new()),
        Widget::ButtonRow(vec![
            RowButton { id: RESET_PAGE, label: words("EU4HARMONY_SETTINGS_RESET_PAGE"), style: ButtonStyle::Ghost, enabled: true, align_right: false },
            RowButton { id: CANCEL, label: words("EU4HARMONY_SETTINGS_CANCEL"), style: ButtonStyle::Normal, enabled: true, align_right: true },
            RowButton { id: SAVE, label: words("EU4HARMONY_SETTINGS_SAVE"), style: ButtonStyle::Primary, enabled: true, align_right: true },
        ]),
    ];
    let side = Side::new("pages", Page::ALL.iter().map(|page| words(page.title())).collect(), SETTINGS_LIST);
    let mut window = Window::new(words("EU4HARMONY_SETTINGS_TITLE"), Vec::new(), SETTINGS_WIDTH).with_side(side).with_pages(pages).with_footer(footer);
    fonts.apply_option_faces(&mut window);
    if let Some(target) = initial_page {
        if let Some(index) = Page::ALL.iter().position(|&p| p == target) {
            window.select_page(index);
        }
    } else if in_game {
        if let Some(first_editable) = Page::ALL.iter().position(|p| p.editable_in_game()) {
            window.select_page(first_editable);
        }
    }
    let resettable = page_resettable(&window, in_game);
    window.set_row_button_enabled(RESET_PAGE, resettable);
    window.footer_mut()[RESTART_LINE] = footer_notice(&window, current, opened_next, fonts, display_hz, in_game, &words);
    window
}

/// Modal confirmation dialog shown after saving restart-required settings from
/// the main menu.
fn restart_prompt_window(words: &dyn Fn(&str) -> String) -> Window {
    let body = vec![Widget::Label(words("EU4HARMONY_RESTART_PROMPT_BODY"))];
    let footer = vec![Widget::ButtonRow(vec![
        RowButton { id: RESTART_LATER, label: words("EU4HARMONY_RESTART_LATER"), style: ButtonStyle::Normal, enabled: true, align_right: true },
        RowButton { id: RESTART_NOW, label: words("EU4HARMONY_RESTART_NOW"), style: ButtonStyle::Primary, enabled: true, align_right: true },
    ])];
    Window::new(words("EU4HARMONY_RESTART_PROMPT_TITLE"), body, 460.0).with_footer(footer)
}

/// Modal confirmation dialog shown when closing Settings with unsaved changes.
fn discard_prompt_window(words: &dyn Fn(&str) -> String) -> Window {
    let body = vec![Widget::Label(words("EU4HARMONY_DISCARD_PROMPT_BODY"))];
    let footer = vec![Widget::ButtonRow(vec![
        RowButton { id: DISCARD_CANCEL, label: words("EU4HARMONY_DISCARD_CANCEL"), style: ButtonStyle::Normal, enabled: true, align_right: true },
        RowButton { id: DISCARD_CONFIRM, label: words("EU4HARMONY_DISCARD_CONFIRM"), style: ButtonStyle::Danger, enabled: true, align_right: true },
    ])];
    Window::new(words("EU4HARMONY_DISCARD_PROMPT_TITLE"), body, 460.0).with_footer(footer)
}

/// The settings the settings window `w` saves: `next`, which it was opened
/// with, as `w` changed it.
fn chosen_settings(w: &Window, next: &Settings, fonts: &FontsPage<impl FontSystem>, display_hz: u32, in_game: bool) -> Settings {
    let mut chosen = next.clone();
    if !in_game {
        fonts.read_back(w, &mut chosen);
    }
    read_back(w, next, display_hz, in_game, &mut chosen);
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

/// Looks up framework wording in `language` when previewing a new language,
/// or in the host's current UI language otherwise.
fn host_words<H: UiHost>(host: &H, language: Option<Language>, key: &str, args: &[(&str, &str)]) -> String {
    match language {
        Some(language) => host.words_in(language, key, args),
        None => host.words(key, args),
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
            in_game_menu: false,
            notice: None,
            settings: None,
            mods: None,
            restart_prompt: None,
            discard_prompt: None,
            dumps_status: DumpStatus::NotConfigured,
            last_dump_outcome: None,
        }
    }

    fn modal(&self) -> bool {
        self.restart_prompt.is_some() || self.discard_prompt.is_some() || self.mods.is_some() || self.settings.is_some()
    }

    /// Single source of truth for the topmost modal window, shared by drawing,
    /// event dispatch, and tooltip hit-testing.
    fn active_modal(&self) -> Option<&Window> {
        self.restart_prompt.as_ref().or(self.discard_prompt.as_ref()).or(self.mods.as_ref().map(|m| &m.window)).or(self.settings.as_ref().map(|s| &s.window))
    }

    fn in_main_menu(&self) -> bool {
        self.main_menu_frame.is_some_and(|menu| self.frame.saturating_sub(menu) <= 2)
    }

    fn corner_context(&self) -> CornerContext {
        if self.in_main_menu() {
            CornerContext::MainMenu
        } else if self.in_game_menu {
            CornerContext::InGameMenu
        } else {
            CornerContext::None
        }
    }

    fn sync_frame(&mut self, frame: u64, main_menu_frame: Option<u64>, in_game_menu: bool, pixels: (f32, f32), scale: f32) {
        self.frame = frame;
        self.main_menu_frame = main_menu_frame;
        self.in_game_menu = in_game_menu;
        self.scale = scale;
        let next_screen = (pixels.0 / scale, pixels.1 / scale);
        if next_screen != self.screen {
            self.screen = next_screen;
            if let Some((_, window)) = self.notice.as_mut() {
                window.set_screen(next_screen.0, next_screen.1);
            }
            if let Some(settings) = self.settings.as_mut() {
                settings.window.set_screen(next_screen.0, next_screen.1);
            }
            if let Some(mods) = self.mods.as_mut() {
                mods.window.set_screen(next_screen.0, next_screen.1);
            }
            if let Some(prompt) = self.restart_prompt.as_mut() {
                prompt.set_screen(next_screen.0, next_screen.1);
            }
            if let Some(prompt) = self.discard_prompt.as_mut() {
                prompt.set_screen(next_screen.0, next_screen.1);
            }
        }
    }

    fn display_language(&self) -> Language {
        self.settings.as_ref().and_then(|s| s.preview_language).unwrap_or_else(|| self.host.ui_language())
    }

    fn open_settings(&mut self, measure: &dyn Measure) {
        self.open_settings_at(measure, None);
    }

    fn open_settings_at(&mut self, measure: &dyn Measure, initial_page: Option<Page>) {
        let in_game = self.corner_context() == CornerContext::InGameMenu;
        let next = self.host.next_settings();
        let current = self.host.current_settings();
        let fonts = FontsPage::new(self.host.font_system(), &next);
        let display_hz = self.host.display_refresh_rate();
        let dumps = if self.host.dump_pending().is_some() {
            DumpStatus::Pending
        } else if let Some(outcome) = self.last_dump_outcome.take() {
            outcome
        } else {
            DumpStatus::from_status(self.host.dumps_status())
        };
        self.dumps_status = dumps.clone();
        let troubleshooting = self.host.troubleshooting_state();
        let ww = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
        let wi = |language: Language, key: &str| self.host.words_in(language, key, &[]);
        let mut window = settings_window(&current, &next, &next, &fonts, display_hz, &dumps, &troubleshooting, in_game, initial_page, &ww, &wi);
        window.center(measure, self.screen.0, self.screen.1);
        let opened_draft = chosen_settings(&window, &next, &fonts, display_hz, in_game);
        self.settings = Some(SettingsState { window, opened_next: next, opened_draft, fonts, preview_language: None, display_hz, in_game });
    }

    fn show_dump_status(&mut self, status: &DumpStatus) {
        self.dumps_status = status.clone();
        let index = Page::ALL.iter().position(|&p| p == Page::Advanced).expect("Advanced page");
        let Some(settings) = self.settings.as_mut() else { return };
        let preview = settings.preview_language;
        let host = &self.host;
        let w = |key: &str| host_words(host, preview, key, &[]);
        let ww = |key: &str, args: &[(&str, &str)]| host_words(host, preview, key, args);
        let draft = chosen_settings(&settings.window, &settings.opened_next, &settings.fonts, settings.display_hz, settings.in_game);
        let widgets = advanced_page(&draft, settings.display_hz, status, settings.in_game, &w, &ww);
        replace_page(&mut settings.window, index, widgets);
    }

    /// Rebuilds the open settings window in its current preview language while
    /// preserving all draft edits across pages, the active page, and window
    /// position/scroll.
    fn rebuild_settings_window(&mut self, measure: &dyn Measure, draft: &Settings, active_page: Option<Page>) {
        let current = self.host.current_settings();
        let troubleshooting = self.host.troubleshooting_state();
        let dumps = self.dumps_status.clone();
        let Some(settings) = self.settings.as_mut() else { return };
        let preview = settings.preview_language;
        let host = &self.host;
        let ww = |key: &str, args: &[(&str, &str)]| host_words(host, preview, key, args);
        let wi = |language: Language, key: &str| host.words_in(language, key, &[]);
        let mut rebuilt = settings_window(
            &current,
            &settings.opened_next,
            draft,
            &settings.fonts,
            settings.display_hz,
            &dumps,
            &troubleshooting,
            settings.in_game,
            active_page,
            &ww,
            &wi,
        );
        rebuilt.restore_view_from(&settings.window, measure);
        settings.window = rebuilt;
    }

    /// The player chose the option at `index` in the settings window's
    /// drop-down list or choice bar `id`.
    fn settings_chose(&mut self, measure: &dyn Measure, id: &str, index: usize) {
        let language_change = self.settings.as_ref().and_then(|settings| {
            if settings.in_game || id != "language" {
                return None;
            }
            let &new_lang = Language::ALL.get(index)?;
            if new_lang == settings.fonts.language() {
                return None;
            }
            let active_page = settings.window.side.as_ref().and_then(|s| Page::ALL.get(s.selected).copied());
            let draft = chosen_settings(&settings.window, &settings.opened_next, &settings.fonts, settings.display_hz, false);
            Some((new_lang, draft, active_page))
        });
        if let Some((new_lang, mut draft, active_page)) = language_change {
            let host = &self.host;
            let w = |key: &str| host.words_in(new_lang, key, &[]);
            let wi = |language: Language, key: &str| host.words_in(language, key, &[]);
            if let Some(settings) = self.settings.as_mut() {
                settings.preview_language = Some(new_lang);
                settings.fonts.chose(&mut settings.window, id, index, &w, &wi);
                draft.language = Some(new_lang);
            }
            self.rebuild_settings_window(measure, &draft, active_page);
            return;
        }
        let Some(settings) = self.settings.as_mut() else { return };
        let preview = settings.preview_language;
        let host = &self.host;
        let w = |key: &str| host_words(host, preview, key, &[]);
        let wi = |language: Language, key: &str| host.words_in(language, key, &[]);
        if !settings.in_game {
            settings.fonts.chose(&mut settings.window, id, index, &w, &wi);
        }
        let resettable = page_resettable(&settings.window, settings.in_game);
        settings.window.set_row_button_enabled(RESET_PAGE, resettable);
    }

    fn settings_switched(&mut self, id: &str, on: bool) {
        if id == "map_outline" {
            if let Some(settings) = self.settings.as_mut() {
                settings.fonts.set_map_outline(&mut settings.window, on);
            }
        }
        self.settings_changed();
    }

    fn settings_changed(&mut self) {
        let current = self.host.current_settings();
        let Some(settings) = self.settings.as_mut() else { return };
        let preview = settings.preview_language;
        let host = &self.host;
        let w = |key: &str| host_words(host, preview, key, &[]);
        settings.window.footer_mut()[RESTART_LINE] =
            footer_notice(&settings.window, &current, &settings.opened_next, &settings.fonts, settings.display_hz, settings.in_game, &w);
    }

    /// Resets only the currently active settings page in the open draft window.
    fn reset_active_page(&mut self) {
        let dumps = self.dumps_status.clone();
        let Some(settings) = self.settings.as_mut() else { return };
        if !page_resettable(&settings.window, settings.in_game) {
            return;
        }
        let preview = settings.preview_language;
        let host = &self.host;
        let w = |key: &str| host_words(host, preview, key, &[]);
        let ww = |key: &str, args: &[(&str, &str)]| host_words(host, preview, key, args);
        let wi = |language: Language, key: &str| host.words_in(language, key, &[]);
        let Some(index) = settings.window.side.as_ref().map(|s| s.selected) else { return };
        let Some(&page) = Page::ALL.get(index) else { return };
        let defaults = Settings::default();
        // Reset the page's read-back baseline to defaults so manual values
        // snapping to the default slider/choice index (e.g. `game_speed_4=5.1`
        // or `FrameRateLimit::Hz(display_hz)`) are cleared rather than preserved.
        for setting in settings::SETTINGS.iter().filter(|s| s.page == page && (!settings.in_game || s.editable_in_game())) {
            match setting.shows {
                Shows::Toggle { field, .. } => (field.set)(&mut settings.opened_next, (field.get)(&defaults)),
                Shows::FrameRate { field, .. } => (field.set)(&mut settings.opened_next, (field.get)(&defaults)),
                Shows::Slider { field, .. } => (field.set)(&mut settings.opened_next, (field.get)(&defaults)),
                Shows::GameSpeed { field, .. } => (field.set)(&mut settings.opened_next, (field.get)(&defaults)),
            }
        }
        match page {
            Page::LanguageFonts => {
                settings.fonts.reset_defaults(&mut settings.window, &w, &wi);
            }
            Page::Advanced => {
                let widgets = advanced_page(&defaults, settings.display_hz, &dumps, settings.in_game, &w, &ww);
                replace_page(&mut settings.window, index, widgets);
            }
            Page::About => {}
            _ => {
                let widgets = settings_page(page, &defaults, &settings.opened_next, settings.display_hz, settings.in_game, &ww);
                replace_page(&mut settings.window, index, widgets);
            }
        }
        self.settings_changed();
    }

    fn settings_dirty(&self) -> bool {
        let Some(settings) = self.settings.as_ref() else { return false };
        chosen_settings(&settings.window, &settings.opened_next, &settings.fonts, settings.display_hz, settings.in_game) != settings.opened_draft
    }

    fn request_close_settings(&mut self, measure: &dyn Measure) {
        if self.settings_dirty() {
            let preview = self.settings.as_ref().and_then(|s| s.preview_language);
            let w = |key: &str| host_words(&self.host, preview, key, &[]);
            let mut prompt = discard_prompt_window(&w);
            prompt.center(measure, self.screen.0, self.screen.1);
            self.discard_prompt = Some(prompt);
        } else {
            self.close_settings();
        }
    }

    fn save_and_close_settings(&mut self, measure: &dyn Measure) {
        let Some(settings) = self.settings.as_ref() else { return };
        let chosen = chosen_settings(&settings.window, &settings.opened_next, &settings.fonts, settings.display_hz, settings.in_game);
        let in_game = settings.in_game;
        let preview = settings.preview_language;
        let needs_restart = !in_game && self.host.current_settings().waits_for_restart(&chosen);
        match self.host.save_settings(chosen.clone(), in_game) {
            Ok(()) => {
                log!("settings saved: {chosen:?}");
                self.settings = None;
                if needs_restart {
                    let w = |key: &str| self.host.words(key, &[]);
                    let mut prompt = restart_prompt_window(&w);
                    prompt.center(measure, self.screen.0, self.screen.1);
                    self.restart_prompt = Some(prompt);
                }
            }
            Err(e) => {
                log!("settings not saved: {e}");
                let line = host_words(&self.host, preview, "EU4HARMONY_SETTINGS_SAVE_FAILED", &[("ERROR", &e)]);
                if let Some(settings) = self.settings.as_mut() {
                    settings.window.footer_mut()[RESTART_LINE] = Widget::Label(line);
                }
            }
        }
    }

    fn close_settings(&mut self) {
        self.discard_prompt = None;
        self.settings = None;
    }

    fn open_mods(&mut self, measure: &dyn Measure) {
        let in_game = self.corner_context() == CornerContext::InGameMenu;
        let ww = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
        let mt = |id: &str, key: &str| self.host.mod_text(id, key);
        let mut window = mods_window::window(self.host.mod_cards(), &ww, &mt);
        window.center(measure, self.screen.0, self.screen.1);
        self.mods = Some(ModsState { window, in_game });
    }

    fn mod_chosen(&mut self, index: usize) {
        let ww = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
        let mt = |id: &str, key: &str| self.host.mod_text(id, key);
        let Some(card) = self.host.mod_cards().get(index) else { return };
        let body = mods_window::details(card, &ww, &mt);
        if let Some(mods) = self.mods.as_mut() {
            mods.window.body = body;
        }
    }

    fn close_mods(&mut self) {
        self.mods = None;
    }

    /// Reconciles notices, in-game modal lifetime, async dump operations
    /// (even after Settings closed), and troubleshooting packing status once
    /// per frame.
    fn tick(&mut self, measure: &dyn Measure) {
        if self.settings.as_ref().is_some_and(|s| s.in_game) && !self.in_game_menu {
            self.close_settings();
        }
        if self.mods.as_ref().is_some_and(|m| m.in_game) && !self.in_game_menu {
            self.close_mods();
        }
        match (self.host.notice_current(), &mut self.notice) {
            (None, _) => self.notice = None,
            (Some(showing), Some((shown, window))) if *shown == showing.number => {
                window.title = showing.title;
                let same_body = matches!(window.body.as_slice(), [Widget::Label(text)] if *text == showing.text);
                if !same_body {
                    window.body = vec![Widget::Label(showing.text)];
                    window.clamp_to_screen(measure);
                }
            }
            (Some(showing), _) => {
                let mut window = Window::new(showing.title, vec![Widget::Label(showing.text)], 460.0);
                window.center(measure, self.screen.0, self.screen.1 * 0.6);
                self.notice = Some((showing.number, window));
            }
        }
        if let Some((action, result)) = self.host.poll_dump_op() {
            if let Err(ref e) = result {
                log!("Windows dump {action:?}: {e}");
            }
            let status = DumpStatus::from_op(action, result);
            if self.settings.is_some() {
                self.show_dump_status(&status);
            } else {
                self.last_dump_outcome = Some(status);
            }
        }
        if let Some(preview) = self.settings.as_ref().map(|s| s.preview_language) {
            let line = troubleshooting_line(&self.host.troubleshooting_state(), &|key, args| host_words(&self.host, preview, key, args));
            if let Some(settings) = self.settings.as_mut() {
                settings.window.set_label_after(TROUBLESHOOTING, line);
            }
        }
    }

    fn event(&mut self, measure: &dyn Measure, event: input::Event) -> bool {
        use input::Event::*;
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
        if let Some(prompt) = self.restart_prompt.as_mut() {
            match Self::modal_event(prompt, measure, event) {
                Some(Action::Pressed(RESTART_NOW)) => {
                    self.restart_prompt = None;
                    if let Err(e) = self.host.request_restart() {
                        log!("framework UI: restart request failed: {e}");
                        crate::base::notice::post(
                            "restart failed",
                            crate::base::notice::Repeat::Always,
                            crate::base::notice::Message::new("EU4HARMONY_RESTART_FAILED"),
                        );
                    }
                }
                Some(Action::Pressed(RESTART_LATER)) | Some(Action::Closed) => {
                    self.restart_prompt = None;
                }
                _ => {}
            }
            return true;
        }
        if let Some(prompt) = self.discard_prompt.as_mut() {
            match Self::modal_event(prompt, measure, event) {
                Some(Action::Pressed(DISCARD_CONFIRM)) => self.close_settings(),
                Some(Action::Pressed(DISCARD_CANCEL)) | Some(Action::Closed) => {
                    self.discard_prompt = None;
                }
                _ => {}
            }
            return true;
        }
        if let Some(mods) = self.mods.as_mut() {
            match Self::modal_event(&mut mods.window, measure, event) {
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
                    self.settings_chose(measure, id, index);
                    self.settings_changed();
                }
                Some(Action::Switched(id, on)) => self.settings_switched(id, on),
                Some(Action::Slid(..)) => self.settings_changed(),
                Some(Action::Pressed(SAVE)) => self.save_and_close_settings(measure),
                Some(Action::Pressed(RESET_PAGE)) => self.reset_active_page(),
                Some(Action::Pressed(OPEN_ISSUES)) => {
                    if let Err(e) = self.host.open_url(ISSUES) {
                        log!("framework UI: failed to open {ISSUES}: {e}");
                    }
                }
                Some(Action::Pressed(OPEN_LOGS)) => {
                    if let Err(e) = self.host.open_logs_folder() {
                        log!("framework UI: failed to open log folder: {e}");
                    }
                }
                Some(Action::Pressed(ENABLE_DUMPS)) => {
                    if self.host.dump_pending().is_none() {
                        let status = match self.host.start_enable_dumps() {
                            Ok(()) => DumpStatus::Pending,
                            Err(e) => DumpStatus::Failed(DumpAction::Enable, e.to_string()),
                        };
                        self.show_dump_status(&status);
                    }
                }
                Some(Action::Pressed(DISABLE_DUMPS)) => {
                    if self.host.dump_pending().is_none() {
                        let status = match self.host.start_disable_dumps() {
                            Ok(()) => DumpStatus::Pending,
                            Err(e) => DumpStatus::Failed(DumpAction::Disable, e.to_string()),
                        };
                        self.show_dump_status(&status);
                    }
                }
                Some(Action::Pressed(TROUBLESHOOTING)) => {
                    self.host.start_troubleshooting();
                    let preview = self.settings.as_ref().and_then(|s| s.preview_language);
                    let line = troubleshooting_line(&self.host.troubleshooting_state(), &|key, args| host_words(&self.host, preview, key, args));
                    if let Some(settings) = self.settings.as_mut() {
                        settings.window.set_label_after(TROUBLESHOOTING, line);
                    }
                }
                Some(Action::Pressed(CANCEL)) | Some(Action::Pressed(_)) | Some(Action::Closed) => self.request_close_settings(measure),
                None => {}
            }
            return true;
        }
        if let Some((_, w)) = self.notice.as_mut() {
            match event {
                Move(x, y) => {
                    let _ = w.mouse_move(x, y);
                }
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
            let w = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
            let ctx = self.corner_context();
            let hit = corner(ctx, self.screen, measure, &w).into_iter().find(|item| item.rect.contains(x, y));
            match hit.map(|item| item.opens) {
                Some(Opens::About) => self.open_settings_at(measure, Some(Page::About)),
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
            Move(x, y) => w.mouse_move(x, y),
            Up(..) => {
                w.mouse_up();
                None
            }
            Down(x, y) if w.covers(measure, x, y) => w.mouse_down(measure, x, y),
            Down(..) => None,
            Wheel(notches) => w.wheel(measure, notches),
            Escape => Some(Action::Closed),
        }
    }

    fn commands(&self, measure: &dyn Measure) -> Vec<Command> {
        let mut out = Vec::new();
        let ctx = self.corner_context();
        if ctx != CornerContext::None && !self.modal() {
            let w = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
            let items = corner(ctx, self.screen, measure, &w);
            if let (Some(first), Some(last)) = (items.first(), items.last()) {
                let bar = Rect { x: first.rect.x, y: first.rect.y, w: last.rect.x + last.rect.w - first.rect.x, h: first.rect.h };
                out.push(Command::RoundFill { rect: bar, color: widgets::BACKGROUND, radius: 6.0 });
                for (i, item) in items.iter().enumerate() {
                    let r = item.rect;
                    if i > 0 {
                        out.push(Command::Fill { rect: Rect { x: r.x, y: r.y + 8.0, w: 1.0, h: r.h - 16.0 }, color: widgets::BORDER_SUBTLE });
                    }
                    let hover = r.contains(self.mouse.0, self.mouse.1);
                    if hover {
                        out.push(Command::RoundFill {
                            rect: Rect { x: r.x + 3.0, y: r.y + 3.0, w: (r.w - 6.0).max(0.0), h: (r.h - 6.0).max(0.0) },
                            color: widgets::NAV_HOVER,
                            radius: 4.0,
                        });
                    }
                    let ty = r.y + ((r.h - CORNER_TEXT_SIZE) / 2.0).round() - 2.0;
                    if item.is_brand {
                        out.push(Command::RoundFill {
                            rect: Rect { x: r.x + 14.0, y: r.y + ((r.h - 14.0) / 2.0).round(), w: 3.5, h: 14.0 },
                            color: widgets::BORDER,
                            radius: 1.75,
                        });
                        out.push(Command::text(r.x + 25.0, ty, item.label.clone(), CORNER_TEXT_SIZE, widgets::TITLE_TEXT));
                    } else {
                        out.push(Command::text(r.x + 16.0, ty, item.label.clone(), CORNER_TEXT_SIZE, widgets::TEXT));
                    }
                }
                out.push(Command::RoundStroke { rect: bar, color: widgets::BORDER_WINDOW, radius: 6.0, thickness: 1.0 });
            }
        }
        if let Some((_, w)) = &self.notice {
            out.extend(w.draw(measure));
        }
        if let Some(w) = self.active_modal() {
            out.push(Command::Fill { rect: Rect { x: 0.0, y: 0.0, w: self.screen.0, h: self.screen.1 }, color: 0x80000000 });
            out.extend(w.draw(measure));
        }
        if let Some(content) = self.tooltip_content(measure) {
            let preview = (self.restart_prompt.is_none() && self.mods.is_none()).then(|| self.settings.as_ref().and_then(|s| s.preview_language)).flatten();
            let text = content.resolve(|key| Some(host_words(&self.host, preview, key, &[])));
            if !text.is_empty() {
                draw_tooltip(&mut out, measure, self.screen, self.mouse, &text);
            }
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
            return Some(window.tooltip_at(measure, x, y).unwrap_or_default());
        }
        if let Some(content) = self.notice.as_ref().and_then(|(_, window)| window.tooltip_at(measure, x, y)) {
            return Some(content);
        }
        let ctx = self.corner_context();
        if ctx != CornerContext::None {
            let w = |key: &str, args: &[(&str, &str)]| self.host.words(key, args);
            if corner(ctx, self.screen, measure, &w).into_iter().any(|item| item.rect.contains(x, y)) {
                return Some(Default::default());
            }
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
            Command::RoundFill { rect: r, color: widgets::BACKGROUND, radius: 6.0 },
            Command::RoundStroke { rect: r, color: widgets::BORDER_WINDOW, radius: 6.0, thickness: 1.0 },
            Command::text(r.x + 24.0, r.y + 14.0, text, widgets::TEXT_SIZE, widgets::TITLE_TEXT),
        ]
    }
}

impl FrameworkUi {
    fn new() -> crate::base::Result<Self> {
        let ui_settings = Settings { language: Some(lang::ui()), ..settings::current() };
        let face = SystemFonts::new().map(|fonts| ui_settings.ui_face(|f| fonts.has_family(f)))?;
        let font = text::UiFont::new(&face)?;
        Ok(Self { font, backend: d3d9::Backend::default(), shell: Shell::new(LiveHost::new()) })
    }

    fn sync_font_language(&self) {
        self.font.set_language(self.shell.display_language(), settings::current().body_font.as_ref());
    }
}

/// Suppresses the game's native tooltip whenever the pointer is over any
/// framework UI surface; framework UI draws its own Scheme A tooltips directly
/// in [`Shell::commands`].
pub(crate) fn tooltip_at() -> Option<Vec<u8>> {
    if FAILED.load(Ordering::Relaxed) || crate::render::present::holding() {
        return None;
    }
    let guard = UI.try_lock().ok()?;
    let ui = guard.as_ref()?;
    ui.shell.tooltip_content(&ui.font).map(|_| Vec::new())
}

/// Shows the framework's words for `key` over the frame the game presented
/// last, and presents that at once: on the main thread, just before work
/// that keeps it from drawing for seconds, such as leaving a session. The
/// game's window shows it until the game presents its next frame.
pub fn still(key: &str) {
    if crate::base::thread_id() != crate::base::main_thread() || FAILED.load(Ordering::Relaxed) {
        return;
    }
    let device = crate::render::d3d9::last_device();
    let mut guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    let (Some(ui), false) = (guard.as_mut(), device.is_null()) else { return };
    ui.sync_font_language();
    let commands = ui.shell.still_commands(&ui.font, words(key));
    let shown = unsafe { ui.backend.draw(device, &ui.font, &commands, ui.shell.scale) }.and_then(|()| unsafe { crate::render::d3d9::present(device) });
    match shown {
        Ok(()) => log!("framework UI: still notice {key}"),
        Err(e) => log!("framework UI: no still notice {key}: {e}"),
    }
}

fn handle(event: input::Event) -> bool {
    let mut guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    let Some(ui) = guard.as_mut() else { return false };
    ui.sync_font_language();
    let taken = ui.shell.event(&ui.font, event);
    ui.sync_font_language();
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
                crate::base::notice::fall_back("the framework UI is unavailable");
                return;
            }
        }
    }
    let ui = guard.as_mut().expect("just made");
    input::attach(handle);
    // The framework UI is as large as the game's own, whatever its scale.
    let scale = crate::native::memory().and_then(crate::native::objects::gui_scale).map_or(1.0, |(across, _)| across.clamp(0.5, 4.0));
    let pixels = unsafe { crate::render::d3d9::screen(device) };
    let menu = MAIN_MENU_FRAME.load(Ordering::Relaxed);
    let in_game_menu = crate::native::memory().is_some_and(crate::session::sessions::in_game_menu_open);
    ui.font.set_scale(scale);
    ui.sync_font_language();
    ui.shell.sync_frame(frame_number, (menu != u64::MAX).then_some(menu), in_game_menu, pixels, scale);
    ui.shell.tick(&ui.font);
    ui.sync_font_language();
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
    use crate::text::unicode::system_font::Face;

    fn keys(key: &str) -> String {
        key.to_string()
    }

    fn keys_with(key: &str, args: &[(&str, &str)]) -> String {
        args.iter().fold(key.to_string(), |text, (name, value)| format!("{text} {name}={value}"))
    }

    fn keys_in(language: Language, key: &str) -> String {
        format!("{}:{key}", language.id())
    }

    fn ids(widgets: &[Widget]) -> Vec<&'static str> {
        widgets
            .iter()
            .filter_map(|w| match w {
                Widget::Toggle { id, .. } | Widget::Choice { id, .. } | Widget::Dropdown { id, .. } | Widget::Slider { id, .. } => Some(*id),
                Widget::Labeled { control, .. } => match control.as_ref() {
                    Widget::Choice { id, .. } | Widget::Dropdown { id, .. } | Widget::Slider { id, .. } => Some(*id),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn pages(shown: &Settings, display_hz: u32) -> Vec<Vec<Widget>> {
        Page::ALL.into_iter().map(|page| settings_page(page, shown, shown, display_hz, false, &keys_with)).collect()
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
        for (page, widgets) in Page::ALL.into_iter().zip(&pages) {
            let content: Vec<&Widget> = widgets.iter().filter(|w| !matches!(w, Widget::PageHeader(_) | Widget::Divider)).collect();
            if page == Page::Display {
                assert_eq!(content.len(), 3);
                assert!(matches!(content[0], Widget::Labeled { control, .. } if matches!(control.as_ref(), Widget::Choice { id: "frame_rate_limit", .. })));
                assert!(matches!(content[1], Widget::Labeled { control, .. } if matches!(control.as_ref(), Widget::Slider { id: "zoom_smoothing", .. })));
                assert!(matches!(content[2], Widget::Labeled { control, .. } if matches!(control.as_ref(), Widget::Slider { id: "zoom_step", .. })));
            } else if page == Page::GameSpeed {
                assert_eq!(content.len(), 5);
                for (i, widget) in content.iter().enumerate() {
                    let expected = format!("game_speed_{}", i + 1);
                    assert!(matches!(widget, Widget::Labeled { control, .. } if matches!(control.as_ref(), Widget::Slider { id, .. } if *id == expected)));
                }
            } else {
                assert!(content.iter().all(|w| matches!(w, Widget::Toggle { .. })), "{page:?}");
            }
        }
    }

    #[test]
    fn the_settings_window_reads_back_what_it_shows() {
        for (shown, before) in [(Settings::default(), every_setting_changed()), (every_setting_changed(), Settings::default())] {
            let side = Side::new("pages", Page::ALL.iter().map(|p| keys(p.title())).collect(), SETTINGS_LIST);
            let window = Window::new("", Vec::new(), SETTINGS_WIDTH).with_side(side).with_pages(pages(&shown, 60));
            let mut read = before;
            read_back(&window, &shown, 60, false, &mut read);
            assert_eq!(read, shown);
        }
        let mut kept = every_setting_changed();
        read_back(&Window::new("", Vec::new(), SETTINGS_WIDTH), &Settings::default(), 60, false, &mut kept);
        assert_eq!(kept, every_setting_changed());

        // A manually configured non-preset game speed stays intact unless slid.
        let manual = Settings { game_speed_3: GameSpeed::days_per_second(7.5), ..Settings::default() };
        let side = Side::new("pages", Page::ALL.iter().map(|p| keys(p.title())).collect(), SETTINGS_LIST);
        let mut window = Window::new("", Vec::new(), SETTINGS_WIDTH).with_side(side).with_pages(pages(&manual, 60));
        let mut unchanged = Settings::default();
        read_back(&window, &manual, 60, false, &mut unchanged);
        assert_eq!(unchanged.game_speed_3, GameSpeed::days_per_second(7.5));
        // Sliding it to another step replaces the manual value with the new preset.
        let speed_page = Page::ALL.iter().position(|&p| p == Page::GameSpeed).unwrap();
        window.select_page(speed_page);
        let target_idx = GameSpeed::days_per_second(15.0).slider_index();
        for widget in &mut window.body {
            if let Widget::Labeled { control, .. } = widget {
                if let Widget::Slider { id: "game_speed_3", value, .. } = control.as_mut() {
                    *value = target_idx;
                }
            }
        }
        let mut slid = manual.clone();
        read_back(&window, &manual, 60, false, &mut slid);
        assert_eq!(slid.game_speed_3, GameSpeed::days_per_second(15.0));
    }

    #[test]
    fn the_settings_window_saved_unchanged_keeps_the_settings() {
        for language in Language::ALL {
            let following = Settings { language: Some(language), ..Settings::default() };
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
                    for in_game in [false, true] {
                        let window = settings_window(
                            &next,
                            &next,
                            &next,
                            &page,
                            180,
                            &DumpStatus::NotConfigured,
                            &crate::base::troubleshooting::State::Idle,
                            in_game,
                            None,
                            &keys_with,
                            &keys_in,
                        );
                        assert_eq!(chosen_settings(&window, &next, &page, 180, in_game), next, "{language:?} in_game={in_game}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_settings_window_saves_the_language_chosen() {
        let next = Settings { language: Some(Language::English), ..Settings::default() };
        let page = FontsPage::new(Some(fonts_page::tests::installed()), &next);
        let mut window = settings_window(
            &next,
            &next,
            &next,
            &page,
            60,
            &DumpStatus::NotConfigured,
            &crate::base::troubleshooting::State::Idle,
            false,
            None,
            &keys_with,
            &keys_in,
        );
        let languages = Language::ALL.iter().map(|l| l.native_name().to_string()).collect();
        let korean = Language::ALL.iter().position(|&l| l == Language::Korean).unwrap();
        window.set_options("language", languages, korean);
        assert_eq!(chosen_settings(&window, &next, &page, 60, false), Settings { language: Some(Language::Korean), ..next });
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
        let idle = crate::base::troubleshooting::State::Idle;
        let mut window = settings_window(&current, &current, &current, &page, 60, &DumpStatus::NotConfigured, &idle, false, None, &keys_with, &keys_in);
        assert_eq!(restart_shown(&mut window), "");
        let mut sound = current.clone();
        sound.music_loudness = !sound.music_loudness;
        let mut window = settings_window(&current, &sound, &sound, &page, 60, &DumpStatus::NotConfigured, &idle, false, None, &keys_with, &keys_in);
        assert_eq!(restart_shown(&mut window), "");
        let mut outline = current.clone();
        outline.map_outline = !outline.map_outline;
        let mut window = settings_window(
            &current,
            &outline,
            &outline,
            &FontsPage::new(Some(fonts_page::tests::installed()), &outline),
            60,
            &DumpStatus::NotConfigured,
            &idle,
            false,
            None,
            &keys_with,
            &keys_in,
        );
        assert_eq!(restart_shown(&mut window), "EU4HARMONY_SETTINGS_RESTART");
        let mut window = settings_window(&current, &current, &current, &page, 60, &DumpStatus::NotConfigured, &idle, false, None, &keys_with, &keys_in);
        let languages = Language::ALL.iter().map(|l| l.native_name().to_string()).collect();
        let korean = Language::ALL.iter().position(|&l| l == Language::Korean).unwrap();
        window.set_options("language", languages, korean);
        window.footer_mut()[RESTART_LINE] = footer_notice(&window, &current, &current, &page, 60, false, &keys);
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
            "EU4HARMONY_SETTINGS_ZOOM_SMOOTHING" => "Map zoom smoothing".to_string(),
            "EU4HARMONY_SETTINGS_ZOOM_STEP" => "Map zoom step".to_string(),
            _ => key.to_string(),
        };

        let current = Settings { language: Some(Language::English), ..Settings::default() };
        let page = FontsPage::new(Some(fonts_page::tests::installed()), &current);
        let short_words_with = |key: &str, args: &[(&str, &str)]| if args.is_empty() { short_words(key) } else { keys_with(key, args) };
        let mut window = settings_window(
            &current,
            &current,
            &current,
            &page,
            180,
            &DumpStatus::NotConfigured,
            &crate::base::troubleshooting::State::Idle,
            false,
            None,
            &short_words_with,
            &keys_in,
        );
        window.center(&Mono, 1200.0, 900.0);
        assert_eq!(chosen_settings(&window, &current, &page, 180, false).frame_rate_limit, FrameRateLimit::Display);

        let display_idx = Page::ALL.iter().position(|&p| p == Page::Display).unwrap();
        let (dx, dy) = text_pos(&window, "Display");
        assert_eq!(window.mouse_down(&Mono, dx, dy), Some(Action::Chose("pages", display_idx)));
        let _ = text_pos(&window, "Frame rate limit");
        let (hx, hy) = text_pos(&window, "180");
        let (fx, fy) = text_pos(&window, "60");
        let (ux, uy) = text_pos(&window, "∞");
        assert_eq!(window.tooltip_at(&Mono, fx, fy), Some(Default::default()));
        assert_eq!(window.tooltip_at(&Mono, ux, uy), Some(eu4_harmony_lua::Text::key("EU4HARMONY_SETTINGS_FRAME_RATE_UNLIMITED_TOOLTIP".into())));

        assert_eq!(window.mouse_down(&Mono, fx, fy), Some(Action::Chose("frame_rate_limit", 1)));
        window.footer_mut()[RESTART_LINE] = footer_notice(&window, &current, &current, &page, 180, false, &short_words);
        assert_eq!(restart_shown(&mut window), "");
        assert_eq!(chosen_settings(&window, &current, &page, 180, false).frame_rate_limit, FrameRateLimit::Hz(60));

        assert_eq!(window.mouse_down(&Mono, ux, uy), Some(Action::Chose("frame_rate_limit", 8)));
        window.footer_mut()[RESTART_LINE] = footer_notice(&window, &current, &current, &page, 180, false, &short_words);
        assert_eq!(restart_shown(&mut window), "");
        assert_eq!(chosen_settings(&window, &current, &page, 180, false).frame_rate_limit, FrameRateLimit::Unlimited);

        assert_eq!(window.mouse_down(&Mono, hx, hy), Some(Action::Chose("frame_rate_limit", 6)));
        assert_eq!(chosen_settings(&window, &current, &page, 180, false).frame_rate_limit, FrameRateLimit::Display);

        let slider_tracks: Vec<Rect> = window
            .draw(&Mono)
            .into_iter()
            .filter_map(|c| match c {
                Command::RoundFill { rect, color, .. } if color == widgets::CONTROL && (rect.h - 4.0).abs() < 0.1 => Some(rect),
                _ => None,
            })
            .collect();
        let _ = text_pos(&window, "50%");
        let _ = text_pos(&window, "Map zoom smoothing");
        assert_eq!(window.mouse_down(&Mono, slider_tracks[0].x + 1.0, slider_tracks[0].y + 2.0), Some(Action::Slid("zoom_smoothing", 0)));
        window.mouse_up();
        let _ = text_pos(&window, "1.30×");
        let _ = text_pos(&window, "Map zoom step");
        assert_eq!(window.mouse_down(&Mono, slider_tracks[1].x + 1.0, slider_tracks[1].y + 2.0), Some(Action::Slid("zoom_step", 105)));
        let _ = window.mouse_move(slider_tracks[1].x + slider_tracks[1].w + 20.0, slider_tracks[1].y + 2.0);
        window.mouse_up();
        window.footer_mut()[RESTART_LINE] = footer_notice(&window, &current, &current, &page, 180, false, &short_words);
        assert_eq!(restart_shown(&mut window), "");
        let saved = chosen_settings(&window, &current, &page, 180, false);
        assert_eq!(saved.zoom_smoothing, 0);
        assert_eq!(saved.zoom_step, 200);
    }

    fn has_action_button(page: &[Widget], target: &str) -> bool {
        page.iter().any(|w| match w {
            Widget::Button { id, .. } => *id == target,
            Widget::ButtonRow(buttons) => buttons.iter().any(|b| b.id == target),
            Widget::Labeled { control, .. } => matches!(control.as_ref(), Widget::Button { id, .. } if *id == target),
            _ => false,
        })
    }

    fn has_status_text(page: &[Widget], expected: &str) -> bool {
        page.iter().any(|w| match w {
            Widget::Label(l) => l == expected,
            Widget::Labeled { label, status, .. } => label == expected || status.as_deref() == Some(expected),
            _ => false,
        })
    }

    #[test]
    fn the_dump_controls_handle_not_configured_harmony_external_and_retry() {
        let cancelled = std::io::Error::from_raw_os_error(windows_sys::Win32::Foundation::ERROR_CANCELLED as i32);
        let settings = Settings::default();
        for (state, has_enable, has_disable, feedback) in [
            (DumpStatus::NotConfigured, true, false, None),
            (DumpStatus::Pending, false, false, Some("EU4HARMONY_DUMPS_PENDING")),
            (DumpStatus::Harmony, false, true, None),
            (DumpStatus::External, false, false, Some("EU4HARMONY_DUMPS_EXTERNAL")),
            (DumpStatus::from_op(DumpAction::Enable, Err(cancelled)), true, false, Some("EU4HARMONY_DUMPS_CANCELLED")),
            (DumpStatus::from_op(DumpAction::Enable, Err(std::io::Error::other("denied"))), true, false, Some("EU4HARMONY_DUMPS_FAILED ERROR=denied")),
            (DumpStatus::from_op(DumpAction::Disable, Err(std::io::Error::other("locked"))), false, true, Some("EU4HARMONY_DUMPS_DISABLE_FAILED ERROR=locked")),
        ] {
            let page = advanced_page(&settings, 60, &state, false, &keys, &keys_with);
            assert!(has_action_button(&page, OPEN_LOGS));
            assert_eq!(has_action_button(&page, ENABLE_DUMPS), has_enable);
            assert_eq!(has_action_button(&page, DISABLE_DUMPS), has_disable);
            if let Some(expected) = feedback {
                assert!(has_status_text(&page, expected), "expected {expected}");
            }
        }
    }

    #[test]
    fn the_about_page_shows_the_version_link_and_troubleshooting_result() {
        let current = Settings::default();
        let page = FontsPage::new(Some(fonts_page::tests::installed()), &current);
        let mut window = settings_window(
            &current,
            &current,
            &current,
            &page,
            60,
            &DumpStatus::NotConfigured,
            &crate::base::troubleshooting::State::Idle,
            false,
            None,
            &keys_with,
            &keys_in,
        );
        let about = Page::ALL.iter().position(|&p| p == Page::About).unwrap();
        let version = active_version_badge(&keys_with);
        assert!(matches!(&window.pages[about][1], Widget::AboutHero { version: v, link_id: OPEN_ISSUES, .. } if *v == version));
        window.set_label_after(TROUBLESHOOTING, troubleshooting_line(&crate::base::troubleshooting::State::Saved("a.zip".into()), &keys_with));
        assert!(has_status_text(&window.pages[about], "EU4HARMONY_ABOUT_SAVED FILE=a.zip"));
    }

    #[test]
    fn version_badge_formats_localized_codename_and_beta_channel() {
        use eu4_harmony_folders::InstallChannel;
        let in_lang = |lang: Language| move |key: &str, args: &[(&str, &str)]| loc::framework(lang, key, args);
        assert_eq!(format_version_badge("0.4.0", None, InstallChannel::Public, &in_lang(Language::English)), "v0.4.0");
        assert_eq!(format_version_badge("0.4.0", None, InstallChannel::Beta, &in_lang(Language::SimpChinese)), "v0.4.0 内测版");
        assert_eq!(format_version_badge("1.0.0", Some("Constantinople"), InstallChannel::Public, &in_lang(Language::English)), "v1.0.0 \"Constantinople\"");
        assert_eq!(format_version_badge("1.0.0", Some("Constantinople"), InstallChannel::Public, &in_lang(Language::SimpChinese)), "v1.0.0「君士坦丁堡」");
        assert_eq!(format_version_badge("1.0.0", Some("Constantinople"), InstallChannel::Beta, &in_lang(Language::SimpChinese)), "v1.0.0「君士坦丁堡」 内测版");
        assert_eq!(
            format_version_badge("1.0.0", Some("Constantinople"), InstallChannel::Public, &in_lang(Language::Japanese)),
            "v1.0.0「コンスタンティノープル」"
        );
        // Unlocalized future codename falls back cleanly to its English name.
        assert_eq!(format_version_badge("2.0.0", Some("Samarkand"), InstallChannel::Public, &in_lang(Language::SimpChinese)), "v2.0.0「Samarkand」");
    }

    struct FixedMeasure;
    impl Measure for FixedMeasure {
        fn width(&self, text: &str, size: f32) -> f32 {
            text.chars().count() as f32 * size * 0.25
        }
        fn wrap(&self, text: &str, _: f32, _: f32) -> Vec<String> {
            vec![text.to_string()]
        }
    }

    struct FakeHost {
        ui_language: Language,
        real_loc: bool,
        current: Settings,
        next: Settings,
        saved: Vec<(Settings, bool)>,
        save_error: Option<String>,
        notice: Option<crate::base::notice::Showing>,
        notice_key: Option<(u64, &'static str, &'static str)>,
        dismissed: usize,
        dumps_config: local_dumps::Configuration,
        dump_pending: Option<DumpAction>,
        dump_finished: Option<(DumpAction, std::io::Result<()>)>,
        troubleshooting: crate::base::troubleshooting::State,
        cards: Vec<mods_window::Card>,
        opened_urls: Vec<String>,
        opened_logs: usize,
        restarts: usize,
    }

    impl Default for FakeHost {
        fn default() -> Self {
            Self {
                ui_language: Language::English,
                real_loc: false,
                current: Settings::default(),
                next: Settings::default(),
                saved: Vec::new(),
                save_error: None,
                notice: None,
                notice_key: None,
                dismissed: 0,
                dumps_config: local_dumps::Configuration::NotConfigured,
                dump_pending: None,
                dump_finished: None,
                troubleshooting: crate::base::troubleshooting::State::Idle,
                cards: Vec::new(),
                opened_urls: Vec::new(),
                opened_logs: 0,
                restarts: 0,
            }
        }
    }

    impl UiHost for FakeHost {
        type Fonts = Option<fonts_page::Table>;
        fn ui_language(&self) -> Language {
            self.ui_language
        }
        fn words(&self, key: &str, args: &[(&str, &str)]) -> String {
            if self.real_loc {
                loc::framework(self.ui_language, key, args)
            } else {
                keys_with(key, args)
            }
        }
        fn words_in(&self, language: Language, key: &str, args: &[(&str, &str)]) -> String {
            if self.real_loc {
                loc::framework(language, key, args)
            } else {
                format!("{}:{}", language.id(), keys_with(key, args))
            }
        }
        fn mod_text(&self, id: &str, key: &str) -> Option<String> {
            self.real_loc.then(|| format!("{}:{id}:{key}", self.ui_language.id()))
        }
        fn current_settings(&self) -> Settings {
            self.current.clone()
        }
        fn next_settings(&self) -> Settings {
            self.next.clone()
        }
        fn save_settings(&mut self, chosen: Settings, in_game: bool) -> Result<(), String> {
            if let Some(err) = &self.save_error {
                return Err(err.clone());
            }
            if !in_game {
                if let Some(language) = chosen.language {
                    self.ui_language = language;
                }
            }
            self.next = chosen.clone();
            self.saved.push((chosen, in_game));
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
        fn notice_current(&self) -> Option<crate::base::notice::Showing> {
            if let Some((number, title_key, body_key)) = self.notice_key {
                return Some(crate::base::notice::Showing { number, title: self.words(title_key, &[]), text: self.words(body_key, &[]) });
            }
            self.notice.clone()
        }
        fn dismiss_notice(&mut self) {
            self.dismissed += 1;
            self.notice = None;
            self.notice_key = None;
        }
        fn troubleshooting_state(&self) -> crate::base::troubleshooting::State {
            self.troubleshooting.clone()
        }
        fn start_troubleshooting(&mut self) {
            self.troubleshooting = crate::base::troubleshooting::State::Packing;
        }
        fn dumps_status(&self) -> std::io::Result<local_dumps::Configuration> {
            Ok(self.dumps_config)
        }
        fn start_enable_dumps(&mut self) -> std::io::Result<()> {
            self.dump_pending = Some(DumpAction::Enable);
            Ok(())
        }
        fn start_disable_dumps(&mut self) -> std::io::Result<()> {
            self.dump_pending = Some(DumpAction::Disable);
            Ok(())
        }
        fn poll_dump_op(&mut self) -> Option<(DumpAction, std::io::Result<()>)> {
            let (action, done) = self.dump_finished.take()?;
            self.dump_pending = None;
            if done.is_ok() {
                self.dumps_config = match action {
                    DumpAction::Enable => local_dumps::Configuration::Harmony,
                    DumpAction::Disable => local_dumps::Configuration::NotConfigured,
                };
            }
            Some((action, done))
        }
        fn dump_pending(&self) -> Option<DumpAction> {
            self.dump_pending
        }
        fn open_url(&mut self, url: &str) -> Result<(), String> {
            self.opened_urls.push(url.to_string());
            Ok(())
        }
        fn open_logs_folder(&mut self) -> Result<(), String> {
            self.opened_logs += 1;
            Ok(())
        }
        fn request_restart(&mut self) -> Result<(), String> {
            self.restarts += 1;
            Ok(())
        }
    }

    fn text_pos(w: &Window, needle: &str) -> (f32, f32) {
        w.draw(&FixedMeasure)
            .into_iter()
            .find_map(|c| match c {
                Command::Text { x, y, text, .. } if text == needle || (text.ends_with('…') && needle.starts_with(text.trim_end_matches('…'))) => {
                    Some((x + 2.0, y + 6.0))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text {needle:?}"))
    }

    fn side_pos(w: &Window, page: Page) -> (f32, f32) {
        let index = Page::ALL.iter().position(|&p| p == page).unwrap();
        w.draw(&FixedMeasure)
            .into_iter()
            .filter_map(|c| match c {
                Command::Text { x, y, .. } if x < w.x + SETTINGS_LIST && y > w.y + 42.0 => Some((x + 2.0, y + 6.0)),
                _ => None,
            })
            .nth(index)
            .unwrap_or_else(|| panic!("missing side row {page:?}"))
    }

    #[test]
    fn shell_corner_buttons_version_link_modal_stacking_and_expiry() {
        let mut shell = Shell::new(FakeHost::default());
        shell.sync_frame(10, Some(10), false, (1280.0, 720.0), 1.0);
        let items = corner(CornerContext::MainMenu, shell.screen, &FixedMeasure, &keys_with);
        assert_eq!(items.len(), 3);
        let brand = items.iter().find(|i| i.opens == Opens::About).unwrap();
        assert!(brand.label.contains(env!("CARGO_PKG_VERSION")));
        let settings_item = items.iter().find(|i| i.opens == Opens::Settings).unwrap();

        // Clicking the version badge opens Settings directly on the About page.
        assert!(shell.event(&FixedMeasure, input::Event::Down(brand.rect.x + 4.0, brand.rect.y + 4.0)));
        let about_idx = Page::ALL.iter().position(|&p| p == Page::About).unwrap();
        assert_eq!(shell.settings.as_ref().unwrap().window.side.as_ref().unwrap().selected, about_idx);
        // Clicking the GitHub issues link opens the URL in the host.
        let (lx, ly) = text_pos(&shell.settings.as_ref().unwrap().window, &format!("EU4HARMONY_ABOUT_ISSUES ISSUES={ISSUES}"));
        assert!(shell.event(&FixedMeasure, input::Event::Down(lx, ly)));
        assert_eq!(shell.host.opened_urls, [ISSUES]);
        assert!(shell.event(&FixedMeasure, input::Event::Escape));
        assert!(!shell.modal());

        // Hovering corner items suppresses native tooltips without showing one of their own.
        let (cx, cy) = (settings_item.rect.x + 4.0, settings_item.rect.y + 4.0);
        assert!(!shell.event(&FixedMeasure, input::Event::Move(cx, cy)));
        assert_eq!(shell.tooltip_content(&FixedMeasure), Some(Default::default()));
        assert!(shell.event(&FixedMeasure, input::Event::Down(cx, cy)));
        let (sv_x, sv_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_SAVE");
        assert_eq!(shell.settings.as_ref().unwrap().window.tooltip_at(&FixedMeasure, sv_x, sv_y), Some(Default::default()));
        assert!(shell.event(&FixedMeasure, input::Event::Escape));
        assert!(!shell.modal());

        // Main-menu bar expires once frame advances more than 2 frames past main_menu_frame.
        shell.sync_frame(13, Some(10), false, (1280.0, 720.0), 1.0);
        assert!(!shell.in_main_menu());
        assert_eq!(shell.tooltip_content(&FixedMeasure), None);
        assert!(!shell.event(&FixedMeasure, input::Event::Down(cx, cy)));
    }

    #[test]
    fn shell_reset_page_save_error_and_restart_prompt_flow() {
        let host = FakeHost {
            current: Settings { language: Some(Language::English), ..Settings::default() },
            next: Settings { language: Some(Language::English), music_loudness: false, return_without_restart: false, ..Settings::default() },
            ..FakeHost::default()
        };
        let mut shell = Shell::new(host);
        shell.sync_frame(1, Some(1), false, (1280.0, 720.0), 1.0);
        shell.open_settings(&FixedMeasure);

        // Switch to Sound page and click Restore defaults: resets Sound only, not Sessions.
        let (sx, sy) = side_pos(&shell.settings.as_ref().unwrap().window, Page::Sound);
        assert!(shell.event(&FixedMeasure, input::Event::Down(sx, sy)));
        let (rx, ry) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_RESET_PAGE");
        assert!(shell.event(&FixedMeasure, input::Event::Down(rx, ry)));
        assert_eq!(shell.settings.as_ref().unwrap().window.is_on("music_loudness"), Some(true));
        assert_eq!(shell.settings.as_ref().unwrap().window.is_on("return_without_restart"), Some(false));

        // Save error keeps Settings open and displays the error in the footer.
        shell.host.save_error = Some("disk full".into());
        let (sv_x, sv_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_SAVE");
        assert!(shell.event(&FixedMeasure, input::Event::Down(sv_x, sv_y)));
        assert!(shell.settings.is_some());
        assert_eq!(restart_shown(&mut shell.settings.as_mut().unwrap().window), "EU4HARMONY_SETTINGS_SAVE_FAILED ERROR=disk full");

        // Switch to LanguageFonts, toggle map_outline (requires restart), clear error, and save.
        shell.host.save_error = None;
        let (lx, ly) = side_pos(&shell.settings.as_ref().unwrap().window, Page::LanguageFonts);
        assert!(shell.event(&FixedMeasure, input::Event::Down(lx, ly)));
        shell.settings.as_mut().unwrap().window.set_toggle("map_outline", false);
        let (sv_x, sv_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_SAVE");
        assert!(shell.event(&FixedMeasure, input::Event::Down(sv_x, sv_y)));
        // Settings closed and restart confirmation modal popped up.
        assert!(shell.settings.is_none());
        assert!(shell.restart_prompt.is_some());
        let (rn_x, rn_y) = text_pos(shell.restart_prompt.as_ref().unwrap(), "EU4HARMONY_RESTART_NOW");
        assert!(shell.event(&FixedMeasure, input::Event::Down(rn_x, rn_y)));
        assert!(shell.restart_prompt.is_none());
        assert_eq!(shell.host.restarts, 1);
    }

    #[test]
    fn shell_in_game_menu_corner_buttons_settings_and_auto_close() {
        let mut shell = Shell::new(FakeHost::default());
        shell.sync_frame(20, None, false, (1280.0, 720.0), 1.0);
        assert_eq!(shell.corner_context(), CornerContext::None);
        assert!(shell.commands(&FixedMeasure).is_empty());

        shell.sync_frame(21, None, true, (1280.0, 720.0), 1.0);
        assert_eq!(shell.corner_context(), CornerContext::InGameMenu);
        let items = corner(CornerContext::InGameMenu, shell.screen, &FixedMeasure, &keys_with);
        assert_eq!(items.len(), 3);
        let mods_rect = items.iter().find(|i| i.opens == Opens::Mods).unwrap().rect;
        let settings_rect = items.iter().find(|i| i.opens == Opens::Settings).unwrap().rect;

        let (mx, my) = (mods_rect.x + 4.0, mods_rect.y + 4.0);
        assert!(!shell.event(&FixedMeasure, input::Event::Move(mx, my)));
        assert_eq!(shell.tooltip_content(&FixedMeasure), Some(Default::default()));
        assert!(shell.event(&FixedMeasure, input::Event::Down(mx, my)));
        assert!(shell.mods.as_ref().is_some_and(|m| m.in_game));

        shell.sync_frame(22, None, false, (1280.0, 720.0), 1.0);
        shell.tick(&FixedMeasure);
        assert!(!shell.modal());

        shell.sync_frame(23, None, true, (1280.0, 720.0), 1.0);
        let (sx, sy) = (settings_rect.x + 4.0, settings_rect.y + 4.0);
        assert!(shell.event(&FixedMeasure, input::Event::Down(sx, sy)));
        let settings = shell.settings.as_mut().expect("settings open");
        assert!(settings.in_game);
        let sessions_idx = Page::ALL.iter().position(|&p| p == Page::Sessions).unwrap();
        assert_eq!(settings.window.side.as_ref().unwrap().selected, sessions_idx);
        assert_eq!(restart_shown(&mut settings.window), "");

        let (ax, ay) = side_pos(&shell.settings.as_ref().unwrap().window, Page::Achievements);
        assert!(shell.event(&FixedMeasure, input::Event::Down(ax, ay)));
        let settings = shell.settings.as_mut().unwrap();
        assert_eq!(restart_shown(&mut settings.window), MAIN_MENU_ONLY_KEY);
        let (tx, ty) = text_pos(&settings.window, "EU4HARMONY_SETTINGS_ACHIEVEMENTS_OUTSIDE_IRONMAN");
        assert_eq!(settings.window.tooltip_at(&FixedMeasure, tx, ty), Some(main_menu_only_tooltip()));
        assert!(shell.event(&FixedMeasure, input::Event::Move(tx, ty)));
        let cmds = shell.commands(&FixedMeasure);
        assert!(
            cmds.iter().any(|c| matches!(c, Command::Text { text, .. } if text == MAIN_MENU_ONLY_KEY)),
            "self-drawn tooltip renders locked-setting explanation"
        );
        assert!(shell.event(&FixedMeasure, input::Event::Down(tx, ty)));
        assert!(!shell.settings.as_ref().unwrap().window.is_on("achievements_outside_ironman").unwrap());

        let (sound_x, sound_y) = side_pos(&shell.settings.as_ref().unwrap().window, Page::Sound);
        assert!(shell.event(&FixedMeasure, input::Event::Down(sound_x, sound_y)));
        assert_eq!(restart_shown(&mut shell.settings.as_mut().unwrap().window), "");
        let (ml_x, ml_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_MUSIC_LOUDNESS");
        assert!(shell.event(&FixedMeasure, input::Event::Down(ml_x, ml_y)));
        let (save_x, save_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_SAVE");
        assert!(shell.event(&FixedMeasure, input::Event::Down(save_x, save_y)));
        assert!(!shell.modal());
        assert_eq!(shell.host.saved.len(), 1);
        assert!(shell.host.saved[0].1);
        assert_eq!(shell.host.saved[0].0.music_loudness, !Settings::default().music_loudness);
        assert_eq!(shell.host.saved[0].0.achievements_outside_ironman, Settings::default().achievements_outside_ironman);

        assert!(shell.event(&FixedMeasure, input::Event::Down(sx, sy)));
        assert!(shell.modal());
        shell.sync_frame(24, None, false, (1280.0, 720.0), 1.0);
        shell.tick(&FixedMeasure);
        assert!(!shell.modal());
        assert_eq!(shell.host.saved.len(), 1);
    }

    #[test]
    fn shell_reaps_async_dump_setup_and_disable_after_settings_closes() {
        let host = FakeHost { notice: Some(crate::base::notice::Showing { number: 1, title: "First".into(), text: "Body".into() }), ..Default::default() };
        let mut shell = Shell::new(host);
        shell.sync_frame(1, Some(1), false, (1280.0, 720.0), 1.0);
        shell.tick(&FixedMeasure);

        let notice = &mut shell.notice.as_mut().unwrap().1;
        notice.x = 77.0;
        notice.y = 88.0;
        shell.host.notice = Some(crate::base::notice::Showing { number: 1, title: "Updated".into(), text: "Body".into() });
        shell.tick(&FixedMeasure);
        let notice = &shell.notice.as_ref().unwrap().1;
        assert_eq!((notice.x, notice.y), (77.0, 88.0));
        assert_eq!(notice.title, "Updated");

        shell.open_settings(&FixedMeasure);
        shell.host.start_enable_dumps().unwrap();
        shell.close_settings();
        shell.host.dump_finished = Some((DumpAction::Enable, Ok(())));
        shell.tick(&FixedMeasure);
        assert!(shell.host.dump_pending().is_none());

        shell.open_settings(&FixedMeasure);
        let advanced = Page::ALL.iter().position(|&p| p == Page::Advanced).unwrap();
        let page = &shell.settings.as_ref().unwrap().window.pages[advanced];
        assert!(has_status_text(page, "EU4HARMONY_DUMPS_CONFIGURED"));
        assert!(has_action_button(page, DISABLE_DUMPS));

        // Disabling transitions back to NotConfigured (Enable button shown).
        shell.host.start_disable_dumps().unwrap();
        shell.host.dump_finished = Some((DumpAction::Disable, Ok(())));
        shell.tick(&FixedMeasure);
        let page = &shell.settings.as_ref().unwrap().window.pages[advanced];
        assert!(has_action_button(page, ENABLE_DUMPS));
    }

    #[test]
    fn shell_confirms_before_discarding_unsaved_settings_changes() {
        let mut shell = Shell::new(FakeHost::default());
        shell.sync_frame(1, Some(1), false, (1280.0, 720.0), 1.0);
        shell.open_settings(&FixedMeasure);

        // Clean settings close directly on Escape without a confirmation dialog.
        assert!(shell.event(&FixedMeasure, input::Event::Escape));
        assert!(shell.settings.is_none());
        assert!(shell.discard_prompt.is_none());

        // Dirty settings prompt before closing; Keep editing preserves the draft, Discard closes.
        shell.open_settings(&FixedMeasure);
        let (sx, sy) = side_pos(&shell.settings.as_ref().unwrap().window, Page::Sound);
        assert!(shell.event(&FixedMeasure, input::Event::Down(sx, sy)));
        let (ml_x, ml_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_MUSIC_LOUDNESS");
        assert!(shell.event(&FixedMeasure, input::Event::Down(ml_x, ml_y)));

        assert!(shell.event(&FixedMeasure, input::Event::Escape));
        assert!(shell.settings.is_some());
        assert!(shell.discard_prompt.is_some());

        let (keep_x, keep_y) = text_pos(shell.discard_prompt.as_ref().unwrap(), "EU4HARMONY_DISCARD_CANCEL");
        assert!(shell.event(&FixedMeasure, input::Event::Down(keep_x, keep_y)));
        assert!(shell.discard_prompt.is_none());
        assert!(shell.settings.is_some());
        assert_eq!(shell.settings.as_ref().unwrap().window.is_on("music_loudness"), Some(!Settings::default().music_loudness));

        let (cancel_x, cancel_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_CANCEL");
        assert!(shell.event(&FixedMeasure, input::Event::Down(cancel_x, cancel_y)));
        assert!(shell.discard_prompt.is_some());
        let (disc_x, disc_y) = text_pos(shell.discard_prompt.as_ref().unwrap(), "EU4HARMONY_DISCARD_CONFIRM");
        assert!(shell.event(&FixedMeasure, input::Event::Down(disc_x, disc_y)));
        assert!(shell.discard_prompt.is_none());
        assert!(shell.settings.is_none());
        assert!(shell.host.saved.is_empty());
    }

    #[test]
    fn shell_advanced_page_handles_remote_control_open_logs_and_in_game_locking() {
        let mut shell = Shell::new(FakeHost::default());
        shell.sync_frame(1, Some(1), false, (1280.0, 720.0), 1.0);
        shell.open_settings_at(&FixedMeasure, Some(Page::Advanced));

        let (logs_x, logs_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_ADVANCED_OPEN_LOGS");
        assert!(shell.event(&FixedMeasure, input::Event::Down(logs_x, logs_y)));
        assert_eq!(shell.host.opened_logs, 1);

        // Enabling remote control requires restart and triggers the restart prompt on Save.
        let (rc_x, rc_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_REMOTE_CONTROL");
        assert!(shell.event(&FixedMeasure, input::Event::Down(rc_x, rc_y)));
        assert_eq!(shell.settings.as_ref().unwrap().window.is_on("remote_control"), Some(true));
        assert_eq!(restart_shown(&mut shell.settings.as_mut().unwrap().window), "EU4HARMONY_SETTINGS_RESTART");
        let (sv_x, sv_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_SAVE");
        assert!(shell.event(&FixedMeasure, input::Event::Down(sv_x, sv_y)));
        assert!(shell.restart_prompt.is_some());
        assert!(shell.host.saved[0].0.remote_control);
        shell.restart_prompt = None;

        // In-game: remote control toggle is locked, while Open log folder and dump controls remain usable.
        shell.sync_frame(20, None, true, (1280.0, 720.0), 1.0);
        shell.open_settings_at(&FixedMeasure, Some(Page::Advanced));
        let (rc_x, rc_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_SETTINGS_REMOTE_CONTROL");
        assert_eq!(shell.settings.as_ref().unwrap().window.tooltip_at(&FixedMeasure, rc_x, rc_y), Some(main_menu_only_tooltip()));
        assert!(shell.event(&FixedMeasure, input::Event::Down(rc_x, rc_y)));
        assert_eq!(shell.settings.as_ref().unwrap().window.is_on("remote_control"), Some(true));
        let (logs_x, logs_y) = text_pos(&shell.settings.as_ref().unwrap().window, "EU4HARMONY_ADVANCED_OPEN_LOGS");
        assert!(shell.event(&FixedMeasure, input::Event::Down(logs_x, logs_y)));
        assert_eq!(shell.host.opened_logs, 2);
    }

    #[test]
    fn switching_language_previews_settings_immediately_and_saving_updates_all_framework_ui() {
        let initial = Settings { language: Some(Language::English), frame_rate_limit: FrameRateLimit::Hz(75), ..Settings::default() };
        let host = FakeHost {
            ui_language: Language::English,
            real_loc: true,
            current: initial.clone(),
            next: initial,
            notice_key: Some((1, "EU4HARMONY_NOTICE_TITLE", "EU4HARMONY_IRONMAN_SAVE_FAILED")),
            cards: vec![mods_window::Card {
                id: "rc".into(),
                name: "Regional Combat".into(),
                version: Some("1.0".into()),
                description: Some("RC_DESC".into()),
                permissions: Default::default(),
                data_folder: "data/rc".into(),
            }],
            ..FakeHost::default()
        };
        let en = |key: &str| loc::framework(Language::English, key, &[]);
        let zh = |key: &str| loc::framework(Language::SimpChinese, key, &[]);
        let mut shell = Shell::new(host);
        shell.sync_frame(1, Some(1), false, (1280.0, 720.0), 1.0);
        shell.tick(&FixedMeasure);
        assert_eq!(shell.notice.as_ref().unwrap().1.title, en("EU4HARMONY_NOTICE_TITLE"));
        assert!(matches!(shell.notice.as_ref().unwrap().1.body.as_slice(), [Widget::Label(t)] if *t == en("EU4HARMONY_IRONMAN_SAVE_FAILED")));

        // Open Settings, tweak Sound and Display (from custom 75 Hz to 60 Hz),
        // move the window, then switch language to Simplified Chinese.
        shell.open_settings(&FixedMeasure);
        {
            let w = &mut shell.settings.as_mut().unwrap().window;
            w.x = 123.0;
            w.y = 45.0;
        }
        let (sx, sy) = side_pos(&shell.settings.as_ref().unwrap().window, Page::Sound);
        assert!(shell.event(&FixedMeasure, input::Event::Down(sx, sy)));
        let (ml_x, ml_y) = text_pos(&shell.settings.as_ref().unwrap().window, &en("EU4HARMONY_SETTINGS_MUSIC_LOUDNESS"));
        assert!(shell.event(&FixedMeasure, input::Event::Down(ml_x, ml_y)));

        let (dx, dy) = side_pos(&shell.settings.as_ref().unwrap().window, Page::Display);
        assert!(shell.event(&FixedMeasure, input::Event::Down(dx, dy)));
        let (fx, fy) = text_pos(&shell.settings.as_ref().unwrap().window, "60");
        assert!(shell.event(&FixedMeasure, input::Event::Down(fx, fy)));

        let (lx, ly) = side_pos(&shell.settings.as_ref().unwrap().window, Page::LanguageFonts);
        assert!(shell.event(&FixedMeasure, input::Event::Down(lx, ly)));
        let zh_idx = Language::ALL.iter().position(|&l| l == Language::SimpChinese).unwrap();
        shell.settings_chose(&FixedMeasure, "language", zh_idx);
        shell.settings_changed();

        // Preview takes effect immediately on the entire settings window while
        // keeping window position and draft edits across pages; host ui_language
        // stays English until saved.
        assert_eq!(shell.display_language(), Language::SimpChinese);
        assert_eq!(shell.host.ui_language(), Language::English);
        let settings = shell.settings.as_mut().unwrap();
        assert_eq!(settings.window.title, zh("EU4HARMONY_SETTINGS_TITLE"));
        assert_eq!(settings.window.side.as_ref().unwrap().items[0], zh("EU4HARMONY_SETTINGS_PAGE_LANGUAGE_FONTS"));
        assert_eq!(restart_shown(&mut settings.window), zh("EU4HARMONY_SETTINGS_RESTART"));
        assert_eq!((settings.window.x, settings.window.y), (123.0, 45.0));
        assert_eq!(settings.window.is_on("music_loudness"), Some(false));

        // Cancelling opens the discard confirmation in the preview language;
        // confirming discards the preview and restores the display language.
        let (cx, cy) = text_pos(&shell.settings.as_ref().unwrap().window, &zh("EU4HARMONY_SETTINGS_CANCEL"));
        assert!(shell.event(&FixedMeasure, input::Event::Down(cx, cy)));
        let discard = shell.discard_prompt.as_ref().expect("discard prompt shown");
        assert_eq!(discard.title, zh("EU4HARMONY_DISCARD_PROMPT_TITLE"));
        let (dc_x, dc_y) = text_pos(discard, &zh("EU4HARMONY_DISCARD_CONFIRM"));
        assert!(shell.event(&FixedMeasure, input::Event::Down(dc_x, dc_y)));
        assert!(shell.settings.is_none());
        assert!(shell.discard_prompt.is_none());
        assert_eq!(shell.display_language(), Language::English);
        assert_eq!(shell.host.ui_language(), Language::English);
        assert!(shell.host.saved.is_empty());

        // Reopen, edit Sound + Display + Language, and save.
        shell.open_settings(&FixedMeasure);
        let (sx, sy) = side_pos(&shell.settings.as_ref().unwrap().window, Page::Sound);
        assert!(shell.event(&FixedMeasure, input::Event::Down(sx, sy)));
        let (ml_x, ml_y) = text_pos(&shell.settings.as_ref().unwrap().window, &en("EU4HARMONY_SETTINGS_MUSIC_LOUDNESS"));
        assert!(shell.event(&FixedMeasure, input::Event::Down(ml_x, ml_y)));
        let (dx, dy) = side_pos(&shell.settings.as_ref().unwrap().window, Page::Display);
        assert!(shell.event(&FixedMeasure, input::Event::Down(dx, dy)));
        let (fx, fy) = text_pos(&shell.settings.as_ref().unwrap().window, "60");
        assert!(shell.event(&FixedMeasure, input::Event::Down(fx, fy)));
        let (lx, ly) = side_pos(&shell.settings.as_ref().unwrap().window, Page::LanguageFonts);
        assert!(shell.event(&FixedMeasure, input::Event::Down(lx, ly)));
        shell.settings_chose(&FixedMeasure, "language", zh_idx);
        shell.settings_changed();
        let (sv_x, sv_y) = text_pos(&shell.settings.as_ref().unwrap().window, &zh("EU4HARMONY_SETTINGS_SAVE"));
        assert!(shell.event(&FixedMeasure, input::Event::Down(sv_x, sv_y)));

        // Saved settings keep all draft edits; host UI language updated at once,
        // and the restart prompt is shown in the new language.
        assert_eq!(shell.host.ui_language(), Language::SimpChinese);
        assert_eq!(shell.host.saved.len(), 1);
        assert_eq!(shell.host.saved[0].0.language, Some(Language::SimpChinese));
        assert!(!shell.host.saved[0].0.music_loudness);
        assert_eq!(shell.host.saved[0].0.frame_rate_limit, FrameRateLimit::Hz(60));
        let prompt = shell.restart_prompt.as_ref().expect("restart prompt shown");
        assert_eq!(prompt.title, zh("EU4HARMONY_RESTART_PROMPT_TITLE"));
        assert!(matches!(prompt.body.as_slice(), [Widget::Label(t)] if *t == zh("EU4HARMONY_RESTART_PROMPT_BODY")));

        // Choosing "Restart later" leaves the corner bar, existing notice window,
        // and newly opened Mods window all in the new language.
        let (rl_x, rl_y) = text_pos(shell.restart_prompt.as_ref().unwrap(), &zh("EU4HARMONY_RESTART_LATER"));
        assert!(shell.event(&FixedMeasure, input::Event::Down(rl_x, rl_y)));
        shell.tick(&FixedMeasure);
        assert_eq!(shell.notice.as_ref().unwrap().1.title, zh("EU4HARMONY_NOTICE_TITLE"));
        assert!(matches!(shell.notice.as_ref().unwrap().1.body.as_slice(), [Widget::Label(t)] if *t == zh("EU4HARMONY_IRONMAN_SAVE_FAILED")));
        let cmds = shell.commands(&FixedMeasure);
        assert!(cmds.iter().any(|c| matches!(c, Command::Text { text, .. } if *text == zh("EU4HARMONY_SETTINGS_BUTTON"))));
        assert!(cmds.iter().any(|c| matches!(c, Command::Text { text, .. } if *text == zh("EU4HARMONY_MODS_BUTTON"))));
        shell.open_mods(&FixedMeasure);
        let mods = &shell.mods.as_ref().unwrap().window;
        assert_eq!(mods.title, zh("EU4HARMONY_MODS_TITLE"));
        assert!(has_status_text(&mods.body, "simp_chinese:rc:RC_DESC"));
    }
}
