//! Localisation in the core, in the player language (`lang`): the
//! framework's own texts, the texts each mod ships in
//! `eu4-harmony/localisation/` (format in `eu4_harmony_loc`), and the splice
//! that adds the mods' keys to the game's own localisation, so that their
//! `.gui` fragments can name them like any vanilla key.
use crate::{
    lang,
    mods::DiscoveredMod,
    splice::{Addition, Plan},
};
use eu4_harmony_loc::{self as format, Catalog, Language};
use std::collections::BTreeMap;
use std::sync::{LazyLock, OnceLock};

static FRAMEWORK: LazyLock<Catalog> = LazyLock::new(|| {
    let mut catalog = Catalog::default();
    for source in [
        include_str!("../localisation/eu4harmony_l_english.yml"),
        include_str!("../localisation/eu4harmony_l_french.yml"),
        include_str!("../localisation/eu4harmony_l_german.yml"),
        include_str!("../localisation/eu4harmony_l_spanish.yml"),
        include_str!("../localisation/eu4harmony_l_simp_chinese.yml"),
    ] {
        catalog.add(format::parse(source).expect("the framework's localisation parses"));
    }
    catalog
});

/// The catalogs of the mods taking part, by mod id.
static MODS: OnceLock<BTreeMap<String, Catalog>> = OnceLock::new();

/// A framework text in `language`, with its `$NAME$` values filled in.
pub fn framework(language: Language, key: &str, args: &[(&str, &str)]) -> String {
    format::fill(&FRAMEWORK.text(language, key).unwrap_or_else(|| key.to_string()), args)
}

/// A mod's text in the player language; `None` when the mod has no such key.
pub fn mod_text(id: &str, key: &str) -> Option<String> {
    MODS.get()?.get(id)?.text(lang::player(), key)
}

/// The vanilla file the mods' keys are appended to. The game loads it in
/// every language it runs in.
fn target(game: Language) -> String {
    format!("localisation/text_l_{}.yml", game.id())
}

/// Loads the localisation of `mods` (those taking part, in playset order) and
/// plans the splice of their keys. Returns problems for the log and the report.
pub fn install(mods: &[&DiscoveredMod], plan: &mut Plan) -> Vec<String> {
    let player = lang::player();
    let mut notes = Vec::new();
    let mut catalogs = BTreeMap::new();
    let mut owners: BTreeMap<String, String> = BTreeMap::new();
    let mut lines = String::new();
    for m in mods {
        let id = &m.manifest.id;
        let (catalog, problems) = format::load_dir(&m.dir.join("eu4-harmony").join("localisation"));
        notes.extend(problems.into_iter().map(|p| format!("{id}: localisation/{p}")));
        for key in catalog.keys() {
            if let Some(owner) = owners.get(key) {
                notes.push(format!("{id}: localisation key {key} is already defined by {owner}; the game shows that text"));
                continue;
            }
            let Some(raw) = catalog.raw(player, key) else { continue };
            owners.insert(key.to_string(), id.clone());
            // As written: the Unicode module keeps the game's text in UTF-8.
            lines.push_str(&format!(" {key}:0 \"{raw}\"\n"));
        }
        catalogs.insert(id.clone(), catalog);
    }
    if !lines.is_empty() {
        plan.add(&target(lang::game()), Addition::Append(format!("# EU4 Harmony: mod texts in {}\n{lines}", player.id())));
    }
    log!("localisation: {} key(s) of {} mod(s) in {}", owners.len(), catalogs.len(), player.id());
    let _ = MODS.set(catalogs);
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn framework_texts_exist_in_every_language() {
        // The `$NAME$` values a text fills in, and its line breaks.
        let shape = |text: &str| (text.split('$').skip(1).step_by(2).map(str::to_string).collect::<BTreeSet<_>>(), text.matches("\\n").count());
        for language in Language::ALL {
            assert_eq!(FRAMEWORK.keys_in(language), FRAMEWORK.keys_in(Language::English), "{language:?}");
            for key in FRAMEWORK.keys_in(language) {
                let (text, english) = (FRAMEWORK.raw(language, key).unwrap(), FRAMEWORK.raw(Language::English, key).unwrap());
                assert_eq!(shape(text), shape(english), "{language:?} {key}");
            }
            let text = framework(language, "EU4HARMONY_NOTICE_TITLE_MORE", &[("COUNT", "2")]);
            assert!(text.contains('2') && !text.contains("$COUNT$"), "{text}");
        }
        assert!(framework(Language::English, "EU4HARMONY_NOT_LOADED", &[("MOD", "X"), ("DETAIL", "d")]).contains("\"X\" was not loaded:\nd"));
    }

    #[test]
    fn mod_keys_are_appended_to_the_game_language_file() {
        let dir = std::env::temp_dir().join(format!("eu4h-loc-{}", std::process::id()));
        let loc = dir.join("eu4-harmony").join("localisation");
        std::fs::create_dir_all(&loc).unwrap();
        std::fs::write(loc.join("t_l_english.yml"), "l_english:\n T_APPLY:0 \"Apply\"\n T_ONLY:0 \"English\"\n").unwrap();
        std::fs::write(loc.join("t_l_german.yml"), "l_german:\n T_APPLY:0 \"Übernehmen\"\n").unwrap();
        let m = crate::mods::DiscoveredMod {
            descriptor_file: dir.join("t.mod"),
            descriptor: crate::mods::Descriptor { name: "T".into(), path: Some(dir.clone()), archive: None },
            dir: dir.clone(),
            manifest: serde_json::from_str(r#"{ "id": "t", "api_version": "0.1" }"#).unwrap(),
        };
        let mut plan = Plan::default();
        let notes = install(&[&m], &mut plan);
        assert!(notes.is_empty(), "{notes:?}");
        // Before a language is decided, the player and the game use English.
        let additions = plan.for_file("localisation/text_l_english.yml").unwrap();
        let Addition::Append(text) = &additions[0] else { panic!() };
        assert!(text.contains(" T_APPLY:0 \"Apply\"\n") && text.contains(" T_ONLY:0 \"English\"\n"), "{text}");
        assert_eq!(mod_text("t", "T_APPLY").as_deref(), Some("Apply"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
