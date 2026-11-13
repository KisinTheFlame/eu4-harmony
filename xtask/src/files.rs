//! The player's files: installed into the game folder as last built, and
//! packed for players.
use crate::{built, built_package, cargo, repo, run, Args, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The files the release build makes that the game loads: each one's name in
/// the build output, and its path in the game folder. `install` puts these
/// into the game folder.
const GAME_FILES: [(&str, &str); 2] = [
    // The proxy, which loads the core into eu4.exe
    // (ADR version-proxy-injection).
    ("version.dll", "version.dll"),
    ("eu4_harmony_core.dll", "eu4-harmony\\eu4_harmony_core.dll"),
];

/// Distributed separately from the framework installation. `install`
/// leaves it out: a developer runs the one in `target\` or the CLI ZIP.
const TOOL: (&str, &str) = ("eu4-harmony.exe", "eu4-harmony.exe");

/// The developer build's options, copied from the repository.
const DEV_OPTIONS: &str = "eu4-harmony\\dev.toml";
/// Where a `version.dll` not ours, found in the game folder, is kept aside.
const KEPT_PROXY: &str = "eu4-harmony\\backup\\version.dll";
/// How our replaced `version.dll` is named while a test game still has it
/// loaded: Windows lets a loaded DLL be renamed but not removed, so it waits
/// in the game folder for a later install to remove it.
const OLD_PROXY: &str = "version.dll.old-";

/// The packages that build them.
const PLAYER_PACKAGES: [&str; 3] = ["eu4-harmony-proxy", "eu4-harmony-core", "eu4-harmony-cli"];

/// The Cargo profile the player package is built in: the release build in
/// one unit, which the release build is not (Cargo.toml).
pub const PACKAGE_PROFILE: &str = "player";

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
    // The player's game, which loads every file installed here, is closed
    // while installing: the user sees to that. The test games load only
    // version.dll from here (docs/plans/test-games.md), which is replaced
    // only when it differs and moved aside while loaded.
    remove_old_proxies(&root);

    if args.flag("uninstall") {
        if ours(&proxy) {
            retire(&proxy, &root)?;
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
        // Everything else under eu4-harmony\ is ours: the core, dev.toml, a
        // command line tool installed before, and the logs the proxy, and the core when it
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
    build_in("release", feature)
}

/// Builds the player's packages in the Cargo profile `profile`, with the
/// core's `feature` if there is one.
fn build_in(profile: &str, feature: Option<&str>) -> Result<()> {
    let mut build = cargo();
    build.args(["build", "--profile", profile, "--locked"]);
    for package in PLAYER_PACKAGES {
        build.args(["-p", package]);
    }
    if let Some(feature) = feature {
        build.args(["--features", feature]);
    }
    run(build, &format!("cargo build --profile {profile}"))
}

/// Copies the files as last built into the game folder `root`.
fn install_built(root: &Path) -> Result<()> {
    let installed = install_from(&built(), &repo().join("dev.toml"), root)?;
    println!("installed:");
    installed.iter().for_each(|path| println!("  {}", path.display()));
    Ok(())
}

/// Copies the files the game loads from the build output `out`, and the
/// options `options` if there are any, into the game folder `root`, keeping
/// aside a `version.dll` not ours and leaving ours alone when it is as built;
/// the files it installed.
pub fn install_from(out: &Path, options: &Path, root: &Path) -> Result<Vec<PathBuf>> {
    let proxy = root.join("version.dll");
    let backup = root.join(KEPT_PROXY);
    for (name, _) in GAME_FILES {
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
    for (name, to) in GAME_FILES {
        let from = out.join(name);
        let to = root.join(to);
        // Even the same bytes cannot be written over a loaded DLL.
        if to == proxy && proxy.exists() {
            if fs::read(&from).ok() == fs::read(&proxy).ok() {
                println!("{} is as built, left as it is", proxy.display());
                installed.push(to);
                continue;
            }
            retire(&proxy, root)?;
        }
        copy(&from, &to)?;
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
    let out = args.value("out").map_or_else(|| repo().join("target").join("dist"), PathBuf::from);
    let out = std::path::absolute(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    // A profile of its own, without features: a developer build or CI's
    // unoptimized release build is never packed.
    build_in(PACKAGE_PROFILE, None)?;
    let from = built_package();
    pack_from(&from, &out, env!("CARGO_PKG_VERSION"))?;
    if !args.flag("no-workshop") {
        crate::workshop::pack(&from)?;
    }
    Ok(())
}

/// Packs the two independent products. Check all inputs before replacing
/// either output, and leave release.json from release-check alone.
fn pack_from(from: &Path, out: &Path, version: &str) -> Result<()> {
    for (name, _) in GAME_FILES.into_iter().chain([TOOL]) {
        if !from.join(name).metadata().is_ok_and(|m| m.is_file() && m.len() > 0) {
            return Err(format!("missing or empty {} after cargo build --profile {PACKAGE_PROFILE}", from.join(name).display()));
        }
    }
    create_dir(out)?;
    let (framework, cli) = crate::release::artifact_names(version);
    pack_archive(from, &out.join("framework"), &out.join(framework), &GAME_FILES)?;
    pack_archive(from, &out.join("cli"), &out.join(cli), &[TOOL])
}

fn tar() -> std::process::Command {
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
    std::process::Command::new(system.join("tar.exe"))
}

fn pack_archive(from: &Path, folder: &Path, zip: &Path, files: &[(&str, &str)]) -> Result<()> {
    if folder.exists() {
        fs::remove_dir_all(folder).map_err(|e| format!("cannot remove {}: {e}", folder.display()))?;
    }
    if zip.exists() {
        remove_file(zip)?;
    }
    for (name, relative) in files {
        let to = folder.join(relative);
        create_dir(to.parent().expect("a file's folder"))?;
        copy(&from.join(name), &to)?;
    }
    let mut archive = tar();
    archive.arg("-a").arg("-c").arg("-f").arg(zip).arg("-C").arg(folder);
    // Explicit file names prevent an unrelated file or an old CLI from
    // being included through a recursively archived folder.
    archive.args(files.iter().map(|(_, relative)| *relative));
    run(archive, "tar")?;
    verify_archive(zip, from, files)?;
    println!("packed: {}", zip.display());
    Ok(())
}

/// Check the actual archive's file set and bytes, not just the staging
/// folder. This is local validation, not a published checksum deliverable.
fn verify_archive(zip: &Path, from: &Path, files: &[(&str, &str)]) -> Result<()> {
    let listing = tar().arg("-t").arg("-f").arg(zip).output().map_err(|e| format!("cannot list {}: {e}", zip.display()))?;
    if !listing.status.success() {
        return Err(format!("cannot list {}: {}", zip.display(), String::from_utf8_lossy(&listing.stderr).trim()));
    }
    let mut actual: Vec<String> = String::from_utf8_lossy(&listing.stdout).lines().map(|line| line.replace('\\', "/")).collect();
    let mut expected: Vec<String> = files.iter().map(|(_, relative)| relative.replace('\\', "/")).collect();
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err(format!("{} has unexpected contents: {actual:?}, expected {expected:?}", zip.display()));
    }
    for (name, relative) in files {
        let extracted = tar()
            .arg("-x")
            .arg("-O")
            .arg("-f")
            .arg(zip)
            .arg(relative.replace('\\', "/"))
            .output()
            .map_err(|e| format!("cannot check {relative} in {}: {e}", zip.display()))?;
        let original = fs::read(from.join(name)).map_err(|e| format!("cannot read {name}: {e}"))?;
        if !extracted.status.success() || extracted.stdout != original {
            return Err(format!("{relative} in {} differs from the player build", zip.display()));
        }
    }
    Ok(())
}

/// Removes our `version.dll` at `proxy` from the game folder `root`, or,
/// while a test game has it loaded, moves it aside for a later install.
fn retire(proxy: &Path, root: &Path) -> Result<()> {
    if fs::remove_file(proxy).is_ok() {
        return Ok(());
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let aside = root.join(format!("{OLD_PROXY}{stamp}"));
    rename(proxy, &aside)?;
    println!("moved {} to {}: a test game has it loaded; a later install removes it", proxy.display(), aside.display());
    Ok(())
}

/// Removes the replaced `version.dll`s in the game folder `root` that no
/// game has loaded any more.
fn remove_old_proxies(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(OLD_PROXY) && fs::remove_file(entry.path()).is_ok() {
            println!("removed {}", entry.path().display());
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::FreeLibrary,
        System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_AS_IMAGE_RESOURCE},
    };

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu4h-install-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A build output whose proxy is Windows' own `version.dll` with
    /// `extra` and our mark after its image, so that it loads and is ours.
    fn build_output(dir: &Path, extra: &[u8]) {
        let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32").join("version.dll");
        let mut proxy = fs::read(system).unwrap();
        proxy.extend_from_slice(extra);
        proxy.extend_from_slice(b"eu4_harmony_bootstrap");
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("version.dll"), proxy).unwrap();
        fs::write(dir.join("eu4_harmony_core.dll"), extra).unwrap();
        fs::write(dir.join("eu4-harmony.exe"), extra).unwrap();
    }

    /// The game folder's `version.dll` mapped as a loaded DLL is, as a test
    /// game holds it.
    fn load(path: &Path) -> *mut std::ffi::c_void {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_AS_IMAGE_RESOURCE) };
        assert!(!module.is_null(), "cannot load {}", path.display());
        module
    }

    #[test]
    fn packages_keep_framework_and_cli_separate_and_preserve_release_metadata() {
        let dir = scratch("packages");
        let from = dir.join("build");
        let out = dir.join("dist");
        build_output(&from, b"release bytes");
        create_dir(&out.join("framework")).unwrap();
        fs::write(out.join("framework").join("dev.toml"), "obsolete").unwrap();
        fs::write(out.join("release.json"), "metadata").unwrap();
        pack_from(&from, &out, "0.2.0").unwrap();
        assert!(!out.join("framework").join("dev.toml").exists());
        assert!(!out.join("framework").join("eu4-harmony.exe").exists());
        assert!(!out.join("framework").join("eu4-harmony").join("eu4-harmony.exe").exists());
        assert!(!out.join("cli").join("version.dll").exists());
        assert_eq!(fs::read(out.join("cli").join("eu4-harmony.exe")).unwrap(), b"release bytes");
        assert_eq!(fs::read_to_string(out.join("release.json")).unwrap(), "metadata");
        let (framework, cli) = crate::release::artifact_names("0.2.0");
        verify_archive(&out.join(&framework), &from, &GAME_FILES).unwrap();
        verify_archive(&out.join(&cli), &from, &[TOOL]).unwrap();
        fs::write(from.join("eu4-harmony.exe"), b"changed").unwrap();
        assert!(verify_archive(&out.join(cli), &from, &[TOOL]).unwrap_err().contains("differs from"));
        fs::remove_file(from.join("eu4-harmony.exe")).unwrap();
        assert!(pack_from(&from, &out, "0.2.0").unwrap_err().contains("missing or empty"));
        assert!(out.join(framework).is_file(), "missing input does not delete a previous package");
        fs::remove_dir_all(dir).unwrap();
    }

    /// While a test game has the proxy loaded, an install of the same proxy
    /// leaves it alone, one of another moves it aside, and a later install
    /// removes it once no game has it.
    #[test]
    fn installs_beside_a_loaded_proxy() {
        let dir = scratch("loaded");
        let (first, second, root) = (dir.join("first"), dir.join("second"), dir.join("game"));
        build_output(&first, b"first");
        build_output(&second, b"second");
        fs::create_dir_all(&root).unwrap();
        let options = dir.join("no-dev.toml");
        install_from(&first, &options, &root).unwrap();

        let module = load(&root.join("version.dll"));
        assert!(fs::remove_file(root.join("version.dll")).is_err(), "a loaded DLL cannot be removed");
        install_from(&first, &options, &root).unwrap();
        install_from(&second, &options, &root).unwrap();
        assert_eq!(fs::read(root.join("version.dll")).unwrap(), fs::read(second.join("version.dll")).unwrap());
        assert_eq!(fs::read(root.join("eu4-harmony").join("eu4_harmony_core.dll")).unwrap(), b"second");
        let old = || fs::read_dir(&root).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(OLD_PROXY)).count();
        assert_eq!(old(), 1, "the loaded proxy waits aside");
        remove_old_proxies(&root);
        assert_eq!(old(), 1, "it stays while loaded");

        unsafe { FreeLibrary(module) };
        remove_old_proxies(&root);
        assert_eq!(old(), 0, "a later install removes it");
        assert!(!root.join(KEPT_PROXY).exists(), "our own proxy is not kept as a foreign one");
        assert!(!root.join(TOOL.1).exists(), "the command line tool is not installed");
        let _ = fs::remove_dir_all(dir);
    }
}
