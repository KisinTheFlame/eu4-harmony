//! The player language: the language the session started in for the game side
//! (mod localisation spliced into the game, wording templates, game font
//! defaults/fallbacks, and extension queries), plus the framework UI language
//! ([`ui`]), which starts equal to [`player`] and updates immediately when the
//! player saves a new language from the main menu so every framework window
//! and notice switches without restarting.
//!
//! The player chooses the language in the framework settings. The first time,
//! when the settings name none, it is the language Windows displays, if it is
//! one of [`Language::ALL`], else English (ADR player-language-in-the-settings).
//! The game's own language, from its settings, plays no part: players of a
//! language the game lacks run it in English, with a language mod.
use eu4_harmony_loc::Language;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

pub struct Decision {
    /// What the player reads.
    pub player: Language,
    /// The game's own language: the localisation files it loads.
    pub game: Language,
    /// How `player` was chosen, for the log and the report.
    pub reason: String,
}

static DECISION: OnceLock<Decision> = OnceLock::new();
/// Index into [`Language::ALL`] when the framework UI language was updated
/// after startup; `u8::MAX` until then (falls back to [`player`]).
static UI_LANGUAGE: AtomicU8 = AtomicU8::new(u8::MAX);

/// The languages the game itself runs in.
const GAME: [Language; 4] = [Language::English, Language::French, Language::German, Language::Spanish];

/// The game's language from `settings.txt` (`language="l_english"`); English
/// when it names none or one the game does not run in.
fn game_language(settings: &str) -> Language {
    eu4_harmony_manifest::line_entries(settings)
        .find(|&(key, _)| key == "language")
        .and_then(|(_, v)| v.strip_prefix("l_"))
        .and_then(Language::from_id)
        .filter(|l| GAME.contains(l))
        .unwrap_or(Language::English)
}

/// The language Windows displays, when the list has it; else English.
pub fn from_windows() -> Language {
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    Language::from_windows(id).unwrap_or(Language::English)
}

/// Decides the language for this session: `chosen` in the framework
/// settings, else Windows'. Later calls return the first decision.
pub(crate) fn decide(user_dir: &Path, chosen: Option<Language>) -> &'static Decision {
    DECISION.get_or_init(|| {
        let settings = std::fs::read_to_string(user_dir.join("settings.txt")).unwrap_or_default();
        let game = game_language(&settings);
        let (player, why) = match chosen {
            Some(chosen) => (chosen, "chosen in the framework settings"),
            None => (from_windows(), "the Windows display language"),
        };
        log!("language: {} ({why}); the game's is {}", player.id(), game.id());
        Decision { player, game, reason: why.to_string() }
    })
}

/// The language texts are shown in for this session; English until decided.
pub fn player() -> Language {
    DECISION.get().map_or(Language::English, |d| d.player)
}

/// The language the framework's own UI and notices show in: follows the latest
/// language saved from the main menu, or [`player`] until changed.
pub fn ui() -> Language {
    Language::ALL.get(UI_LANGUAGE.load(Ordering::Relaxed) as usize).copied().unwrap_or_else(player)
}

/// Updates the framework UI language for the remainder of this session without
/// changing the startup session language returned by [`player`].
pub fn set_ui(language: Language) {
    if let Some(index) = Language::ALL.iter().position(|&l| l == language) {
        UI_LANGUAGE.store(index as u8, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_language_comes_from_the_settings() {
        assert_eq!(game_language("force_pow2_textures=no\nlanguage=\"l_german\"\n"), Language::German);
        assert_eq!(game_language("language=\"l_simp_chinese\"\n"), Language::English);
        assert_eq!(game_language("language=\"l_russian\"\n"), Language::English);
        assert_eq!(game_language(""), Language::English);
    }

    #[test]
    fn ui_language_updates_independently_of_session_language() {
        let before = UI_LANGUAGE.load(Ordering::Relaxed);
        let session = player();
        set_ui(Language::Japanese);
        assert_eq!(ui(), Language::Japanese);
        assert_eq!(player(), session);
        UI_LANGUAGE.store(before, Ordering::Relaxed);
    }
}
