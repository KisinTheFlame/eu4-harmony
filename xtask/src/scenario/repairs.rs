//! `repairs`: targeted regression scenarios for native game repairs added in
//! 0.2.2+ that ordinary long observe runs may not reliably hit:
//! 1. UTF-8 character-boundary color removal when hiring a mercenary company
//!    whose localized name contains both color markers and multi-byte UTF-8
//!    continuation byte `0xA7` (`"§Y大佣兵团§!"` -> `"大佣兵团"`), verified in the
//!    written plain-text save and across an in-game reload.
//! 2. Reformation-center conversion reference cleanup when a province being
//!    converted by center A becomes center B and center A is then removed,
//!    verified through continued daily updates, save state and in-game reload.
use super::{game::set_setting, text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    menu::MenuRequest,
    mercenaries::{Company, MercenaryRequest},
    questions::{Question, QuestionRequest},
    session::Session,
    time::TimeRequest,
    CountryTag, ProvinceId,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Repairs;

const MOD: &str = "qa-repairs";
const MERC_KEY: &str = "merc__local_grand_company";
const EXPECTED_MERC_NAME: &str = "大佣兵团";
const CENTER_A_PROVINCE: u32 = 50;

impl Scenario for Repairs {
    fn name(&self) -> &'static str {
        "repairs"
    }

    fn about(&self) -> &'static str {
        "targeted native repairs: UTF-8 mercenary color-code removal and reformation-center conversion detach across saves and reloads"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        1
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn mods(&self, _: &Parameters) -> Vec<PathBuf> {
        vec![crate::repo().join("xtask/src/scenario/mods").join(MOD)]
    }

    fn play(&self, game: &mut Game, _: &Parameters, record: &mut Value) -> Played<()> {
        let user = game.user()?;
        game.end();
        set_setting(&user.path().join("settings.txt"), "compress_autosave", "no")?;
        game.restart()?;

        let _: Session = game.ask(&MenuRequest::Start { country: Some(CountryTag(*b"FRA")) })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        answer_all_questions(game)?;
        *record = json!({});

        // 1. UTF-8 color-code removal during mercenary company hiring.
        game.console("cash 500")?;
        let companies: Vec<Company> = game.ask(&MercenaryRequest::List {})?;
        let grand = companies.iter().find(|c| c.company == MERC_KEY).ok_or_else(|| Stop::Failed(format!("{MERC_KEY} not listed in mercenary.list")))?;
        game.check(grand.choice.can, format!("cannot hire {MERC_KEY}: {:?}", grand.choice.reasons))?;
        let () = game.ask(&MercenaryRequest::Hire { company: MERC_KEY.into(), province: ProvinceId(183) })?;
        game.run(20)?;
        let save_bytes = write_complete_autosave(game)?;
        let expected_entry = format!("name=\"{EXPECTED_MERC_NAME}\"").into_bytes();
        let has_merc_before = contains_bytes(&save_bytes, &expected_entry);
        game.check(has_merc_before, format!("plain-text autosave did not preserve intact UTF-8 mercenary name {EXPECTED_MERC_NAME:?}"))?;
        let _: Session = game.ask(&MenuRequest::Load { save: "autosave".into() })?;
        let () = game.ask(&TimeRequest::Pause {})?;
        let save_after_reload = write_complete_autosave(game)?;
        let has_merc_after = contains_bytes(&save_after_reload, &expected_entry);
        game.check(has_merc_after, "intact UTF-8 mercenary name did not survive an in-game reload")?;
        record["mercenary_utf8"] = json!({
            "company": MERC_KEY,
            "listed_name": grand.name,
            "preserved_before_reload": has_merc_before,
            "preserved_after_reload": has_merc_after,
        });

        // 2. Reformation-center detach when a targeted province becomes a new center.
        fire_event(game, &format!("event qa_repairs.1 {CENTER_A_PROVINCE}"))?;
        let mut target_province = None;
        for _ in 0..5 {
            game.run(2)?;
            let bytes = write_complete_autosave(game)?;
            let centers = parse_reformation_centers(&bytes);
            if let Some(a) = centers.iter().find(|c| c.province == CENTER_A_PROVINCE && c.target.is_some()) {
                target_province = a.target;
                break;
            }
        }
        let target =
            target_province.ok_or_else(|| Stop::Failed(format!("Protestant center at province {CENTER_A_PROVINCE} did not pick a target province")))?;

        fire_event(game, &format!("event qa_repairs.2 {target}"))?;
        fire_event(game, &format!("event qa_repairs.3 {CENTER_A_PROVINCE}"))?;
        game.run(10)?;
        let after_removal = write_complete_autosave(game)?;
        let centers_after = parse_reformation_centers(&after_removal);
        let a_gone = !centers_after.iter().any(|c| c.province == CENTER_A_PROVINCE);
        let b_present = centers_after.iter().find(|c| c.province == target).cloned();
        game.check(a_gone, format!("center A at province {CENTER_A_PROVINCE} still present after removal"))?;
        game.check(b_present.is_some(), format!("center B at province {target} missing after removing center A"))?;

        let _: Session = game.ask(&MenuRequest::Load { save: "autosave".into() })?;
        game.run(5)?;
        record["reformation_center"] = json!({
            "center_a_province": CENTER_A_PROVINCE,
            "center_b_province": target,
            "center_a_removed": a_gone,
            "center_b_target_after_10_days": b_present.and_then(|c| c.target),
            "reloaded_and_advanced": true,
        });
        game.return_to_menu()
    }

    fn report(&self, record: &Value) -> Vec<String> {
        vec![
            "### Targeted native repairs".into(),
            String::new(),
            format!(
                "UTF-8 mercenary color-code preservation ({}): before reload {}, after reload {}.",
                text(&record["mercenary_utf8"]["company"]),
                text(&record["mercenary_utf8"]["preserved_before_reload"]),
                text(&record["mercenary_utf8"]["preserved_after_reload"])
            ),
            format!(
                "Reformation-center detach: center A at {} targeted {}, replaced with center B and removed (A removed: {}, B target after 10 days: {}, reloaded & advanced: {}).",
                text(&record["reformation_center"]["center_a_province"]),
                text(&record["reformation_center"]["center_b_province"]),
                text(&record["reformation_center"]["center_a_removed"]),
                text(&record["reformation_center"]["center_b_target_after_10_days"]),
                text(&record["reformation_center"]["reloaded_and_advanced"])
            ),
        ]
    }
}

fn answer_all_questions(game: &mut Game) -> Played<()> {
    let questions: Vec<Question> = game.ask(&QuestionRequest::List {})?;
    for q in questions {
        if let Some(first) = q.options.first() {
            let () = game.ask(&QuestionRequest::Answer { question: q.id, option: first.option })?;
        }
    }
    Ok(())
}

fn fire_event(game: &mut Game, command: &str) -> Played<()> {
    game.console(command)?;
    std::thread::sleep(Duration::from_millis(200));
    answer_all_questions(game)
}

/// Writes an uncompressed text `autosave.eu4` and waits until the trailing
/// `checksum="..."` line is present so background saving is complete.
fn write_complete_autosave(game: &mut Game) -> Played<Vec<u8>> {
    let path = game.user()?.save_games().join("autosave.eu4");
    if path.exists() {
        fs::remove_file(&path).map_err(|e| Stop::Failed(format!("cannot remove {}: {e}", path.display())))?;
    }
    game.console("autosave")?;
    let began = Instant::now();
    loop {
        if let Ok(bytes) = fs::read(&path) {
            if is_complete_text_save(&bytes) {
                return Ok(bytes);
            }
        }
        if began.elapsed() > Duration::from_secs(180) {
            return Err(Stop::Hung("the complete text autosave was not written in 180 s".into()));
        }
        let _: Session = game.session()?;
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn is_complete_text_save(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"EU4txt") {
        return false;
    }
    let tail = &bytes[bytes.len().saturating_sub(256)..];
    contains_bytes(tail, b"checksum=\"")
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CenterEntry {
    province: u32,
    target: Option<u32>,
}

/// Extracts `reformation_center={ ... }` records from a plain-text EU4 save.
fn parse_reformation_centers(bytes: &[u8]) -> Vec<CenterEntry> {
    let text = String::from_utf8_lossy(bytes);
    let mut centers = Vec::new();
    let mut rest = text.as_ref();
    while let Some(pos) = rest.find("reformation_center=") {
        let after = &rest[pos + "reformation_center=".len()..];
        let Some(open) = after.find('{') else { break };
        let body = &after[open + 1..];
        let Some(close) = matching_brace(body) else { break };
        let block = &body[..close];
        if let Some(province) = parse_u32_field(block, "province_id") {
            let target = parse_u32_field(block, "any_target_province").filter(|&id| id > 0);
            centers.push(CenterEntry { province, target });
        }
        rest = &body[close + 1..];
    }
    centers
}

fn matching_brace(text: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (i, b) in text.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_u32_field(block: &str, key: &str) -> Option<u32> {
    for line in block.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix(key).and_then(|s| s.trim_start().strip_prefix('=')) {
            if let Ok(n) = val.trim().parse::<u32>() {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reformation_center_blocks_and_targets() {
        let save = b"EU4txt\nreligion_instance_data={\n\tprotestant={\n\t\treformation_center={\n\t\t\ttype=\"center_of_reformation\"\n\t\t\tprovince_id=50\n\t\t\tany_target_province=1759\n\t\t}\n\t}\n\treformed={\n\t\treformation_center={\n\t\t\tprovince_id=1759\n\t\t}\n\t}\n}\nchecksum=\"0123456789abcdef0123456789abcdef\"\n";
        assert!(is_complete_text_save(save));
        assert_eq!(parse_reformation_centers(save), vec![CenterEntry { province: 50, target: Some(1759) }, CenterEntry { province: 1759, target: None }]);
    }

    #[test]
    fn detects_intact_utf8_mercenary_name_bytes() {
        let intact = "army={\n\tname=\"大佣兵团\"\n}\n".as_bytes();
        let needle = format!("name=\"{EXPECTED_MERC_NAME}\"").into_bytes();
        assert!(contains_bytes(intact, &needle));
        let damaged = b"army={\n\tname=\"\xe5\xa4\xbd\xa3\xe5\x85\xb5\xe5\x9b\xa2\"\n}\n";
        assert!(!contains_bytes(damaged, &needle));
    }
}
