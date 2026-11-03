//! `files`, the `files` sensitive capability (ADR 0025): the extension's
//! data folder, `eu4-harmony\data\<id>\` in the user directory, made the
//! first time the extension writes to it, kept across mod updates and never
//! cleaned up by the framework.
use super::sandbox::{failure, undeclared, within};
use mlua::{Lua, MultiValue, Table, Value as LuaValue};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// The end of a file's name while it is being written.
const WRITING: &str = ".eu4harmony-tmp";

/// `files`, for the data folder `root` (none outside a game), usable when
/// `declared`.
pub fn table(lua: &Lua, root: Option<PathBuf>, declared: bool) -> mlua::Result<Table> {
    let files = lua.create_table()?;
    // The data folder, for `files.<what>`.
    let folder = move |what: &str| -> mlua::Result<PathBuf> {
        if !declared {
            return Err(undeclared(&format!("files.{what}"), "files", "files = true"));
        }
        root.clone().ok_or_else(|| mlua::Error::runtime(format!("files.{what}: there is no data folder outside a game")))
    };
    // A path in it.
    let at = folder.clone();
    let place =
        move |what: &str, path: &str| -> mlua::Result<PathBuf> { within(&at(what)?, path).map_err(|e| mlua::Error::runtime(format!("files.{what}: {e}"))) };

    let at = place.clone();
    files.set(
        "read",
        lua.create_function(move |lua, path: String| {
            let file = at("read", &path)?;
            match std::fs::read(&file) {
                Ok(bytes) => Ok(MultiValue::from_iter([LuaValue::String(lua.create_string(bytes)?)])),
                Err(e) => io_failure(lua, &path, &e),
            }
        })?,
    )?;

    let at = place.clone();
    files.set(
        "write",
        lua.create_function(move |lua, (path, text): (String, mlua::String)| {
            let file = at("write", &path)?;
            match write(&file, &text.as_bytes()) {
                Ok(()) => Ok(MultiValue::from_iter([LuaValue::Boolean(true)])),
                Err(e) => io_failure(lua, &path, &e),
            }
        })?,
    )?;

    let at = place.clone();
    files.set(
        "list",
        lua.create_function(move |lua, path: Option<String>| {
            // The data folder itself, when no folder in it is named.
            let folder = match &path {
                Some(path) => at("list", path)?,
                None => folder("list")?,
            };
            match list(&folder) {
                Ok(names) => Ok(MultiValue::from_iter([LuaValue::Table(lua.create_sequence_from(names)?)])),
                Err(e) => io_failure(lua, path.as_deref().unwrap_or("."), &e),
            }
        })?,
    )?;

    let at = place;
    files.set(
        "remove",
        lua.create_function(move |lua, path: String| {
            let file = at("remove", &path)?;
            let removed = match std::fs::symlink_metadata(&file) {
                Ok(m) if m.is_dir() => std::fs::remove_dir(&file),
                Ok(_) => std::fs::remove_file(&file),
                Err(e) => Err(e),
            };
            match removed {
                Ok(()) => Ok(MultiValue::from_iter([LuaValue::Boolean(true)])),
                Err(e) => io_failure(lua, &path, &e),
            }
        })?,
    )?;
    Ok(files)
}

/// Writes `file` whole or not at all: a file beside it, renamed over it.
fn write(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let mut writing = file.as_os_str().to_owned();
    writing.push(WRITING);
    let writing = PathBuf::from(writing);
    let written = std::fs::write(&writing, bytes).and_then(|()| std::fs::rename(&writing, file));
    if written.is_err() {
        let _ = std::fs::remove_file(&writing);
    }
    written
}

/// The names in `folder`, sorted, a folder's ending in `/`; none for a
/// folder not made yet.
fn list(folder: &Path) -> std::io::Result<Vec<String>> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(WRITING) {
            continue;
        }
        names.push(if entry.file_type()?.is_dir() { format!("{name}/") } else { name });
    }
    names.sort();
    Ok(names)
}

/// `nil, err` for a file that cannot be read, written or removed.
fn io_failure(lua: &Lua, path: &str, e: &std::io::Error) -> mlua::Result<MultiValue> {
    let code = if e.kind() == ErrorKind::NotFound { "not_found" } else { "failed" };
    failure(lua, code, &format!("{path}: {e}"))
}
