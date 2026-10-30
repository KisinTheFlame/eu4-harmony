//! Framework settings (GLOSSARY): the player's choices for the framework,
//! kept in `eu4-harmony/settings.txt` under the user directory and written the
//! way the game writes its own `settings.txt`. They are read once at startup;
//! a change made in the settings window takes effect at the next start.
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
    /// The system font family for text in general, and for the framework UI.
    pub body_font: String,
    /// The system font family for names on the map.
    pub map_font: String,
}

/// The system font family text falls back to, and the framework UI draws
/// with, when the one chosen is not installed: one Windows has in every
/// language.
pub const DEFAULT_FONT: &str = "Microsoft YaHei";

impl Default for Settings {
    fn default() -> Self {
        Self { text_appearance: TextAppearance::Bitmap, body_font: DEFAULT_FONT.into(), map_font: "KaiTi".into() }
    }
}

impl Settings {
    /// The font families chosen that `installed` does not have; text that
    /// asks for one is drawn with [`DEFAULT_FONT`] instead, and the player
    /// is told once (ADR 0012).
    pub fn missing_fonts(&self, installed: impl Fn(&str) -> bool) -> Vec<&str> {
        let mut missing: Vec<&str> = [self.body_font.as_str(), self.map_font.as_str()].into_iter().filter(|f| !installed(f)).collect();
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
                "body_font" if !value.is_empty() => settings.body_font = value.to_string(),
                "map_font" if !value.is_empty() => settings.map_font = value.to_string(),
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
        format!("text_appearance=\"{appearance}\"\nbody_font=\"{}\"\nmap_font=\"{}\"\n", self.body_font, self.map_font)
    }
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
        let chosen = Settings { body_font: "Gone".into(), map_font: "Gone".into(), ..Settings::default() };
        assert_eq!(chosen.missing_fonts(|f| f != "Gone"), ["Gone"]);
        assert!(Settings::default().missing_fonts(|_| true).is_empty());
    }

    #[test]
    fn round_trips_and_keeps_defaults_for_the_rest() {
        let custom = Settings { text_appearance: TextAppearance::System, body_font: "SimSun".into(), map_font: "KaiTi".into() };
        assert_eq!(Settings::parse(&custom.render()), custom);
        let partial = Settings::parse("# comment\nbody_font=\"DengXian\"\ntext_appearance=\"neither\"\nunknown=1\n");
        assert_eq!(partial, Settings { body_font: "DengXian".into(), ..Settings::default() });
        assert_eq!(Settings::parse(""), Settings::default());
    }
}
