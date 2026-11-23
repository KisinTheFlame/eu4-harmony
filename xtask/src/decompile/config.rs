//! Configuration, toolchain discovery, and cache layout for persistent Ghidra
//! projects (`cargo xtask decompile`).
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const READY_VERSION: u32 = 1;

/// User-optional settings in `<dev_dir>\ghidra.toml`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GhidraToml {
    pub ghidra_dir: Option<PathBuf>,
    pub java_home: Option<PathBuf>,
    pub max_mem: Option<String>,
}

/// Resolved Ghidra headless toolchain and JVM settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolchain {
    pub ghidra_dir: PathBuf,
    pub analyze_headless: PathBuf,
    pub java_home: Option<PathBuf>,
    pub max_mem: String,
}

/// Cache paths for one analyzed binary `(module, sha256)`.
#[derive(Debug, Clone)]
pub struct ProjectLayout {
    pub key: String,
    pub module_name: String,
    pub sha256: String,
    pub root: PathBuf,
    pub ghidra_project_dir: PathBuf,
    pub ready_path: PathBuf,
    pub init_log: PathBuf,
    pub script_log: PathBuf,
    pub server_path: PathBuf,
    pub daemon_log: PathBuf,
    pub daemon_ghidra_log: PathBuf,
    pub daemon_config_path: PathBuf,
    pub scripts_dir: PathBuf,
    pub bridge_script_path: PathBuf,
}

/// Written atomically to `ready.json` only after full headless import + analysis exits 0.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadyMarker {
    pub version: u32,
    pub module: String,
    pub sha256: String,
    pub image_base: u64,
    pub image_size: u64,
    pub completed_at_unix: u64,
}

/// Active warm-server descriptor written by `HarmonyDecompileBridge.java`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerInfo {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub image_base_hex: String,
    pub program: String,
    pub started_at_unix: u64,
}

pub fn ghidra_root(dev_dir: &Path) -> PathBuf {
    dev_dir.join("ghidra")
}

pub fn projects_dir(dev_dir: &Path) -> PathBuf {
    ghidra_root(dev_dir).join("projects")
}

pub fn scripts_dir(dev_dir: &Path) -> PathBuf {
    ghidra_root(dev_dir).join("scripts")
}

pub fn config_path(dev_dir: &Path) -> PathBuf {
    dev_dir.join("ghidra.toml")
}

pub fn load_ghidra_toml(path: &Path) -> Result<GhidraToml> {
    if !path.exists() {
        return Ok(GhidraToml::default());
    }
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    parse_ghidra_toml(path, &text)
}

fn parse_ghidra_toml(path: &Path, text: &str) -> Result<GhidraToml> {
    let table: toml::Table = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = GhidraToml::default();
    for (key, value) in table {
        let s = value.as_str().ok_or_else(|| format!("{}: {key} must be a string", path.display()))?;
        let trimmed = s.trim();
        let non_empty = (!trimmed.is_empty()).then(|| trimmed.to_string());
        match key.as_str() {
            "ghidra_dir" => out.ghidra_dir = non_empty.map(PathBuf::from),
            "java_home" => out.java_home = non_empty.map(PathBuf::from),
            "max_mem" => out.max_mem = non_empty,
            other => return Err(format!("{}: unknown option {other:?} (expected ghidra_dir, java_home, max_mem)", path.display())),
        }
    }
    Ok(out)
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn find_on_path(file_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Resolves Ghidra, JDK, and JVM max heap memory using standard locations:
/// environment variables -> `<dev_dir>\ghidra.toml` -> `PATH`.
pub fn resolve_toolchain(dev_dir: &Path) -> Result<Toolchain> {
    let cfg_path = config_path(dev_dir);
    let cfg = load_ghidra_toml(&cfg_path)?;

    let ghidra_dir = if let Some(dir) = non_empty_env("GHIDRA_INSTALL_DIR") {
        PathBuf::from(dir)
    } else if let Some(dir) = cfg.ghidra_dir {
        dir
    } else if let Some(bat) = find_on_path("analyzeHeadless.bat") {
        bat.parent().and_then(Path::parent).ok_or_else(|| format!("cannot determine Ghidra install root from {}", bat.display()))?.to_path_buf()
    } else {
        return Err(missing_ghidra_error(&cfg_path));
    };

    let analyze_headless = ghidra_dir.join("support").join("analyzeHeadless.bat");
    if !analyze_headless.is_file() {
        return Err(format!("cannot find {} (check GHIDRA_INSTALL_DIR or ghidra_dir in {})", analyze_headless.display(), cfg_path.display()));
    }

    let java_home = if let Some(dir) = non_empty_env("JAVA_HOME") {
        Some(PathBuf::from(dir))
    } else if let Some(dir) = cfg.java_home {
        Some(dir)
    } else {
        find_on_path("java.exe").and_then(|exe| exe.parent().and_then(Path::parent).map(Path::to_path_buf))
    };

    if let Some(ref home) = java_home {
        let java_exe = home.join("bin").join("java.exe");
        if !java_exe.is_file() {
            return Err(format!("cannot find {} (check JAVA_HOME or java_home in {})", java_exe.display(), cfg_path.display()));
        }
    }

    let max_mem = resolve_max_mem(cfg.max_mem.as_deref());
    Ok(Toolchain { ghidra_dir, analyze_headless, java_home, max_mem })
}

fn missing_ghidra_error(cfg_path: &Path) -> String {
    format!(
        "Ghidra was not found. Configure it using one of:\n\
         1. Environment variables GHIDRA_INSTALL_DIR (and JAVA_HOME if needed) or add support\\analyzeHeadless.bat to PATH\n\
         2. Create {} with:\n\
            ghidra_dir = 'C:\\path\\to\\ghidra_12.1.4_PUBLIC'\n\
            java_home = 'C:\\path\\to\\jdk-21'",
        cfg_path.display()
    )
}

fn resolve_max_mem(toml_max_mem: Option<&str>) -> String {
    if let Some(v) = non_empty_env("GHIDRA_HEADLESS_MAXMEM") {
        return v;
    }
    if let Some(v) = non_empty_env("GHIDRA_MAXMEM") {
        return v;
    }
    if let Some(v) = toml_max_mem {
        return v.to_string();
    }
    default_max_mem_for_ram_bytes(physical_memory_bytes())
}

fn default_max_mem_for_ram_bytes(bytes: u64) -> String {
    let total_gb = bytes / (1024 * 1024 * 1024);
    let chosen = (total_gb / 2).clamp(2, 8);
    format!("{chosen}G")
}

fn physical_memory_bytes() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            status.ullTotalPhys
        } else {
            8 * 1024 * 1024 * 1024
        }
    }
}

pub fn project_layout(dev_dir: &Path, module_name: &str, sha256: &str) -> ProjectLayout {
    let safe_module: String = module_name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' }).collect();
    let short_hash = &sha256[..16.min(sha256.len())];
    let key = format!("{safe_module}-{short_hash}");
    let root = projects_dir(dev_dir).join(&key);
    let scripts = scripts_dir(dev_dir);
    ProjectLayout {
        key,
        module_name: module_name.to_string(),
        sha256: sha256.to_string(),
        ghidra_project_dir: root.join("project"),
        ready_path: root.join("ready.json"),
        init_log: root.join("init.log"),
        script_log: root.join("script.log"),
        server_path: root.join("server.json"),
        daemon_log: root.join("daemon.log"),
        daemon_ghidra_log: root.join("daemon-ghidra.log"),
        daemon_config_path: root.join("daemon-config.json"),
        scripts_dir: scripts.clone(),
        bridge_script_path: scripts.join("HarmonyDecompileBridge.java"),
        root,
    }
}

impl ProjectLayout {
    pub fn read_ready(&self) -> Option<ReadyMarker> {
        let text = fs::read_to_string(&self.ready_path).ok()?;
        let marker: ReadyMarker = serde_json::from_str(&text).ok()?;
        let gpr = self.ghidra_project_dir.join("harmony.gpr");
        (marker.version == READY_VERSION && marker.sha256 == self.sha256 && gpr.is_file()).then_some(marker)
    }

    pub fn read_server(&self) -> Option<ServerInfo> {
        let text = fs::read_to_string(&self.server_path).ok()?;
        serde_json::from_str(&text).ok()
    }
}

pub fn read_ready_marker(path: &Path) -> Option<ReadyMarker> {
    let text = fs::read_to_string(path).ok()?;
    let marker: ReadyMarker = serde_json::from_str(&text).ok()?;
    (marker.version == READY_VERSION).then_some(marker)
}

pub fn new_ready_marker(module: &str, sha256: &str, image_base: u64, image_size: u64) -> ReadyMarker {
    let completed_at_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    ReadyMarker { version: READY_VERSION, module: module.to_string(), sha256: sha256.to_string(), image_base, image_size, completed_at_unix }
}

/// Lists all project directories under `<dev_dir>\ghidra\projects` in sorted order.
pub fn list_project_roots(dev_dir: &Path) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(projects_dir(dev_dir)) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                out.insert(name.to_string(), path);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ghidra_toml_and_rejects_unknown_keys() {
        let parsed = parse_ghidra_toml(Path::new("ghidra.toml"), "ghidra_dir = 'C:\\ghidra'\njava_home = 'C:\\jdk'\nmax_mem = '6G'\n").unwrap();
        assert_eq!(parsed.ghidra_dir, Some(PathBuf::from("C:\\ghidra")));
        assert_eq!(parsed.java_home, Some(PathBuf::from("C:\\jdk")));
        assert_eq!(parsed.max_mem.as_deref(), Some("6G"));
        assert!(parse_ghidra_toml(Path::new("ghidra.toml"), "unknown = 'x'\n").unwrap_err().contains("unknown option"));
    }

    #[test]
    fn max_mem_scales_with_physical_ram() {
        let gb = 1024 * 1024 * 1024;
        assert_eq!(default_max_mem_for_ram_bytes(2 * gb), "2G");
        assert_eq!(default_max_mem_for_ram_bytes(8 * gb), "4G");
        assert_eq!(default_max_mem_for_ram_bytes(32 * gb), "8G");
    }
}
