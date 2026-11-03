//! `eu4-harmony skills`: guides for a mod author's coding agent, written by
//! hand in `cli/skills/` and embedded in the tool. Topics nest as folders
//! do: `skills ui` is `ui.md`, `skills ui buttons` is `ui/buttons.md`. They
//! point into `eu4-harmony wiki` instead of repeating it.

/// Every guide, by its topic.
const GUIDES: &[(&str, &str)] = &[
    ("", include_str!("../skills/index.md")),
    ("mod", include_str!("../skills/mod.md")),
    ("tasks", include_str!("../skills/tasks.md")),
    ("state", include_str!("../skills/state.md")),
    ("ui", include_str!("../skills/ui.md")),
    ("ui buttons", include_str!("../skills/ui/buttons.md")),
    ("localisation", include_str!("../skills/localisation.md")),
    ("testing", include_str!("../skills/testing.md")),
    ("game", include_str!("../skills/game.md")),
];

/// The guide for `topics`, or why there is none.
pub fn page(topics: &[String]) -> Result<&'static str, String> {
    let topic = topics.iter().flat_map(|t| t.split(['.', '/'])).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    GUIDES.iter().find(|(t, _)| *t == topic).map(|(_, text)| *text).ok_or_else(|| format!("no guide {topic}; eu4-harmony skills lists them"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    /// Every guide in `cli/skills/`, by its topic.
    fn files(dir: &Path, topic: &str, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let nested = if topic.is_empty() { name.clone() } else { format!("{topic} {name}") };
            if path.is_dir() {
                files(&path, &nested, found);
            } else if name == "index" && topic.is_empty() {
                found.push(String::new());
            } else {
                found.push(nested);
            }
        }
    }

    #[test]
    fn every_guide_is_embedded_and_listed() {
        let mut found = Vec::new();
        files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("skills"), "", &mut found);
        found.sort();
        let mut embedded: Vec<String> = GUIDES.iter().map(|(t, _)| t.to_string()).collect();
        embedded.sort();
        assert_eq!(found, embedded);
        let index = page(&[]).unwrap();
        for (topic, text) in GUIDES.iter().filter(|(t, _)| !t.is_empty()) {
            assert!(index.contains(&format!("\n  {topic} ")), "the index does not list {topic}");
            assert!(text.starts_with(&format!("{topic}: ")), "{topic} does not begin with its topic");
        }
    }

    /// Every `eu4-harmony skills ...` and `eu4-harmony wiki ...` a guide
    /// names is a page there is.
    #[test]
    fn every_page_a_guide_points_to_is_there() {
        for (topic, text) in GUIDES {
            let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
            for (command, rest) in flat.split("eu4-harmony ").skip(1).filter_map(|s| s.split_once(' ')) {
                // The words that may name a page; prose may follow them.
                let named: Vec<String> =
                    rest.split_whitespace().take_while(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '_')).take(2).map(String::from).collect();
                let found = |words: &[String]| match command {
                    "skills" => page(words).is_ok(),
                    _ => crate::wiki::page(words).is_ok(),
                };
                if !["skills", "wiki"].contains(&command) || named.is_empty() {
                    continue;
                }
                // The first word names a page; a second only where the two do.
                assert!(found(&named[..1]), "{topic:?} points to eu4-harmony {command} {}", named[0]);
            }
        }
    }

    #[test]
    fn topics_nest_and_unknown_ones_are_refused() {
        assert!(page(&words("ui buttons")).unwrap().starts_with("ui buttons:"));
        assert_eq!(page(&words("ui/buttons")), page(&words("ui buttons")));
        assert!(page(&words("ui knobs")).is_err());
    }
}
