//! The rest of startup, once the game's file system is up and the user folder
//! known (the first PhysFS call, on the game thread): the framework's
//! folders, the settings and the player language, remote control, the mods
//! discovered, their texts and extensions, and the interface fragments they
//! splice. What it hands the framework is worked out first, as values
//! ([`Found`], [`Setup`]) the startup report's text is made from, and
//! applied after.
use crate::framework_ui::mods_window::Card;
use crate::splice::{Element, Plan};
use crate::{lang, loading, loc, lua, mods, notice, physfs, pipe, report, saves, saving, settings, splice, ui, unicode};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What startup found before the mods, for the report.
struct Found<'a> {
    user_dir: &'a Path,
    /// A test game's name and its pipe.
    test_game: Option<(String, String)>,
    language: &'a lang::Decision,
    /// What the bootstrap installed, or `?` when it did not say.
    installed: &'a str,
}

impl Found<'_> {
    fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!("user dir: {}", self.user_dir.display())];
        if let Some((game, pipe)) = &self.test_game {
            lines.push(format!("test game {game}: remote control on {pipe}"));
        }
        lines.push(format!("language: {} ({}); the game runs in {}", self.language.player.id(), self.language.reason, self.language.game.id()));
        lines.push(format!("startup: {}", self.installed));
        lines
    }
}

/// What startup hands the framework once the mods' extensions are loaded.
struct Setup {
    /// The mods window's cards.
    cards: Vec<Card>,
    /// The extensions loaded, the only ones a save's data is kept for.
    extensions: Vec<String>,
    /// The elements spliced into host windows, each with its extension's
    /// index.
    elements: Vec<(Element, Option<usize>)>,
    /// What the game's file system is to add to the files it reads.
    plan: Plan,
    /// The startup report's part on the mods.
    report: Vec<String>,
}

impl Setup {
    /// From the mods discovered and what came of loading each one's
    /// extension, `indices` the extensions' by the mods' places, `notes`
    /// those of discovering, splicing and the texts, and `data_dir` the
    /// extensions' data folders' folder.
    fn of(
        mods: &[mods::DiscoveredMod],
        outcomes: &[lua::LoadOutcome],
        extensions: Vec<String>,
        indices: &BTreeMap<usize, usize>,
        notes: &[String],
        plan: Plan,
        data_dir: &Path,
    ) -> Self {
        let cards = mods
            .iter()
            .map(|m| Card {
                id: m.manifest.id.clone(),
                name: if m.descriptor.name.is_empty() { m.manifest.id.clone() } else { m.descriptor.name.clone() },
                version: m.descriptor.version.clone(),
                description: m.manifest.description.clone(),
                permissions: m.manifest.permissions.clone(),
                data_folder: data_dir.join(&m.manifest.id).display().to_string(),
            })
            .collect();
        let mut report = vec![format!("\nMods with an EU4 Harmony manifest, in playset order: {}", mods.len())];
        for (m, outcome) in mods.iter().zip(outcomes) {
            report.push(format!(
                "  {} (id {}, api {}): {}; {} UI fragment(s)",
                m.descriptor.name,
                m.manifest.id,
                m.manifest.api_version,
                outcome.status,
                m.layout.ui.len()
            ));
            report.push(format!("    {}", m.dir.display()));
            for file in &m.layout.misplaced {
                report.push(format!("    eu4-harmony/{file}: the layout has no place for it, so it is not used"));
            }
        }
        report.extend(notes.iter().map(|note| format!("  {note}")));
        report.push(format!("UI: {} mod element(s) spliced, {} button-area slot(s)", plan.elements.len(), plan.slots));
        let elements = plan.elements.iter().map(|e| (e.clone(), indices.get(&e.owner).copied())).collect();
        Self { cards, extensions, elements, plan, report }
    }

    fn apply(self) {
        crate::framework_ui::mods_window::set(self.cards);
        saves::keep_for(self.extensions);
        for line in self.report {
            report::line(line);
        }
        report::write();
        ui::set_elements(self.elements);
        physfs::set_plan(self.plan);
    }
}

/// Runs once, from the first PhysFS call: `game_dir` the game folder,
/// `installed` what the bootstrap installed.
pub(crate) fn run(game_dir: PathBuf, installed: &str) {
    let user_dir = match crate::paths::user_dir(&game_dir) {
        Ok(d) => d,
        Err(e) => {
            log!("user dir: {e}");
            crate::open_fallback_log();
            return;
        }
    };
    let logs = crate::open_logs(&user_dir);
    crate::report_header(Some(&game_dir));
    log!("user dir {}", user_dir.display());
    let chosen = settings::load(&user_dir);
    let language = lang::decide(&user_dir, chosen.language);
    log!("settings: {chosen:?}");
    saving::background::set_folder(&user_dir);
    loading::set_folders(&game_dir, &user_dir);
    saving::parallel::start();
    tell_missing_fonts(&chosen);
    let found = Found { user_dir: &user_dir, test_game: pipe::test_game().map(|game| (game.to_string(), pipe::name())), language, installed };
    for line in found.lines() {
        report::line(line);
    }
    // Remote control is always on (ADR remote-control-is-always-on).
    pipe::start();
    let Some((mods, discovery_notes)) = discover(&user_dir) else { return };
    // Texts first: an extension may read its own at the top of main.lua.
    let mut plan = Plan::default();
    let every: Vec<_> = mods.iter().collect();
    let loc_notes = loc::install(language, &every, &mut plan);
    for note in &loc_notes {
        log!("localisation: {note}");
    }
    let data_dir = user_dir.join("eu4-harmony").join("data");
    let outcomes = lua::load(&mods, &logs, &data_dir);
    let included = |i: usize| outcomes[i].included;
    let ui_notes = splice::build(&mods, included, &mut plan);
    for note in &ui_notes {
        log!("ui: {note}");
    }
    unicode::framework_fonts::plan(&mut plan);
    unicode::map_field::plan(&mut plan);
    log!("splice plan ready: {} mod element(s), {} button slot(s)", plan.elements.len(), plan.slots);
    let notes: Vec<String> = discovery_notes
        .iter()
        .map(|n| format!("discovery: {}", n.text))
        .chain(ui_notes.iter().map(|n| format!("ui: {n}")))
        .chain(loc_notes.iter().map(|n| format!("localisation: {n}")))
        .collect();
    Setup::of(&mods, &outcomes, lua::mod_ids(), &lua::extension_indices(&mods), &notes, plan, &data_dir).apply();
}

/// A font chosen in the settings that is no longer installed: the player
/// language's default draws instead, and the player is told once
/// (ADR framework-succeeds-or-fails-as-a-whole). A weight its font no longer
/// has draws in the one below it, which only the log tells.
fn tell_missing_fonts(chosen: &settings::Settings) {
    let Ok(fonts) = unicode::system_font::SystemFonts::new() else { return };
    let installed = |family: &str| fonts.has_family(family);
    for (font, role) in chosen.missing_fonts(installed) {
        let default = settings::default_face(chosen.language(), role, installed).family;
        log!("settings: the font {font:?} is not installed; {default} draws instead");
        notice::post(&format!("font:{font}"), notice::Repeat::Run, notice::Message::new("EU4HARMONY_FONT_MISSING").arg("FONT", font).arg("DEFAULT", &default));
    }
    for face in settings::Role::ALL.into_iter().filter_map(|role| chosen.chosen(role)) {
        if let Some(drawn) = fonts.drawn(face).filter(|drawn| drawn != face) {
            log!("settings: the font {:?} has no weight {}; {} draws instead", face.family, face.weight, drawn.weight);
        }
    }
}

/// The mods of the playset with an EU4 Harmony manifest, in its order, and
/// what was skipped, which the player is told of; None when the playset
/// cannot be read.
fn discover(user_dir: &Path) -> Option<(Vec<mods::DiscoveredMod>, Vec<mods::Skipped>)> {
    let (mods, notes) = match mods::discover(user_dir) {
        Ok(v) => v,
        Err(e) => {
            log!("mod discovery failed: {e}");
            report::line(format!("mods: discovery failed: {e}"));
            report::write();
            return None;
        }
    };
    for note in &notes {
        log!("discovery: {}", note.text);
        if let Some(message) = &note.notice {
            notice::post(&format!("skipped:{}", note.text), notice::Repeat::Run, message.clone());
        }
    }
    // Lets the notice window be tried without a broken mod.
    if test_switch!(test_notice) {
        for number in ["1", "2"] {
            notice::post(&format!("test:{number}"), notice::Repeat::Run, notice::Message::new("EU4HARMONY_TEST_NOTICE").arg("NUMBER", number));
        }
    }
    for m in &mods {
        log!(
            "mod {:?} id={:?} api={} dir={} extension={:?} ui={}",
            m.descriptor.name,
            m.manifest.id,
            m.manifest.api_version,
            m.dir.display(),
            m.layout.extension,
            m.layout.ui.len()
        );
    }
    Some((mods, notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_loc::Language;

    fn a_mod(id: &str, name: &str, misplaced: &[&str]) -> mods::DiscoveredMod {
        mods::DiscoveredMod {
            descriptor_file: PathBuf::from(format!("mod\\{id}.mod")),
            descriptor: mods::Descriptor { name: name.into(), version: Some("1.2".into()), path: None, archive: None },
            dir: PathBuf::from(format!("D:\\mods\\{id}")),
            manifest: mods::Manifest {
                id: id.into(),
                api_version: "1".into(),
                description: Some(format!("{id}_DESC")),
                permissions: mods::Permissions::default(),
            },
            layout: mods::Layout { extension: None, ui: Vec::new(), misplaced: misplaced.iter().map(|f| f.to_string()).collect() },
        }
    }

    #[test]
    fn what_was_found_reads_in_the_report_s_order() {
        let language = lang::Decision { player: Language::German, game: Language::English, reason: "the settings".into() };
        let pipe = "\\\\.\\pipe\\eu4-harmony-me".to_string();
        let found = Found { user_dir: Path::new("C:\\user"), test_game: Some(("me".into(), pipe.clone())), language: &language, installed: "baseline 1.37.5" };
        assert_eq!(
            found.lines(),
            [
                "user dir: C:\\user".to_string(),
                format!("test game me: remote control on {pipe}"),
                format!("language: {} (the settings); the game runs in {}", Language::German.id(), Language::English.id()),
                "startup: baseline 1.37.5".to_string(),
            ]
        );
        let found = Found { test_game: None, ..found };
        assert!(!found.lines().iter().any(|l| l.starts_with("test game")));
    }

    #[test]
    fn the_setup_gives_each_mod_a_card_and_a_part_of_the_report() {
        let mods = [a_mod("alpha", "Alpha", &["ui/stray.gui"]), a_mod("beta", "", &[])];
        let outcomes =
            [lua::LoadOutcome { included: true, status: "loaded".into() }, lua::LoadOutcome { included: false, status: "not loaded: main.lua failed".into() }];
        let notes = ["discovery: a note".to_string()];
        let setup = Setup::of(&mods, &outcomes, vec!["alpha".into()], &BTreeMap::new(), &notes, Plan::default(), Path::new("D:\\data"));
        assert_eq!(setup.extensions, ["alpha"]);
        let names: Vec<&str> = setup.cards.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta"], "a mod without a name is named by its id");
        assert_eq!(setup.cards[1].data_folder, "D:\\data\\beta");
        assert_eq!(setup.cards[0].description.as_deref(), Some("alpha_DESC"));
        assert_eq!(
            setup.report,
            [
                "\nMods with an EU4 Harmony manifest, in playset order: 2",
                "  Alpha (id alpha, api 1): loaded; 0 UI fragment(s)",
                "    D:\\mods\\alpha",
                "    eu4-harmony/ui/stray.gui: the layout has no place for it, so it is not used",
                "   (id beta, api 1): not loaded: main.lua failed; 0 UI fragment(s)",
                "    D:\\mods\\beta",
                "  discovery: a note",
                "UI: 0 mod element(s) spliced, 0 button-area slot(s)",
            ]
        );
        assert!(setup.elements.is_empty());
    }
}
