//! The player's files: installed into the game folder as last built, and
//! packed for players.
use crate::{built, cargo, repo, run, Args, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The files the release build makes that a player puts into the game
/// folder: each one's name in the build output, and its path in the game
/// folder. The README's installing section names the same files.
const PLAYER_FILES: [(&str, &str); 3] = [
    // The proxy, which loads the core into eu4.exe
    // (ADR version-proxy-injection).
    ("version.dll", "version.dll"),
    ("eu4_harmony_core.dll", "eu4-harmony\\eu4_harmony_core.dll"),
    // The command line tool, for remote control (eu4-harmony game) and for
    // making mods.
    ("eu4-harmony.exe", "eu4-harmony\\eu4-harmony.exe"),
];

/// The developer build's options, copied from the repository.
const DEV_OPTIONS: &str = "eu4-harmony\\dev.toml";
/// Where a `version.dll` not ours, found in the game folder, is kept aside.
const KEPT_PROXY: &str = "eu4-harmony\\backup\\version.dll";

/// The packages that build them.
const PLAYER_PACKAGES: [&str; 3] = ["eu4-harmony-proxy", "eu4-harmony-core", "eu4-harmony-cli"];

/// Whether `path` is our proxy: it names the core's bootstrap export, which
/// a foreign DLL does not.
fn ours(path: &Path) -> bool {
    const MARK: &[u8] = b"eu4_harmony_bootstrap";
    fs::read(path).is_ok_and(|bytes| bytes.windows(MARK.len()).any(|w| w == MARK))
}

pub fn install(args: &Args) -> Result<()> {
    let root = args.game_dir()?;
    let harmony = root.join("eu4-harmony");
    let proxy = root.join("version.dll");
    let backup = root.join(KEPT_PROXY);
    if !root.join("eu4.exe").is_file() {
        return Err(format!("no eu4.exe in {}", root.display()));
    }
    // A loaded version.dll cannot be replaced: no game may run, the test
    // games included (docs/test-games.md).
    eu4_harmony_client::game::not_running()?;

    if args.flag("uninstall") {
        if ours(&proxy) {
            remove_file(&proxy)?;
            println!("removed {}", proxy.display());
        } else if proxy.exists() {
            println!("left {} alone: not ours", proxy.display());
        }
        if backup.exists() {
            if proxy.exists() {
                return Err(format!("cannot restore {}: {} still exists", backup.display(), proxy.display()));
            }
            rename(&backup, &proxy)?;
            println!("restored the backed-up version.dll to {}", proxy.display());
        }
        // Everything else under eu4-harmony\ is ours: the core, the command
        // line tool, dev.toml, and the logs the proxy, and the core when it
        // cannot find the user folder, write there.
        if harmony.exists() {
            fs::remove_dir_all(&harmony).map_err(|e| format!("cannot remove {}: {e}", harmony.display()))?;
            println!("removed {}", harmony.display());
        }
        println!("logs, reports and extension save data stay in the user folder's eu4-harmony\\");
        return Ok(());
    }

    install_built(&root)
}

/// Builds the player's packages, with the core's `feature` for a developer
/// build. Only they are built, so that this command's own program, which
/// runs, is not built again.
pub fn build(feature: Option<&str>) -> Result<()> {
    let mut build = cargo();
    build.args(["build", "--release", "--locked"]);
    for package in PLAYER_PACKAGES {
        build.args(["-p", package]);
    }
    if let Some(feature) = feature {
        build.args(["--features", feature]);
    }
    run(build, "cargo build --release")
}

/// Copies the files as last built into the game folder `root`, which the
/// caller found closed.
fn install_built(root: &Path) -> Result<()> {
    let installed = install_from(&built(), &repo().join("dev.toml"), root)?;
    println!("installed:");
    installed.iter().for_each(|path| println!("  {}", path.display()));
    Ok(())
}

/// Copies the player's files from the build output `out`, and the options
/// `options` if there are any, into the game folder `root`, keeping aside a
/// `version.dll` not ours; the files it copied.
pub fn install_from(out: &Path, options: &Path, root: &Path) -> Result<Vec<PathBuf>> {
    let proxy = root.join("version.dll");
    let backup = root.join(KEPT_PROXY);
    for (name, _) in PLAYER_FILES {
        if !out.join(name).is_file() {
            return Err(format!("missing {}; run cargo build --release", out.join(name).display()));
        }
    }
    create_dir(&root.join("eu4-harmony"))?;
    if proxy.exists() && !ours(&proxy) {
        if backup.exists() {
            return Err(format!("both {} and {} exist and neither is ours", proxy.display(), backup.display()));
        }
        create_dir(backup.parent().expect("a file's folder"))?;
        rename(&proxy, &backup)?;
        println!("moved the existing version.dll to {} (kept, never loaded)", backup.display());
    }
    let mut installed = Vec::new();
    for (name, to) in PLAYER_FILES {
        let to = root.join(to);
        copy(&out.join(name), &to)?;
        installed.push(to);
    }
    let options_installed = root.join(DEV_OPTIONS);
    if options.is_file() {
        copy(options, &options_installed)?;
        installed.push(options_installed);
    } else if options_installed.exists() {
        remove_file(&options_installed)?;
        println!("removed {}: the repository has no dev.toml", options_installed.display());
    }
    Ok(installed)
}

pub fn release(args: &Args) -> Result<()> {
    let out = args.value("out").map_or_else(|| repo().join("target").join("eu4-harmony"), PathBuf::from);
    let out = std::path::absolute(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let zip = PathBuf::from(format!("{}.zip", out.display()));

    // With no features, so a developer build built last is never packed.
    build(None)?;
    let from = built();
    for (name, _) in PLAYER_FILES {
        if !from.join(name).is_file() {
            return Err(format!("missing {} after cargo build --release", from.join(name).display()));
        }
    }
    if out.exists() {
        fs::remove_dir_all(&out).map_err(|e| format!("cannot remove {}: {e}", out.display()))?;
    }
    if zip.exists() {
        remove_file(&zip)?;
    }
    for (name, to) in PLAYER_FILES {
        let to = out.join(to);
        create_dir(to.parent().expect("a file's folder"))?;
        copy(&from.join(name), &to)?;
    }
    // Windows' own tar writes zip archives; the zip holds what the folder
    // holds at its top.
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
    let mut tar = std::process::Command::new(system.join("tar.exe"));
    tar.arg("-a").arg("-c").arg("-f").arg(&zip).arg("-C").arg(&out);
    let mut top: Vec<&str> = PLAYER_FILES.iter().map(|(_, to)| to.split('\\').next().expect("a path")).collect();
    top.dedup();
    tar.args(&top);
    run(tar, "tar")?;
    println!("packed:\n  {}\n  {}", out.display(), zip.display());
    for (_, to) in PLAYER_FILES {
        println!("    {to}");
    }
    Ok(())
}

fn create_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    fs::copy(from, to).map(drop).map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))
}

fn rename(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to).map_err(|e| format!("cannot move {} to {}: {e}", from.display(), to.display()))
}

fn remove_file(path: &Path) -> Result<()> {
    fs::remove_file(path).map_err(|e| format!("cannot remove {}: {e}", path.display()))
}
