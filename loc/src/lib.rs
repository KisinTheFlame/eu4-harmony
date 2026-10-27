//! Localisation files of EU4 Harmony mods and of the framework itself.
//!
//! A mod keeps them in `eu4-harmony/localisation/`, one file per language,
//! named and laid out like the game's own (`<name>_l_<language>.yml`):
//!
//! ```text
//! l_simp_chinese:
//!  RC_APPLY:0 "应用"
//!  RC_RADIUS:0 "半径 $RADIUS$"
//! ```
//!
//! Unlike the game's files they are plain UTF-8: the framework encodes the
//! text for the game. `\n` in a text is a line break and `$NAME$` a value the
//! code fills in. A key missing in one language falls back to English.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The languages texts can be written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Language {
    English,
    French,
    German,
    Spanish,
    /// Shown only with the CJK font plugin.
    SimpChinese,
}

impl Language {
    pub const ALL: [Language; 5] = [Self::English, Self::French, Self::German, Self::Spanish, Self::SimpChinese];

    /// The Paradox name, as in `l_english`.
    pub fn id(self) -> &'static str {
        match self {
            Self::English => "english",
            Self::French => "french",
            Self::German => "german",
            Self::Spanish => "spanish",
            Self::SimpChinese => "simp_chinese",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.id() == id)
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
            let name = line.strip_prefix("l_").and_then(|l| l.strip_suffix(':')).ok_or_else(|| error(format!("expected a language header such as l_english:, found {line:?}")))?;
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

/// Replaces each `$NAME$` in `text` with its value.
pub fn fill(text: &str, args: &[(&str, &str)]) -> String {
    args.iter().fold(text.to_string(), |text, (name, value)| text.replace(&format!("${name}$"), value))
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
            problems.push(format!("{name}: the name does not end in _l_<language>.yml with one of {}", supported()));
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
    }

    #[test]
    fn file_names_declare_the_language() {
        assert_eq!(file_language("rc_l_simp_chinese.yml"), Some(Language::SimpChinese));
        assert_eq!(file_language("rc_l_english.yml"), Some(Language::English));
        assert_eq!(file_language("rc_english.yml"), None);
        assert_eq!(file_language("rc_l_english.txt"), None);
    }
}
