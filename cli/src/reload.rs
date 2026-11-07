//! `eu4-harmony reload`: reloads a mod's extension in the running game
//! through remote control (docs/extension-reload.md), once, or whenever a
//! file of its code or texts changes.
use crate::pipe;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// How long to wait for the game to open its pipe.
const PIPE_WAIT: Duration = Duration::from_secs(10);
/// How often the files are looked at, and how long they must stay as they
/// are before a reload, so an editor's save of several files is one.
const LOOK: Duration = Duration::from_millis(250);
const SETTLE: Duration = Duration::from_millis(400);

/// Reloads the mod `id`'s extension in the game serving the pipe `name`;
/// what came of it, as a line to print.
pub fn once(id: &str, name: &str) -> Result<String, String> {
    let request = json!({"capability": "extension.reload", "id": id}).to_string();
    let started = Instant::now();
    let answer = pipe::connect(name, pipe::GAME, started + PIPE_WAIT)
        .and_then(|mut client| client.ask(&request, None))
        .map_err(|f| format!("{f}; is the game running?"))?;
    said(id, &answer)
}

/// The line for the game's answer to a reload.
fn said(id: &str, answer: &str) -> Result<String, String> {
    let response: Value = serde_json::from_str(answer).map_err(|e| format!("the game's answer is not JSON ({e}): {answer}"))?;
    if response["ok"] == true {
        let result = &response["result"];
        return Ok(match (result["session"].as_bool(), result["lost"].as_str()) {
            (Some(true), None) => format!("reloaded {id}; on_session got what save_data gave"),
            (Some(true), Some(lost)) => format!("reloaded {id}; on_session got nil: {lost}"),
            _ => format!("reloaded {id}; no session is going on, so on_session comes with the next"),
        });
    }
    let error = &response["error"];
    Err(format!("{} ({})", error["message"].as_str().unwrap_or(answer), error["code"].as_str().unwrap_or("error")))
}

/// The files a reload takes up: the code and the texts.
fn watched(root: &Path) -> [PathBuf; 2] {
    let harmony = root.join("eu4-harmony");
    [harmony.join("lua"), harmony.join("localisation")]
}

/// Every file under `dirs`, with when it was last written and its size.
fn look(dirs: &[PathBuf]) -> BTreeMap<PathBuf, (SystemTime, u64)> {
    let mut seen = BTreeMap::new();
    let mut stack: Vec<PathBuf> = dirs.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.metadata() {
                Ok(m) if m.is_dir() => stack.push(path),
                Ok(m) => {
                    seen.insert(path, (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()));
                }
                Err(_) => {}
            }
        }
    }
    seen
}

/// Reloads the mod `id` at `root` in the game serving the pipe `name`
/// whenever its code or texts change, until the command is stopped; a
/// failed reload is printed and waited past.
pub fn watch(root: &Path, id: &str, name: &str) -> ! {
    let dirs = watched(root);
    let mut last = look(&dirs);
    println!("watching {} and {} for {id}; Ctrl+C stops", dirs[0].display(), dirs[1].display());
    loop {
        std::thread::sleep(LOOK);
        let now = look(&dirs);
        if now == last {
            continue;
        }
        // Wait for the files to settle.
        let mut settled = now;
        loop {
            std::thread::sleep(SETTLE);
            let again = look(&dirs);
            if again == settled {
                break;
            }
            settled = again;
        }
        last = settled;
        match once(id, name) {
            Ok(line) => println!("{} {line}", clock()),
            Err(e) => eprintln!("{} not reloaded: {e}", clock()),
        }
    }
}

/// The local time of day, for the lines --watch prints.
fn clock() -> String {
    // SAFETY: GetLocalTime only writes the struct it is given.
    let now = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    format!("[{:02}:{:02}:{:02}]", now.wHour, now.wMinute, now.wSecond)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_says_what_the_reload_came_to() {
        let ok = |result: &str| said("rc", &format!("{{\"ok\": true, \"result\": {result}}}"));
        assert_eq!(ok(r#"{"session": true, "lost": null}"#).unwrap(), "reloaded rc; on_session got what save_data gave");
        assert!(ok(r#"{"session": true, "lost": "save_data failed: x"}"#).unwrap().ends_with("on_session got nil: save_data failed: x"));
        assert!(ok(r#"{"session": false, "lost": null}"#).unwrap().contains("no session is going on"));
        let refused = said("rc", r#"{"ok": false, "error": {"code": "rejected", "message": "the new code did not load"}}"#).unwrap_err();
        assert_eq!(refused, "the new code did not load (rejected)");
    }

    #[test]
    fn a_change_to_a_file_of_the_code_or_texts_is_seen() {
        let root = std::env::temp_dir().join(format!("eu4h-watch-{}", std::process::id()));
        let lua = root.join("eu4-harmony").join("lua");
        fs::create_dir_all(lua.join("deep")).unwrap();
        fs::write(lua.join("main.lua"), "return {}").unwrap();
        let dirs = watched(&root);
        let before = look(&dirs);
        assert_eq!(before.len(), 1);
        fs::write(lua.join("deep").join("more.lua"), "return 1").unwrap();
        let after = look(&dirs);
        assert_ne!(before, after);
        assert_eq!(after.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }
}
