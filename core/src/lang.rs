//! The player language: the one language the framework and every mod show
//! their texts in this session. Everything else asks [`player`]; how it is
//! chosen is known only here.
//!
//! The player chooses it in the framework settings. The first time, when
//! the settings name none, it is the language Windows displays, if it is one
//! of [`Language::ALL`], else English (ADR player-language-in-the-settings).
//! The game's own language, from its settings, plays no part: players of a
//! language the game lacks run it in English, with a language mod.
//!
//! Decided once, at startup: the game reads its localisation files once, so
//! the mod texts spliced into them (`loc`) could not follow a later change.
use eu4_harmony_loc::Language;
use std::path::Path;
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

/// The language texts are shown in; English until it is decided.
pub fn player() -> Language {
    DECISION.get().map_or(Language::English, |d| d.player)
}

/// The language whose localisation files the game loads.
pub fn game() -> Language {
    DECISION.get().map_or(Language::English, |d| d.game)
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
}
