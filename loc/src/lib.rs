//! Localisation files of EU4 Harmony mods and of the framework itself.
//!
//! A mod keeps them in `eu4-harmony/localisation/`, one file per language,
//! named and laid out like the game's own (`<name>_l_<language>.yml`):
//!
//! ```text
//! l_french:
//!  RC_APPLY:0 "Appliquer"
//!  RC_RADIUS:0 "Rayon de $RADIUS$ provinces"
//! ```
//!
//! Unlike the game's files they are plain UTF-8: the framework encodes the
//! text for the game. `\n` in a text is a line break and `$NAME$` a value the
//! code fills in. A key missing in one language falls back to English.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The languages texts can be written in: the game's own four, and those
/// players read it in through a language mod. Their names are Paradox's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Language {
    English,
    French,
    German,
    Spanish,
    SimpChinese,
    TradChinese,
    Russian,
    Polish,
    BrazPor,
    Japanese,
    Korean,
    Turkish,
}

impl Language {
    pub const ALL: [Language; 12] = [
        Self::English,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::SimpChinese,
        Self::TradChinese,
        Self::Russian,
        Self::Polish,
        Self::BrazPor,
        Self::Japanese,
        Self::Korean,
        Self::Turkish,
    ];

    /// The Paradox name, as in `l_english`.
    pub fn id(self) -> &'static str {
        match self {
            Self::English => "english",
            Self::French => "french",
            Self::German => "german",
            Self::Spanish => "spanish",
            Self::SimpChinese => "simp_chinese",
            Self::TradChinese => "trad_chinese",
            Self::Russian => "russian",
            Self::Polish => "polish",
            Self::BrazPor => "braz_por",
            Self::Japanese => "japanese",
            Self::Korean => "korean",
            Self::Turkish => "turkish",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.id() == id)
    }

    /// What the language calls itself, as a list to choose it from shows it.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::SimpChinese => "简体中文",
            Self::TradChinese => "繁體中文",
            Self::Russian => "Русский",
            Self::Polish => "Polski",
            Self::BrazPor => "Português do Brasil",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
            Self::Turkish => "Türkçe",
        }
    }

    /// Its locale name, as font names are looked up by.
    pub fn locale(self) -> &'static str {
        match self {
            Self::English => "en-us",
            Self::French => "fr-fr",
            Self::German => "de-de",
            Self::Spanish => "es-es",
            Self::SimpChinese => "zh-cn",
            Self::TradChinese => "zh-tw",
            Self::Russian => "ru-ru",
            Self::Polish => "pl-pl",
            Self::BrazPor => "pt-br",
            Self::Japanese => "ja-jp",
            Self::Korean => "ko-kr",
            Self::Turkish => "tr-tr",
        }
    }

    /// The language a Windows language identifier (`LANGID`: the primary
    /// language in the low 10 bits, the sublanguage above them) stands for;
    /// None for one not in the list. Chinese is Traditional unless it is the
    /// Simplified of the mainland or Singapore; every Portuguese is Brazil's.
    pub fn from_windows(langid: u16) -> Option<Self> {
        let (primary, sub) = (langid & 0x3ff, langid >> 10);
        Some(match primary {
            0x09 => Self::English,
            0x0c => Self::French,
            0x07 => Self::German,
            0x0a => Self::Spanish,
            0x04 if langid == 0x0004 || matches!(sub, 0x02 | 0x04) => Self::SimpChinese,
            0x04 => Self::TradChinese,
            0x19 => Self::Russian,
            0x15 => Self::Polish,
            0x16 => Self::BrazPor,
            0x11 => Self::Japanese,
            0x12 => Self::Korean,
            0x1f => Self::Turkish,
            _ => return None,
        })
    }
}

/// One parsed localisation file.
#[derive(Debug)]
pub struct File {
    pub language: Language,
    /// Keys and texts as written, in file order.
    pub entries: Vec<(String, String)>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    /// 1-based.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Parses one file. As in the game, a text runs from the first quote after
/// the key to the last quote on the line.
pub fn parse(source: &str) -> Result<File, Error> {
    let mut language = None;
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, raw) in source.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = raw.trim();
        let error = |message: String| Error { line: index + 1, message };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if language.is_none() {
            let name = line
                .strip_prefix("l_")
                .and_then(|l| l.strip_suffix(':'))
                .ok_or_else(|| error(format!("expected a language header such as l_english:, found {line:?}")))?;
            language = Some(Language::from_id(name).ok_or_else(|| error(format!("unknown language l_{name}")))?);
            continue;
        }
        let (key, rest) = line.split_once(':').ok_or_else(|| error(format!("expected KEY:0 \"text\", found {line:?}")))?;
        let key = key.trim();
        if !valid_key(key) {
            return Err(error(format!("invalid key {key:?}")));
        }
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        let (Some(open), Some(close)) = (rest.find('"'), rest.rfind('"')) else {
            return Err(error(format!("the text of {key} is not quoted")));
        };
        if open == close || !rest[..open].trim().is_empty() {
            return Err(error(format!("the text of {key} is not quoted")));
        }
        let after = rest[close + 1..].trim();
        if !after.is_empty() && !after.starts_with('#') {
            return Err(error(format!("unexpected {after:?} after the text of {key}")));
        }
        if !seen.insert(key.to_string()) {
            return Err(error(format!("{key} is defined twice")));
        }
        entries.push((key.to_string(), rest[open + 1..close].to_string()));
    }
    let language = language.ok_or(Error { line: 1, message: "empty file".into() })?;
    Ok(File { language, entries })
}

/// The language a file name declares (`x_l_english.yml`), if it is one.
pub fn file_language(name: &str) -> Option<Language> {
    let stem = name.strip_suffix(".yml")?;
    let (_, id) = stem.rsplit_once("_l_")?;
    Language::from_id(id)
}

/// A text as shown: `\n` becomes a line break.
pub fn unescape(raw: &str) -> String {
    raw.replace("\\n", "\n")
}

/// Replaces each `$NAME$` in `text` with its value, in one pass: a value
/// that holds a `$NAME$` of its own, a mod's name or an error message, stays
/// as it is. A `$` that starts no name given stays too.
pub fn fill(text: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('$') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let named = after.find('$').and_then(|end| args.iter().find(|(name, _)| *name == &after[..end]).map(|(_, value)| (value, end)));
        match named {
            Some((value, end)) => {
                out.push_str(value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('$');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Texts by language and key.
#[derive(Debug, Default)]
pub struct Catalog {
    texts: BTreeMap<Language, BTreeMap<String, String>>,
}

impl Catalog {
    /// Adds a file's entries; returns the keys its language already had,
    /// which keep their earlier text.
    pub fn add(&mut self, file: File) -> Vec<String> {
        let texts = self.texts.entry(file.language).or_default();
        let mut duplicates = Vec::new();
        for (key, text) in file.entries {
            match texts.entry(key) {
                std::collections::btree_map::Entry::Occupied(e) => duplicates.push(e.key().clone()),
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(text);
                }
            }
        }
        duplicates
    }

    /// The text of `key` as written, in `language` or else in English.
    pub fn raw(&self, language: Language, key: &str) -> Option<&str> {
        [language, Language::English].iter().find_map(|l| self.texts.get(l)?.get(key)).map(String::as_str)
    }

    /// The text of `key` as written in `language` itself, never English's.
    pub fn raw_in(&self, language: Language, key: &str) -> Option<&str> {
        self.texts.get(&language)?.get(key).map(String::as_str)
    }

    /// The text of `key` as shown, in `language` or else in English.
    pub fn text(&self, language: Language, key: &str) -> Option<String> {
        self.raw(language, key).map(unescape)
    }

    /// Every key, in any language.
    pub fn keys(&self) -> BTreeSet<&str> {
        self.texts.values().flat_map(|t| t.keys().map(String::as_str)).collect()
    }

    /// The keys `language` itself has, without falling back.
    pub fn keys_in(&self, language: Language) -> BTreeSet<&str> {
        self.texts.get(&language).map(|t| t.keys().map(String::as_str).collect()).unwrap_or_default()
    }
}

/// Loads every `*_l_<language>.yml` in `dir` (none when it does not exist).
/// Files that cannot be used are left out and described in the second value.
pub fn load_dir(dir: &Path) -> (Catalog, Vec<String>) {
    let mut catalog = Catalog::default();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return (catalog, problems) };
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "yml")).collect();
    files.sort();
    for path in files {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let Some(declared) = file_language(&name) else {
            problems.push(format!("{name}: left out, as the name does not end in _l_<language>.yml with one of {}", supported()));
            continue;
        };
        let file = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| parse(&t).map_err(|e| e.to_string())) {
            Ok(f) => f,
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                continue;
            }
        };
        if file.language != declared {
            problems.push(format!("{name}: the header says l_{}", file.language.id()));
            continue;
        }
        for key in catalog.add(file) {
            problems.push(format!("{name}: {key} is already defined for {} by an earlier file", declared.id()));
        }
    }
    (catalog, problems)
}

fn supported() -> String {
    Language::ALL.map(Language::id).join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_game_format() {
        let file = parse("\u{feff}l_simp_chinese:\n # comment\n A:0 \"应用\"\n B: \"say \"hi\"\" # note\n\n C:1 \"\"\n").unwrap();
        assert_eq!(file.language, Language::SimpChinese);
        assert_eq!(file.entries, [("A".into(), "应用".into()), ("B".into(), "say \"hi\"".into()), ("C".into(), String::new())]);
    }

    #[test]
    fn reports_the_line_of_a_mistake() {
        assert_eq!(parse("l_english:\n A:0 \"x\"\n B:0 x\n").unwrap_err().line, 3);
        assert!(parse("l_klingon:\n").unwrap_err().message.contains("unknown language"));
        assert!(parse("l_english:\n A:0 \"x\"\n A:0 \"y\"\n").unwrap_err().message.contains("twice"));
        assert!(parse(" A:0 \"x\"\n").is_err());
    }

    #[test]
    fn falls_back_to_english() {
        let mut c = Catalog::default();
        c.add(parse("l_english:\n A:0 \"Apply\"\n B:0 \"Radius $R$\\nmore\"\n").unwrap());
        c.add(parse("l_simp_chinese:\n A:0 \"应用\"\n").unwrap());
        assert_eq!(c.text(Language::SimpChinese, "A").as_deref(), Some("应用"));
        assert_eq!(c.text(Language::SimpChinese, "B").as_deref(), Some("Radius $R$\nmore"));
        assert_eq!(c.raw(Language::French, "B"), Some("Radius $R$\\nmore"));
        assert_eq!(c.text(Language::English, "C"), None);
        assert_eq!(fill("Radius $R$", &[("R", "5")]), "Radius 5");
        // A value is not filled in again, and a lone `$` stays.
        assert_eq!(fill("$MOD$: $DETAIL$", &[("MOD", "$DETAIL$ mod"), ("DETAIL", "failed")]), "$DETAIL$ mod: failed");
        assert_eq!(fill("costs $5, $R$ and $X$$", &[("R", "5")]), "costs $5, 5 and $X$$");
    }

    #[test]
    fn windows_languages_map_to_the_list() {
        let from = Language::from_windows;
        assert_eq!(from(0x0409), Some(Language::English));
        assert_eq!(from(0x0809), Some(Language::English));
        assert_eq!(from(0x0407), Some(Language::German));
        assert_eq!(from(0x0804), Some(Language::SimpChinese));
        assert_eq!(from(0x1004), Some(Language::SimpChinese));
        assert_eq!(from(0x0004), Some(Language::SimpChinese));
        assert_eq!(from(0x0404), Some(Language::TradChinese));
        assert_eq!(from(0x0c04), Some(Language::TradChinese));
        assert_eq!(from(0x7c04), Some(Language::TradChinese));
        assert_eq!(from(0x0816), Some(Language::BrazPor));
        assert_eq!(from(0x0411), Some(Language::Japanese));
        // Swedish is not in the list.
        assert_eq!(from(0x041d), None);
        for language in Language::ALL {
            assert_eq!(Language::from_id(language.id()), Some(language));
        }
    }

    #[test]
    fn file_names_declare_the_language() {
        assert_eq!(file_language("rc_l_simp_chinese.yml"), Some(Language::SimpChinese));
        assert_eq!(file_language("rc_l_english.yml"), Some(Language::English));
        assert_eq!(file_language("rc_l_braz_por.yml"), Some(Language::BrazPor));
        assert_eq!(file_language("rc_l_italian.yml"), None);
        assert_eq!(file_language("rc_english.yml"), None);
        assert_eq!(file_language("rc_l_english.txt"), None);
    }
}
