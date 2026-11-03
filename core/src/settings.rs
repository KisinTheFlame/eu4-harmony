//! Framework settings (GLOSSARY): the player's choices for the framework,
//! kept in `eu4-harmony/settings.txt` under the user directory and written the
//! way the game writes its own `settings.txt`. They are read once at startup.
//! A change made in the settings window to how a session is left, or to the
//! pace at the fastest speed, takes effect when it is saved; a change to the
//! fonts, at the next start.
use crate::speed::Pace;
use crate::unicode::system_font::Face;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The system font and weight for body text (GLOSSARY), and for the
    /// framework UI.
    pub body_font: Face,
    /// The system font and weight for headings (GLOSSARY).
    pub heading_font: Face,
    /// The system font and weight for names on the map.
    pub map_font: Face,
    /// The system font and weight for the game console (GLOSSARY).
    pub console_font: Face,
    /// Whether system glyphs on the map get the outline the map font's own
    /// glyphs have.
    pub map_outline: bool,
    /// Whether returning to the main menu is an in-process return (GLOSSARY)
    /// rather than a restart (ADR 0022).
    pub return_without_restart: bool,
    /// Whether an in-game load in ironman writes the ironman exit save first;
    /// without it, loading the same save again is an undo (ADR 0023).
    pub save_ironman_before_load: bool,
    /// Whether the game draws frames inside a day at the fastest speed
    /// (docs/game-speed.md, S4).
    pub fastest_speed: Pace,
}

/// The system font family text falls back to, and the framework UI draws
/// with, when the one chosen is not installed: one Windows has in every
/// language.
pub const DEFAULT_FONT: &str = "Microsoft YaHei";

/// The game console's font when none is chosen: monospaced, as consoles are;
/// Chinese in it falls back to [`DEFAULT_FONT`].
pub const DEFAULT_CONSOLE_FONT: &str = "Consolas";

impl Default for Settings {
    fn default() -> Self {
        Self {
            body_font: Face::regular(DEFAULT_FONT),
            heading_font: Face::regular(DEFAULT_FONT),
            map_font: Face::regular("KaiTi"),
            console_font: Face::regular(DEFAULT_CONSOLE_FONT),
            map_outline: true,
            return_without_restart: true,
            save_ironman_before_load: true,
            fastest_speed: Pace::Smooth,
        }
    }
}

impl Settings {
    /// Every font chosen: for body text, headings, the map and the game
    /// console.
    pub fn fonts(&self) -> [&Face; 4] {
        [&self.body_font, &self.heading_font, &self.map_font, &self.console_font]
    }

    /// The font families chosen that `installed` does not have, each once;
    /// text that asks for one is drawn with [`DEFAULT_FONT`] instead, and the
    /// player is told once (ADR 0012).
    pub fn missing_fonts(&self, installed: impl Fn(&str) -> bool) -> Vec<&str> {
        let mut missing: Vec<&str> = Vec::new();
        for face in self.fonts() {
            if !installed(&face.family) && !missing.contains(&face.family.as_str()) {
                missing.push(&face.family);
            }
        }
        missing
    }
}

impl Settings {
    /// Reads `key="value"` lines; anything unknown or malformed keeps its default.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "body_font" if !value.is_empty() => settings.body_font.family = value.to_string(),
                "heading_font" if !value.is_empty() => settings.heading_font.family = value.to_string(),
                "map_font" if !value.is_empty() => settings.map_font.family = value.to_string(),
                "console_font" if !value.is_empty() => settings.console_font.family = value.to_string(),
                "body_weight" => settings.body_font.weight = weight(value).unwrap_or(settings.body_font.weight),
                "heading_weight" => settings.heading_font.weight = weight(value).unwrap_or(settings.heading_font.weight),
                "map_weight" => settings.map_font.weight = weight(value).unwrap_or(settings.map_font.weight),
                "console_weight" => settings.console_font.weight = weight(value).unwrap_or(settings.console_font.weight),
                "map_outline" => settings.map_outline = yes(value).unwrap_or(settings.map_outline),
                "return_without_restart" => settings.return_without_restart = yes(value).unwrap_or(settings.return_without_restart),
                "save_ironman_before_load" => settings.save_ironman_before_load = yes(value).unwrap_or(settings.save_ironman_before_load),
                "fastest_speed" => settings.fastest_speed = Pace::from_name(value).unwrap_or(settings.fastest_speed),
                _ => {}
            }
        }
        settings
    }

    pub fn render(&self) -> String {
        let yes_no = |on: bool| if on { "yes" } else { "no" };
        let (body, heading, map, console) = (&self.body_font, &self.heading_font, &self.map_font, &self.console_font);
        format!(
            "body_font=\"{}\"\nbody_weight={}\nheading_font=\"{}\"\nheading_weight={}\nmap_font=\"{}\"\nmap_weight={}\nmap_outline={}\nconsole_font=\"{}\"\nconsole_weight={}\nreturn_without_restart={}\nsave_ironman_before_load={}\nfastest_speed=\"{}\"\n",
            body.family,
            body.weight,
            heading.family,
            heading.weight,
            map.family,
            map.weight,
            yes_no(self.map_outline),
            console.family,
            console.weight,
            yes_no(self.return_without_restart),
            yes_no(self.save_ironman_before_load),
            self.fastest_speed.name()
        )
    }

    /// Takes from `chosen` the settings that take effect when they are saved:
    /// those for leaving a session, and the pace at the fastest speed.
    fn take_immediate(&mut self, chosen: &Settings) {
        self.return_without_restart = chosen.return_without_restart;
        self.save_ironman_before_load = chosen.save_ironman_before_load;
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

/// Reads the settings for this session from the user directory.
pub fn load(user_dir: &Path) -> Settings {
    let path = user_dir.join("eu4-harmony").join("settings.txt");
    let current = std::fs::read_to_string(&path).map(|t| Settings::parse(&t)).unwrap_or_default();
    let _ = STORE.set(Mutex::new(Store { path, current: current.clone(), next: current.clone() }));
    current
}

/// The settings this session runs with; defaults before [`load`].
pub fn current() -> Settings {
    store().map(|s| s.current.clone()).unwrap_or_default()
}

/// The settings the next session will run with.
pub fn next() -> Settings {
    store().map(|s| s.next.clone()).unwrap_or_default()
}

/// Saves settings: those for leaving a session and the pace at the fastest
/// speed take effect now, the rest at the next start.
pub fn save(settings: Settings) -> crate::Result<()> {
    let mut store = store().ok_or("settings not loaded")?;
    if let Some(dir) = store.path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&store.path, settings.render()).map_err(|e| format!("{}: {e}", store.path.display()))?;
    store.current.take_immediate(&settings);
    store.next = settings;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_font_not_installed_is_missing() {
        let chosen = Settings {
            body_font: Face::new("Gone", 700),
            heading_font: Face::regular("Also gone"),
            map_font: Face::regular("Gone"),
            console_font: Face::regular("Console gone"),
            ..Settings::default()
        };
        assert_eq!(chosen.missing_fonts(|f| !f.contains("one")), ["Gone", "Also gone", "Console gone"]);
        assert!(Settings::default().missing_fonts(|_| true).is_empty());
    }

    #[test]
    fn round_trips_and_keeps_defaults_for_the_rest() {
        let custom = Settings {
            body_font: Face::new("SimSun", 700),
            heading_font: Face::new("Microsoft YaHei", 700),
            map_font: Face::new("KaiTi", 290),
            console_font: Face::new("Cascadia Mono", 600),
            map_outline: false,
            return_without_restart: false,
            save_ironman_before_load: false,
            fastest_speed: Pace::Faster,
        };
        assert_eq!(Settings::parse(&custom.render()), custom);
        assert!(custom.render().ends_with("\nfastest_speed=\"faster\"\n"));
        assert_eq!(Settings::parse("fastest_speed=faster").fastest_speed, Pace::Faster);
        assert_eq!(Settings::parse("fastest_speed=\"fastest\"").fastest_speed, Pace::Smooth);
        assert!(custom.render().contains("\nbody_weight=700\n"));
        assert!(custom.render().contains("\nheading_font=\"Microsoft YaHei\"\nheading_weight=700\n"));
        assert!(custom.render().contains("\nmap_outline=no\nconsole_font=\"Cascadia Mono\"\nconsole_weight=600\nreturn_without_restart=no\n"));
        // A file from before the game console had a font of its own.
        assert_eq!(Settings::parse("map_outline=no\n").console_font, Face::regular("Consolas"));
        assert_eq!(Settings::parse("console_weight=700\n").console_font, Face::new("Consolas", 700));
        // A file from before headings had a font of their own, and from when
        // text could keep the fonts' own glyphs.
        let partial = Settings::parse("# comment\ntext_appearance=\"bitmap\"\nbody_font=\"DengXian\"\nmap_outline=maybe\nmap_weight=1000\nunknown=1\n");
        assert_eq!(partial, Settings { body_font: Face::regular("DengXian"), ..Settings::default() });
        assert!(!Settings::parse("map_outline=\"no\"").map_outline);
        // A weight before its font, in quotes, keeps to its font.
        assert_eq!(Settings::parse("body_weight=\"300\"\nbody_font=\"DengXian\"").body_font, Face::new("DengXian", 300));
        assert_eq!(Settings::parse("body_weight=bold").body_font, Settings::default().body_font);
        assert_eq!(Settings::parse(""), Settings::default());
        assert!(Settings::parse("return_without_restart=maybe").return_without_restart);
        assert!(!Settings::parse("save_ironman_before_load=\"no\"").save_ironman_before_load);
    }

    #[test]
    fn the_settings_for_leaving_a_session_and_the_pace_take_effect_when_saved() {
        let mut current = Settings::default();
        let chosen = Settings {
            body_font: Face::regular("SimSun"),
            map_outline: false,
            return_without_restart: false,
            save_ironman_before_load: false,
            fastest_speed: Pace::Faster,
            ..Settings::default()
        };
        current.take_immediate(&chosen);
        assert_eq!(current, Settings { return_without_restart: false, save_ironman_before_load: false, fastest_speed: Pace::Faster, ..Settings::default() });
    }
}
