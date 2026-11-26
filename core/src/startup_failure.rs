//! Startup failure triage (cold path only).
//!
//! When [`crate::baseline::recognize`] or [`crate::points::install`] fails,
//! normal startup already aborted without hashing `eu4.exe` or touching disk.
//! On the failure thread (outside the Windows loader lock), [`diagnose`]
//! inspects `launcher-settings.json`, the on-disk `eu4.exe` SHA-256, and
//! non-stock DLLs loaded from the game folder so the player gets an actionable
//! notice instead of raw hexadecimal byte mismatches. The startup report and
//! `core.log` still keep the complete technical failure details.
use crate::base::notice::Message;
use serde::Deserialize;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

/// Which startup check failed and triggered triage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailureStage {
    /// `baseline::recognize` did not match the 1.37.5 PE section layout.
    Baseline,
    /// `baseline::recognize` matched 1.37.5, then `points::install` found a
    /// byte, vtable or import mismatch.
    Points,
}

/// SHA-256 of the unmodified Steam Windows EU4 1.37.5 `eu4.exe`.
const VANILLA_1_37_5_SHA256: [u8; 32] = [
    0x9a, 0xd3, 0xef, 0xe1, 0xaf, 0x16, 0x9f, 0x40, 0xee, 0x57, 0x7f, 0x9d, 0xae, 0x5d, 0xeb, 0xbc, 0x87, 0xaf, 0x6f, 0xb8, 0xb5, 0x45, 0x0f, 0xb3, 0x45, 0xeb,
    0xf1, 0x10, 0xdc, 0x4d, 0x77, 0x1a,
];

/// Stock DLLs shipped in the EU4 1.37.5 game root, plus Harmony's own proxy.
const STOCK_GAME_ROOT_DLLS: &[&str] =
    &["dbghelp.dll", "eossdk-win64-shipping.dll", "nakama-cpp.dll", "pops_api.dll", "steam_api64.dll", "tbb.dll", "version.dll"];

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LauncherSettings {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    raw_version: Option<String>,
    #[serde(default)]
    dist_platform: Option<String>,
}

/// A concrete cause identified from the game folder and loaded modules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Diagnosis {
    WrongPlatform { platform: String },
    WrongVersion { version: String },
    ModifiedExe,
    UnsupportedOrModifiedExe,
    ConflictingDlls { dlls: Vec<String> },
}

impl Diagnosis {
    /// Player-facing notice for the message box.
    pub(crate) fn notice(&self) -> Message {
        match self {
            Self::WrongPlatform { platform } => Message::new("EU4HARMONY_STARTUP_WRONG_PLATFORM").arg("PLATFORM", platform),
            Self::WrongVersion { version } => Message::new("EU4HARMONY_STARTUP_WRONG_VERSION").arg("VERSION", version),
            Self::ModifiedExe => Message::new("EU4HARMONY_STARTUP_MODIFIED_EXE"),
            Self::UnsupportedOrModifiedExe => Message::new("EU4HARMONY_STARTUP_UNSUPPORTED_OR_MODIFIED_EXE"),
            Self::ConflictingDlls { dlls } => Message::new("EU4HARMONY_STARTUP_CONFLICTING_DLLS").arg("DLLS", dlls.join(", ")),
        }
    }

    /// Short English summary for `core.log` and `report.txt`.
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::WrongPlatform { platform } => format!("non-Steam distribution platform ({platform})"),
            Self::WrongVersion { version } => format!("unsupported game version ({version}; requires 1.37.5)"),
            Self::ModifiedExe => "eu4.exe on disk does not match unmodified Steam Windows EU4 1.37.5".into(),
            Self::UnsupportedOrModifiedExe => "eu4.exe does not match unmodified Steam Windows EU4 1.37.5 (version unconfirmed)".into(),
            Self::ConflictingDlls { dlls } => format!("non-stock DLL(s) loaded from the game folder: {}", dlls.join(", ")),
        }
    }
}

/// Evidence collected on the failure thread for [`classify`].
#[derive(Debug, Default)]
struct Evidence {
    settings: Option<LauncherSettings>,
    /// `Some(true)` when on-disk `eu4.exe` matches `VANILLA_1_37_5_SHA256`,
    /// `Some(false)` when it was read and differs, `None` when unreadable.
    exe_matches_vanilla: Option<bool>,
    /// Non-stock DLLs loaded in the process from inside `game_dir` (excluding
    /// `eu4-harmony\` and Harmony's own proxy files).
    conflicting_dlls: Vec<String>,
}

/// Diagnoses a `baseline` or `points` startup failure for `game_dir`.
///
/// Must never be called under the Windows loader lock.
pub(crate) fn diagnose(game_dir: &Path, stage: FailureStage) -> Option<Diagnosis> {
    let settings = read_launcher_settings(game_dir);
    let exe_path = current_exe().unwrap_or_else(|| game_dir.join("eu4.exe"));
    let exe_matches_vanilla = sha256_file(&exe_path).ok().map(|digest| digest == VANILLA_1_37_5_SHA256);
    let conflicting_dlls = find_conflicting_dlls(game_dir, &loaded_module_paths());
    classify(stage, &Evidence { settings, exe_matches_vanilla, conflicting_dlls })
}

fn classify(stage: FailureStage, evidence: &Evidence) -> Option<Diagnosis> {
    if let Some(settings) = &evidence.settings {
        if let Some(platform) = settings.dist_platform.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            if !platform.eq_ignore_ascii_case("steam") {
                return Some(Diagnosis::WrongPlatform { platform: platform.to_string() });
            }
        }
        if let Some(version) = reported_version(settings) {
            if !is_1_37_5(version) {
                return Some(Diagnosis::WrongVersion { version: version.to_string() });
            }
        }
    }

    match evidence.exe_matches_vanilla {
        Some(false) => {
            let confirmed_1_37_5 = stage == FailureStage::Points
                || evidence.settings.as_ref().is_some_and(|s| {
                    s.dist_platform.as_deref().is_none_or(|p| p.trim().eq_ignore_ascii_case("steam")) && reported_version(s).is_some_and(is_1_37_5)
                });
            if confirmed_1_37_5 {
                return Some(Diagnosis::ModifiedExe);
            }
            return Some(Diagnosis::UnsupportedOrModifiedExe);
        }
        Some(true) => {
            if !evidence.conflicting_dlls.is_empty() {
                return Some(Diagnosis::ConflictingDlls { dlls: evidence.conflicting_dlls.clone() });
            }
        }
        None => {
            if stage == FailureStage::Points && !evidence.conflicting_dlls.is_empty() {
                return Some(Diagnosis::ConflictingDlls { dlls: evidence.conflicting_dlls.clone() });
            }
        }
    }

    None
}

fn reported_version(settings: &LauncherSettings) -> Option<&str> {
    settings.raw_version.as_deref().or(settings.version.as_deref()).map(str::trim).filter(|v| !v.is_empty())
}

fn is_1_37_5(version: &str) -> bool {
    let trimmed = version.trim();
    let bare = trimmed.strip_prefix('v').or_else(|| trimmed.strip_prefix('V')).unwrap_or(trimmed);
    if bare == "1.37.5" || bare == "1.37.5.0" {
        return true;
    }
    trimmed.split_whitespace().any(|token| {
        let token = token.strip_prefix('v').or_else(|| token.strip_prefix('V')).unwrap_or(token);
        token == "1.37.5" || token == "1.37.5.0"
    })
}

fn read_launcher_settings(game_dir: &Path) -> Option<LauncherSettings> {
    let text = std::fs::read_to_string(game_dir.join("launcher-settings.json")).ok()?;
    let text = text.trim_start_matches('\u{feff}');
    serde_json::from_str(text).ok()
}

/// Returns relative display names of non-stock DLLs loaded from inside
/// `game_dir`, excluding Harmony's own `eu4-harmony\` subfolder and retired/
/// staged proxy files.
fn find_conflicting_dlls(game_dir: &Path, modules: &[PathBuf]) -> Vec<String> {
    let root = normalize_path(game_dir);
    let prefix = format!("{root}\\");
    let mut found = Vec::new();
    for module in modules {
        let norm = normalize_path(module);
        let Some(rel_lower) = norm.strip_prefix(&prefix) else { continue };
        if !rel_lower.ends_with(".dll") {
            continue;
        }
        // Harmony's own manual/developer core and crash components live in `eu4-harmony\`.
        if rel_lower.starts_with(&format!("{}\\", eu4_harmony_folders::HARMONY)) {
            continue;
        }
        if !rel_lower.contains('\\')
            && (STOCK_GAME_ROOT_DLLS.contains(&rel_lower)
                || rel_lower.starts_with(eu4_harmony_folders::OLD_PROXY_PREFIX)
                || rel_lower.starts_with(eu4_harmony_folders::NEW_PROXY_PREFIX))
        {
            continue;
        }
        // Preserve the original casing of the relative path for readability.
        let display = relative_preserving_case(module, rel_lower);
        if !found.iter().any(|existing: &String| existing.eq_ignore_ascii_case(&display)) {
            found.push(display);
        }
    }
    found.sort_by_key(|name| name.to_ascii_lowercase());
    found
}

fn relative_preserving_case(module: &Path, rel_lower: &str) -> String {
    let raw = module.to_string_lossy();
    let raw = raw.strip_prefix(r"\\?\").unwrap_or(&raw).replace('/', "\\");
    let components = rel_lower.matches('\\').count() + 1;
    let parts: Vec<&str> = raw.rsplit('\\').take(components).collect();
    if parts.len() == components {
        parts.into_iter().rev().collect::<Vec<_>>().join("\\")
    } else {
        rel_lower.to_string()
    }
}

fn normalize_path(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.trim_end_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase()
}

fn current_exe() -> Option<PathBuf> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    (n > 0 && n < buf.len()).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

fn loaded_module_paths() -> Vec<PathBuf> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE},
    };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return Vec::new();
    }
    let mut entry: MODULEENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
    let mut paths = Vec::new();
    if unsafe { Module32FirstW(snapshot, &mut entry) } != 0 {
        loop {
            let len = entry.szExePath.iter().position(|&c| c == 0).unwrap_or(entry.szExePath.len());
            if len > 0 {
                paths.push(PathBuf::from(String::from_utf16_lossy(&entry.szExePath[..len])));
            }
            if unsafe { Module32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }
    }
    unsafe { CloseHandle(snapshot) };
    paths
}

fn sha256_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finish())
}

/// Minimal FIPS 180-4 SHA-256 implementation so cold-path startup triage needs
/// no extra crate or FFI dependency.
struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    block_len: usize,
    total_len: u64,
}

impl Sha256 {
    const H0: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    fn new() -> Self {
        Self { state: Self::H0, block: [0; 64], block_len: 0, total_len: 0 }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total_len += data.len() as u64;
        if self.block_len > 0 {
            let take = (64 - self.block_len).min(data.len());
            self.block[self.block_len..self.block_len + take].copy_from_slice(&data[..take]);
            self.block_len += take;
            data = &data[take..];
            if self.block_len == 64 {
                let block = self.block;
                self.compress(&block);
                self.block_len = 0;
            }
        }
        for chunk in data.chunks_exact(64) {
            self.compress(chunk.try_into().expect("64-byte chunk"));
        }
        let rem = data.chunks_exact(64).remainder();
        if !rem.is_empty() {
            self.block[..rem.len()].copy_from_slice(rem);
            self.block_len = rem.len();
        }
    }

    fn finish(mut self) -> [u8; 32] {
        let bits = self.total_len * 8;
        self.block[self.block_len] = 0x80;
        self.block_len += 1;
        if self.block_len > 56 {
            self.block[self.block_len..64].fill(0);
            let block = self.block;
            self.compress(&block);
            self.block_len = 0;
        }
        self.block[self.block_len..56].fill(0);
        self.block[56..64].copy_from_slice(&bits.to_be_bytes());
        let block = self.block;
        self.compress(&block);
        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[4 * i..4 * i + 4].try_into().expect("4-byte word"));
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for (&k, &word) in Self::K.iter().zip(&w) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(k).wrapping_add(word);
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

    fn hex(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn settings(raw_version: Option<&str>, dist_platform: Option<&str>) -> Option<LauncherSettings> {
        Some(LauncherSettings { version: None, raw_version: raw_version.map(str::to_string), dist_platform: dist_platform.map(str::to_string) })
    }

    #[test]
    fn sha256_matches_standard_test_vectors() {
        for (input, expected) in [
            (b"".as_slice(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            (b"abc".as_slice(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
            (b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".as_slice(), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"),
        ] {
            let mut hasher = Sha256::new();
            hasher.update(input);
            assert_eq!(hex(hasher.finish()), expected);
        }
    }

    #[test]
    fn classifies_platform_version_modified_exe_and_conflicting_dlls() {
        // Non-Steam platform wins first.
        assert_eq!(
            classify(
                FailureStage::Baseline,
                &Evidence { settings: settings(Some("v1.37.5.0"), Some("epic")), exe_matches_vanilla: Some(false), conflicting_dlls: vec!["d3d9.dll".into()] }
            ),
            Some(Diagnosis::WrongPlatform { platform: "epic".into() })
        );

        // Wrong version on Steam.
        assert_eq!(
            classify(
                FailureStage::Baseline,
                &Evidence { settings: settings(Some("v1.36.2.0"), Some("steam")), exe_matches_vanilla: Some(false), conflicting_dlls: Vec::new() }
            ),
            Some(Diagnosis::WrongVersion { version: "v1.36.2.0".into() })
        );

        // Modified 1.37.5 exe (points failed with matching baseline, even if launcher-settings is missing).
        assert_eq!(
            classify(FailureStage::Points, &Evidence { settings: None, exe_matches_vanilla: Some(false), conflicting_dlls: vec!["d3d9.dll".into()] }),
            Some(Diagnosis::ModifiedExe)
        );

        // Modified 1.37.5 exe whose sections also changed, confirmed via launcher-settings.json.
        assert_eq!(
            classify(
                FailureStage::Baseline,
                &Evidence { settings: settings(Some("v1.37.5.0"), Some("steam")), exe_matches_vanilla: Some(false), conflicting_dlls: Vec::new() }
            ),
            Some(Diagnosis::ModifiedExe)
        );

        // Unconfirmed baseline failure without launcher-settings.json.
        assert_eq!(
            classify(FailureStage::Baseline, &Evidence { settings: None, exe_matches_vanilla: Some(false), conflicting_dlls: Vec::new() }),
            Some(Diagnosis::UnsupportedOrModifiedExe)
        );

        // Vanilla 1.37.5 exe with a third-party DLL loaded from the game folder.
        assert_eq!(
            classify(
                FailureStage::Points,
                &Evidence {
                    settings: settings(Some("v1.37.5.0"), Some("steam")),
                    exe_matches_vanilla: Some(true),
                    conflicting_dlls: vec!["d3d9.dll".into(), "winmm.dll".into()],
                }
            ),
            Some(Diagnosis::ConflictingDlls { dlls: vec!["d3d9.dll".into(), "winmm.dll".into()] })
        );

        // Vanilla 1.37.5 exe with no third-party game-folder DLL falls back to generic failure.
        assert_eq!(
            classify(
                FailureStage::Points,
                &Evidence { settings: settings(Some("v1.37.5.0"), Some("steam")), exe_matches_vanilla: Some(true), conflicting_dlls: Vec::new() }
            ),
            None
        );
    }

    #[test]
    fn filters_stock_harmony_and_system_dlls_when_finding_conflicts() {
        let game = Path::new(r"E:\SteamLibrary\steamapps\common\Europa Universalis IV");
        let modules = vec![
            game.join("eu4.exe"),
            game.join("version.dll"),
            game.join("version.dll.old-123"),
            game.join("version.dll.new-456"),
            game.join("steam_api64.dll"),
            game.join("pops_api.dll"),
            game.join("EOSSDK-Win64-Shipping.dll"),
            game.join("eu4-harmony").join("eu4_harmony_core.dll"),
            game.join("d3d9.dll"),
            game.join("plugins").join("Patch64.dll"),
            PathBuf::from(r"C:\Windows\System32\d3d9.dll"),
        ];
        assert_eq!(find_conflicting_dlls(game, &modules), vec!["d3d9.dll".to_string(), r"plugins\Patch64.dll".to_string()]);
    }

    #[test]
    fn reads_launcher_settings_with_or_without_bom() {
        let dir = std::env::temp_dir().join(format!("eu4h-triage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("launcher-settings.json"), "\u{feff}{\"rawVersion\":\"v1.37.5.0\",\"distPlatform\":\"steam\"}").unwrap();
        let read = read_launcher_settings(&dir).unwrap();
        assert_eq!(reported_version(&read), Some("v1.37.5.0"));
        assert!(is_1_37_5("v1.37.5.0") && is_1_37_5("EU4 v1.37.5.0 Inca (491d)") && !is_1_37_5("v1.37.4.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
