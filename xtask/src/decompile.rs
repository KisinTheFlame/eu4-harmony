//! Persistent Ghidra headless decompilation (`cargo xtask decompile`).
mod bridge;
mod config;
mod format;

use crate::{addresses, disasm, Result};
use bridge::DecompileTarget;
use eu4_harmony_analysis as analysis;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const DEFAULT_TIMEOUT_SECONDS: u32 = 60;

enum Mode {
    Init { rebuild: bool },
    Status,
    Stop { module: Option<String> },
    Query { rvas: Vec<u64>, oneshot: bool, timeout_seconds: u32 },
}

struct ParsedDecompile {
    mode: Mode,
    module: String,
    game_dir: Option<PathBuf>,
}

pub fn run(raw: &[String]) -> Result<()> {
    let parsed = parse_args(raw)?;
    let dev_dir = crate::dev_dir_path()?;

    match parsed.mode {
        Mode::Status => return bridge::print_status(&dev_dir),
        Mode::Stop { module } => return bridge::stop_daemons(&dev_dir, module.as_deref()),
        Mode::Init { .. } | Mode::Query { .. } => {}
    }

    let toolchain = config::resolve_toolchain(&dev_dir)?;
    let dir = eu4_harmony_folders::game_dir(parsed.game_dir.as_deref())?;
    let module_path = dir.join(&parsed.module);
    if !module_path.is_file() {
        return Err(format!("cannot find {}", module_path.display()));
    }

    let sha256 = crate::saves::hash_file(&module_path)?;
    let layout = config::project_layout(&dev_dir, &parsed.module, &sha256);

    match parsed.mode {
        Mode::Init { rebuild } => {
            if !rebuild {
                if let Some(ready) = layout.read_ready() {
                    println!("already initialized: {} ({}) at {}", ready.module, &ready.sha256[..16.min(ready.sha256.len())], layout.root.display());
                    return Ok(());
                }
            }
            let (image_base, image_size, _) = load_pe_info(&module_path)?;
            println!("initializing persistent Ghidra project for {} ({})...", parsed.module, &sha256[..16.min(sha256.len())]);
            println!("cache: {}", layout.root.display());
            println!("heap: {}", toolchain.max_mem);
            let marker = bridge::init_project(&toolchain, &layout, &module_path, image_base, image_size, rebuild)?;
            println!("ready: {} ({}) at {}", marker.module, &marker.sha256[..16.min(marker.sha256.len())], layout.root.display());
            Ok(())
        }
        Mode::Query { rvas, oneshot, timeout_seconds } => {
            let ready = layout.read_ready().ok_or_else(|| {
                let module_flag = if parsed.module.eq_ignore_ascii_case("eu4.exe") { String::new() } else { format!(" --module {}", parsed.module) };
                format!(
                    "no analyzed Ghidra project for {} ({}); run `cargo xtask decompile --init{module_flag}` first",
                    parsed.module,
                    &sha256[..16.min(sha256.len())]
                )
            })?;

            let (_, _, image) = load_pe_info(&module_path)?;
            let targets: Vec<DecompileTarget> =
                rvas.iter().map(|&rva| DecompileTarget { rva, pdata_start: containing_pdata_start(&image, rva as usize).map(|s| s as u64) }).collect();

            let (image_base, results) = if oneshot {
                bridge::decompile_oneshot(&toolchain, &layout, &targets, timeout_seconds)?
            } else {
                bridge::decompile_warm(&toolchain, &layout, &targets, timeout_seconds)?
            };

            let index = if parsed.module.eq_ignore_ascii_case("eu4.exe") { addresses::load(&crate::repo()).unwrap_or_default() } else { BTreeMap::new() };

            let mut any_failed = false;
            for (i, item) in results.iter().enumerate() {
                if !item.ok {
                    any_failed = true;
                }
                if i > 0 {
                    println!();
                }
                print!("{}", format::format_item(&parsed.module, item, image_base, ready.image_size, &index));
            }
            if any_failed {
                Err("one or more functions failed to decompile".into())
            } else {
                Ok(())
            }
        }
        Mode::Status | Mode::Stop { .. } => unreachable!(),
    }
}

fn load_pe_info(path: &Path) -> Result<(u64, u64, Vec<u8>)> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if bytes.len() < 0x100 || &bytes[..2] != b"MZ" {
        return Err(format!("{} is not a PE image", path.display()));
    }
    let image = analysis::map_file(&bytes);
    let image_base = analysis::preferred_base(&image);
    let image_size = image.len() as u64;
    Ok((image_base, image_size, image))
}

/// Finds the `.pdata` function start containing `rva`, walking chained
/// `UNW_FLAG_CHAININFO` entries back to the function's entry when split.
fn containing_pdata_start(image: &[u8], rva: usize) -> Option<usize> {
    if analysis::bounds(image, rva).is_some() {
        return Some(rva);
    }
    let mut best = None;
    for range in analysis::functions(image) {
        if range.contains(&rva) {
            best = Some(range.start);
            break;
        }
    }
    let mut start = best?;
    // Walk backwards if this runtime function is chained from an earlier part.
    const CHAINED: u8 = 4;
    let pe = analysis::pe(image);
    let dir = pe + 24 + 112 + 8 * 3;
    let pdata_start = u32_at(image, dir);
    let pdata_size = u32_at(image, dir + 4);
    if pdata_start == 0 || pdata_size < 12 {
        return Some(start);
    }
    let pdata_end = pdata_start.saturating_add(pdata_size).min(image.len());
    let mut raw_entries: Vec<(usize, usize, usize)> = (pdata_start..pdata_end)
        .step_by(12)
        .take_while(|&e| e + 12 <= pdata_end)
        .map(|e| (u32_at(image, e), u32_at(image, e + 4), u32_at(image, e + 8)))
        .collect();
    raw_entries.sort_unstable();
    let mut idx = raw_entries.partition_point(|e| e.0 <= start).checked_sub(1)?;
    while let Some(&(begin, _, unwind)) = raw_entries.get(idx) {
        start = begin;
        if image.get(unwind).is_none_or(|&b| (b >> 3) & CHAINED == 0) {
            break;
        }
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    Some(start)
}

fn u32_at(image: &[u8], at: usize) -> usize {
    u32::from_le_bytes(image[at..at + 4].try_into().unwrap()) as usize
}

fn parse_rva(raw: &str) -> Result<u64> {
    let hex = raw.trim().strip_prefix("0x").or_else(|| raw.trim().strip_prefix("0X")).unwrap_or(raw.trim());
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("invalid RVA {raw:?} (expected hex such as 0x8fa80)"));
    }
    u64::from_str_radix(hex, 16).map_err(|_| format!("invalid RVA {raw:?}"))
}

fn parse_args(raw: &[String]) -> Result<ParsedDecompile> {
    let mut init = false;
    let mut rebuild = false;
    let mut status = false;
    let mut stop = false;
    let mut oneshot = false;
    let mut module: Option<String> = None;
    let mut game_dir: Option<PathBuf> = None;
    let mut timeout_seconds = DEFAULT_TIMEOUT_SECONDS;
    let mut rvas = Vec::new();

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--init" => {
                init = true;
                i += 1;
            }
            "--rebuild" => {
                rebuild = true;
                i += 1;
            }
            "--status" => {
                status = true;
                i += 1;
            }
            "--stop" => {
                stop = true;
                i += 1;
            }
            "--oneshot" => {
                oneshot = true;
                i += 1;
            }
            "--module" => {
                let value = raw.get(i + 1).ok_or("--module needs a file name")?;
                module = Some(disasm::validate_module_name(value)?.to_string());
                i += 2;
            }
            "--game-dir" => {
                let value = raw.get(i + 1).ok_or("--game-dir needs a folder path")?;
                game_dir = Some(PathBuf::from(value));
                i += 2;
            }
            "--timeout" => {
                let value = raw.get(i + 1).ok_or("--timeout needs seconds")?;
                let secs: u32 = value.parse().map_err(|_| format!("--timeout takes positive whole seconds; {value:?} is not one"))?;
                if secs == 0 {
                    return Err("--timeout must be at least 1 second".into());
                }
                timeout_seconds = secs;
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option {other}; cargo xtask --help lists them"));
            }
            positional => {
                rvas.push(parse_rva(positional)?);
                i += 1;
            }
        }
    }

    let action_flags = u8::from(init || rebuild) + u8::from(status) + u8::from(stop);
    if action_flags > 1 {
        return Err("--init/--rebuild, --status, and --stop cannot be combined".into());
    }
    if status {
        if !rvas.is_empty() || module.is_some() || oneshot {
            return Err("--status takes no RVA, --module, or --oneshot".into());
        }
        return Ok(ParsedDecompile { mode: Mode::Status, module: String::from("eu4.exe"), game_dir });
    }
    if stop {
        if !rvas.is_empty() || oneshot {
            return Err("--stop takes no RVA or --oneshot".into());
        }
        return Ok(ParsedDecompile { mode: Mode::Stop { module }, module: String::from("eu4.exe"), game_dir });
    }
    if init || rebuild {
        if !rvas.is_empty() || oneshot {
            return Err("--init/--rebuild takes no RVA or --oneshot".into());
        }
        return Ok(ParsedDecompile { mode: Mode::Init { rebuild }, module: module.unwrap_or_else(|| String::from("eu4.exe")), game_dir });
    }
    if rvas.is_empty() {
        return Err("decompile needs one or more RVAs, or --init / --status / --stop".into());
    }
    Ok(ParsedDecompile { mode: Mode::Query { rvas, oneshot, timeout_seconds }, module: module.unwrap_or_else(|| String::from("eu4.exe")), game_dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modes_and_rejects_conflicting_flags() {
        let s = |vals: &[&str]| vals.iter().map(|v| (*v).to_string()).collect::<Vec<_>>();
        assert!(matches!(parse_args(&s(&["--init"])).unwrap().mode, Mode::Init { rebuild: false }));
        assert!(matches!(parse_args(&s(&["--rebuild", "--module", "pops_api.dll"])).unwrap().mode, Mode::Init { rebuild: true }));
        assert!(matches!(parse_args(&s(&["--status"])).unwrap().mode, Mode::Status));
        assert!(matches!(parse_args(&s(&["--stop"])).unwrap().mode, Mode::Stop { module: None }));
        assert!(matches!(parse_args(&s(&["--stop", "--module", "pops_api.dll"])).unwrap().mode, Mode::Stop { module: Some(_) }));
        match parse_args(&s(&["0x8fa80", "8f450", "--oneshot", "--timeout", "90"])).unwrap().mode {
            Mode::Query { rvas, oneshot, timeout_seconds } => {
                assert_eq!(rvas, [0x8fa80, 0x8f450]);
                assert!(oneshot);
                assert_eq!(timeout_seconds, 90);
            }
            _ => panic!("expected query mode"),
        }
        assert!(parse_args(&s(&[])).is_err());
        assert!(parse_args(&s(&["--init", "--status"])).is_err());
        assert!(parse_args(&s(&["--init", "0x8fa80"])).is_err());
        assert!(parse_args(&s(&["--timeout", "0", "0x8fa80"])).is_err());
    }
}
