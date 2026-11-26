//! Starting the game from its folder, for `eu4-harmony start` and the claim
//! of `cargo xtask game`: hidden or shown (`-harmony_hidden`), muted or with
//! sound (`-harmony_mute`, docs/game/program/headless.md), the player's game
//! or a test game with a user folder, a core and a pipe of its own
//! (docs/dev-tools.md#test-games), returning once remote control says the main
//! menu is up. Steam must be running, as for any start of the game.
use crate::Result;
use crate::{
    game::Game,
    pipe::{self, NoMainMenu},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Why a start failed, and the game's end when it ended before its main
/// menu, so that whoever started it can keep a record of how it ended.
#[derive(Debug)]
pub struct Failed {
    pub message: String,
    pub ended: Option<Ended>,
}

/// A game that ended before its main menu: its process, the time it
/// started, as [`Game::started`] has it, and its exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ended {
    pub id: u32,
    pub started: u64,
    pub code: u32,
}

impl From<String> for Failed {
    fn from(message: String) -> Self {
        Self { message, ended: None }
    }
}

impl From<Failed> for String {
    fn from(failed: Failed) -> Self {
        failed.message
    }
}

/// How long the game may take to reach its main menu without `--timeout`.
pub const WAIT: Duration = Duration::from_secs(300);

/// How the game is started.
#[derive(Clone, Debug, Default)]
pub struct Start {
    /// Start the game window hidden (`-harmony_hidden`).
    pub hidden: bool,
    /// Start the game muted (`-harmony_mute`).
    pub mute: bool,
    /// The game's own `-seed`, which seeds its random generator and gives
    /// each AI agent a generator of its own (docs/game/ai/work-scheduling.md).
    pub seed: Option<u32>,
    /// Explicit `-userdir` override for non-test-game starts.
    pub user_dir: Option<PathBuf>,
    /// A test game, not the player's.
    pub test_game: Option<TestGame>,
}

/// A test game: its name, which names its pipe; its user folder, which the
/// game's `-userdir` names; and the core the proxy loads, which
/// `-harmony_core` names.
#[derive(Clone, Debug)]
pub struct TestGame {
    pub name: String,
    pub user_dir: PathBuf,
    pub core: PathBuf,
}

impl Start {
    /// The game's command line switches.
    pub fn switches(&self) -> Vec<String> {
        // A word with a space in it is quoted whole, as the game's command
        // line is split.
        let path = |switch: &str, value: &Path| {
            let word = format!("{switch}={}", value.display());
            if word.contains(' ') {
                format!("\"{word}\"")
            } else {
                word
            }
        };
        let mut words = Vec::new();
        if self.hidden {
            words.push("-harmony_hidden".to_string());
        }
        if self.mute {
            words.push("-harmony_mute".to_string());
        }
        words.extend(self.seed.map(|seed| format!("-seed={seed}")));
        match &self.test_game {
            Some(game) => {
                words.push(path("-userdir", &game.user_dir));
                words.push(path("-harmony_core", &game.core));
                words.push(format!("-harmony_test_game={}", game.name));
            }
            None => {
                if let Some(dir) = &self.user_dir {
                    words.push(path("-userdir", dir));
                }
            }
        }
        words
    }

    /// The pipe the game serves.
    pub fn pipe(&self) -> Result<String> {
        eu4_harmony_remote::pipe_of(self.test_game.as_ref().map(|g| g.name.as_str()))
    }
}

/// Whether the file at `path` has `text` in it: a switch a build of the
/// framework knows.
pub fn knows(path: &Path, text: &str) -> bool {
    let file = std::fs::read(path).unwrap_or_default();
    file.windows(text.len()).any(|w| w == text.as_bytes())
}

/// What a start needs of the framework in `game_dir` and of a test game's
/// core: an installed `version.dll` proxy and an active core DLL (either
/// beside the proxy or in Harmony's own Steam Workshop item); a proxy that
/// knows `-harmony_core` and a core with the test games' pipes when starting a
/// test game; for a hidden start a core that knows `-harmony_hidden`; and for
/// a muted start a core that knows `-harmony_mute`.
fn framework_has(game_dir: &Path, start: &Start) -> Result<()> {
    let proxy = game_dir.join("version.dll");
    let core = match &start.test_game {
        Some(game) => {
            if !knows(&proxy, "-harmony_core") {
                return Err(format!(
                    "the framework's proxy in {} cannot load a test game's core: install a build from this one on (cargo xtask install, with no game running)",
                    game_dir.display()
                ));
            }
            // Only a core with the test games' pipes has this line of its log.
            if !knows(&game.core, "serving the test game's own pipe") {
                return Err(format!("{} has no test games' pipes: build it with --features eu4-harmony-core/dev-test-games", game.core.display()));
            }
            game.core.clone()
        }
        None => {
            if !proxy.is_file() {
                return Err(format!("Harmony is not installed in {}: {} is missing", game_dir.display(), proxy.display()));
            }
            eu4_harmony_folders::active_core(game_dir).ok_or_else(|| {
                let places = eu4_harmony_folders::core_places(game_dir, eu4_harmony_folders::PUBLISHED_FILE_ID);
                let looked: Vec<String> = places.iter().map(|p| p.display().to_string()).collect();
                format!("Harmony's core DLL was not found; looked at {}", looked.join(", "))
            })?
        }
    };
    if start.hidden && !knows(&core, "-harmony_hidden") {
        return Err(format!(
            "the framework in {} has no hidden start: install a build that has it (cargo xtask install), or start the game shown",
            game_dir.display()
        ));
    }
    if start.mute && !knows(&core, "-harmony_mute") {
        return Err(format!("the framework in {} does not support -harmony_mute: rebuild or install a current build, or start with sound", game_dir.display()));
    }
    Ok(())
}

/// Starts `eu4.exe` from `game_dir` as `start` says and waits up to `wait`
/// for its main menu; returns the game, held open so that its exit code
/// stays to be read once it ends. `log` is the framework's log, which says
/// why when the game ends first. The player's game starts only with no game
/// running, a test game only while its pipe is not served.
pub fn start(game_dir: &Path, log: &Path, start: &Start, wait: Duration) -> std::result::Result<Game, Failed> {
    let pipe = start.pipe()?;
    match &start.test_game {
        None => crate::game::not_running()?,
        Some(game) if pipe::connect(&pipe, pipe::GAME, Instant::now()).is_ok() => {
            return Err(format!("the test game {} runs already: its pipe is served", game.name).into());
        }
        Some(_) => {}
    }
    framework_has(game_dir, start)?;
    crate::game::steam_ready()?;
    let exe = game_dir.join(crate::game::EXE);
    let started = Instant::now();
    let since = std::time::SystemTime::now();
    let game = Game::spawn(&exe, &start.switches(), game_dir)?;
    let id = game.id();
    let how = match (start.hidden, start.mute) {
        (true, true) => ", hidden, muted",
        (true, false) => ", hidden",
        (false, true) => ", muted",
        (false, false) => "",
    };
    let seed = start.seed.map_or(String::new(), |seed| format!(", -seed={seed}"));
    let test_game = start.test_game.as_ref().map_or(String::new(), |g| format!(", the test game {}", g.name));
    println!("started eu4.exe (process {id}){how}{seed}{test_game}");
    let ended = || game.exit_code().map_or(Ok(()), Err);
    match pipe::main_menu(&pipe, pipe::GAME, started + wait, ended) {
        Ok(_) => {
            println!("at the main menu after {:.0} s", started.elapsed().as_secs_f64());
            Ok(game)
        }
        Err(NoMainMenu::Stopped(code)) => {
            let why = why_it_ended(log, since).unwrap_or_else(|| format!("the framework's log of this start ({}) says nothing of it", log.display()));
            let message = format!("the game ended (exit code {code}) before its main menu: {why}");
            Err(Failed { message, ended: Some(Ended { id, started: game.started(), code }) })
        }
        Err(NoMainMenu::TimedOut) if start.test_game.is_some() => {
            let _ = crate::game::end_game(id);
            Err(format!("the test game (process {id}) had not reached its main menu after {} s; it was ended", wait.as_secs()).into())
        }
        Err(NoMainMenu::TimedOut) => Err(format!("the game (process {id}) had not reached its main menu after {} s; it still runs", wait.as_secs()).into()),
    }
}

/// The framework's own word on why the game ended: the last failed startup,
/// fatal error, crash record, panic, or proxy load error in its logs folder
/// (or in `log` itself when only a single file is given) written since `since`.
fn why_it_ended(log: &Path, since: std::time::SystemTime) -> Option<String> {
    if let Some(dir) = log.parent() {
        if let Some(found) = eu4_harmony_logging::startup_failure_reason(dir, since) {
            return Some(found);
        }
    }
    std::fs::metadata(log).and_then(|m| m.modified()).ok().filter(|&written| written >= since)?;
    let text = std::fs::read_to_string(log).ok()?;
    reason(&text)
}

/// The last line of a log that tells why the framework ended the game, and
/// the lines a failed startup lists after it.
fn reason(log: &str) -> Option<String> {
    eu4_harmony_logging::extract_startup_reason(log, eu4_harmony_logging::CORE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switches(hidden: bool, mute: bool, seed: Option<u32>) -> Vec<String> {
        Start { hidden, mute, seed, ..Default::default() }.switches()
    }

    /// A test game names its user folder, its core and its pipe, a word
    /// quoted whole when it has a space, as the framework reads them back.
    #[test]
    fn a_test_game_names_its_user_folder_core_and_pipe() {
        let game = TestGame { name: "b".into(), user_dir: PathBuf::from(r"C:\claims\b\user"), core: PathBuf::from(r"C:\a b\core.dll") };
        let start = Start { hidden: true, mute: true, seed: None, user_dir: None, test_game: Some(game) };
        let words = start.switches();
        assert_eq!(words, ["-harmony_hidden", "-harmony_mute", r"-userdir=C:\claims\b\user", r#""-harmony_core=C:\a b\core.dll""#, "-harmony_test_game=b"]);
        assert_eq!(start.pipe().unwrap(), r"\\.\pipe\eu4-harmony-b");
        assert_eq!(Start::default().pipe().unwrap(), eu4_harmony_remote::PIPE);
    }

    #[test]
    fn explicit_user_dir_adds_switch_and_framework_has_checks_workshop_and_shown_starts() {
        let with_user = Start { user_dir: Some(PathBuf::from(r"C:\my user\dir")), ..Default::default() };
        assert_eq!(with_user.switches(), [r#""-userdir=C:\my user\dir""#]);

        let root = std::env::temp_dir().join(format!("eu4h-client-framework-has-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("SteamLibrary").join("steamapps").join("common").join("Europa Universalis IV");
        std::fs::create_dir_all(&game).unwrap();

        // Missing proxy fails even for shown start.
        let shown = Start::default();
        assert!(framework_has(&game, &shown).unwrap_err().contains("version.dll is missing"));

        // Proxy present but no core anywhere fails.
        std::fs::write(game.join("version.dll"), b"").unwrap();
        assert!(framework_has(&game, &shown).unwrap_err().contains("core DLL was not found"));

        // Workshop core satisfies shown start, hidden start once it carries `-harmony_hidden`,
        // and muted start once it carries `-harmony_mute`.
        if let Some(workshop_dir) = eu4_harmony_folders::workshop_item(&game, eu4_harmony_folders::PUBLISHED_FILE_ID) {
            let core = workshop_dir.join("eu4-harmony").join("eu4_harmony_core.dll");
            std::fs::create_dir_all(core.parent().unwrap()).unwrap();
            std::fs::write(&core, b"plain core").unwrap();
            assert!(framework_has(&game, &shown).is_ok());

            let hidden = Start { hidden: true, ..Default::default() };
            assert!(framework_has(&game, &hidden).unwrap_err().contains("has no hidden start"));
            let muted = Start { mute: true, ..Default::default() };
            assert!(framework_has(&game, &muted).unwrap_err().contains("-harmony_mute"));

            std::fs::write(&core, b"core supporting -harmony_hidden and -harmony_mute").unwrap();
            assert!(framework_has(&game, &hidden).is_ok());
            assert!(framework_has(&game, &muted).is_ok());
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn hidden_and_mute_are_independent_switches() {
        assert_eq!(switches(false, false, None), Vec::<String>::new());
        assert_eq!(switches(false, true, None), ["-harmony_mute"]);
        assert_eq!(switches(true, false, None), ["-harmony_hidden"]);
        assert_eq!(switches(true, true, None), ["-harmony_hidden", "-harmony_mute"]);
    }

    #[test]
    fn a_seed_is_the_game_s_own_switch() {
        assert_eq!(switches(true, false, Some(7)), ["-harmony_hidden", "-seed=7"]);
        assert_eq!(switches(false, true, Some(0)), ["-harmony_mute", "-seed=0"]);
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
