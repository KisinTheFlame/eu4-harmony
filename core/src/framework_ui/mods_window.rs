//! The mods window (GLOSSARY): opened from the main menu's corner, it lists
//! the Harmony mods of this start in playset order and shows the one chosen:
//! its name and version, its description in the player language, and the
//! sensitive capabilities its manifest declares
//! (ADR sensitive-capabilities-are-declared), each in words a player
//! understands. It shows; it decides nothing.
use super::widgets::{Side, Widget, Window};
use crate::mods::Permissions;
use std::sync::OnceLock;

/// The window's width, and its list's.
const WIDTH: f32 = 640.0;
const LIST: f32 = 200.0;

/// What the window shows of one mod.
pub struct Card {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    /// Its description: a key in its own localisation.
    pub description: Option<String>,
    pub permissions: Permissions,
    /// Where its data folder is, for `files`.
    pub data_folder: String,
}

static CARDS: OnceLock<Vec<Card>> = OnceLock::new();

/// Keeps the Harmony mods of this start for the window; once, at startup.
pub fn set(cards: Vec<Card>) {
    let _ = CARDS.set(cards);
}

pub fn cards() -> &'static [Card] {
    CARDS.get().map_or(&[], Vec::as_slice)
}

/// The framework's words for a key, with its values filled in.
pub type Words<'a> = &'a dyn Fn(&str, &[(&str, &str)]) -> String;
/// A mod's own text for a key, in the player language.
pub type ModText<'a> = &'a dyn Fn(&str, &str) -> Option<String>;

/// The window for `cards`, the first chosen.
pub fn window(cards: &[Card], words: Words, mod_text: ModText) -> Window {
    let title = words("EU4HARMONY_MODS_TITLE", &[]);
    let Some(first) = cards.first() else {
        return Window::new(title, vec![Widget::Label(words("EU4HARMONY_MODS_NONE", &[]))], WIDTH - LIST);
    };
    let side = Side::new("mods", cards.iter().map(|c| c.name.clone()).collect(), LIST);
    Window::new(title, details(first, words, mod_text), WIDTH).with_side(side)
}

/// What the window says of `card`.
pub fn details(card: &Card, words: Words, mod_text: ModText) -> Vec<Widget> {
    let mut body = vec![Widget::Heading(card.name.clone())];
    if let Some(version) = &card.version {
        body.push(Widget::Label(words("EU4HARMONY_MODS_VERSION", &[("VERSION", version)])));
    }
    let description = card.description.as_deref().and_then(|key| mod_text(&card.id, key));
    body.push(Widget::Gap);
    body.push(Widget::Label(description.unwrap_or_else(|| words("EU4HARMONY_MODS_NO_DESCRIPTION", &[]))));
    body.push(Widget::Gap);
    let p = &card.permissions;
    let mut can = Vec::new();
    if p.clock {
        can.push(words("EU4HARMONY_MODS_CLOCK", &[]));
    }
    if p.files {
        can.push(words("EU4HARMONY_MODS_FILES", &[("FOLDER", &card.data_folder)]));
    }
    if !p.network.is_empty() {
        can.push(words("EU4HARMONY_MODS_NETWORK", &[("HOSTS", &p.network.join(", "))]));
    }
    if can.is_empty() {
        body.push(Widget::Label(words("EU4HARMONY_MODS_NOTHING_BEYOND", &[])));
    } else {
        body.push(Widget::Label(words("EU4HARMONY_MODS_BEYOND", &[])));
        body.extend(can.into_iter().map(Widget::Item));
        body.push(Widget::Gap);
        body.push(Widget::Label(words("EU4HARMONY_MODS_DECLARED", &[])));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key and its values, as the words.
    fn words(key: &str, args: &[(&str, &str)]) -> String {
        args.iter().fold(key.to_string(), |text, (name, value)| format!("{text} {name}={value}"))
    }

    /// The body as text: a heading marked `#`, an item `-`, a gap `|`.
    fn labels(body: &[Widget]) -> Vec<String> {
        body.iter()
            .map(|w| match w {
                Widget::Label(text) => text.clone(),
                Widget::Heading(text) => format!("# {text}"),
                Widget::Item(text) => format!("- {text}"),
                Widget::Gap => "|".into(),
                _ => "?".into(),
            })
            .collect()
    }

    fn card(id: &str, permissions: Permissions) -> Card {
        Card {
            id: id.into(),
            name: format!("Mod {id}"),
            version: Some("1.2".into()),
            description: Some("M_DESCRIPTION".into()),
            permissions,
            data_folder: format!(r"C:\u\eu4-harmony\data\{id}"),
        }
    }

    #[test]
    fn a_mod_shows_its_name_version_description_and_what_it_declares() {
        let mod_text = |id: &str, key: &str| (id == "a" && key == "M_DESCRIPTION").then(|| "Fights for you.".to_string());
        let permissions = Permissions { clock: true, files: true, network: vec!["api.example.com".into(), "192.168.1.10:8080".into()] };
        assert_eq!(
            labels(&details(&card("a", permissions), &words, &mod_text)),
            [
                "# Mod a",
                "EU4HARMONY_MODS_VERSION VERSION=1.2",
                "|",
                "Fights for you.",
                "|",
                "EU4HARMONY_MODS_BEYOND",
                "- EU4HARMONY_MODS_CLOCK",
                r"- EU4HARMONY_MODS_FILES FOLDER=C:\u\eu4-harmony\data\a",
                "- EU4HARMONY_MODS_NETWORK HOSTS=api.example.com, 192.168.1.10:8080",
                "|",
                "EU4HARMONY_MODS_DECLARED",
            ]
        );
        // No description in its localisation, no version, nothing declared.
        let mut bare = card("b", Permissions::default());
        bare.version = None;
        assert_eq!(labels(&details(&bare, &words, &mod_text)), ["# Mod b", "|", "EU4HARMONY_MODS_NO_DESCRIPTION", "|", "EU4HARMONY_MODS_NOTHING_BEYOND"]);
    }

    #[test]
    fn the_window_lists_the_mods_in_order_or_says_there_are_none() {
        let none = |_: &str, _: &str| None;
        let w = window(&[card("a", Permissions::default()), card("b", Permissions::default())], &words, &none);
        let side = w.side.as_ref().unwrap();
        assert_eq!((side.items.clone(), side.selected), (vec!["Mod a".to_string(), "Mod b".to_string()], 0));
        assert_eq!(labels(&w.body)[0], "# Mod a");
        let empty = window(&[], &words, &none);
        assert!(empty.side.is_none());
        assert_eq!(labels(&empty.body), ["EU4HARMONY_MODS_NONE"]);
    }
}
