//! Framework settings (GLOSSARY): the player's choices for the framework,
//! kept in `eu4-harmony/settings.txt` under the user directory and written the
//! way the game writes its own `settings.txt`. They are read once at startup.
//! A change to the language or the fonts takes effect at the next start.
//!
//! Every other setting is declared once, in [`SETTINGS`]: its key in the
//! file, its default, the page and widget it shows as in the settings window,
//! when a change to it takes effect, and which part of the framework is told.
//! The file, the settings window and what is applied at startup and on a
//! save all follow from that table, so a new setting is one row of it (and
//! one field of [`Settings`], its default, and its words in the localisation).
//!
//! A font the player has not chosen follows the player language: it is the
//! first installed of the language's [`defaults`], and is not written to the
//! file, so that it changes with the language.
use crate::unicode::system_font::{Face, REGULAR};
use eu4_harmony_loc::Language;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The framework settings. Each field after the fonts is kept by a row of
/// [`SETTINGS`], which the file, the settings window and what is applied
/// follow; its default is [`Settings::default`]'s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The player language (GLOSSARY); None in a file from before it was
    /// chosen, which [`load`] fills in.
    pub language: Option<Language>,
    /// The system font and weight chosen for body text (GLOSSARY), which the
    /// framework UI draws with too; None to follow the language.
    pub body_font: Option<Face>,
    /// The system font and weight chosen for headings (GLOSSARY).
    pub heading_font: Option<Face>,
    /// The system font and weight chosen for names on the map.
    pub map_font: Option<Face>,
    /// The system font and weight chosen for the game console (GLOSSARY).
    pub console_font: Option<Face>,
    /// Whether system glyphs on the map get the outline the map font's own
    /// glyphs have.
    pub map_outline: bool,
    /// Whether returning to the main menu is an in-process return (GLOSSARY)
    /// rather than a restart (ADR return-to-the-main-menu-in-process).
    pub return_without_restart: bool,
    /// Whether an in-game load in ironman writes the ironman exit save first;
    /// without it, loading the same save again is an undo
    /// (ADR in-game-load-in-ironman).
    pub save_ironman_before_load: bool,
    /// Whether a game that is not ironman can earn achievements
    /// (ADR achievement-rules-can-be-relaxed).
    pub achievements_outside_ironman: bool,
    /// Whether achievements can be earned with a game checksum (GLOSSARY)
    /// that is not vanilla's (ADR achievement-rules-can-be-relaxed).
    pub achievements_with_changed_checksum: bool,
    /// Whether the drawing thread (GLOSSARY; docs/plans/drawing-thread.md)
    /// draws with no limit, rather than at the display's refresh rate.
    pub drawing_unlimited: bool,
    /// Whether every track is brought to the same loudness (GLOSSARY:
    /// Loudness normalization; docs/plans/music-volume.md).
    pub music_loudness: bool,
    /// Whether quiet and loud passages of the music are brought closer
    /// (GLOSSARY: Music leveling).
    pub music_leveling: bool,
}

/// A framework setting the settings window shows as a toggle or a choice
/// bar, declared once; [`SETTINGS`] holds them all.
pub struct Setting {
    /// Its key in the settings file, which is also its widget's id in the
    /// settings window.
    pub key: &'static str,
    /// The settings window's page it shows on.
    pub page: Page,
    /// How it shows there, and the field of [`Settings`] that keeps it.
    pub shows: Shows,
    /// When a change to it takes effect.
    pub effect: Effect,
}

/// A field of [`Settings`], read and written.
#[derive(Clone, Copy)]
pub struct Field<T> {
    pub get: fn(&Settings) -> T,
    pub set: fn(&mut Settings, T),
}

/// The [`Field`] of [`Settings`] named `$name`.
macro_rules! field {
    ($name:ident) => {
        Field { get: |s| s.$name, set: |s, v| s.$name = v }
    };
}

/// How a [`Setting`] shows in the settings window, and how the file keeps it.
#[derive(Clone, Copy)]
pub enum Shows {
    /// A toggle (GLOSSARY) with the words of `label`; `yes` or `no` in the
    /// file.
    Toggle { label: &'static str, field: Field<bool> },
}

/// When a change to a [`Setting`] made in the settings window takes effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// At the next start: this session keeps what it started with.
    NextStart,
    /// When it is saved: [`current`] has it from then on, and `tells` is
    /// told then, and at startup. A setting no part is told of is read from
    /// [`current`] where it is used.
    WhenSaved { tells: Option<Listener> },
}

/// A part of the framework that is told the settings it runs with, at startup
/// and whenever settings are saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listener {
    /// The achievement rules.
    Achievements,
    /// The music's loudness.
    Sound,
    /// The drawing thread.
    Drawing,
}

impl Listener {
    fn tell(self, settings: &Settings) {
        match self {
            Listener::Achievements => crate::achievements::follow(settings),
            Listener::Sound => crate::sound::choose(settings),
            Listener::Drawing => crate::drawing::set(settings.drawing_unlimited),
        }
    }
}

/// The settings window's pages, in the order its side list shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    LanguageFonts,
    Sessions,
    Achievements,
    GameSpeed,
    Sound,
    /// The framework's version, where to report a problem, and the
    /// troubleshooting files; no settings.
    About,
}

impl Page {
    pub const ALL: [Page; 6] = [Page::LanguageFonts, Page::Sessions, Page::Achievements, Page::GameSpeed, Page::Sound, Page::About];

    /// The key of its name in the side list.
    pub fn title(self) -> &'static str {
        match self {
            Page::LanguageFonts => "EU4HARMONY_SETTINGS_PAGE_LANGUAGE_FONTS",
            Page::Sessions => "EU4HARMONY_SETTINGS_PAGE_SESSIONS",
            Page::Achievements => "EU4HARMONY_SETTINGS_PAGE_ACHIEVEMENTS",
            Page::GameSpeed => "EU4HARMONY_SETTINGS_PAGE_GAME_SPEED",
            Page::Sound => "EU4HARMONY_SETTINGS_PAGE_SOUND",
            Page::About => "EU4HARMONY_SETTINGS_PAGE_ABOUT",
        }
    }
}

/// Every [`Setting`], in the order the file writes them and each page of
/// the settings window shows them; those on the language and fonts page show
/// after the map's font. Their defaults are [`Settings::default`]'s.
pub static SETTINGS: [Setting; 8] = [
    Setting {
        key: "map_outline",
        page: Page::LanguageFonts,
        shows: Shows::Toggle { label: "EU4HARMONY_SETTINGS_MAP_OUTLINE", field: field!(map_outline) },
        effect: Effect::NextStart,
    },
    Setting {
        key: "return_without_restart",
        page: Page::Sessions,
        shows: Shows::Toggle { label: "EU4HARMONY_SETTINGS_RETURN_WITHOUT_RESTART", field: field!(return_without_restart) },
        effect: Effect::WhenSaved { tells: None },
    },
    Setting {
        key: "save_ironman_before_load",
        page: Page::Sessions,
        shows: Shows::Toggle { label: "EU4HARMONY_SETTINGS_SAVE_IRONMAN_BEFORE_LOAD", field: field!(save_ironman_before_load) },
        effect: Effect::WhenSaved { tells: None },
    },
    Setting {
        key: "achievements_outside_ironman",
        page: Page::Achievements,
        shows: Shows::Toggle { label: "EU4HARMONY_SETTINGS_ACHIEVEMENTS_OUTSIDE_IRONMAN", field: field!(achievements_outside_ironman) },
        effect: Effect::WhenSaved { tells: Some(Listener::Achievements) },
    },
    Setting {
        key: "achievements_with_changed_checksum",
        page: Page::Achievements,
        shows: Shows::Toggle { label: "EU4HARMONY_SETTINGS_ACHIEVEMENTS_CHANGED_CHECKSUM", field: field!(achievements_with_changed_checksum) },
        effect: Effect::WhenSaved { tells: Some(Listener::Achievements) },
    },
    Setting {
        key: "drawing_unlimited",
        page: Page::GameSpeed,
        shows: Shows::Toggle { label: "EU4HARMONY_SETTINGS_DRAWING_UNLIMITED", field: field!(drawing_unlimited) },
        effect: Effect::WhenSaved { tells: Some(Listener::Drawing) },
    },
    Setting {
        key: "music_loudness",
        page: Page::Sound,
        shows: Shows::Toggle { label: "EU4HARMONY_SETTINGS_MUSIC_LOUDNESS", field: field!(music_loudness) },
        effect: Effect::WhenSaved { tells: Some(Listener::Sound) },
    },
    Setting {
        key: "music_leveling",
        page: Page::Sound,
        shows: Shows::Toggle { label: "EU4HARMONY_SETTINGS_MUSIC_LEVELING", field: field!(music_leveling) },
        effect: Effect::WhenSaved { tells: Some(Listener::Sound) },
    },
];

impl Setting {
    /// Its value in `settings` as the file keeps it.
    fn render(&self, settings: &Settings) -> String {
        match self.shows {
            Shows::Toggle { field, .. } => if (field.get)(settings) { "yes" } else { "no" }.to_string(),
        }
    }

    /// Takes `value`, as the file keeps it without quotes, into `settings`;
    /// one it cannot read leaves `settings` as they are.
    fn parse(&self, settings: &mut Settings, value: &str) {
        match self.shows {
            Shows::Toggle { field, .. } => {
                if let Some(on) = yes(value) {
                    (field.set)(settings, on);
                }
            }
        }
    }

    /// Copies its value from `from` into `to`.
    fn copy(&self, from: &Settings, to: &mut Settings) {
        match self.shows {
            Shows::Toggle { field, .. } => (field.set)(to, (field.get)(from)),
        }
    }

    /// Changes its value in `settings` to another: for tests that want every
    /// setting away from its default.
    #[cfg(test)]
    pub fn change(&self, settings: &mut Settings) {
        match self.shows {
            Shows::Toggle { field, .. } => (field.set)(settings, !(field.get)(settings)),
        }
    }
}

/// The parts told of the settings that take effect when saved, each once, in
/// the order of [`SETTINGS`].
fn listeners() -> Vec<Listener> {
    let mut out = Vec::new();
    for setting in &SETTINGS {
        if let Effect::WhenSaved { tells: Some(listener) } = setting.effect {
            if !out.contains(&listener) {
                out.push(listener);
            }
        }
    }
    out
}

/// Tells every [`Listener`] the settings this session runs with.
fn tell_listeners(current: &Settings) {
    for listener in listeners() {
        listener.tell(current);
    }
}

/// What a system font is chosen for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Body,
    Heading,
    Map,
    Console,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Body, Role::Heading, Role::Map, Role::Console];

    /// Its name in the settings file: `body` for `body_font` and `body_weight`.
    fn key(self) -> &'static str {
        match self {
            Role::Body => "body",
            Role::Heading => "heading",
            Role::Map => "map",
            Role::Console => "console",
        }
    }
}

/// The system font family that draws when none of a role's [`defaults`] is
/// installed: one Windows has in every language.
pub const LAST_RESORT: &str = "Segoe UI";

/// The fonts that draw `role` in `language` when the player chose none, most
/// wanted first: the first installed draws. Each language has its own, so
/// that Chinese, Japanese and Korean characters, which share code points,
/// are drawn in the shapes their readers write them in.
pub fn defaults(language: Language, role: Role) -> &'static [(&'static str, u16)] {
    use Language::*;
    match (role, language) {
        (Role::Console, _) => &[("Consolas", REGULAR)],
        (Role::Body, SimpChinese) => &[("FZPingXianYaSongS-R-GB", REGULAR), ("Microsoft YaHei", REGULAR)],
        (Role::Heading, SimpChinese) => &[("FZYaSongS-B-GB", REGULAR), ("Microsoft YaHei", REGULAR)],
        (Role::Map, SimpChinese) => &[("FZShengShiKaiShuS-EB-GB", REGULAR), ("KaiTi", REGULAR)],
        (Role::Body | Role::Heading, TradChinese) => &[("Microsoft JhengHei", REGULAR)],
        (Role::Map, TradChinese) => &[("DFKai-SB", REGULAR)],
        (Role::Body | Role::Heading, Japanese) => &[("Yu Gothic UI", REGULAR)],
        (Role::Map, Japanese) => &[("Yu Mincho", REGULAR)],
        (Role::Body | Role::Heading, Korean) => &[("Malgun Gothic", REGULAR)],
        (Role::Map, Korean) => &[("Batang", REGULAR)],
        (Role::Body, _) => &[("Segoe UI", REGULAR)],
        (Role::Heading, _) => &[("Segoe UI", 600)],
        (Role::Map, _) => &[("Georgia", REGULAR)],
    }
}

/// The face `role` is drawn in for `language` when the player chose none:
/// the first of its [`defaults`] that is installed, else [`LAST_RESORT`].
pub fn default_face(language: Language, role: Role, installed: impl Fn(&str) -> bool) -> Face {
    defaults(language, role).iter().find(|(family, _)| installed(family)).map_or(Face::regular(LAST_RESORT), |&(family, weight)| Face::new(family, weight))
}

/// The fonts a file from before the language could be chosen held when the
/// player had chosen none: they follow the language from then on.
const OLD_DEFAULTS: [(Role, &str); 4] =
    [(Role::Body, "Microsoft YaHei"), (Role::Heading, "Microsoft YaHei"), (Role::Map, "KaiTi"), (Role::Console, "Consolas")];

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: None,
            body_font: None,
            heading_font: None,
            map_font: None,
            console_font: None,
            map_outline: true,
            return_without_restart: true,
            save_ironman_before_load: true,
            achievements_outside_ironman: false,
            achievements_with_changed_checksum: false,
            drawing_unlimited: false,
            music_loudness: true,
            music_leveling: false,
        }
    }
}

impl Settings {
    /// The font chosen for `role`, if one is.
    pub fn chosen(&self, role: Role) -> Option<&Face> {
        match role {
            Role::Body => self.body_font.as_ref(),
            Role::Heading => self.heading_font.as_ref(),
            Role::Map => self.map_font.as_ref(),
            Role::Console => self.console_font.as_ref(),
        }
    }

    pub fn chosen_mut(&mut self, role: Role) -> &mut Option<Face> {
        match role {
            Role::Body => &mut self.body_font,
            Role::Heading => &mut self.heading_font,
            Role::Map => &mut self.map_font,
            Role::Console => &mut self.console_font,
        }
    }

    /// The language these settings show texts in: the one chosen, else English.
    pub fn language(&self) -> Language {
        self.language.unwrap_or(Language::English)
    }

    /// The face `role` is drawn in: the one chosen when it is installed, else
    /// the language's default ([`default_face`]).
    pub fn face(&self, role: Role, installed: impl Fn(&str) -> bool) -> Face {
        match self.chosen(role) {
            Some(face) if installed(&face.family) => face.clone(),
            _ => default_face(self.language(), role, installed),
        }
    }

    /// The font families chosen that `installed` does not have, each once,
    /// with the role each was first chosen for; text that asks for one is
    /// drawn in the role's default instead, and the player is told once
    /// (ADR framework-succeeds-or-fails-as-a-whole).
    pub fn missing_fonts(&self, installed: impl Fn(&str) -> bool) -> Vec<(&str, Role)> {
        let mut missing: Vec<(&str, Role)> = Vec::new();
        for role in Role::ALL {
            let Some(face) = self.chosen(role) else { continue };
            if !installed(&face.family) && !missing.iter().any(|(f, _)| *f == face.family) {
                missing.push((&face.family, role));
            }
        }
        missing
    }
}

impl Settings {
    /// Reads `key="value"` lines; anything unknown or malformed keeps its default.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        // A weight may come before its font.
        let mut families: [Option<String>; 4] = Default::default();
        let mut weights: [Option<u16>; 4] = Default::default();
        for (key, value) in eu4_harmony_manifest::line_entries(text) {
            if let Some(i) = Role::ALL.iter().position(|r| key.strip_prefix(r.key()) == Some("_font")) {
                if !value.is_empty() {
                    families[i] = Some(value.to_string());
                }
                continue;
            }
            if let Some(i) = Role::ALL.iter().position(|r| key.strip_prefix(r.key()) == Some("_weight")) {
                weights[i] = weight(value).or(weights[i]);
                continue;
            }
            if key == "language" {
                settings.language = Language::from_id(value).or(settings.language);
            } else if let Some(setting) = SETTINGS.iter().find(|s| s.key == key) {
                setting.parse(&mut settings, value);
            }
        }
        for (i, role) in Role::ALL.into_iter().enumerate() {
            let Some(family) = families[i].take() else { continue };
            let face = Face::new(family, weights[i].unwrap_or(REGULAR));
            let old_default = OLD_DEFAULTS.iter().any(|&(r, f)| r == role && face == Face::regular(f));
            if settings.language.is_some() || !old_default {
                *settings.chosen_mut(role) = Some(face);
            }
        }
        settings
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(language) = self.language {
            out.push_str(&format!("language=\"{}\"\n", language.id()));
        }
        for role in Role::ALL {
            if let Some(face) = self.chosen(role) {
                out.push_str(&format!("{0}_font=\"{1}\"\n{0}_weight={2}\n", role.key(), face.family, face.weight));
            }
        }
        for setting in &SETTINGS {
            out.push_str(&format!("{}={}\n", setting.key, setting.render(self)));
        }
        out
    }

    /// Takes from `chosen` the settings that take effect when they are saved
    /// ([`Effect::WhenSaved`]).
    fn take_immediate(&mut self, chosen: &Settings) {
        for setting in SETTINGS.iter().filter(|s| s.effect != Effect::NextStart) {
            setting.copy(chosen, self);
        }
    }

    /// Whether `chosen`, saved, would differ from these settings, which a
    /// session runs with, in what takes effect only at the next start: the
    /// language, the fonts, and each [`Effect::NextStart`] setting.
    pub fn waits_for_restart(&self, chosen: &Settings) -> bool {
        let mut saved = self.clone();
        saved.take_immediate(chosen);
        saved != *chosen
    }
}

/// A yes-or-no value as the file keeps it.
fn yes(value: &str) -> Option<bool> {
    match value {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// A weight as the file keeps it: its number, from 1 to 999.
fn weight(value: &str) -> Option<u16> {
    value.parse().ok().filter(|w| (1..=999).contains(w))
}

struct Store {
    path: PathBuf,
    /// What this session runs with.
    current: Settings,
    /// What the next session will run with, after changes in the settings window.
    next: Settings,
}

static STORE: OnceLock<Mutex<Store>> = OnceLock::new();

fn store() -> Option<std::sync::MutexGuard<'static, Store>> {
    STORE.get().map(|s| s.lock().unwrap_or_else(|e| e.into_inner()))
}

fn path(user_dir: &Path) -> PathBuf {
    user_dir.join("eu4-harmony").join("settings.txt")
}

/// What the settings file holds, as far as it can be read.
enum Stored {
    /// There is no file.
    Missing,
    /// The file's settings; `exact` is false when it is not UTF-8.
    Read { settings: Settings, exact: bool },
    /// The file is there, but could not be read.
    Unreadable(String),
}

fn stored(path: &Path) -> Stored {
    match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Stored::Read { settings: Settings::parse(&text), exact: true },
            // A file the player saved in the code page of Windows: the keys
            // and most values are ASCII, so the rest of the settings are
            // read past the bytes UTF-8 cannot; a font named in other
            // characters is then not installed, and drawn in its default.
            Err(e) => Stored::Read { settings: Settings::parse(&String::from_utf8_lossy(e.as_bytes())), exact: false },
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Stored::Missing,
        Err(e) => Stored::Unreadable(format!("{}: {e}", path.display())),
    }
}

/// Reads the settings in the user directory without keeping them, for a
/// startup that failed: what can be read of them, else defaults.
pub fn read(user_dir: &Path) -> Settings {
    match stored(&path(user_dir)) {
        Stored::Read { settings, .. } => settings,
        Stored::Missing | Stored::Unreadable(_) => Settings::default(),
    }
}

/// Reads the settings for this session from the user directory. The first
/// time, when they name no language, the language Windows displays is
/// chosen and written to the file. Every [`Listener`] is told them.
pub fn load(user_dir: &Path) -> Settings {
    let path = path(user_dir);
    let current = read_for_session(&path, crate::lang::from_windows);
    let _ = STORE.set(Mutex::new(Store { path, current: current.clone(), next: current.clone() }));
    tell_listeners(&current);
    current
}

/// The settings for a session from the file at `path`, with the language
/// `windows` answers when they name none. The language is written only
/// where writing loses nothing: to no file, or to one read as it was
/// written. A file that could not be read, or was not UTF-8, is left as the
/// player has it, which a write of what was read of it would not be.
fn read_for_session(path: &Path, windows: impl FnOnce() -> Language) -> Settings {
    let (mut current, kept) = match stored(path) {
        Stored::Missing => (Settings::default(), None),
        Stored::Read { settings, exact: true } => (settings, None),
        Stored::Read { settings, exact: false } => (settings, Some(format!("{} is not UTF-8", path.display()))),
        Stored::Unreadable(e) => {
            log!("settings: not read, defaults this session: {e}");
            (Settings::default(), Some(e))
        }
    };
    if current.language.is_none() {
        current.language = Some(windows());
        let language = current.language().id();
        let written = match kept {
            None => write(path, &current),
            Some(why) => Err(why),
        };
        match written {
            Ok(()) => log!("settings: the language is {language}, as Windows displays it"),
            Err(e) => log!("settings: the language is {language}, as Windows displays it, not written: {e}"),
        }
    }
    current
}

/// Writes the file whole or not at all: into a file beside it, which then
/// takes its place, so that the game ending between the two leaves the
/// player's settings as they were rather than an empty file.
fn write(path: &Path, settings: &Settings) -> crate::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let new = path.with_extension("txt.new");
    let written = std::fs::write(&new, settings.render()).map_err(|e| format!("{}: {e}", new.display()));
    let replaced = written.and_then(|()| std::fs::rename(&new, path).map_err(|e| format!("{}: {e}", path.display())));
    if replaced.is_err() {
        let _ = std::fs::remove_file(&new);
    }
    replaced
}

/// The settings this session runs with; defaults before [`load`].
pub fn current() -> Settings {
    store().map(|s| s.current.clone()).unwrap_or_default()
}

/// The settings the next session will run with.
pub fn next() -> Settings {
    store().map(|s| s.next.clone()).unwrap_or_default()
}

/// Saves settings: those that take effect when saved ([`Effect`]) do now,
/// and every [`Listener`] is told; the rest, the language and the fonts
/// among them, take effect at the next start.
pub fn save(settings: Settings) -> crate::Result<()> {
    let current = {
        let mut store = store().ok_or("settings not loaded")?;
        write(&store.path, &settings)?;
        store.current.take_immediate(&settings);
        store.next = settings;
        store.current.clone()
    };
    tell_listeners(&current);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_font_not_installed_is_missing() {
        let chosen = Settings {
            body_font: Some(Face::new("Gone", 700)),
            heading_font: Some(Face::regular("Also gone")),
            map_font: Some(Face::regular("Gone")),
            console_font: Some(Face::regular("Console gone")),
            ..Settings::default()
        };
        assert_eq!(chosen.missing_fonts(|f| !f.contains("one")), [("Gone", Role::Body), ("Also gone", Role::Heading), ("Console gone", Role::Console)]);
        assert!(Settings::default().missing_fonts(|_| false).is_empty());
    }

    #[test]
    fn a_font_not_chosen_follows_the_language() {
        let installed = |f: &str| ["Microsoft YaHei", "KaiTi", "Segoe UI", "Georgia", "Consolas", "Yu Gothic UI"].contains(&f);
        let chinese = Settings { language: Some(Language::SimpChinese), ..Settings::default() };
        assert_eq!(chinese.face(Role::Body, installed), Face::regular("Microsoft YaHei"));
        assert_eq!(chinese.face(Role::Body, |_| true), Face::regular("FZPingXianYaSongS-R-GB"));
        assert_eq!(chinese.face(Role::Map, installed), Face::regular("KaiTi"));
        let english = Settings { language: Some(Language::English), ..Settings::default() };
        assert_eq!(english.face(Role::Heading, installed), Face::new("Segoe UI", 600));
        assert_eq!(english.face(Role::Map, installed), Face::regular("Georgia"));
        assert_eq!(english.face(Role::Console, installed), Face::regular("Consolas"));
        // Japanese without its map font installed.
        let japanese = Settings { language: Some(Language::Japanese), ..Settings::default() };
        assert_eq!(japanese.face(Role::Body, installed), Face::regular("Yu Gothic UI"));
        assert_eq!(japanese.face(Role::Map, installed), Face::regular(LAST_RESORT));
        // One chosen draws while it is installed.
        let chosen = Settings { map_font: Some(Face::new("SimSun", 700)), ..english.clone() };
        assert_eq!(chosen.face(Role::Map, |f| f == "SimSun"), Face::new("SimSun", 700));
        assert_eq!(chosen.face(Role::Map, installed), Face::regular("Georgia"));
    }

    #[test]
    fn the_default_for_latin_headings_is_a_weight_segoe_ui_has() {
        let Ok(fonts) = crate::unicode::system_font::SystemFonts::new() else { return };
        if !fonts.has_family("Segoe UI") {
            eprintln!("Segoe UI is not installed; skipped");
            return;
        }
        let heading = default_face(Language::French, Role::Heading, |f| fonts.has_family(f));
        assert_eq!(fonts.drawn(&heading), Some(heading));
    }

    #[test]
    fn round_trips_and_keeps_defaults_for_the_rest() {
        let custom = Settings {
            language: Some(Language::BrazPor),
            body_font: Some(Face::new("SimSun", 700)),
            heading_font: Some(Face::new("Microsoft YaHei", 700)),
            map_font: Some(Face::new("KaiTi", 290)),
            console_font: Some(Face::new("Cascadia Mono", 600)),
            map_outline: false,
            return_without_restart: false,
            save_ironman_before_load: false,
            achievements_outside_ironman: true,
            achievements_with_changed_checksum: true,
            drawing_unlimited: true,
            music_loudness: false,
            music_leveling: true,
        };
        assert_eq!(Settings::parse(&custom.render()), custom);
        assert!(custom.render().starts_with("language=\"braz_por\"\nbody_font=\"SimSun\"\nbody_weight=700\n"));
        assert!(custom.render().ends_with("\nachievements_with_changed_checksum=yes\ndrawing_unlimited=yes\nmusic_loudness=no\nmusic_leveling=yes\n"));
        assert!(custom.render().contains("\nheading_font=\"Microsoft YaHei\"\nheading_weight=700\n"));
        assert!(custom.render().contains("\nconsole_font=\"Cascadia Mono\"\nconsole_weight=600\nmap_outline=no\nreturn_without_restart=no\n"));
        // Fonts not chosen are not written.
        let following = Settings { language: Some(Language::English), ..Settings::default() };
        assert_eq!(following.render().lines().next(), Some("language=\"english\""));
        // As every file written before has it.
        assert_eq!(
            following.render(),
            "language=\"english\"\nmap_outline=yes\nreturn_without_restart=yes\nsave_ironman_before_load=yes\nachievements_outside_ironman=no\nachievements_with_changed_checksum=no\ndrawing_unlimited=no\nmusic_loudness=yes\nmusic_leveling=no\n"
        );
        assert!(!following.render().contains("_font"));
        assert_eq!(Settings::parse(&following.render()), following);
        // A weight without its font follows the language with the font.
        assert_eq!(Settings::parse("language=english\nconsole_weight=700\n").console_font, None);
        assert_eq!(Settings::parse("language=\"klingon\"").language, None);
        // A file from before the language was chosen: the fonts it holds that
        // were the defaults then follow the language; others stay chosen.
        let old = "body_font=\"Microsoft YaHei\"\nbody_weight=400\nheading_font=\"Microsoft YaHei\"\nheading_weight=700\nmap_font=\"KaiTi\"\nmap_weight=400\nconsole_font=\"Consolas\"\nconsole_weight=400\n";
        let old = Settings::parse(old);
        assert_eq!((old.language, old.body_font, old.map_font, old.console_font), (None, None, None, None));
        assert_eq!(old.heading_font, Some(Face::new("Microsoft YaHei", 700)));
        // A file from when text could keep the fonts' own glyphs.
        let partial = Settings::parse("# comment\ntext_appearance=\"bitmap\"\nbody_font=\"DengXian\"\nmap_outline=maybe\nmap_weight=1000\nunknown=1\n");
        assert_eq!(partial, Settings { body_font: Some(Face::regular("DengXian")), ..Settings::default() });
        assert!(!Settings::parse("map_outline=\"no\"").map_outline);
        // A weight before its font, in quotes, keeps to its font.
        assert_eq!(Settings::parse("body_weight=\"300\"\nbody_font=\"DengXian\"").body_font, Some(Face::new("DengXian", 300)));
        assert_eq!(Settings::parse("body_weight=bold\nbody_font=\"DengXian\"").body_font, Some(Face::regular("DengXian")));
        assert_eq!(Settings::parse(""), Settings::default());
        assert!(Settings::parse("return_without_restart=maybe").return_without_restart);
        assert!(!Settings::parse("save_ironman_before_load=\"no\"").save_ironman_before_load);
        // Achievements follow vanilla's rules unless the file says otherwise.
        assert!(!Settings::parse("achievements_outside_ironman=maybe").achievements_outside_ironman);
        assert!(Settings::parse("achievements_with_changed_checksum=yes").achievements_with_changed_checksum);
        // A file from when remote control's view was a setting reads as one
        // without it (ADR requests-act-as-the-player-or-as-the-game).
        assert_eq!(Settings::parse("remote_full_view=yes\n"), Settings::default());
        // The music is brought to one loudness and not leveled unless the file says otherwise.
        assert!(Settings::parse("music_loudness=maybe").music_loudness);
        assert!(!Settings::parse("music_leveling=maybe").music_leveling);
    }

    /// A directory of its own for a test, empty.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu4h-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_first_line() {
        let read = Settings::parse("\u{feff}language=\"english\"\nbody_font=\"Microsoft YaHei\"\nbody_weight=400\n");
        assert_eq!(read.language, Some(Language::English));
        assert_eq!(read.body_font, Some(Face::regular("Microsoft YaHei")));
    }

    #[test]
    fn the_language_is_written_only_to_a_file_read_as_it_was_written() {
        let dir = scratch("language");
        let file = dir.join("settings.txt");
        // No file: the language Windows displays is chosen and written.
        let read = read_for_session(&file, || Language::German);
        assert_eq!(read.language, Some(Language::German));
        assert_eq!(Settings::parse(&std::fs::read_to_string(&file).unwrap()), read);
        // A file without the language gets it, and keeps the rest.
        std::fs::write(&file, "map_outline=no\n").unwrap();
        let read = read_for_session(&file, || Language::French);
        assert_eq!((read.language, read.map_outline), (Some(Language::French), false));
        assert_eq!(Settings::parse(&std::fs::read_to_string(&file).unwrap()), read);
        // A file saved with a byte order mark keeps its language and fonts.
        let marked = "\u{feff}language=\"english\"\nbody_font=\"Microsoft YaHei\"\nbody_weight=400\n";
        std::fs::write(&file, marked).unwrap();
        let read = read_for_session(&file, || Language::German);
        assert_eq!((read.language, read.body_font), (Some(Language::English), Some(Face::regular("Microsoft YaHei"))));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), marked);
        // A file saved in the code page of Windows, not in UTF-8: what can be
        // read of it is, and the file is left as the player saved it.
        let ansi = b"map_outline=no\nbody_font=\"\xce\xa2\xc8\xed\xd1\xc5\xba\xda\"\n".to_vec();
        std::fs::write(&file, &ansi).unwrap();
        let read = read_for_session(&file, || Language::German);
        assert_eq!((read.language, read.map_outline), (Some(Language::German), false));
        assert_eq!(std::fs::read(&file).unwrap(), ansi);
        // A file another program holds open: nothing is read, nothing written.
        std::fs::write(&file, "map_outline=no\n").unwrap();
        {
            use std::os::windows::fs::OpenOptionsExt;
            let _held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
            let read = read_for_session(&file, || Language::German);
            assert_eq!(read, Settings { language: Some(Language::German), ..Settings::default() });
        }
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "map_outline=no\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_write_replaces_the_file_whole_and_leaves_nothing_beside_it() {
        let dir = scratch("write");
        let file = dir.join("eu4-harmony").join("settings.txt");
        let english = Settings { language: Some(Language::English), ..Settings::default() };
        write(&file, &english).unwrap();
        let german = Settings { language: Some(Language::German), map_outline: false, ..Settings::default() };
        write(&file, &german).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), german.render());
        let beside: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(beside, ["settings.txt"]);
        // A file that cannot be replaced is left as it was, and nothing beside it.
        std::fs::write(&file, "map_outline=no\n").unwrap();
        {
            use std::os::windows::fs::OpenOptionsExt;
            let _held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
            assert!(write(&file, &english).is_err());
        }
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "map_outline=no\n");
        assert_eq!(std::fs::read_dir(file.parent().unwrap()).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_settings_that_take_effect_when_saved_do_and_reach_their_listeners() {
        for setting in &SETTINGS {
            let mut chosen = Settings { language: Some(Language::German), body_font: Some(Face::regular("SimSun")), ..Settings::default() };
            setting.change(&mut chosen);
            let mut current = Settings::default();
            current.take_immediate(&chosen);
            match setting.effect {
                Effect::NextStart => assert_eq!(current, Settings::default(), "{}", setting.key),
                Effect::WhenSaved { tells } => {
                    let mut expected = Settings::default();
                    setting.change(&mut expected);
                    assert_eq!(current, expected, "{}", setting.key);
                    if let Some(listener) = tells {
                        assert!(listeners().contains(&listener), "{}", setting.key);
                    }
                }
            }
        }
        // The language and the fonts wait for the next start.
        let mut current = Settings::default();
        current.take_immediate(&Settings { language: Some(Language::German), body_font: Some(Face::regular("SimSun")), ..Settings::default() });
        assert_eq!(current, Settings::default());
        // Each part is told once.
        assert_eq!(listeners(), [Listener::Achievements, Listener::Drawing, Listener::Sound]);
    }

    #[test]
    fn every_setting_round_trips_through_the_file() {
        for setting in &SETTINGS {
            let mut changed = Settings { language: Some(Language::English), ..Settings::default() };
            setting.change(&mut changed);
            assert_ne!(changed, Settings { language: Some(Language::English), ..Settings::default() }, "{}", setting.key);
            assert_eq!(Settings::parse(&changed.render()), changed, "{}", setting.key);
        }
        let mut every = Settings::default();
        for setting in &SETTINGS {
            setting.change(&mut every);
        }
        assert_eq!(Settings::parse(&every.render()), every);
    }

    #[test]
    fn an_unknown_or_unreadable_value_keeps_the_default() {
        for setting in &SETTINGS {
            for value in ["maybe", "\"maybe\"", "", "1", "YES"] {
                assert_eq!(Settings::parse(&format!("{}={value}\n", setting.key)), Settings::default(), "{}={value}", setting.key);
            }
            // Away from the default, a line it cannot read keeps the one before.
            let mut changed = Settings::default();
            setting.change(&mut changed);
            assert_eq!(Settings::parse(&format!("{}{}=maybe\n", changed.render(), setting.key)), changed, "{}", setting.key);
        }
        assert_eq!(Settings::parse("unknown=yes\nmap_outlines=no\n"), Settings::default());
    }

    #[test]
    fn every_setting_has_a_key_of_its_own() {
        let mut keys: Vec<&str> = vec!["language"];
        keys.extend(Role::ALL.iter().flat_map(|r| [format!("{}_font", r.key()), format!("{}_weight", r.key())]).map(|k| &*k.leak()));
        for setting in &SETTINGS {
            assert!(!keys.contains(&setting.key), "{} is taken", setting.key);
            keys.push(setting.key);
        }
    }
}
