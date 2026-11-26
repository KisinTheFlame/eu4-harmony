//! A developer build's options, read from `dev.toml` beside the core
//! (`eu4-harmony\dev.toml` in the game folder or a test game's own folder).
//! Every development tool is compiled into a `dev` build; tools with runtime
//! hooks or listeners (`heap-census`, `crossings`, `command-log`,
//! `performance`) are off unless their section sets `enabled = true`, while
//! `memory-account`, `test-switches`, `call-trace` and `write-watch` act only
//! when configured or requested (`core/dev.toml.example` shows every option).
use super::Result;
use std::path::Path;

/// What this build is, appended after the version and API version in logs and
/// the startup report; `None` for a player build.
pub(crate) fn build_label() -> Option<&'static str> {
    #[cfg(feature = "dev")]
    return Some("developer build");
    #[cfg(all(feature = "dev-test-games", not(feature = "dev")))]
    return Some("test games' pipes");
    #[cfg(not(feature = "dev-test-games"))]
    return None;
}

/// Developer-only localisation files embedded into the framework catalogue.
#[cfg(feature = "dev")]
pub(crate) fn localisations() -> &'static [&'static str] {
    &[
        include_str!("../../localisation/dev/eu4harmony_dev_l_english.yml"),
        include_str!("../../localisation/dev/eu4harmony_dev_l_french.yml"),
        include_str!("../../localisation/dev/eu4harmony_dev_l_german.yml"),
        include_str!("../../localisation/dev/eu4harmony_dev_l_spanish.yml"),
        include_str!("../../localisation/dev/eu4harmony_dev_l_simp_chinese.yml"),
    ]
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn localisations() -> &'static [&'static str] {
    &[]
}

/// Reads and validates `dev.toml` beside the core in a developer build.
#[cfg(feature = "dev")]
pub(crate) fn load(core_dir: &Path) -> Result<()> {
    parsed::load(core_dir)
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn load(_core_dir: &Path) -> Result<()> {
    Ok(())
}

/// Startup report lines describing active development tool options.
#[cfg(feature = "dev")]
pub(crate) fn report_lines() -> Vec<String> {
    parsed::report_lines()
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn report_lines() -> Vec<String> {
    Vec::new()
}

#[cfg(feature = "dev")]
pub(crate) fn fail_startup() -> bool {
    parsed::options().test_switches.fail_startup
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn fail_startup() -> bool {
    false
}

#[cfg(feature = "dev")]
pub(crate) fn test_notice() -> bool {
    parsed::options().test_switches.test_notice
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn test_notice() -> bool {
    false
}

#[cfg(feature = "dev")]
pub(crate) fn game_ends_dependencies() -> bool {
    parsed::options().test_switches.game_ends_dependencies
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn game_ends_dependencies() -> bool {
    false
}

#[cfg(feature = "dev")]
pub(crate) fn game_shares_ai_draws() -> bool {
    parsed::options().test_switches.game_shares_ai_draws
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn game_shares_ai_draws() -> bool {
    false
}

/// The `game-paths` and `checked` speed-up lists from `dev.toml`.
#[cfg(feature = "dev")]
pub(crate) fn speed_switches() -> (&'static [String], &'static [String]) {
    let switches = &parsed::options().test_switches;
    (&switches.game_paths, &switches.checked)
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn speed_switches() -> (&'static [String], &'static [String]) {
    (&[], &[])
}

/// Whether periodic repair statistics should be logged.
#[cfg(feature = "dev")]
#[inline(always)]
pub(crate) const fn log_repair_stats() -> bool {
    true
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn log_repair_stats() -> bool {
    false
}

/// Whether extra repair invariant checks should run.
#[cfg(feature = "dev")]
pub(crate) fn check_repairs() -> bool {
    parsed::options().heap_census.enabled
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn check_repairs() -> bool {
    false
}

#[cfg(feature = "dev")]
pub(crate) fn options() -> &'static Options {
    parsed::options()
}

#[cfg(feature = "dev")]
pub(crate) use parsed::{AgentsOrder, Options};

#[cfg(any(test, feature = "dev"))]
mod parsed {
    use serde::Deserialize;
    use std::collections::BTreeMap;
    #[cfg(feature = "dev")]
    use std::path::Path;
    #[cfg(feature = "dev")]
    use std::sync::OnceLock;

    /// What `dev.toml` says.
    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
    pub struct Options {
        pub call_trace: CallTrace,
        pub memory_account: MemoryAccount,
        pub performance: Performance,
        pub heap_census: HeapCensus,
        pub command_log: CommandLog,
        pub test_switches: TestSwitches,
        pub write_watch: WriteWatch,
        pub test_games: TestGames,
        pub crossings: Crossings,
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

    /// Bounded on-demand performance windows (`performance.begin` / `performance.end`).
    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    pub struct Performance {
        /// Installs the performance hooks and allows `performance.begin`/`end`.
        pub enabled: bool,
    }

    /// The heap census counts live game-heap blocks by site and generation.
    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    pub struct HeapCensus {
        /// Hooks the game's C runtime allocator and enables `memory.leftovers`,
        /// `memory.watch`, `memory.generation`, `memory.quarantine`,
        /// `memory.pages` and `memory.uncounted`.
        pub enabled: bool,
    }

    /// The test games' pipes have no options: the pipe is the one
    /// `-harmony_test_game` names (src/pipe.rs).
    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    pub struct TestGames {}

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    pub struct CommandLog {
        /// Logs commands the game executes through the command layer.
        pub enabled: bool,
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
        /// Every AI country draws from the world's one random generator as the
        /// game's do, on several threads at once, instead of from a stream of its
        /// own, so that the repair can be measured against it
        /// (`core/src/repairs/ai_draws.rs`).
        pub game_shares_ai_draws: bool,
        /// The order in which an AI country's top agent has its agents do their
        /// day: `game` leaves the game's walks, `forward` and
        /// `reversed` are the framework's, in the game's order or the opposite
        /// one, so that whether an agent's day depends on another's can be tried
        /// in a game started with `-seed` (`core/src/dev/agents_order.rs`).
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
        /// The drawing thread's watch instead of a function's
        /// (`write_watch::drawing`): what the game's code reads in the drawing
        /// thread's first `drawing-frames` frames, every other thread held but
        /// `let-run`'s, then who writes it in the next `drawing-days` days, the
        /// main thread without the presentation lock first.
        pub drawing: bool,
        pub drawing_frames: usize,
        pub drawing_days: usize,
    }

    impl WriteWatch {
        /// Whether the write watch is configured to watch a function or the
        /// drawing thread.
        pub fn active(&self) -> bool {
            !self.function.is_empty() || self.drawing
        }
    }

    impl Default for WriteWatch {
        fn default() -> Self {
            Self {
                function: String::new(),
                calls: 1,
                after_days: 0,
                reads_before: Vec::new(),
                let_run: Vec::new(),
                give_up_after: 30,
                drawing: false,
                drawing_frames: 1,
                drawing_days: 3,
            }
        }
    }

    /// Where the day goes into the interface's or the graphics' code.
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
    pub struct Crossings {
        /// Installs the exception handler that counts crossings during each day.
        pub enabled: bool,
        /// How many game days pass between two reports in the framework log.
        pub report_every: usize,
    }

    impl Default for Crossings {
        fn default() -> Self {
            Self { enabled: false, report_every: 30 }
        }
    }

    impl Options {
        pub fn parse(text: &str) -> crate::base::Result<Self> {
            let options: Options = toml::from_str(text).map_err(|e| e.to_string())?;
            options.validate()?;
            Ok(options)
        }

        /// Refuses tool combinations that cannot run together.
        fn validate(&self) -> crate::base::Result<()> {
            if self.heap_census.enabled && self.write_watch.active() {
                return Err("[heap-census] and [write-watch] cannot both be enabled: suspended threads may hold a census lock".into());
            }
            Ok(())
        }

        /// The options as they took effect, for the startup report.
        #[cfg(feature = "dev")]
        pub fn described(&self) -> Vec<String> {
            let on = |on: bool| if on { "on" } else { "off" };
            let list = |names: &[String]| if names.is_empty() { "none".to_string() } else { names.join(", ") };
            let mut lines = Vec::new();
            let (functions, calls) = (&self.call_trace.functions, self.call_trace.calls);
            lines.push(match functions.is_empty() {
                true => "call trace: off (no functions)".to_string(),
                false => format!(
                    "call trace: {}; the first {calls} calls of each{}",
                    functions.join(", "),
                    if self.call_trace.times { ", every call timed into call-times.tsv" } else { "" }
                ),
            });
            lines.push("memory account: at memory, from remote control".to_string());
            lines.push(match self.performance.enabled {
                true => "performance: on (bounded windows at performance.begin and performance.end)".to_string(),
                false => "performance: off".to_string(),
            });
            lines.push(match self.heap_census.enabled {
                true => "heap census: on (a generation ends at every world reset and at memory.generation)".to_string(),
                false => "heap census: off".to_string(),
            });
            lines.push(match self.command_log.enabled {
                true => {
                    let types = &self.command_log.types;
                    format!("command log: on ({})", if types.is_empty() { "every type".to_string() } else { types.join(", ") })
                }
                false => "command log: off".to_string(),
            });
            let switches = &self.test_switches;
            lines.push(format!(
            "test switches: fail-startup {}, test-notice {}, crash {}, panic {}, game-paths {}, checked {}, game-ends-dependencies {}, game-shares-ai-draws {}, agents-order {}, {} command(s) to post, grow-pages {}",
            on(switches.fail_startup),
            on(switches.test_notice),
            on(switches.crash),
            on(switches.panic),
            list(&switches.game_paths),
            list(&switches.checked),
            on(switches.game_ends_dependencies),
            on(switches.game_shares_ai_draws),
            format!("{:?}", switches.agents_order).to_lowercase(),
            switches.post.len(),
            on(switches.grow_pages)
        ));
            lines.push(match self.crossings.enabled {
                true => format!("crossings: on (a report every {} day(s))", self.crossings.report_every),
                false => "crossings: off".to_string(),
            });
            let watch = &self.write_watch;
            lines.push(match watch.function.is_empty() {
                true if watch.drawing => format!(
                    "write watch: the drawing thread's reads in {} frame(s){}, then their writes for {} day(s)",
                    watch.drawing_frames,
                    match watch.after_days {
                        0 => String::new(),
                        n => format!(" after {n} game days"),
                    },
                    watch.drawing_days
                ),
                true => "write watch: off (no function)".to_string(),
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
            lines.push("test games' pipes: the pipe is the test game's own when -harmony_test_game names it".to_string());
            lines
        }
    }

    #[cfg(feature = "dev")]
    static OPTIONS: OnceLock<Options> = OnceLock::new();
    /// Where the options came from, for the report.
    #[cfg(feature = "dev")]
    static SOURCE: OnceLock<String> = OnceLock::new();

    /// Reads `dev.toml` beside the core, in `eu4-harmony\` in the game folder or
    /// in a test game's own folder; with no such file every tool takes its
    /// defaults. A file that cannot be read or validated fails startup.
    #[cfg(feature = "dev")]
    pub(super) fn load(core_dir: &Path) -> super::Result<()> {
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
    #[cfg(feature = "dev")]
    pub(super) fn options() -> &'static Options {
        OPTIONS.get_or_init(Options::default)
    }

    /// The report's lines on the tools' options, which are none when `dev.toml`
    /// could not be read.
    #[cfg(feature = "dev")]
    pub(super) fn report_lines() -> Vec<String> {
        let (Some(options), Some(source)) = (OPTIONS.get(), SOURCE.get()) else {
            return vec!["developer tools' options: dev.toml was not read".to_string()];
        };
        let mut lines = vec![format!("developer tools' options ({source}):")];
        lines.extend(options.described().into_iter().map(|l| format!("  {l}")));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::parsed::*;
    use std::collections::BTreeMap;

    #[test]
    fn a_section_left_out_takes_its_defaults() {
        let options = Options::parse("[command-log]\nenabled = true\ntypes = [\"buildunit\"]\n").unwrap();
        assert!(options.command_log.enabled);
        assert_eq!(options.command_log.types, ["buildunit"]);
        assert_eq!(options.call_trace, CallTrace::default());
        assert_eq!(options.call_trace.calls, 20);
        assert_eq!(options.test_switches, TestSwitches::default());
        assert_eq!(options.write_watch, WriteWatch::default());
        assert_eq!(options.write_watch.calls, 1);
        assert!(!options.heap_census.enabled);
        assert!(!options.performance.enabled);
        assert!(!options.crossings.enabled);
        assert_eq!(Options::parse("").unwrap(), Options::default());
    }

    #[test]
    fn every_option_is_read() {
        let text = "[call-trace]\nfunctions = [\"0x15988e0\", \"0x159c600:3\"]\ncalls = 5\ntimes = true\nfields = { \"0x15988e0\" = \"0x20\" }\n\n[performance]\nenabled = true\n\n[command-log]\nenabled = true\ntypes = [\"buildunit\"]\n\n[crossings]\nenabled = true\nreport-every = 15\n\n[test-switches]\nfail-startup = true\ntest-notice = true\ngame-paths = [\"saving\"]\nchecked = [\"music\", \"lookups\"]\ngame-shares-ai-draws = true\nagents-order = \"reversed\"\npost = [\"a={ }\"]\n\n[write-watch]\nfunction = \"0x74ff30\"\ncalls = 3\nafter-days = 30\nreads-before = [\"0x10a59a0\"]\nlet-run = [\"nvd3dumx.dll\"]\ngive-up-after = 10\ndrawing = true\ndrawing-frames = 2\ndrawing-days = 5\n";
        let options = Options::parse(text).unwrap();
        assert_eq!(options.call_trace.functions, ["0x15988e0", "0x159c600:3"]);
        assert_eq!(options.call_trace.calls, 5);
        assert!(options.call_trace.times);
        assert_eq!(options.call_trace.fields, BTreeMap::from([("0x15988e0".to_string(), "0x20".to_string())]));
        assert!(options.performance.enabled);
        assert!(options.command_log.enabled);
        assert_eq!(options.command_log.types, ["buildunit"]);
        assert!(options.crossings.enabled);
        assert_eq!(options.crossings.report_every, 15);
        assert!(options.test_switches.fail_startup && options.test_switches.test_notice);
        assert_eq!(options.test_switches.game_paths, ["saving"]);
        assert_eq!(options.test_switches.checked, ["music", "lookups"]);
        assert!(options.test_switches.game_shares_ai_draws);
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
                give_up_after: 10,
                drawing: true,
                drawing_frames: 2,
                drawing_days: 5,
            }
        );
    }

    #[test]
    fn heap_census_and_write_watch_are_mutually_exclusive() {
        assert!(Options::parse("[heap-census]\nenabled = true\n[write-watch]\nfunction = \"0x74ff30\"\n").is_err());
        assert!(Options::parse("[heap-census]\nenabled = true\n[write-watch]\ndrawing = true\n").is_err());
        assert!(Options::parse("[heap-census]\nenabled = true\n[write-watch]\n").unwrap().heap_census.enabled);
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
        Options::parse(include_str!("../../dev.toml.example")).unwrap();
    }
}
