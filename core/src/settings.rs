//! Framework settings (GLOSSARY): the player's choices for the framework,
//! kept in `eu4-harmony/settings.txt` under the user directory and written the
//! way the game writes its own `settings.txt`. They are read once at startup;
//! a change made in the settings window takes effect at the next start.
use crate::unicode::system_font::Face;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// How the game's own text is drawn (ADR 0010).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAppearance {
    /// A font's bitmap glyphs first; system fonts only for what it lacks.
    Bitmap,
    /// Every character from the chosen system fonts.
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub text_appearance: TextAppearance,
    /// The system font and weight for text in general, and for the framework
    /// UI.
    pub body_font: Face,
    /// The system font and weight for names on the map.
    pub map_font: Face,
    /// Whether system glyphs on the map get the outline the map font's own
    /// glyphs have.
    pub map_outline: bool,
}

/// The system font family text falls back to, and the framework UI draws
/// with, when the one chosen is not installed: one Windows has in every
/// language.
pub const DEFAULT_FONT: &str = "Microsoft YaHei";

impl Default for Settings {
    fn default() -> Self {
        Self { text_appearance: TextAppearance::Bitmap, body_font: Face::regular(DEFAULT_FONT), map_font: Face::regular("KaiTi"), map_outline: true }
    }
}

impl Settings {
    /// The font families chosen that `installed` does not have; text that
    /// asks for one is drawn with [`DEFAULT_FONT`] instead, and the player
    /// is told once (ADR 0012).
    pub fn missing_fonts(&self, installed: impl Fn(&str) -> bool) -> Vec<&str> {
        let mut missing: Vec<&str> = [self.body_font.family.as_str(), self.map_font.family.as_str()].into_iter().filter(|f| !installed(f)).collect();
        missing.dedup();
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
                "text_appearance" => match value {
                    "bitmap" => settings.text_appearance = TextAppearance::Bitmap,
                    "system" => settings.text_appearance = TextAppearance::System,
                    _ => {}
                },
                "body_font" if !value.is_empty() => settings.body_font.family = value.to_string(),
                "map_font" if !value.is_empty() => settings.map_font.family = value.to_string(),
                "body_weight" => settings.body_font.weight = weight(value).unwrap_or(settings.body_font.weight),
                "map_weight" => settings.map_font.weight = weight(value).unwrap_or(settings.map_font.weight),
                "map_outline" => match value {
                    "yes" => settings.map_outline = true,
                    "no" => settings.map_outline = false,
                    _ => {}
                },
                _ => {}
            }
        }
        settings
    }

    pub fn render(&self) -> String {
        let appearance = match self.text_appearance {
            TextAppearance::Bitmap => "bitmap",
            TextAppearance::System => "system",
        };
        let outline = if self.map_outline { "yes" } else { "no" };
        let (body, map) = (&self.body_font, &self.map_font);
        format!(
            "text_appearance=\"{appearance}\"\nbody_font=\"{}\"\nbody_weight={}\nmap_font=\"{}\"\nmap_weight={}\nmap_outline={outline}\n",
            body.family, body.weight, map.family, map.weight
        )
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

/// Saves settings for the next session.
pub fn save(settings: Settings) -> crate::Result<()> {
    let mut store = store().ok_or("settings not loaded")?;
    if let Some(dir) = store.path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&store.path, settings.render()).map_err(|e| format!("{}: {e}", store.path.display()))?;
    store.next = settings;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_font_not_installed_is_missing() {
        let chosen = Settings { body_font: Face::new("Gone", 700), map_font: Face::regular("Gone"), ..Settings::default() };
        assert_eq!(chosen.missing_fonts(|f| f != "Gone"), ["Gone"]);
        assert!(Settings::default().missing_fonts(|_| true).is_empty());
    }

    #[test]
    fn round_trips_and_keeps_defaults_for_the_rest() {
        let custom =
            Settings { text_appearance: TextAppearance::System, body_font: Face::new("SimSun", 700), map_font: Face::new("KaiTi", 290), map_outline: false };
        assert_eq!(Settings::parse(&custom.render()), custom);
        assert!(custom.render().contains("\nbody_weight=700\n"));
        assert!(custom.render().contains("\nmap_outline=no\n"));
        let partial = Settings::parse("# comment\nbody_font=\"DengXian\"\ntext_appearance=\"neither\"\nmap_outline=maybe\nmap_weight=1000\nunknown=1\n");
        assert_eq!(partial, Settings { body_font: Face::regular("DengXian"), ..Settings::default() });
        assert!(!Settings::parse("map_outline=\"no\"").map_outline);
        // A weight before its font, in quotes, keeps to its font.
        assert_eq!(Settings::parse("body_weight=\"300\"\nbody_font=\"DengXian\"").body_font, Face::new("DengXian", 300));
        assert_eq!(Settings::parse("body_weight=bold").body_font, Settings::default().body_font);
        assert_eq!(Settings::parse(""), Settings::default());
    }
}
