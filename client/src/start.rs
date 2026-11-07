//! Starting the game from its folder, for `eu4-harmony start` and the claim
//! of `cargo xtask game`: hidden or shown (the framework's switch
//! `-harmony_hidden`, docs/hidden-start.md), returning once remote control
//! says the main menu is up. Steam must be running, as for any start of the
//! game.
use crate::{
    game::Game,
    pipe::{self, NoMainMenu},
    Result,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// How long the game may take to reach its main menu without `--timeout`.
pub const WAIT: Duration = Duration::from_secs(300);

/// The game's command line switches for a start. A `seed` is the game's own
/// `-seed`, which seeds its random generator and gives each AI agent a
/// generator of its own (docs/research/late-game.md).
pub fn switches(hidden: bool, sound: bool, seed: Option<u32>) -> Vec<String> {
    let mut words = Vec::new();
    if hidden {
        words.push("-harmony_hidden".to_string());
        if sound {
            words.push("-harmony_sound".to_string());
        }
    }
    words.extend(seed.map(|seed| format!("-seed={seed}")));
    words
}

/// The framework in the game folder has the hidden start: its proxy is there
/// and its core knows the switch. Without it nothing would keep the game
/// hidden, and a core from before the hidden start passes the switch over.
fn hidden_start_installed(game_dir: &Path) -> bool {
    const SWITCH: &[u8] = b"-harmony_hidden";
    let core = std::fs::read(game_dir.join("eu4-harmony").join("eu4_harmony_core.dll")).unwrap_or_default();
    game_dir.join("version.dll").is_file() && core.windows(SWITCH.len()).any(|w| w == SWITCH)
}

/// Starts `eu4.exe` from `game_dir` and waits up to `wait` for its main
/// menu; returns the game, held open so that its exit code stays to be read
/// once it ends. `log` is the framework's log, which says why when the game
/// ends first.
pub fn start(game_dir: &Path, log: &Path, hidden: bool, sound: bool, seed: Option<u32>, wait: Duration) -> Result<Game> {
    crate::game::not_running()?;
    if hidden && !hidden_start_installed(game_dir) {
        return Err(format!(
            "the framework in {} has no hidden start: install a build that has it (cargo xtask install), or start the game shown",
            game_dir.display()
        ));
    }
    let exe = game_dir.join(crate::game::EXE);
    let started = Instant::now();
    let since = std::time::SystemTime::now();
    let game = Game::spawn(&exe, &switches(hidden, sound, seed), game_dir)?;
    let id = game.id();
    println!(
        "started eu4.exe (process {id}){}{}",
        if hidden {
            if sound {
                ", hidden, with sound"
            } else {
                ", hidden"
            }
        } else {
            ""
        },
        seed.map_or(String::new(), |seed| format!(", -seed={seed}"))
    );
    let ended = || game.exit_code().map_or(Ok(()), Err);
    match pipe::main_menu(pipe::NAME, pipe::GAME, started + wait, ended) {
        Ok(_) => {
            println!("at the main menu after {:.0} s", started.elapsed().as_secs_f64());
            Ok(game)
        }
        Err(NoMainMenu::Stopped(code)) => {
            let why = why_it_ended(log, since).unwrap_or_else(|| format!("the framework's log of this start ({}) says nothing of it", log.display()));
            Err(format!("the game ended (exit code {code}) before its main menu: {why}"))
        }
        Err(NoMainMenu::TimedOut) => Err(format!("the game (process {id}) had not reached its main menu after {} s; it still runs", wait.as_secs())),
    }
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
        assert_eq!(switches(false, false, None), Vec::<String>::new());
        assert_eq!(switches(false, true, None), Vec::<String>::new());
        assert_eq!(switches(true, false, None), ["-harmony_hidden"]);
        assert_eq!(switches(true, true, None), ["-harmony_hidden", "-harmony_sound"]);
    }

    #[test]
    fn a_seed_is_the_game_s_own_switch() {
        assert_eq!(switches(true, false, Some(7)), ["-harmony_hidden", "-seed=7"]);
        assert_eq!(switches(false, false, Some(0)), ["-seed=0"]);
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
