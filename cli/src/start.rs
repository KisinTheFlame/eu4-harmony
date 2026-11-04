//! `eu4-harmony start`: starts the game from its folder, hidden with
//! `--hidden` (the framework's switch `-harmony_hidden`, docs/hidden-start.md),
//! and returns once remote control says the main menu is up. Steam must be
//! running, as for any start of the game.
use crate::pipe;
use crate::Result;
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

/// How long the game may take to reach its main menu without `--timeout`.
pub const WAIT: Duration = Duration::from_secs(300);
/// How often to ask remote control where the game is.
const ASK_EVERY: Duration = Duration::from_millis(500);

/// The game's command line switches for a start.
pub fn switches(hidden: bool, sound: bool) -> Vec<&'static str> {
    let mut words = Vec::new();
    if hidden {
        words.push("-harmony_hidden");
        if sound {
            words.push("-harmony_sound");
        }
    }
    words
}

/// The framework's proxy is in the game folder: without it nothing would
/// keep a hidden start hidden.
fn framework_installed(game_dir: &Path) -> bool {
    game_dir.join("version.dll").is_file() && game_dir.join("eu4-harmony").join("eu4_harmony_core.dll").is_file()
}

/// Starts `eu4.exe` from `game_dir` and waits up to `wait` for its main
/// menu; returns the game's process id. `log` is the framework's log, which
/// says why when the game ends first.
pub fn start(game_dir: &Path, log: &Path, hidden: bool, sound: bool, wait: Duration) -> Result<u32> {
    crate::backup::game_not_running()?;
    if hidden && !framework_installed(game_dir) {
        return Err(format!("EU4 Harmony is not installed in {}; a hidden start needs it", game_dir.display()));
    }
    let exe = game_dir.join("eu4.exe");
    let started = Instant::now();
    let since = std::time::SystemTime::now();
    let mut game =
        Command::new(&exe).args(switches(hidden, sound)).current_dir(game_dir).spawn().map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    let id = game.id();
    println!(
        "started eu4.exe (process {id}){}",
        if hidden {
            if sound {
                ", hidden, with sound"
            } else {
                ", hidden"
            }
        } else {
            ""
        }
    );
    loop {
        if let Some(status) = game.try_wait().map_err(|e| e.to_string())? {
            let why = why_it_ended(log, since).unwrap_or_else(|| format!("the framework's log of this start ({}) says nothing of it", log.display()));
            return Err(format!("the game ended ({status}) before its main menu: {why}"));
        }
        if at_main_menu() {
            println!("at the main menu after {:.0} s", started.elapsed().as_secs_f64());
            return Ok(id);
        }
        if started.elapsed() > wait {
            return Err(format!("the game (process {id}) had not reached its main menu after {} s; it still runs", wait.as_secs()));
        }
        std::thread::sleep(ASK_EVERY);
    }
}

/// Whether remote control answers that the main menu is up.
fn at_main_menu() -> bool {
    let deadline = Instant::now() + ASK_EVERY;
    let Ok(mut client) = pipe::connect(pipe::NAME, pipe::GAME, deadline) else { return false };
    let Ok(answer) = client.ask(r#"{"capability":"menu"}"#, Some(Instant::now() + Duration::from_secs(5))) else { return false };
    serde_json::from_str::<serde_json::Value>(&answer).is_ok_and(|v| v["result"]["place"] == "main_menu")
}

/// The framework's own word on why the game ended: the last failed startup
/// or fatal error in its log, when the log was written since `since`.
fn why_it_ended(log: &Path, since: std::time::SystemTime) -> Option<String> {
    std::fs::metadata(log).and_then(|m| m.modified()).ok().filter(|&written| written >= since)?;
    let text = std::fs::read_to_string(log).ok()?;
    reason(&text)
}

/// The last line of a log that tells why the framework ended the game, and
/// the lines a failed startup lists after it.
fn reason(log: &str) -> Option<String> {
    let lines: Vec<&str> = log.lines().collect();
    let at = lines.iter().rposition(|line| line.contains("fatal error, ending the game") || line.contains("startup failed"))?;
    Some(lines[at..].iter().take(4).map(|line| line.trim()).collect::<Vec<_>>().join(" / "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_is_a_switch_of_a_hidden_start_only() {
        assert_eq!(switches(false, false), Vec::<&str>::new());
        assert_eq!(switches(false, true), Vec::<&str>::new());
        assert_eq!(switches(true, false), ["-harmony_hidden"]);
        assert_eq!(switches(true, true), ["-harmony_hidden", "-harmony_sound"]);
    }

    #[test]
    fn the_reason_is_the_last_fatal_error_or_failed_startup() {
        let log = "\
[12:00:00] bootstrap
[12:00:01] fatal error, ending the game: an older one
[12:00:02] startup: fine
[12:00:03] fatal error, ending the game: a hidden start needs a windowed or borderless game
";
        assert_eq!(reason(log).unwrap(), "[12:00:03] fatal error, ending the game: a hidden start needs a windowed or borderless game");
        assert_eq!(reason("[12:00:00] startup: fine\n"), None);
    }
}
