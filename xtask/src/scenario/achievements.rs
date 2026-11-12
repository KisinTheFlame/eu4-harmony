//! `achievements`: an eligible non-ironman game must track achievements,
//! including after an in-game load. The game's date never advances, so the
//! scenario does not deliberately unlock an achievement on the platform.
use super::{Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{menu::MenuRequest, session::Session, time::TimeRequest, CountryTag};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

pub struct Achievements;

impl Scenario for Achievements {
    fn name(&self) -> &'static str {
        "achievements"
    }

    fn about(&self) -> &'static str {
        "non-ironman achievement tracking, an in-game load and the setting off"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        5
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn play(&self, game: &mut Game, _: &Parameters, record: &mut Value) -> Played<()> {
        let user = game.user()?;
        // These files belong to this scenario's claim, never the player.
        game.end();
        setting(&user.path().join("settings.txt"), "compress_autosave", "no")?;
        let settings = user.harmony().join("settings.txt");
        setting(&settings, "achievements_with_changed_checksum", "yes")?;
        setting(&settings, "achievements_outside_ironman", "yes")?;
        game.restart()?;
        let country = CountryTag(*b"CAS");
        let began: Session = game.ask(&MenuRequest::Start { country: Some(country) })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        let before = save(game)?;
        *record = json!({ "before_load": before.json() });
        game.check(!before.ironman && before.eligible, "the new game is not eligible outside ironman")?;
        game.check(!before.tracked.is_empty(), "eligible non-ironman game has no tracked achievements")?;

        let loaded: Session = game.ask(&MenuRequest::Load { save: "autosave".into() })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        game.check(loaded.date == began.date && loaded.player == began.player, "the in-game load changed the game date or player")?;
        let after = save(game)?;
        record["after_load"] = after.json();
        game.check(!after.ironman && after.eligible, "the in-game load lost non-ironman achievement eligibility")?;
        game.check(after.tracked == before.tracked, "the in-game load changed the tracked achievement list while paused")?;

        game.end();
        setting(&settings, "achievements_outside_ironman", "no")?;
        game.restart()?;
        let _: Session = game.ask(&MenuRequest::Start { country: Some(country) })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        let off = save(game)?;
        record["setting_off"] = off.json();
        game.check(!off.ironman && !off.eligible, "a non-ironman game earned achievement eligibility with the setting off")?;
        game.check(off.tracked.is_empty(), "a non-ironman game tracked achievements with the setting off")?;
        game.return_to_menu()
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let mut lines =
            vec!["### Achievement tracking".into(), String::new(), "| Step | Ironman | Eligible | Tracked |".into(), "| --- | --- | --- | --- |".into()];
        for (key, label) in [("before_load", "New game"), ("after_load", "In-game load"), ("setting_off", "Setting off")] {
            if let Some(step) = record.get(key) {
                lines.push(format!("| {label} | {} | {} | {} |", step["ironman"], step["eligible"], step["tracked_count"]));
            }
        }
        lines
    }
}

/// Changes one top-level setting while keeping the rest of the claim's file.
fn setting(path: &Path, key: &str, value: &str) -> Played<()> {
    let text = fs::read_to_string(path).map_err(|e| Stop::Failed(format!("cannot read {}: {e}", path.display())))?;
    fs::write(path, with_setting(&text, key, value)).map_err(|e| Stop::Failed(format!("cannot write {}: {e}", path.display())))
}

fn with_setting(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut changed = String::new();
    for line in text.lines() {
        if line.split_once('=').is_some_and(|(name, _)| name.trim() == key) {
            changed.push_str(&format!("{key}={value}\n"));
            found = true;
        } else {
            changed.push_str(line);
            changed.push('\n');
        }
    }
    if !found {
        changed.push_str(&format!("{key}={value}\n"));
    }
    changed
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
}
