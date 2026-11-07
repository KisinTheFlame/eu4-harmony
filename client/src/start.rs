//! Starting the game from its folder, for `eu4-harmony start` and the claim
//! of `cargo xtask game`: hidden or shown (the framework's switch
//! `-harmony_hidden`, docs/hidden-start.md), the player's game or a test game
//! with a user folder, a core and a pipe of its own (docs/test-games.md),
//! returning once remote control says the main menu is up. Steam must be
//! running, as for any start of the game.
use crate::pipe;
use crate::Result;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long the game may take to reach its main menu without `--timeout`.
pub const WAIT: Duration = Duration::from_secs(300);
/// How often to ask remote control where the game is.
const ASK_EVERY: Duration = Duration::from_millis(500);

/// How the game is started.
#[derive(Clone, Debug, Default)]
pub struct Start {
    /// The framework's hidden start.
    pub hidden: bool,
    /// Sound in a hidden start.
    pub sound: bool,
    /// The game's own `-seed`, which seeds its random generator and gives
    /// each AI agent a generator of its own (docs/research/late-game.md).
    pub seed: Option<u32>,
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
            if self.sound {
                words.push("-harmony_sound".to_string());
            }
        }
        words.extend(self.seed.map(|seed| format!("-seed={seed}")));
        if let Some(game) = &self.test_game {
            words.push(path("-userdir", &game.user_dir));
            words.push(path("-harmony_core", &game.core));
            words.push(format!("-harmony_test_game={}", game.name));
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
/// core: a proxy that knows `-harmony_core`, the only one that loads a test
/// game's core; a test game's core with the test games' pipes, without which
/// it would serve the player's pipe; and for a hidden start a core that
/// knows the switch, without which nothing would keep the game hidden.
fn framework_has(game_dir: &Path, start: &Start) -> Result<()> {
    let proxy = game_dir.join("version.dll");
    let core = start.test_game.as_ref().map_or_else(|| game_dir.join("eu4-harmony").join("eu4_harmony_core.dll"), |g| g.core.clone());
    if let Some(game) = &start.test_game {
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
    }
    if start.hidden && !(proxy.is_file() && knows(&core, "-harmony_hidden")) {
        return Err(format!(
            "the framework in {} has no hidden start: install a build that has it (cargo xtask install), or start the game shown",
            game_dir.display()
        ));
    }
    Ok(())
}

/// Starts `eu4.exe` from `game_dir` as `start` says and waits up to `wait`
/// for its main menu; returns the game's process id. `log` is the
/// framework's log, which says why when the game ends first. The player's
/// game starts only with no game running, a test game only while its pipe
/// is not served.
pub fn start(game_dir: &Path, log: &Path, start: &Start, wait: Duration) -> Result<u32> {
    let pipe = start.pipe()?;
    match &start.test_game {
        None => crate::game_not_running()?,
        Some(game) if pipe::connect(&pipe, pipe::GAME, Instant::now()).is_ok() => {
            return Err(format!("the test game {} runs already: its pipe is served", game.name));
        }
        Some(_) => {}
    }
    framework_has(game_dir, start)?;
    let exe = game_dir.join("eu4.exe");
    let started = Instant::now();
    let since = std::time::SystemTime::now();
    let game = Game::spawn(&exe, &start.switches(), game_dir)?;
    let id = game.id;
    let how = match (start.hidden, start.sound) {
        (true, true) => ", hidden, with sound",
        (true, false) => ", hidden",
        (false, _) => "",
    };
    let seed = start.seed.map_or(String::new(), |seed| format!(", -seed={seed}"));
    let test_game = start.test_game.as_ref().map_or(String::new(), |g| format!(", the test game {}", g.name));
    println!("started eu4.exe (process {id}){how}{seed}{test_game}");
    loop {
        if let Some(code) = game.exit_code() {
            let why = why_it_ended(log, since).unwrap_or_else(|| format!("the framework's log of this start ({}) says nothing of it", log.display()));
            return Err(format!("the game ended (exit code {code}) before its main menu: {why}"));
        }
        if at_main_menu(&pipe) {
            println!("at the main menu after {:.0} s", started.elapsed().as_secs_f64());
            return Ok(id);
        }
        if started.elapsed() > wait {
            if start.test_game.is_some() {
                game.end();
                return Err(format!("the test game (process {id}) had not reached its main menu after {} s; it was ended", wait.as_secs()));
            }
            return Err(format!("the game (process {id}) had not reached its main menu after {} s; it still runs", wait.as_secs()));
        }
        std::thread::sleep(ASK_EVERY);
    }
}

/// The game's process, started with none of this command's handles: one
/// that kept this command's output, a pipe, would keep whoever reads it
/// waiting until the game ends, and every inheritable handle is passed on
/// unless none is.
struct Game {
    process: windows_sys::Win32::Foundation::HANDLE,
    id: u32,
}

impl Game {
    fn spawn(exe: &Path, switches: &[String], dir: &Path) -> Result<Self> {
        use windows_sys::Win32::System::Threading::{CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW};
        let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let mut line = wide(&std::iter::once(format!("\"{}\"", exe.display())).chain(switches.iter().cloned()).collect::<Vec<_>>().join(" "));
        let (exe_w, dir_w) = (wide(&exe.display().to_string()), wide(&dir.display().to_string()));
        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let made = unsafe {
            CreateProcessW(exe_w.as_ptr(), line.as_mut_ptr(), std::ptr::null(), std::ptr::null(), 0, 0, std::ptr::null(), dir_w.as_ptr(), &startup, &mut info)
        };
        if made == 0 {
            return Err(format!("cannot start {}: {}", exe.display(), std::io::Error::last_os_error()));
        }
        unsafe { windows_sys::Win32::Foundation::CloseHandle(info.hThread) };
        Ok(Self { process: info.hProcess, id: info.dwProcessId })
    }

    /// Ends it, as `game stop` does.
    fn end(&self) {
        unsafe { windows_sys::Win32::System::Threading::TerminateProcess(self.process, 1) };
    }

    /// Its exit code, once it has ended.
    fn exit_code(&self) -> Option<u32> {
        use windows_sys::Win32::{
            Foundation::WAIT_OBJECT_0,
            System::Threading::{GetExitCodeProcess, WaitForSingleObject},
        };
        let mut code = 0;
        (unsafe { WaitForSingleObject(self.process, 0) } == WAIT_OBJECT_0 && unsafe { GetExitCodeProcess(self.process, &mut code) } != 0).then_some(code)
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.process) };
    }
}
/// Whether remote control, on the pipe `name`, answers that the main menu is
/// up.
fn at_main_menu(name: &str) -> bool {
    let deadline = Instant::now() + ASK_EVERY;
    let Ok(mut client) = pipe::connect(name, pipe::GAME, deadline) else { return false };
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

    fn switches(hidden: bool, sound: bool, seed: Option<u32>) -> Vec<String> {
        Start { hidden, sound, seed, test_game: None }.switches()
    }

    /// A test game names its user folder, its core and its pipe, a word
    /// quoted whole when it has a space, as the framework reads them back.
    #[test]
    fn a_test_game_names_its_user_folder_core_and_pipe() {
        let game = TestGame { name: "b".into(), user_dir: PathBuf::from(r"C:\claims\b\user"), core: PathBuf::from(r"C:\a b\core.dll") };
        let start = Start { hidden: true, sound: false, seed: None, test_game: Some(game) };
        let words = start.switches();
        assert_eq!(words, ["-harmony_hidden", r"-userdir=C:\claims\b\user", r#""-harmony_core=C:\a b\core.dll""#, "-harmony_test_game=b"]);
        assert_eq!(start.pipe().unwrap(), r"\\.\pipe\eu4-harmony-b");
        assert_eq!(Start::default().pipe().unwrap(), eu4_harmony_remote::PIPE);
    }

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
