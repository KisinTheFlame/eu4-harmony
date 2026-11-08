//! A developer build: which development tools are built in, each a feature of
//! its own (`dev-call-trace`, `dev-memory`, `dev-heap-census`,
//! `dev-command-log`, `dev-test-switches`, `dev-write-watch`,
//! `dev-test-games`), and their options. A tool built in is on; its options
//! are in `dev.toml` beside the core, `eu4-harmony\dev.toml` in the game
//! folder or a test game's own, one section a tool, and a section left out
//! takes its defaults (`core/dev.toml.example` shows them all). A player's
//! build has none of this.
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

/// The tools: each one's name, its section in dev.toml, and whether it is
/// built in.
const TOOLS: [(&str, &str, bool); 7] = [
    ("call trace", "call-trace", cfg!(feature = "dev-call-trace")),
    ("memory account", "memory-account", cfg!(feature = "dev-memory")),
    ("heap census", "heap-census", cfg!(feature = "dev-heap-census")),
    ("command log", "command-log", cfg!(feature = "dev-command-log")),
    ("test switches", "test-switches", cfg!(feature = "dev-test-switches")),
    ("write watch", "write-watch", cfg!(feature = "dev-write-watch")),
    ("test games' pipes", "test-games", cfg!(feature = "dev-test-games")),
];

/// What dev.toml says.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub call_trace: CallTrace,
    pub memory_account: MemoryAccount,
    pub heap_census: HeapCensus,
    pub command_log: CommandLog,
    pub test_switches: TestSwitches,
    pub write_watch: WriteWatch,
    pub test_games: TestGames,
    /// The sections dev.toml has, to tell those of tools not built in.
    #[serde(skip)]
    sections: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct CallTrace {
    /// The functions to trace, as RVAs in hex; `RVA:N` gives the number of
    /// arguments, for a function the exception directory has no entry for.
    pub functions: Vec<String>,
    /// How many of each function's first calls are logged.
    pub calls: usize,
    /// Every call of each function is timed too, into `call-times.tsv` next
    /// to the log: its thread, start and end, the game's date and its first
    /// argument.
    pub times: bool,
    /// For a function whose first argument points at an object, an offset
    /// into it, in hex, whose eight bytes the times record with each call
    /// (`{ "0xb0fa0" = "0x20" }`).
    pub fields: BTreeMap<String, String>,
}

impl Default for CallTrace {
    fn default() -> Self {
        Self { functions: Vec::new(), calls: 20, times: false, fields: BTreeMap::new() }
    }
}

/// The memory account has no options: it reads what the process holds
/// when remote control asks.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MemoryAccount {}

/// The census has no options: a generation ends at every world reset and
/// when remote control asks.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeapCensus {}

/// The test games' pipes have no options: the pipe is the one
/// `-harmony_test_game` names (src/pipe.rs).
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TestGames {}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct CommandLog {
    /// The command types to log, by the game's names; every type when empty.
    pub types: Vec<String>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TestSwitches {
    /// Startup fails, so that the failure can be tried on a supported game.
    pub fail_startup: bool,
    /// Two test notices are shown, so that the notice window can be tried.
    pub test_notice: bool,
    /// The game crashes at the first frame of a game, so that what follows a
    /// crash can be tried.
    pub crash: bool,
    /// Framework code panics at the first frame of a game, so that what
    /// follows a panic, under the guard every function the game calls runs
    /// in, can be tried.
    pub panic: bool,
    /// The speed-ups left to the game, by name, group or `all`: their points
    /// are not installed, so that the game's own code can be measured against
    /// them (`core/src/speed.rs`).
    pub game_paths: Vec<String>,
    /// The speed-ups whose answers are checked against the game's, by name,
    /// group or `all`: each difference logged and both counted in the
    /// startup report.
    pub checked: Vec<String>,
    /// A subject relation's end is the game's own: its walk of the subject's
    /// supporters refreshes the subject as it goes, freeing the list it
    /// walks, so that the defect can be tried (with the heap census's
    /// quarantine on, the walk then reads what it freed filled with `0xdd`).
    pub game_ends_dependencies: bool,
    /// The order in which an AI country's top agent has its agents do their
    /// day: `game` leaves the game's walks, `forward` and
    /// `reversed` are the framework's, in the game's order or the opposite
    /// one, so that whether an agent's day depends on another's can be tried
    /// in a game started with `-seed` (`core/src/agents_order.rs`).
    pub agents_order: AgentsOrder,
    /// Commands in their written form, posted through the command layer at
    /// the first frame of every game, so that the game's commands can be
    /// tried.
    pub post: Vec<String>,
    /// Every filled font's page grows at the first frame of every game, so
    /// that a page growing in a game, the names on the map built again with
    /// it, can be tried (`core/src/unicode/fill.rs`).
    pub grow_pages: bool,
}

/// The test switch `agents-order`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentsOrder {
    /// The game's own walks.
    #[default]
    Game,
    /// The framework's walks, in the game's order.
    Forward,
    /// The framework's walks, in the opposite order.
    Reversed,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct WriteWatch {
    /// The function whose writes are watched, as an RVA in hex; none when
    /// empty.
    pub function: String,
    /// How many of its first calls on the main thread are watched.
    pub calls: usize,
    /// The game days that are to pass one after another, as the watched
    /// functions see the date, before the watch begins; 0 begins at once.
    pub after_days: usize,
    /// Functions, as RVAs in hex, whose next call on the main thread before
    /// each watched call of `function` has what it reads watched as well;
    /// that call's writes to what they read are marked.
    pub reads_before: Vec<String>,
    /// Modules, by file name, whose threads go on while a call is watched
    /// (`nvd3dumx.dll`, the graphics driver), what they read and write
    /// recorded as another thread's.
    pub let_run: Vec<String>,
    /// The seconds a watched call may go with nothing read or written before
    /// the watch is given up, where the main thread waits logged; 0 waits for
    /// ever.
    pub give_up_after: u64,
}

impl Default for WriteWatch {
    fn default() -> Self {
        Self { function: String::new(), calls: 1, after_days: 0, reads_before: Vec::new(), let_run: Vec::new(), give_up_after: 30 }
    }
}

impl Options {
    pub fn parse(text: &str) -> crate::Result<Self> {
        let mut options: Options = toml::from_str(text).map_err(|e| e.to_string())?;
        let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        options.sections = table.keys().cloned().collect();
        Ok(options)
    }

    /// The options as they took effect, for the startup report, and the
    /// sections of tools that are not built in, which are not used.
    pub fn described(&self) -> Vec<String> {
        let built = |section: &str| TOOLS.iter().any(|t| t.1 == section && t.2);
        let mut lines = Vec::new();
        if built("call-trace") {
            let (functions, calls) = (&self.call_trace.functions, self.call_trace.calls);
            lines.push(match functions.is_empty() {
                true => "call trace: no functions".to_string(),
                false => format!(
                    "call trace: {}; the first {calls} calls of each{}",
                    functions.join(", "),
                    if self.call_trace.times { ", every call timed into call-times.tsv" } else { "" }
                ),
            });
        }
        if built("memory-account") {
            lines.push("memory account: at memory, from remote control".to_string());
        }
        if built("heap-census") {
            lines.push("heap census: a generation ends at every world reset and at memory.generation".to_string());
        }
        if built("command-log") {
            let types = &self.command_log.types;
            lines.push(format!("command log: {}", if types.is_empty() { "every type".to_string() } else { types.join(", ") }));
        }
        if built("test-switches") {
            let on = |on: bool| if on { "on" } else { "off" };
            let list = |names: &[String]| if names.is_empty() { "none".to_string() } else { names.join(", ") };
            let switches = &self.test_switches;
            lines.push(format!(
                "test switches: fail-startup {}, test-notice {}, crash {}, panic {}, game-paths {}, checked {}, game-ends-dependencies {}, agents-order {}, {} command(s) to post, grow-pages {}",
                on(switches.fail_startup),
                on(switches.test_notice),
                on(switches.crash),
                on(switches.panic),
                list(&switches.game_paths),
                list(&switches.checked),
                on(switches.game_ends_dependencies),
                format!("{:?}", switches.agents_order).to_lowercase(),
                switches.post.len(),
                on(switches.grow_pages)
            ));
        }
        if built("write-watch") {
            let watch = &self.write_watch;
            lines.push(match watch.function.is_empty() {
                true => "write watch: no function".to_string(),
                false => format!(
                    "write watch: {}, its first {} call(s) on the main thread{}{}{}{}",
                    watch.function,
                    watch.calls,
                    match watch.after_days {
                        0 => String::new(),
                        n => format!(" after {n} game days"),
                    },
                    match watch.reads_before.is_empty() {
                        true => String::new(),
                        false => format!(", each after what {} read(s)", watch.reads_before.join(", ")),
                    },
                    match watch.let_run.is_empty() {
                        true => String::new(),
                        false => format!(", the threads of {} not held", watch.let_run.join(", ")),
                    },
                    match watch.give_up_after {
                        0 => String::new(),
                        n => format!(", given up after {n} s with nothing watched"),
                    }
                ),
            });
        }
        if built("test-games") {
            lines.push("test games' pipes: the pipe is the test game's own when -harmony_test_game names it".to_string());
        }
        for (name, section, built) in TOOLS {
            if !built && self.sections.iter().any(|s| s == section) {
                lines.push(format!("{name}: not built in, so [{section}] is not used"));
            }
        }
        lines
    }
}

static OPTIONS: OnceLock<Options> = OnceLock::new();
/// Where the options came from, for the report.
static SOURCE: OnceLock<String> = OnceLock::new();

/// "developer build with the call trace, …": what this build is, for the
/// first lines of the log and the startup report.
pub fn build() -> String {
    let built: Vec<String> = TOOLS.iter().filter(|t| t.2).map(|t| format!("the {}", t.0)).collect();
    format!("developer build with {}", built.join(", "))
}

/// Reads `dev.toml` beside the core, in `eu4-harmony\` in the game folder or
/// in a test game's own folder; with no such file every tool takes its
/// defaults. A file that cannot be read fails startup.
pub fn load(core_dir: &Path) -> crate::Result<()> {
    let path = core_dir.join("dev.toml");
    let (options, source) = match std::fs::read_to_string(&path) {
        Ok(text) => (Options::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?, path.display().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Options::default(), format!("defaults, no {}", path.display())),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let _ = OPTIONS.set(options);
    let _ = SOURCE.set(source);
    Ok(())
}

/// The options; the defaults until [`load`].
pub fn options() -> &'static Options {
    OPTIONS.get_or_init(Options::default)
}

/// The report's lines on the tools' options, which are none when dev.toml
/// could not be read.
pub fn report_lines() -> Vec<String> {
    let (Some(options), Some(source)) = (OPTIONS.get(), SOURCE.get()) else {
        return vec!["developer tools' options: dev.toml was not read".to_string()];
    };
    let mut lines = vec![format!("developer tools' options ({source}):")];
    lines.extend(options.described().into_iter().map(|l| format!("  {l}")));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_section_left_out_takes_its_defaults() {
        let options = Options::parse("[command-log]\ntypes = [\"buildunit\"]\n").unwrap();
        assert_eq!(options.command_log.types, ["buildunit"]);
        assert_eq!(options.call_trace, CallTrace::default());
        assert_eq!(options.call_trace.calls, 20);
        assert_eq!(options.test_switches, TestSwitches::default());
        assert_eq!(options.write_watch, WriteWatch::default());
        assert_eq!(options.write_watch.calls, 1);
        assert_eq!(Options::parse("").unwrap(), Options::default());
    }

    #[test]
    fn every_option_is_read() {
        let text = "[call-trace]\nfunctions = [\"0x15988e0\", \"0x159c600:3\"]\ncalls = 5\ntimes = true\nfields = { \"0x15988e0\" = \"0x20\" }\n\n[heap-census]\n\n[test-switches]\nfail-startup = true\ntest-notice = true\ngame-paths = [\"saving\"]\nchecked = [\"music\", \"lookups\"]\nagents-order = \"reversed\"\npost = [\"a={ }\"]\n\n[write-watch]\nfunction = \"0x74ff30\"\ncalls = 3\nafter-days = 30\nreads-before = [\"0x10a59a0\"]\nlet-run = [\"nvd3dumx.dll\"]\ngive-up-after = 10\n";
        let options = Options::parse(text).unwrap();
        assert_eq!(options.call_trace.functions, ["0x15988e0", "0x159c600:3"]);
        assert_eq!(options.call_trace.calls, 5);
        assert!(options.call_trace.times);
        assert_eq!(options.call_trace.fields, BTreeMap::from([("0x15988e0".to_string(), "0x20".to_string())]));
        assert!(options.test_switches.fail_startup && options.test_switches.test_notice);
        assert_eq!(options.test_switches.game_paths, ["saving"]);
        assert_eq!(options.test_switches.checked, ["music", "lookups"]);
        assert_eq!(options.test_switches.agents_order, AgentsOrder::Reversed);
        assert_eq!(options.test_switches.post, ["a={ }"]);
        assert_eq!(
            options.write_watch,
            WriteWatch {
                function: "0x74ff30".into(),
                calls: 3,
                after_days: 30,
                reads_before: vec!["0x10a59a0".into()],
                let_run: vec!["nvd3dumx.dll".into()],
                give_up_after: 10
            }
        );
    }

    #[test]
    fn what_is_not_an_option_is_refused() {
        assert!(Options::parse("[command-log]\ntype = [\"buildunit\"]\n").is_err());
        assert!(Options::parse("[commands]\n").is_err());
        assert!(Options::parse("[heap-census]\nevery = 2\n").is_err());
        assert!(Options::parse("[call-trace]\ncalls = \"20\"\n").is_err());
    }

    #[test]
    fn the_example_is_read() {
        Options::parse(include_str!("../dev.toml.example")).unwrap();
    }
}
