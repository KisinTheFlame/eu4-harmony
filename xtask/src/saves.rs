//! Shared full-DLC test saves (`xtask/saves.toml`): resolution, SHA-256
//! verification, download from `EU4_HARMONY_SAVES_URL` or the GitHub Release,
//! and clean generation inside isolated test games.
use crate::{
    claim,
    scenario::{days_between, Game, Played, Stop},
    Args, Result,
};
use eu4_harmony_remote::{
    menu::{MenuRequest, Save},
    session::Session,
    time::TimeRequest,
    CountryTag, Date,
};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MANIFEST_TOML: &str = include_str!("../saves.toml");
const USAGE: &str = "\
cargo xtask saves fetch [--force] [--only short|late]
cargo xtask saves verify [--print-hashes] [--dir <dir>]
cargo xtask saves generate (--short | --late | --all) --owner <name> [--out <dir>] [--game-dir <dir>]";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveKind {
    Short,
    Late,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SaveEntry {
    pub name: String,
    pub file: String,
    pub asset: String,
    pub kind: SaveKind,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub release: String,
    pub url: String,
    #[serde(rename = "save")]
    pub saves: Vec<SaveEntry>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(text).map_err(|e| format!("xtask/saves.toml: {e}"))?;
        if manifest.release.trim().is_empty() || manifest.url.trim().is_empty() {
            return Err("xtask/saves.toml: release and url must be non-empty".into());
        }
        let (mut names, mut files, mut assets) = (HashSet::new(), HashSet::new(), HashSet::new());
        for save in &manifest.saves {
            if save.name.is_empty() || save.file.is_empty() || save.asset.is_empty() {
                return Err("xtask/saves.toml: save entries must have non-empty name, file and asset".into());
            }
            if !names.insert(save.name.as_str()) {
                return Err(format!("xtask/saves.toml: duplicate save name {:?}", save.name));
            }
            if !files.insert(save.file.as_str()) {
                return Err(format!("xtask/saves.toml: duplicate save file {:?}", save.file));
            }
            if !assets.insert(save.asset.as_str()) {
                return Err(format!("xtask/saves.toml: duplicate save asset {:?}", save.asset));
            }
            if !save.sha256.is_empty() && !is_valid_sha256(&save.sha256) {
                return Err(format!("xtask/saves.toml: {} has invalid sha256 {:?}", save.name, save.sha256));
            }
        }
        Ok(manifest)
    }
}

fn is_valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The parsed manifest from `xtask/saves.toml`.
pub fn manifest() -> &'static Manifest {
    static MANIFEST: std::sync::OnceLock<Manifest> = std::sync::OnceLock::new();
    MANIFEST.get_or_init(|| Manifest::parse(MANIFEST_TOML).expect("valid xtask/saves.toml"))
}

/// The shared test saves directory (`%LOCALAPPDATA%\eu4-harmony-dev\saves`,
/// respecting `EU4_HARMONY_DEV_DIR`), created when missing.
pub fn saves_dir() -> Result<PathBuf> {
    let dir = crate::dev_dir()?.join("saves");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Base URL for downloading test saves (`EU4_HARMONY_SAVES_URL` if set and
/// non-empty, else the release URL in `xtask/saves.toml`).
pub fn base_url(manifest: &Manifest) -> String {
    std::env::var("EU4_HARMONY_SAVES_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| manifest.url.trim_end_matches('/').to_string())
}

/// The in-game save name (as `menu.saves` lists it) for a bare save name or
/// file path.
pub fn save_name(spec: &str) -> String {
    let base = Path::new(spec).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| spec.to_string());
    match base.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("eu4") => stem.to_string(),
        _ => base,
    }
}

/// Summary metadata read from a `.eu4` save's `gamestate` (compressed ZIP or
/// plain text).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveStartInfo {
    /// The rank-1 great power at the start of the save (`great_powers` ->
    /// `original` entry with `rank=1`), if present.
    pub top_great_power: Option<CountryTag>,
    /// The capital province ID of the requested country (or of
    /// `top_great_power` when none was requested), if found.
    pub capital: Option<u32>,
}

/// Reads a `.eu4` save's `gamestate` (decompressing the ZIP entry when the
/// save is compressed) and extracts the rank-1 great power and the capital of
/// `target` (or of the rank-1 great power when `target` is `None`).
pub fn read_start_info(path: &Path, target: Option<CountryTag>) -> Result<SaveStartInfo> {
    let gamestate = read_gamestate_bytes(path)?;
    Ok(parse_start_info(&gamestate, target))
}

fn read_gamestate_bytes(path: &Path) -> Result<Vec<u8>> {
    let data = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if !data.starts_with(b"PK\x03\x04") {
        return Ok(data);
    }
    extract_zip_entry(&data, b"gamestate").map_err(|e| format!("{}: {e}", path.display()))
}

/// Extracts `wanted` from an in-memory ZIP archive using the central directory
/// (which carries reliable sizes even when local headers set the data
/// descriptor flag).
fn extract_zip_entry(data: &[u8], wanted: &[u8]) -> Result<Vec<u8>> {
    let eocd_start = data.len().saturating_sub(65_557);
    let eocd = (eocd_start..=data.len().saturating_sub(22))
        .rev()
        .find(|&i| &data[i..i + 4] == b"PK\x05\x06")
        .ok_or_else(|| "invalid zip: missing end-of-central-directory record".to_string())?;
    let entries = u16::from_le_bytes(data[eocd + 10..eocd + 12].try_into().expect("2 bytes")) as usize;
    let cd_offset = u32::from_le_bytes(data[eocd + 16..eocd + 20].try_into().expect("4 bytes")) as usize;
    let mut pos = cd_offset;
    for _ in 0..entries {
        if pos + 46 > data.len() || &data[pos..pos + 4] != b"PK\x01\x02" {
            return Err("invalid zip: malformed central directory entry".into());
        }
        let method = u16::from_le_bytes(data[pos + 10..pos + 12].try_into().expect("2 bytes"));
        let comp_size = u32::from_le_bytes(data[pos + 20..pos + 24].try_into().expect("4 bytes")) as usize;
        let name_len = u16::from_le_bytes(data[pos + 28..pos + 30].try_into().expect("2 bytes")) as usize;
        let extra_len = u16::from_le_bytes(data[pos + 30..pos + 32].try_into().expect("2 bytes")) as usize;
        let comment_len = u16::from_le_bytes(data[pos + 32..pos + 34].try_into().expect("2 bytes")) as usize;
        let local_offset = u32::from_le_bytes(data[pos + 42..pos + 46].try_into().expect("4 bytes")) as usize;
        let name_start = pos + 46;
        let name_end = name_start + name_len;
        if name_end > data.len() {
            return Err("invalid zip: central directory entry truncated".into());
        }
        if &data[name_start..name_end] == wanted {
            if local_offset + 30 > data.len() || &data[local_offset..local_offset + 4] != b"PK\x03\x04" {
                return Err("invalid zip: malformed local file header".into());
            }
            let l_name_len = u16::from_le_bytes(data[local_offset + 26..local_offset + 28].try_into().expect("2 bytes")) as usize;
            let l_extra_len = u16::from_le_bytes(data[local_offset + 28..local_offset + 30].try_into().expect("2 bytes")) as usize;
            let payload_start = local_offset + 30 + l_name_len + l_extra_len;
            let payload_end = payload_start.checked_add(comp_size).ok_or_else(|| "invalid zip: entry size overflow".to_string())?;
            if payload_end > data.len() {
                return Err("invalid zip: entry payload truncated".into());
            }
            let payload = &data[payload_start..payload_end];
            return match method {
                0 => Ok(payload.to_vec()),
                8 => miniz_oxide::inflate::decompress_to_vec(payload).map_err(|e| format!("deflate error: {e:?}")),
                other => Err(format!("unsupported zip compression method {other}")),
            };
        }
        pos = name_end + extra_len + comment_len;
    }
    Err(format!("zip archive has no {:?} entry", String::from_utf8_lossy(wanted)))
}

/// Parses `great_powers` (`rank=1`) and the capital of `target` (or the
/// rank-1 great power when `target` is `None`) from raw `gamestate` bytes.
pub fn parse_start_info(gamestate: &[u8], target: Option<CountryTag>) -> SaveStartInfo {
    let mut top_great_power: Option<CountryTag> = None;
    let mut capital: Option<u32> = None;

    let mut in_gp = false;
    let mut gp_depth: i32 = 0;
    let mut in_original = false;
    let mut orig_rank: Option<u32> = None;
    let mut orig_country: Option<CountryTag> = None;

    let mut in_countries = false;
    let mut countries_depth: i32 = 0;
    let mut in_target_country = false;
    let mut country_depth: i32 = 0;
    let mut target_prefix: Option<String> = target.map(|t| format!("\t{t}={{"));

    for raw_line in gamestate.split(|&b| b == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        if in_gp {
            let trimmed = trim_ascii(line);
            if gp_depth == 1 && trimmed == b"original={" {
                in_original = true;
                orig_rank = None;
                orig_country = None;
            } else if in_original && gp_depth == 2 {
                if let Some(rest) = trimmed.strip_prefix(b"rank=") {
                    orig_rank = parse_ascii_u32(rest);
                } else if let Some(rest) = trimmed.strip_prefix(b"country=") {
                    orig_country = parse_quoted_tag(rest);
                }
            }
            gp_depth += brace_delta(line);
            if in_original && gp_depth <= 1 {
                if orig_rank == Some(1) {
                    if let Some(tag) = orig_country {
                        top_great_power = Some(tag);
                        if target_prefix.is_none() {
                            target_prefix = Some(format!("\t{tag}={{"));
                        }
                    }
                }
                in_original = false;
            }
            if gp_depth <= 0 {
                in_gp = false;
            }
            continue;
        }

        if in_countries {
            if in_target_country {
                if country_depth == 1 {
                    if let Some(rest) = line.strip_prefix(b"\t\tcapital=") {
                        capital = parse_ascii_u32(trim_ascii(rest));
                        if capital.is_some() {
                            break;
                        }
                    }
                }
                country_depth += brace_delta(line);
                if country_depth <= 0 {
                    break;
                }
            } else {
                if countries_depth == 1 {
                    if let Some(prefix) = &target_prefix {
                        if line == prefix.as_bytes() {
                            in_target_country = true;
                            country_depth = 1;
                            countries_depth += 1;
                            continue;
                        }
                    }
                }
                countries_depth += brace_delta(line);
                if countries_depth <= 0 {
                    in_countries = false;
                }
            }
            continue;
        }

        if line == b"great_powers={" && top_great_power.is_none() {
            in_gp = true;
            gp_depth = 1;
        } else if line == b"countries={" && target_prefix.is_some() {
            in_countries = true;
            countries_depth = 1;
        }
    }

    SaveStartInfo { top_great_power, capital }
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(bytes.len());
    let end = bytes.iter().rposition(|b| !b.is_ascii_whitespace()).map_or(start, |i| i + 1);
    &bytes[start..end]
}

fn brace_delta(bytes: &[u8]) -> i32 {
    let mut delta = 0i32;
    let mut in_quotes = false;
    for &b in bytes {
        if b == b'"' {
            in_quotes = !in_quotes;
        } else if !in_quotes {
            if b == b'{' {
                delta += 1;
            } else if b == b'}' {
                delta -= 1;
            }
        }
    }
    delta
}

fn parse_ascii_u32(bytes: &[u8]) -> Option<u32> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn parse_quoted_tag(bytes: &[u8]) -> Option<CountryTag> {
    let inner = bytes.strip_prefix(b"\"")?.strip_suffix(b"\"")?;
    CountryTag::parse(std::str::from_utf8(inner).ok()?)
}

/// Resolves a save specification as an explicit `.eu4` file path first, then
/// inside `dir` (with or without `.eu4`).
pub fn resolve_save(spec: &str, dir: &Path) -> Result<PathBuf> {
    let explicit = Path::new(spec);
    if explicit.is_file() {
        return Ok(explicit.to_path_buf());
    }
    if let Some(found) = [dir.join(spec), dir.join(format!("{spec}.eu4"))].into_iter().find(|p| p.is_file()) {
        return Ok(found);
    }
    let name = save_name(spec);
    let hint = if manifest().saves.iter().any(|s| s.name == name) { "; run `cargo xtask saves fetch` to download the standard test saves" } else { "" };
    Err(format!("no save {spec:?} (looked as a file path and in {}){hint}", dir.display()))
}

pub fn run(raw: &[String]) -> Result<()> {
    let Some((command, rest)) = raw.split_first() else {
        println!("{USAGE}");
        return Ok(());
    };
    match command.as_str() {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        "fetch" => {
            let args = Args::parse(rest, &["force"], &["only"])?;
            let only = match args.value("only") {
                None => None,
                Some("short") => Some(SaveKind::Short),
                Some("late") => Some(SaveKind::Late),
                Some(other) => return Err(format!("--only {other:?}: expected \"short\" or \"late\"")),
            };
            let manifest = manifest();
            fetch_with(manifest, &base_url(manifest), &saves_dir()?, only, args.flag("force"))
        }
        "verify" => {
            let args = Args::parse(rest, &["print-hashes"], &["dir"])?;
            let dir = match args.value("dir") {
                Some(d) => PathBuf::from(d),
                None => saves_dir()?,
            };
            verify_with(manifest(), &dir, args.flag("print-hashes"))
        }
        "generate" => {
            let args = Args::parse(rest, &["short", "late", "all"], &["owner", "out", "game-dir"])?;
            generate(&args)
        }
        other => Err(format!("unknown command saves {other:?}\n\n{USAGE}")),
    }
}

fn fetch_with(manifest: &Manifest, base_url: &str, dest_dir: &Path, only: Option<SaveKind>, force: bool) -> Result<()> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("cannot make {}: {e}", dest_dir.display()))?;
    let entries: Vec<&SaveEntry> = manifest.saves.iter().filter(|s| only.is_none_or(|k| s.kind == k)).collect();
    for entry in &entries {
        if entry.sha256.is_empty() {
            return Err(format!("save {} has no pinned sha256 in xtask/saves.toml yet", entry.name));
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!("tokio runtime: {e}"))?;
    let client = {
        let _guard = runtime.handle().enter();
        reqwest::Client::builder()
            .user_agent(concat!("eu4-harmony-xtask/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|e| format!("http client: {e}"))?
    };
    let base = base_url.trim_end_matches('/');
    for entry in entries {
        let dest = dest_dir.join(&entry.file);
        if dest.is_file() {
            let existing = hash_file(&dest)?;
            if existing == entry.sha256 {
                println!("ok (cached): {}", entry.file);
                continue;
            }
            if !force {
                return Err(format!("{} exists with sha256 {existing}, expected {}; pass --force to replace it", dest.display(), entry.sha256));
            }
        }
        let url = format!("{base}/{}", entry.asset);
        println!("downloading {} <- {url}", entry.file);
        let part = dest_dir.join(format!("{}.part", entry.file));
        let _ = fs::remove_file(&part);
        let downloaded = runtime.block_on(download_and_hash(&client, &url, &part));
        let actual = match downloaded {
            Ok(hash) => hash,
            Err(e) => {
                let _ = fs::remove_file(&part);
                return Err(format!("download {}: {e}", entry.file));
            }
        };
        if actual != entry.sha256 {
            let _ = fs::remove_file(&part);
            return Err(format!("checksum mismatch for {}: got {actual}, expected {}", entry.file, entry.sha256));
        }
        if dest.exists() {
            fs::remove_file(&dest).map_err(|e| format!("cannot replace {}: {e}", dest.display()))?;
        }
        fs::rename(&part, &dest).map_err(|e| {
            let _ = fs::remove_file(&part);
            format!("cannot move {} to {}: {e}", part.display(), dest.display())
        })?;
        println!("ok: {} ({actual})", entry.file);
    }
    Ok(())
}

async fn download_and_hash(client: &reqwest::Client, url: &str, part: &Path) -> Result<String> {
    let mut response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let mut file = fs::File::create(part).map_err(|e| format!("create {}: {e}", part.display()))?;
    let mut hasher = Sha256::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| format!("write {}: {e}", part.display()))?;
    }
    file.flush().map_err(|e| format!("flush {}: {e}", part.display()))?;
    Ok(hasher.finish_hex())
}

fn verify_with(manifest: &Manifest, dir: &Path, print_hashes: bool) -> Result<()> {
    let mut issues = 0usize;
    for entry in &manifest.saves {
        let path = dir.join(&entry.file);
        if !path.is_file() {
            println!("{}: missing ({})", entry.file, path.display());
            if !print_hashes {
                issues += 1;
            }
            continue;
        }
        let actual = hash_file(&path)?;
        if entry.sha256.is_empty() {
            println!("{}: unpinned sha256={actual}", entry.file);
            if !print_hashes {
                issues += 1;
            }
        } else if actual == entry.sha256 {
            println!("{}: ok sha256={actual}", entry.file);
        } else {
            println!("{}: mismatch got={actual} expected={}", entry.file, entry.sha256);
            issues += 1;
        }
    }
    if issues > 0 {
        return Err(format!("{issues} save check(s) failed in {}", dir.display()));
    }
    Ok(())
}

fn generate(args: &Args) -> Result<()> {
    let all = args.flag("all");
    let short = all || args.flag("short");
    let late = all || args.flag("late");
    if !short && !late {
        return Err("choose --short, --late or --all\n\n".to_string() + USAGE);
    }
    let owner = claim::named_owner(args.value("owner"))?;
    let out = match args.value("out") {
        Some(dir) => PathBuf::from(dir),
        None => saves_dir()?,
    };
    fs::create_dir_all(&out).map_err(|e| format!("cannot make {}: {e}", out.display()))?;
    let game_dir = eu4_harmony_folders::game_dir(args.value("game-dir").map(Path::new))?;
    eu4_harmony_client::game::steam_ready()?;
    crate::files::build(Some("eu4-harmony-core/dev-test-games"))?;

    let _catch = crate::scenario::Interrupts::catch();
    let run = claim::claim_for_run(&owner, "generate clean full-DLC test saves", &game_dir)?;
    claim::play_within(&run, &crate::scenario::interrupted, || {
        let mut game = Game::new(&owner, &game_dir)?;
        if short {
            generate_short(&mut game, &out).map_err(|stop| format!("{stop:?}"))?;
        }
        if late {
            generate_late(&mut game, &out).map_err(|stop| format!("{stop:?}"))?;
        }
        Ok(())
    })
}

fn generate_short(game: &mut Game, out: &Path) -> Played<()> {
    let reg = out.join("test-regular-1444.eu4");
    let utf8 = out.join("test-中文-1444.eu4");

    let _: Session = game.ask(&MenuRequest::Start { country: Some(CountryTag(*b"ENG")) })?;
    let () = game.ask(&TimeRequest::Pause {})?;
    let hash = write_compressed_autosave(game, &reg)?;
    println!("generated {} sha256={hash}", reg.display());
    game.return_to_menu()?;

    let _: Session = game.ask(&MenuRequest::Start { country: Some(CountryTag(*b"FRA")) })?;
    let () = game.ask(&TimeRequest::Pause {})?;
    let hash = write_compressed_autosave(game, &utf8)?;
    println!("generated {} sha256={hash}", utf8.display());
    game.return_to_menu()?;

    println!("note: test-ironman-1444.eu4 is created once in a clean ironman claim and distributed with the test-saves release");
    Ok(())
}

fn resume_index(milestones: &[(i32, u32, u32, &str)], out: &Path) -> Option<usize> {
    milestones.iter().enumerate().rev().find_map(|(i, (_, _, _, filename))| is_complete_zip(&out.join(filename)).then_some(i))
}

fn generate_late(game: &mut Game, out: &Path) -> Played<()> {
    const MILESTONES: [(i32, u32, u32, &str); 6] = [
        (1600, 1, 1, "observer-1600.1.1.eu4"),
        (1700, 1, 1, "observer-1700.1.1.eu4"),
        (1760, 7, 1, "observer-1760.7.1.eu4"),
        (1780, 7, 1, "observer-1780.7.1.eu4"),
        (1800, 7, 1, "observer-1800.7.1.eu4"),
        (1820, 7, 1, "observer-1820.7.1.eu4"),
    ];
    let (mut session, remaining_milestones) = match resume_index(&MILESTONES, out) {
        Some(i) if i + 1 == MILESTONES.len() => {
            println!("all late observer saves already exist in {}", out.display());
            return Ok(());
        }
        Some(i) => {
            let resume_file = MILESTONES[i].3;
            let resume_path = out.join(resume_file);
            println!("resuming late-save generation from {resume_file}");
            game.loads(vec![resume_path.display().to_string()]);
            game.next_scenario();
            let _: Session = game.ask(&MenuRequest::Load { save: save_name(resume_file) })?;
            (game.observe_with_no_country()?, &MILESTONES[i + 1..])
        }
        None => {
            game.loads(Vec::new());
            game.next_scenario();
            let _: Session = game.ask(&MenuRequest::Start { country: None })?;
            (game.observe_with_no_country()?, &MILESTONES[..])
        }
    };
    let mut last_load = session.date;
    let mut steps_since_log = 0u32;

    for &(y, m, d, filename) in remaining_milestones {
        let target = Date::from_ymd(y, m, d).expect("valid milestone date");
        while days_between(session.date, target) > 0 {
            let remaining = days_between(session.date, target) as u32;
            let step = remaining.min(365);
            let step_start = Instant::now();
            let date = game.run(step)?;
            steps_since_log += 1;
            if steps_since_log >= 10 || days_between(date, target) <= 0 {
                let actual_days = days_between(session.date, date).max(1) as f64;
                let mins = (step_start.elapsed().as_secs_f64() / 60.0).max(1e-6);
                let dpm = actual_days / mins;
                println!("at {date} (next {target}, {dpm:.0} days/min)");
                steps_since_log = 0;
            }
            if days_between(last_load, date) >= 20 * 365 && days_between(date, target) > 0 {
                game.autosave_and_load()?;
                session = game.observe_with_no_country()?;
                last_load = session.date;
            } else {
                session = game.session()?;
            }
        }
        session = game.observe_with_no_country()?;
        game.check(session.date == target, format!("expected {target}, reached {}", session.date))?;
        let dest = out.join(filename);
        let hash = write_compressed_autosave(game, &dest)?;
        let mb = fs::metadata(&dest).map_or(0.0, |m| m.len() as f64 / 1_048_576.0);
        println!("generated {filename} at {target} ({mb:.1} MB) sha256={hash}");
    }
    game.return_to_menu()
}

/// Runs `autosave` on a clean claim (which uses `compress_autosave=yes`),
/// waits until a complete compressed ZIP save is written on disk, atomically
/// moves it to `dest`, and returns its SHA-256 hex digest.
fn write_compressed_autosave(game: &mut Game, dest: &Path) -> Played<String> {
    let autosave = game.user()?.save_games().join("autosave.eu4");
    if autosave.exists() {
        fs::remove_file(&autosave).map_err(|e| Stop::Failed(format!("cannot remove {}: {e}", autosave.display())))?;
    }
    game.console("autosave")?;
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut stable_bytes = None;
    loop {
        if let Ok(saves) = game.ask::<Vec<Save>>(&MenuRequest::Saves {}) {
            if let Some(entry) = saves.iter().find(|s| s.name == "autosave") {
                if stable_bytes == Some(entry.bytes) && is_complete_zip(&autosave) {
                    break;
                }
                stable_bytes = Some(entry.bytes);
            }
        }
        if Instant::now() > deadline {
            return Err(Stop::Hung("the compressed autosave was not written in 180 s".into()));
        }
        let _: Session = game.session()?;
        std::thread::sleep(Duration::from_secs(1));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| Stop::Failed(format!("cannot make {}: {e}", parent.display())))?;
    }
    let part = dest.with_extension("eu4.part");
    let _ = fs::remove_file(&part);
    fs::copy(&autosave, &part).map_err(|e| {
        let _ = fs::remove_file(&part);
        Stop::Failed(format!("cannot copy {} to {}: {e}", autosave.display(), part.display()))
    })?;
    if dest.exists() {
        fs::remove_file(dest).map_err(|e| {
            let _ = fs::remove_file(&part);
            Stop::Failed(format!("cannot replace {}: {e}", dest.display()))
        })?;
    }
    fs::rename(&part, dest).map_err(|e| {
        let _ = fs::remove_file(&part);
        Stop::Failed(format!("cannot move {} to {}: {e}", part.display(), dest.display()))
    })?;
    hash_file(dest).map_err(Stop::Failed)
}

fn is_complete_zip(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else { return false };
    let Ok(len) = file.metadata().map(|m| m.len()) else { return false };
    if len < 22 {
        return false;
    }
    let mut header = [0u8; 4];
    if file.read_exact(&mut header).is_err() || &header != b"PK\x03\x04" {
        return false;
    }
    let tail_len = len.min(65558) as usize;
    use std::io::Seek as _;
    if file.seek(std::io::SeekFrom::End(-(tail_len as i64))).is_err() {
        return false;
    }
    let mut tail = vec![0u8; tail_len];
    if file.read_exact(&mut tail).is_err() {
        return false;
    }
    tail.windows(4).any(|w| w == b"PK\x05\x06")
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finish_hex())
}

/// Minimal pure-Rust FIPS 180-4 SHA-256 implementation.
pub struct Sha256 {
    state: [u32; 8],
    len: u64,
    buf: [u8; 64],
    used: usize,
}

impl Sha256 {
    pub fn new() -> Self {
        Self { state: [0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19], len: 0, buf: [0; 64], used: 0 }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        if self.used > 0 {
            let take = data.len().min(64 - self.used);
            self.buf[self.used..self.used + take].copy_from_slice(&data[..take]);
            self.used += take;
            data = &data[take..];
            if self.used == 64 {
                let block = self.buf;
                self.compress(&block);
                self.used = 0;
            }
        }
        while data.len() >= 64 {
            let (block, rest) = data.split_at(64);
            self.compress(block.try_into().expect("64 bytes"));
            data = rest;
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.used = data.len();
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.len.wrapping_mul(8);
        self.buf[self.used] = 0x80;
        self.used += 1;
        if self.used > 56 {
            self.buf[self.used..].fill(0);
            let block = self.buf;
            self.compress(&block);
            self.used = 0;
        }
        self.buf[self.used..56].fill(0);
        self.buf[56..64].copy_from_slice(&bits.to_be_bytes());
        let block = self.buf;
        self.compress(&block);
        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    pub fn finish_hex(self) -> String {
        use std::fmt::Write as _;
        let digest = self.finish();
        let mut hex = String::with_capacity(64);
        for byte in digest {
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }

    fn compress(&mut self, block: &[u8; 64]) {
        #[rustfmt::skip]
        const K: [u32; 64] = [
            0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
            0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
            0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
            0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
            0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
            0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
            0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
            0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
        ];
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().expect("4 bytes"));
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, delta) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha256_hex(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        h.finish_hex()
    }

    #[test]
    fn sha256_matches_nist_test_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        let mut h = Sha256::new();
        let chunk = [b'a'; 1000];
        for _ in 0..1000 {
            h.update(&chunk);
        }
        assert_eq!(h.finish_hex(), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    #[test]
    fn manifest_parses_and_lists_all_nine_saves() {
        let m = manifest();
        assert_eq!(m.saves.len(), 9);
        assert_eq!(m.saves.iter().filter(|s| s.kind == SaveKind::Short).count(), 3);
        assert_eq!(m.saves.iter().filter(|s| s.kind == SaveKind::Late).count(), 6);
        assert!(m.saves.iter().any(|s| s.name == "test-中文-1444" && s.asset == "test-utf8-1444.eu4"));
        assert!(m.saves.iter().all(|s| s.sha256.len() == 64 && s.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())));
    }

    #[test]
    fn save_name_handles_bare_names_extensions_and_paths() {
        assert_eq!(save_name("test-regular-1444"), "test-regular-1444");
        assert_eq!(save_name("test-中文-1444.eu4"), "test-中文-1444");
        assert_eq!(save_name(r"C:\saves\custom.EU4"), "custom");
        assert_eq!(save_name("observer-1600.1.1"), "observer-1600.1.1");
        assert_eq!(save_name("observer-1600.1.1.eu4"), "observer-1600.1.1");
    }

    #[test]
    fn resolve_save_checks_explicit_path_then_dev_saves_and_suggests_fetch() {
        let dir = std::env::temp_dir().join(format!("eu4h-saves-resolve-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let dev = dir.join("dev-saves");
        let outside = dir.join("outside");
        fs::create_dir_all(&dev).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let explicit = outside.join("custom.eu4");
        fs::write(&explicit, b"explicit").unwrap();
        let managed = dev.join("test-中文-1444.eu4");
        fs::write(&managed, b"managed").unwrap();

        assert_eq!(resolve_save(&explicit.display().to_string(), &dev).unwrap(), explicit);
        assert_eq!(resolve_save("test-中文-1444", &dev).unwrap(), managed);
        assert_eq!(resolve_save("test-中文-1444.eu4", &dev).unwrap(), managed);
        let err = resolve_save("test-regular-1444", &dev).unwrap_err();
        assert!(err.contains("cargo xtask saves fetch"), "{err}");
        let custom_err = resolve_save("nonexistent-save", &dev).unwrap_err();
        assert!(!custom_err.contains("saves fetch"), "{custom_err}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fetch_downloads_follows_redirect_skips_cached_and_checks_sha256() {
        use std::io::Read as _;
        use std::net::TcpListener;

        let dir = std::env::temp_dir().join(format!("eu4h-saves-fetch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let payload = b"clean compressed save bytes".to_vec();
        let hash = sha256_hex(&payload);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}/release");

        let manifest = Manifest {
            release: "v1".into(),
            url: base.clone(),
            saves: vec![SaveEntry {
                name: "test-regular-1444".into(),
                file: "test-regular-1444.eu4".into(),
                asset: "test-regular-1444.eu4".into(),
                kind: SaveKind::Short,
                sha256: hash.clone(),
            }],
        };

        let server_payload = payload.clone();
        let server = std::thread::spawn(move || {
            for mode in ["redirect", "target", "corrupt", "good"] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut req = [0u8; 2048];
                let _ = stream.read(&mut req);
                match mode {
                    "redirect" => {
                        let resp = format!(
                            "HTTP/1.1 302 Found\r\nLocation: http://{addr}/cdn/test-regular-1444.eu4\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                        stream.write_all(resp.as_bytes()).unwrap();
                    }
                    "target" | "good" => {
                        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", server_payload.len());
                        stream.write_all(head.as_bytes()).unwrap();
                        stream.write_all(&server_payload).unwrap();
                    }
                    "corrupt" => {
                        let bad = b"corrupted";
                        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bad.len());
                        stream.write_all(head.as_bytes()).unwrap();
                        stream.write_all(bad).unwrap();
                    }
                    _ => unreachable!(),
                }
            }
        });

        // First fetch follows 302 redirect and writes file.
        fetch_with(&manifest, &base, &dir, None, false).unwrap();
        assert_eq!(fs::read(dir.join("test-regular-1444.eu4")).unwrap(), payload);

        // Cached matching file succeeds without hitting server.
        fetch_with(&manifest, &base, &dir, None, false).unwrap();

        // Mismatched existing file is refused without --force.
        fs::write(dir.join("test-regular-1444.eu4"), b"stale").unwrap();
        let err = fetch_with(&manifest, &base, &dir, None, false).unwrap_err();
        assert!(err.contains("--force"), "{err}");

        // Corrupted download with --force deletes .part and keeps existing file intact.
        let err = fetch_with(&manifest, &base, &dir, None, true).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(!dir.join("test-regular-1444.eu4.part").exists());
        assert_eq!(fs::read(dir.join("test-regular-1444.eu4")).unwrap(), b"stale");

        // Valid download with --force replaces stale file.
        fetch_with(&manifest, &base, &dir, None, true).unwrap();
        assert_eq!(fs::read(dir.join("test-regular-1444.eu4")).unwrap(), payload);

        server.join().unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resume_index_finds_latest_complete_zip_milestone() {
        const MILESTONES: [(i32, u32, u32, &str); 3] =
            [(1600, 1, 1, "observer-1600.1.1.eu4"), (1700, 1, 1, "observer-1700.1.1.eu4"), (1760, 7, 1, "observer-1760.7.1.eu4")];
        let dir = std::env::temp_dir().join(format!("eu4h-saves-resume-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        assert_eq!(resume_index(&MILESTONES, &dir), None);

        // Truncated/invalid file does not count as a completed milestone.
        fs::write(dir.join(MILESTONES[0].3), b"not-a-zip").unwrap();
        assert_eq!(resume_index(&MILESTONES, &dir), None);

        let mut valid_zip = Vec::from(*b"PK\x03\x04");
        valid_zip.resize(26, 0);
        valid_zip.extend_from_slice(b"PK\x05\x06");
        valid_zip.resize(48, 0);

        fs::write(dir.join(MILESTONES[0].3), &valid_zip).unwrap();
        assert_eq!(resume_index(&MILESTONES, &dir), Some(0));

        // Partial newer milestone falls back to the earlier completed one.
        fs::write(dir.join(MILESTONES[1].3), b"PK\x03\x04truncated").unwrap();
        assert_eq!(resume_index(&MILESTONES, &dir), Some(0));

        fs::write(dir.join(MILESTONES[1].3), &valid_zip).unwrap();
        assert_eq!(resume_index(&MILESTONES, &dir), Some(1));

        fs::write(dir.join(MILESTONES[2].3), &valid_zip).unwrap();
        assert_eq!(resume_index(&MILESTONES, &dir), Some(2));

        let _ = fs::remove_dir_all(&dir);
    }

    fn make_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for &(name, raw, compress) in entries {
            let local_offset = out.len() as u32;
            let (method, payload): (u16, Vec<u8>) = if compress { (8, miniz_oxide::deflate::compress_to_vec(raw, 6)) } else { (0, raw.to_vec()) };
            let name_bytes = name.as_bytes();
            // Local file header (with zeroed sizes + data-descriptor bit to test CD reliance).
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0x0008u16.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(&payload);

            // Central directory entry.
            central.extend_from_slice(b"PK\x01\x02");
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0x0008u16.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            central.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }
        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);
        // End of central directory.
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn start_info_parses_top_great_power_and_capital_from_plain_and_zipped_saves() {
        let gamestate = concat!(
            "EU4txt\n",
            "date=1780.7.1\n",
            "great_powers={\n",
            "\tfrenzy=600\n",
            "\toriginal={\n",
            "\t\trank=2\n",
            "\t\tcountry=\"AUH\"\n",
            "\t\tvalue=3958.000\n",
            "\t}\n",
            "\toriginal={\n",
            "\t\trank=1\n",
            "\t\tcountry=\"TUR\"\n",
            "\t\tvalue=4505.000\n",
            "\t}\n",
            "\tleaving={\n",
            "\t\tcountry=\"PLC\"\n",
            "\t\tdate=1781.8.1\n",
            "\t}\n",
            "}\n",
            "countries={\n",
            "\tAUH={\n",
            "\t\tcapital=134\n",
            "\t}\n",
            "\tTUR={\n",
            "\t\thistory={\n",
            "\t\t\tcapital=149\n",
            "\t\t}\n",
            "\t\tcapital=151\n",
            "\t\toriginal_capital=151\n",
            "\t}\n",
            "}\n",
        );
        let plain = parse_start_info(gamestate.as_bytes(), None);
        assert_eq!(plain, SaveStartInfo { top_great_power: Some(CountryTag(*b"TUR")), capital: Some(151) });

        let explicit = parse_start_info(gamestate.as_bytes(), Some(CountryTag(*b"AUH")));
        assert_eq!(explicit, SaveStartInfo { top_great_power: Some(CountryTag(*b"TUR")), capital: Some(134) });

        let dir = std::env::temp_dir().join(format!("eu4h-saves-startinfo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let zip_path = dir.join("compressed.eu4");
        fs::write(&zip_path, make_zip(&[("meta", b"meta", false), ("gamestate", gamestate.as_bytes(), true)])).unwrap();
        assert_eq!(read_start_info(&zip_path, None).unwrap(), plain);

        let plain_path = dir.join("plain.eu4");
        fs::write(&plain_path, gamestate.as_bytes()).unwrap();
        assert_eq!(read_start_info(&plain_path, Some(CountryTag(*b"AUH"))).unwrap(), explicit);

        let _ = fs::remove_dir_all(&dir);
    }
}
