//! The player language: the one language the framework and every mod show
//! their texts in this session. Everything else asks [`player`]; how it is
//! chosen is known only here, so letting the player pick it (an in-game
//! setting, say) changes this module and nothing that uses it.
//!
//! The game itself runs in the language of its settings: English, French,
//! German or Spanish. Chinese players run it in English, with a Chinese
//! localisation mod; the Unicode module draws the characters for anyone, so
//! the Windows display language tells who reads Chinese (ADR 0010). Japanese
//! and Korean are not offered.
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

/// What the choice is made from.
#[derive(Clone, Copy, Debug)]
struct Signals {
    game: Language,
    windows_chinese: bool,
}

fn choose(s: Signals) -> (Language, &'static str) {
    if s.windows_chinese {
        (Language::SimpChinese, "Windows displays Chinese")
    } else {
        (s.game, "the game's language")
    }
}

/// The game's language from `settings.txt` (`language="l_english"`); English
/// when it names none or one the game does not run in.
fn game_language(settings: &str) -> Language {
    settings
        .lines()
        .find_map(|l| l.trim().strip_prefix("language="))
        .and_then(|v| v.trim().trim_matches('"').strip_prefix("l_"))
        .and_then(Language::from_id)
        .filter(|l| *l != Language::SimpChinese)
        .unwrap_or(Language::English)
}

fn windows_chinese() -> bool {
    const LANG_CHINESE: u16 = 0x04;
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    id & 0x3ff == LANG_CHINESE
}

/// Decides the language for this session; later calls return the first decision.
pub(crate) fn decide(user_dir: &Path) -> &'static Decision {
    DECISION.get_or_init(|| {
        let settings = std::fs::read_to_string(user_dir.join("settings.txt")).unwrap_or_default();
        let signals = Signals { game: game_language(&settings), windows_chinese: windows_chinese() };
        let (player, why) = choose(signals);
        log!("language: {} ({why}; {signals:?})", player.id());
        Decision { player, game: signals.game, reason: why.to_string() }
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
    fn chinese_follows_the_windows_display_language() {
        assert_eq!(choose(Signals { game: Language::English, windows_chinese: true }).0, Language::SimpChinese);
        assert_eq!(choose(Signals { game: Language::French, windows_chinese: true }).0, Language::SimpChinese);
        assert_eq!(choose(Signals { game: Language::French, windows_chinese: false }).0, Language::French);
    }

    #[test]
    fn game_language_comes_from_the_settings() {
        assert_eq!(game_language("force_pow2_textures=no\nlanguage=\"l_german\"\n"), Language::German);
        assert_eq!(game_language("language=\"l_simp_chinese\"\n"), Language::English);
        assert_eq!(game_language(""), Language::English);
    }
}
