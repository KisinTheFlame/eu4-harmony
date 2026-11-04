//! `eu4-harmony backup` and `restore`: the player's files a run of the game
//! may change, backed up before it and put back after it. The game rewrites
//! a save it loads and rotates its autosaves, rewrites its settings when it
//! applies them, and rewrites its login cache.
//!
//! A backup is a folder with `files\` (the copies, by their paths under the
//! user folder), `items.txt` (what was backed up, one a line), `crashes.txt`
//! (the crash folders there were) and `user-folder.txt`.
use crate::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// The folders a run may change, under the user folder; every file at its
/// top is added to them.
const FOLDERS: [&str; 2] = ["save games", "eu4-harmony"];

/// Refuses while the game runs, which writes the files.
pub fn game_not_running() -> Result<()> {
    if game_running() {
        return Err("eu4.exe is running; close it first".into());
    }
    Ok(())
}

/// Whether an `eu4.exe` runs.
fn game_running() -> bool {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot as isize == -1 {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        let mut found = false;
        while more && !found {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            found = String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("eu4.exe");
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        found
    }
}

/// The user folder, absolute, which must be there.
pub fn user_folder(user: &Path) -> Result<PathBuf> {
    let user = std::path::absolute(user).map_err(|e| format!("{}: {e}", user.display()))?;
    if !user.is_dir() {
        return Err(format!("no user folder at {}", user.display()));
    }
    println!("user folder: {}", user.display());
    Ok(user)
}

/// Copies the player's files into `dir`, a folder that does not exist yet.
pub fn backup(user: &Path, dir: &Path) -> Result<()> {
    let dir = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if dir.exists() {
        return Err(format!("{} exists already; name a new folder", dir.display()));
    }
    fs::create_dir_all(&dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    let mut items: Vec<String> = FOLDERS.iter().map(|f| f.to_string()).collect();
    items.extend(top_files(user)?);
    for item in &items {
        let from = user.join(item);
        if !from.exists() {
            println!("  {item} (none)");
            continue;
        }
        copy_tree(&from, &dir.join("files").join(item))?;
        println!("  {item}");
    }
    write_lines(&dir.join("items.txt"), &items)?;
    write_lines(&dir.join("crashes.txt"), &crashes(user)?)?;
    write_lines(&dir.join("user-folder.txt"), &[user.display().to_string()])?;
    println!("backed up to {}; afterwards: eu4-harmony restore \"{}\"", dir.display(), dir.display());
    Ok(())
}

/// Lists what changed since the backup in `dir`, puts every file back as the
/// backup has it, compares each, and removes the crash folders the run left
/// unless `keep_crashes`, and the backup once all match unless `keep`.
pub fn restore(user: &Path, dir: &Path, keep: bool, keep_crashes: bool) -> Result<()> {
    let dir = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let saved = dir.join("files");
    let from = read_lines(&dir.join("user-folder.txt")).map_err(|_| format!("{} holds no backup of eu4-harmony backup", dir.display()))?;
    let from = from.first().map(String::as_str).unwrap_or_default();
    // Windows' paths are the same in any case.
    if !from.eq_ignore_ascii_case(&user.display().to_string()) {
        return Err(format!("the backup is of {from}, not of {}; pass --user-dir", user.display()));
    }

    // What was backed up, and the files the run added at the top.
    let items: BTreeSet<String> = read_lines(&dir.join("items.txt"))?.into_iter().chain(top_files(user)?).collect();
    let mut differ = 0;
    for item in &items {
        let before = files(&saved, item)?;
        let now = files(user, item)?;
        for (path, file) in &now {
            match before.get(path) {
                None => println!("  added:   {path}"),
                Some(copy) if !same(copy, file) => println!("  changed: {path}"),
                Some(_) => {}
            }
        }
        for path in before.keys().filter(|p| !now.contains_key(*p)) {
            println!("  deleted: {path}");
        }
        let target = user.join(item);
        remove_tree(&target)?;
        let copy = saved.join(item);
        if copy.exists() {
            copy_tree(&copy, &target)?;
        }
        let after = files(user, item)?;
        for path in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
            let restored = matches!((before.get(path), after.get(path)), (Some(a), Some(b)) if same(a, b));
            if !restored {
                println!("  NOT RESTORED: {path}");
                differ += 1;
            }
        }
    }

    let known: BTreeSet<String> = read_lines(&dir.join("crashes.txt")).unwrap_or_default().into_iter().collect();
    for crash in crashes(user)?.into_iter().filter(|c| !known.contains(c)) {
        let path = user.join("crashes").join(&crash);
        if keep_crashes {
            println!("  crash folder left: {}", path.display());
        } else {
            remove_tree(&path)?;
            println!("  crash folder removed: {}", path.display());
        }
    }

    if differ > 0 {
        return Err(format!("{differ} file(s) differ from the backup, which stays at {}", dir.display()));
    }
    println!("every file is as the backup has it");
    if keep {
        println!("the backup stays at {}", dir.display());
    } else {
        fs::remove_dir_all(&dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))?;
        println!("removed the backup {}", dir.display());
    }
    Ok(())
}

/// Every file under `root\item`, by its path under `root`, with
/// backslashes.
fn files(root: &Path, item: &str) -> Result<BTreeMap<String, PathBuf>> {
    let mut found = BTreeMap::new();
    let path = root.join(item);
    if path.is_file() {
        found.insert(item.to_string(), path);
    } else if path.is_dir() {
        let mut add = |file: &Path| {
            let name = file.strip_prefix(root).expect("under the root").to_string_lossy().replace('/', "\\");
            found.insert(name, file.to_path_buf());
        };
        walk(&path, &mut add)?;
    }
    Ok(found)
}

fn walk(dir: &Path, each: &mut dyn FnMut(&Path)) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            walk(&path, each)?;
        } else {
            each(&path);
        }
    }
    Ok(())
}

/// Whether the two files hold the same bytes.
fn same(a: &Path, b: &Path) -> bool {
    let size = |p: &Path| fs::metadata(p).map(|m| m.len()).ok();
    size(a).is_some() && size(a) == size(b) && matches!((fs::read(a), fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

/// The names of the files at the top of the user folder, hidden ones too.
fn top_files(user: &Path) -> Result<Vec<String>> {
    names(user, |kind| kind.is_file())
}

/// The crash folders there are now.
fn crashes(user: &Path) -> Result<Vec<String>> {
    let dir = user.join("crashes");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    names(&dir, |kind| kind.is_dir())
}

/// The names in `dir` of the entries of the kind `wanted` takes, in order.
fn names(dir: &Path, wanted: fn(&fs::FileType) -> bool) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if wanted(&entry.file_type().map_err(|e| e.to_string())?) {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Copies a file, or a folder with everything in it, to `to`.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to).map_err(|e| format!("cannot make {}: {e}", to.display()))?;
        for entry in fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        let folder = to.parent().expect("a file's folder");
        fs::create_dir_all(folder).map_err(|e| format!("cannot make {}: {e}", folder.display()))?;
        fs::copy(from, to).map(drop).map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))
    }
}

fn remove_tree(path: &Path) -> Result<()> {
    let removed = if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    };
    removed.map_err(|e| format!("cannot remove {}: {e}", path.display()))
}

fn write_lines(path: &Path, lines: &[String]) -> Result<()> {
    let text: String = lines.iter().map(|l| format!("{l}\r\n")).collect();
    fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The lines of a text file, without a byte order mark or empty lines.
fn read_lines(path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(text.trim_start_matches('\u{feff}').lines().map(str::trim_end).filter(|l| !l.is_empty()).map(String::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu4h-backup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn put(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Whatever a run changes, adds or deletes, and the crash folders it
    /// leaves, are put back as they were, and the backup is removed.
    #[test]
    fn restore_puts_back_what_a_run_changed() {
        let user = scratch("user");
        let backups = scratch("backups");
        put(&user.join("settings.txt"), "volume=1");
        put(&user.join("save games").join("a.eu4"), "a");
        put(&user.join("eu4-harmony").join("logs").join("core.log"), "log");
        put(&user.join("crashes").join("old").join("minidump.dmp"), "old");
        put(&user.join("mod").join("x.mod"), "not ours");
        let dir = backups.join("b");
        backup(&user, &dir).unwrap();

        put(&user.join("settings.txt"), "volume=0");
        put(&user.join("save games").join("autosave.eu4"), "new");
        fs::remove_file(user.join("eu4-harmony").join("logs").join("core.log")).unwrap();
        put(&user.join("pdx_login_cache.txt"), "new");
        put(&user.join("crashes").join("new").join("minidump.dmp"), "new");
        put(&user.join("mod").join("x.mod"), "changed, not ours to restore");
        restore(&user, &dir, false, false).unwrap();

        assert_eq!(fs::read_to_string(user.join("settings.txt")).unwrap(), "volume=1");
        assert!(!user.join("save games").join("autosave.eu4").exists());
        assert_eq!(fs::read_to_string(user.join("eu4-harmony").join("logs").join("core.log")).unwrap(), "log");
        assert!(!user.join("pdx_login_cache.txt").exists());
        assert!(!user.join("crashes").join("new").exists() && user.join("crashes").join("old").exists());
        assert_eq!(fs::read_to_string(user.join("mod").join("x.mod")).unwrap(), "changed, not ours to restore");
        assert!(!dir.exists());
        let _ = fs::remove_dir_all(user);
        let _ = fs::remove_dir_all(backups);
    }

    #[test]
    fn backup_refuses_a_folder_that_exists_and_restore_another_user_folder() {
        let user = scratch("user2");
        let dir = scratch("there");
        put(&user.join("settings.txt"), "x");
        fs::create_dir_all(&dir).unwrap();
        assert!(backup(&user, &dir).unwrap_err().contains("exists already"));
        fs::remove_dir_all(&dir).unwrap();
        backup(&user, &dir).unwrap();
        let other = scratch("other");
        fs::create_dir_all(&other).unwrap();
        assert!(restore(&other, &dir, false, false).unwrap_err().contains("pass --user-dir"));
        for d in [user, dir, other] {
            let _ = fs::remove_dir_all(d);
        }
    }
}
