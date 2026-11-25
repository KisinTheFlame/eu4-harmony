//! `achievements`: an eligible non-ironman modded game must track
//! achievements, including after an in-game load, and refuse eligibility when
//! either `achievements_with_changed_checksum` or `achievements_outside_ironman`
//! is off. The game's date never advances, so the scenario does not
//! deliberately unlock an achievement on the platform.
use super::{game::set_setting, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{menu::MenuRequest, session::Session, time::TimeRequest, CountryTag};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Achievements;

impl Scenario for Achievements {
    fn name(&self) -> &'static str {
        "achievements"
    }

    fn about(&self) -> &'static str {
        "non-ironman achievement tracking with a checksum-changing mod, an in-game load and both settings off"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        6
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn mods(&self, _: &Parameters) -> Vec<PathBuf> {
        vec![crate::repo().join("xtask/src/scenario/mods/qa-checksum")]
    }

    fn play(&self, game: &mut Game, _: &Parameters, record: &mut Value) -> Played<()> {
        let user = game.user()?;
        // These files belong to this scenario's claim, never the player.
        game.end();
        set_setting(&user.path().join("settings.txt"), "compress_autosave", "no")?;
        let settings = user.harmony().join("settings.txt");
        set_setting(&settings, "achievements_with_changed_checksum", "yes")?;
        set_setting(&settings, "achievements_outside_ironman", "yes")?;
        game.restart()?;
        let country = CountryTag(*b"CAS");
        let began: Session = game.ask(&MenuRequest::Start { country: Some(country) })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        let before = save(game)?;
        *record = json!({ "before_load": before.json() });
        game.check(!before.ironman && before.eligible, "the new game is not eligible outside ironman with changed checksum")?;
        game.check(!before.tracked.is_empty(), "eligible non-ironman modded game has no tracked achievements")?;

        let loaded: Session = game.ask(&MenuRequest::Load { save: "autosave".into() })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        game.check(loaded.date == began.date && loaded.player == began.player, "the in-game load changed the game date or player")?;
        let after = save(game)?;
        record["after_load"] = after.json();
        game.check(!after.ironman && after.eligible, "the in-game load lost non-ironman modded achievement eligibility")?;
        game.check(after.tracked == before.tracked, "the in-game load changed the tracked achievement list while paused")?;

        game.end();
        set_setting(&settings, "achievements_with_changed_checksum", "no")?;
        game.restart()?;
        let _: Session = game.ask(&MenuRequest::Start { country: Some(country) })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        let checksum_off = save(game)?;
        record["changed_checksum_off"] = checksum_off.json();
        game.check(
            !checksum_off.ironman && !checksum_off.eligible,
            "a modded game earned achievement eligibility with achievements_with_changed_checksum off",
        )?;
        game.check(checksum_off.tracked.is_empty(), "a modded game tracked achievements with achievements_with_changed_checksum off")?;

        game.end();
        set_setting(&settings, "achievements_with_changed_checksum", "yes")?;
        set_setting(&settings, "achievements_outside_ironman", "no")?;
        game.restart()?;
        let _: Session = game.ask(&MenuRequest::Start { country: Some(country) })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        let off = save(game)?;
        record["outside_ironman_off"] = off.json();
        game.check(!off.ironman && !off.eligible, "a non-ironman game earned achievement eligibility with achievements_outside_ironman off")?;
        game.check(off.tracked.is_empty(), "a non-ironman game tracked achievements with achievements_outside_ironman off")?;
        game.return_to_menu()
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let mut lines =
            vec!["### Achievement tracking".into(), String::new(), "| Step | Ironman | Eligible | Tracked |".into(), "| --- | --- | --- | --- |".into()];
        for (key, label) in [
            ("before_load", "New game"),
            ("after_load", "In-game load"),
            ("changed_checksum_off", "Changed checksum off"),
            ("outside_ironman_off", "Outside ironman off"),
            ("setting_off", "Setting off"),
        ] {
            if let Some(step) = record.get(key) {
                lines.push(format!("| {label} | {} | {} | {} |", step["ironman"], step["eligible"], step["tracked_count"]));
            }
        }
        lines
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Saved {
    ironman: bool,
    eligible: bool,
    tracked: Vec<u32>,
}

impl Saved {
    fn json(&self) -> Value {
        json!({ "ironman": self.ironman, "eligible": self.eligible, "tracked_count": self.tracked.len(), "tracked": self.tracked })
    }
}

/// The checksum is the last top-level entry, so reading it also establishes
/// that the background writer has finished the text we are checking.
fn save(game: &mut Game) -> Played<Saved> {
    let path = game.user()?.save_games().join("autosave.eu4");
    // A previous autosave must not satisfy this check before the new writer
    // has replaced it. This file belongs only to this scenario's claim.
    if path.exists() {
        fs::remove_file(&path).map_err(|e| Stop::Failed(format!("cannot remove {}: {e}", path.display())))?;
    }
    game.console("autosave")?;
    let began = Instant::now();
    loop {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(saved) = read_saved(&bytes) {
                return Ok(saved);
            }
        }
        if began.elapsed() > Duration::from_secs(180) {
            return Err(Stop::Hung("the complete text autosave was not written in 180 s".into()));
        }
        // Also fails promptly if the game has stopped or crashed.
        let _: Session = game.session()?;
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Reads only top-level entries, skipping braces and field-like text in
/// quoted strings and comments. No decoding of the game's text is needed.
fn read_saved(bytes: &[u8]) -> Result<Saved, &'static str> {
    let mut tokens = Tokens(bytes);
    if tokens.next()? != Some(b"EU4txt") {
        return Err("not a text save");
    }
    let (mut depth, mut checksum, mut eligibility) = (0usize, false, false);
    let mut saved = Saved::default();
    while let Some(token) = tokens.next()? {
        match token {
            b"{" => depth += 1,
            b"}" => depth = depth.checked_sub(1).ok_or("unmatched closing brace")?,
            b"is_ironman" | b"achievement_ok" | b"achievement" | b"checksum" if depth == 0 => {
                if tokens.next()? != Some(b"=") {
                    return Err("missing field separator");
                }
                let value = tokens.next()?.ok_or("missing field value")?;
                match token {
                    b"is_ironman" => saved.ironman = value == b"yes",
                    b"achievement_ok" => {
                        saved.eligible = value == b"yes";
                        eligibility = true;
                    }
                    b"achievement" => {
                        if value != b"{" {
                            return Err("achievement list is not a block");
                        }
                        loop {
                            let id = tokens.next()?.ok_or("incomplete achievement list")?;
                            if id == b"}" {
                                break;
                            }
                            saved.tracked.push(std::str::from_utf8(id).map_err(|_| "invalid achievement id")?.parse().map_err(|_| "invalid achievement id")?);
                        }
                    }
                    b"checksum" => checksum = value.len() == 34 && value.starts_with(b"\"") && value.ends_with(b"\""),
                    _ => unreachable!(),
                }
            }
            _ => {}
        }
    }
    if depth != 0 || !checksum || !eligibility {
        return Err("incomplete save");
    }
    saved.tracked.sort_unstable();
    Ok(saved)
}

struct Tokens<'a>(&'a [u8]);

impl<'a> Tokens<'a> {
    fn next(&mut self) -> Result<Option<&'a [u8]>, &'static str> {
        loop {
            self.0 = self.0.trim_ascii_start();
            if !self.0.starts_with(b"#") {
                break;
            }
            self.0 = self.0.get(self.0.iter().position(|b| *b == b'\n').unwrap_or(self.0.len())..).unwrap_or_default();
        }
        let Some(first) = self.0.first() else { return Ok(None) };
        let len = if *first == b'"' {
            let mut end = 1;
            loop {
                match self.0.get(end) {
                    Some(b'"') => break end + 1,
                    Some(b'\\') => end += 2,
                    Some(_) => end += 1,
                    None => return Err("incomplete quoted string"),
                }
            }
        } else if b"{}=".contains(first) {
            1
        } else {
            self.0.iter().position(|b| b.is_ascii_whitespace() || b"{}=#\"".contains(b)).unwrap_or(self.0.len())
        };
        let (token, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(Some(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::game::with_setting;

    const CHECKSUM: &str = "checksum=\"0123456789abcdef0123456789abcdef\"";

    #[test]
    fn achievement_tracking_uses_only_complete_top_level_fields() {
        let text = format!("EU4txt\nnested={{achievement_ok=no achievement={{999}}}}\ntext=\"achievement={{888}} # \\\"\"\n# achievement={{777}}\nachievement_ok=yes\nachievement={{1 23 4}}\n{CHECKSUM}\n");
        assert_eq!(read_saved(text.as_bytes()), Ok(Saved { ironman: false, eligible: true, tracked: vec![1, 4, 23] }));
        assert!(read_saved(text.trim_end_matches('\n').trim_end_matches('"').as_bytes()).is_err());
    }

    #[test]
    fn empty_and_absent_lists_do_not_become_tracking() {
        for list in ["", "achievement={}"] {
            let text = format!("EU4txt\nachievement_ok=yes\n{list}\n{CHECKSUM}\n");
            assert!(read_saved(text.as_bytes()).unwrap().tracked.is_empty());
        }
        let ironman = format!("EU4txt\nis_ironman=yes\nachievement_ok=no\n{CHECKSUM}\n");
        assert_eq!(read_saved(ironman.as_bytes()), Ok(Saved { ironman: true, eligible: false, tracked: vec![] }));
    }

    #[test]
    fn setting_changes_keep_other_settings() {
        assert_eq!(
            with_setting("body_font=\"Example\"\nachievements_outside_ironman=no\n", "achievements_outside_ironman", "yes"),
            "body_font=\"Example\"\nachievements_outside_ironman=yes\n"
        );
        assert_eq!(with_setting("compress_saves=yes\n", "compress_autosave", "no"), "compress_saves=yes\ncompress_autosave=no\n");
    }

    #[test]
    fn missing_settings_file_is_created_with_requested_setting() {
        let dir = std::env::temp_dir().join(format!("eu4h-ach-setting-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("eu4-harmony").join("settings.txt");
        set_setting(&path, "achievements_outside_ironman", "yes").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "achievements_outside_ironman=yes\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
