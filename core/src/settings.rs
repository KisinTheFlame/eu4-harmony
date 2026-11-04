//! Framework settings (GLOSSARY): the player's choices for the framework,
//! kept in `eu4-harmony/settings.txt` under the user directory and written the
//! way the game writes its own `settings.txt`. They are read once at startup.
//! A change made in the settings window to how a session is left, or to the
//! pace at the fastest speed or the achievement rules, takes effect when it is
//! saved; a change to the language or the fonts, at the next start.
//!
//! A font the player has not chosen follows the player language: it is the
//! first installed of the language's [`defaults`], and is not written to the
//! file, so that it changes with the language.
use crate::speed::Pace;
use crate::unicode::system_font::{Face, REGULAR};
use eu4_harmony_loc::Language;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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
    /// Whether the game draws frames inside a day at the fastest speed
    /// (docs/game-speed.md, S4).
    pub fastest_speed: Pace,
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
            fastest_speed: Pace::Smooth,
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
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim().trim_matches('"'));
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
            match key {
                "language" => settings.language = Language::from_id(value).or(settings.language),
                "map_outline" => settings.map_outline = yes(value).unwrap_or(settings.map_outline),
                "return_without_restart" => settings.return_without_restart = yes(value).unwrap_or(settings.return_without_restart),
                "save_ironman_before_load" => settings.save_ironman_before_load = yes(value).unwrap_or(settings.save_ironman_before_load),
                "achievements_outside_ironman" => settings.achievements_outside_ironman = yes(value).unwrap_or(settings.achievements_outside_ironman),
                "achievements_with_changed_checksum" => {
                    settings.achievements_with_changed_checksum = yes(value).unwrap_or(settings.achievements_with_changed_checksum)
                }
                "fastest_speed" => settings.fastest_speed = Pace::from_name(value).unwrap_or(settings.fastest_speed),
                _ => {}
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
        let yes_no = |on: bool| if on { "yes" } else { "no" };
        let mut out = String::new();
        if let Some(language) = self.language {
            out.push_str(&format!("language=\"{}\"\n", language.id()));
        }
        for role in Role::ALL {
            if let Some(face) = self.chosen(role) {
                out.push_str(&format!("{0}_font=\"{1}\"\n{0}_weight={2}\n", role.key(), face.family, face.weight));
            }
        }
        out.push_str(&format!(
            "map_outline={}\nreturn_without_restart={}\nsave_ironman_before_load={}\nachievements_outside_ironman={}\nachievements_with_changed_checksum={}\nfastest_speed=\"{}\"\n",
            yes_no(self.map_outline),
            yes_no(self.return_without_restart),
            yes_no(self.save_ironman_before_load),
            yes_no(self.achievements_outside_ironman),
            yes_no(self.achievements_with_changed_checksum),
            self.fastest_speed.name()
        ));
        out
    }

    /// Takes from `chosen` the settings that take effect when they are saved:
    /// those for leaving a session, the achievement rules, and the pace at the
    /// fastest speed.
    fn take_immediate(&mut self, chosen: &Settings) {
        self.return_without_restart = chosen.return_without_restart;
        self.save_ironman_before_load = chosen.save_ironman_before_load;
        self.achievements_outside_ironman = chosen.achievements_outside_ironman;
        self.achievements_with_changed_checksum = chosen.achievements_with_changed_checksum;
        self.fastest_speed = chosen.fastest_speed;
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

/// Reads the settings in the user directory without keeping them, for a
/// startup that failed.
pub fn read(user_dir: &Path) -> Settings {
    std::fs::read_to_string(path(user_dir)).map(|t| Settings::parse(&t)).unwrap_or_default()
}

/// Reads the settings for this session from the user directory. The first
/// time, when they name no language, the language Windows displays is
/// chosen and written to the file.
pub fn load(user_dir: &Path) -> Settings {
    let path = path(user_dir);
    let mut current = read(user_dir);
    if current.language.is_none() {
        current.language = Some(crate::lang::from_windows());
        match write(&path, &current) {
            Ok(()) => log!("settings: the language is {}, as Windows displays it", current.language().id()),
            Err(e) => log!("settings: the language chosen from Windows was not written: {e}"),
        }
    }
    let _ = STORE.set(Mutex::new(Store { path, current: current.clone(), next: current.clone() }));
    crate::achievements::follow(&current);
    current
}

fn write(path: &Path, settings: &Settings) -> crate::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, settings.render()).map_err(|e| format!("{}: {e}", path.display()))
}

/// The settings this session runs with; defaults before [`load`].
pub fn current() -> Settings {
    store().map(|s| s.current.clone()).unwrap_or_default()
}

/// The settings the next session will run with.
pub fn next() -> Settings {
    store().map(|s| s.next.clone()).unwrap_or_default()
}

/// Saves settings: those for leaving a session, the achievement rules and the
/// pace at the fastest speed take effect now, the language and the fonts at
/// the next start.
pub fn save(settings: Settings) -> crate::Result<()> {
    let mut store = store().ok_or("settings not loaded")?;
    write(&store.path, &settings)?;
    store.current.take_immediate(&settings);
    crate::achievements::follow(&store.current);
    store.next = settings;
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
            fastest_speed: Pace::Faster,
        };
        assert_eq!(Settings::parse(&custom.render()), custom);
        assert!(custom.render().starts_with("language=\"braz_por\"\nbody_font=\"SimSun\"\nbody_weight=700\n"));
        assert!(custom.render().ends_with("\nfastest_speed=\"faster\"\n"));
        assert_eq!(Settings::parse("fastest_speed=faster").fastest_speed, Pace::Faster);
        assert_eq!(Settings::parse("fastest_speed=\"fastest\"").fastest_speed, Pace::Smooth);
        assert!(custom.render().contains("\nheading_font=\"Microsoft YaHei\"\nheading_weight=700\n"));
        assert!(custom.render().contains("\nconsole_font=\"Cascadia Mono\"\nconsole_weight=600\nmap_outline=no\nreturn_without_restart=no\n"));
        // Fonts not chosen are not written.
        let following = Settings { language: Some(Language::English), ..Settings::default() };
        assert_eq!(following.render().lines().next(), Some("language=\"english\""));
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
    }

    #[test]
    fn the_settings_for_leaving_a_session_achievements_and_the_pace_take_effect_when_saved() {
        let mut current = Settings::default();
        let chosen = Settings {
            language: Some(Language::German),
            body_font: Some(Face::regular("SimSun")),
            map_outline: false,
            return_without_restart: false,
            save_ironman_before_load: false,
            achievements_outside_ironman: true,
            achievements_with_changed_checksum: true,
            fastest_speed: Pace::Faster,
            ..Settings::default()
        };
        current.take_immediate(&chosen);
        let immediate = Settings {
            return_without_restart: false,
            save_ironman_before_load: false,
            achievements_outside_ironman: true,
            achievements_with_changed_checksum: true,
            fastest_speed: Pace::Faster,
            ..Settings::default()
        };
        assert_eq!(current, immediate);
    }
}
