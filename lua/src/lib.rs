//! The Lua host (ADR extensions-are-lua-in-a-sandbox): runs a mod's extension,
//! its Lua scripts, in a Lua state of its own, in the sandbox, and calls its
//! handlers.
//!
//! An extension's `main.lua` runs once, when it is opened, and returns its
//! handlers. Every handler call runs as a task
//! (ADR handlers-run-as-tasks-that-can-wait), which may wait for an answer and
//! is taken up again by a later [`Extension::poll`]; a task `spawn` starts has
//! a handle, which other tasks await and which cancels it
//! (ADR tasks-are-awaited-like-promises). `game`, the same table for
//! every handler, reaches the capability catalogue through the [`Host`], which
//! is the running game in the core and a fake game in the command line tool, so
//! both run the same host. An error ends the call and is logged, and the
//! extension is called again (ADR an-error-does-not-stop-an-extension); three
//! stretches in a row stopped by the time limit ([`watch`]) stop it. The
//! sensitive capabilities (ADR sensitive-capabilities-are-declared) are `files`
//! and `http.fetch`, with the clock in the sandbox.
pub mod dispatch;
pub mod fetch;
mod files;
pub mod images;
pub mod layout;
pub mod numbers;
mod sandbox;
pub mod text;
mod times;
pub mod values;
mod watch;
pub mod windows;
pub use text::Text;

use eu4_harmony_remote::{Code, Request};
use mlua::{Function, Lua, MultiValue, Table, Thread, ThreadStatus, Value as LuaValue};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};

pub use eu4_harmony_manifest::Permissions;
pub use times::{Times, SLOW};
pub use watch::LIMIT;
pub use windows::{Popup, Property, WindowChange};

/// The same error at the same place is written in full this many times in a
/// session, then only counted.
const WRITTEN: u32 = 3;

/// Stretches stopped by the time limit in a row that stop the extension.
const TIMEOUTS: u32 = 3;

// ---------- the host ----------

/// What an extension reaches beyond its Lua state: the game, its interface
/// and map, its texts, the network, its log and the player. The running game
/// in the core, a fake game in the command line tool. `extension` is the
/// index the extension was opened with.
pub trait Host: Send + Sync {
    /// Runs a capability of the catalogue as the extension.
    fn run(&self, extension: usize, request: Request) -> Called;
    /// Sets a property of one of the extension's interface elements.
    fn set_element(&self, extension: usize, element: &str, value: Element) -> Result<(), String>;
    /// Makes a change to one of the extension's windows, checked against
    /// its layout already.
    fn window(&self, extension: usize, change: WindowChange) -> Result<(), String>;
    /// The tooltip content an extension currently owns. A window name selects
    /// a declared window widget; none selects a supported hosted element.
    fn tooltip(&self, extension: usize, window: Option<&str>, widget: &str) -> Result<Text, String>;
    /// Shows a pop-up of the extension's, and gives its number. A button
    /// pressed comes back as a [`dispatch::PopupPressed`], which runs the
    /// button's action; the pop-up stays open until it is closed.
    fn popup(&self, extension: usize, popup: Popup) -> Result<u32, String>;
    /// Closes the extension's pop-up `popup`; nothing when it is not open.
    fn close_popup(&self, extension: usize, popup: u32) -> Result<(), String>;
    /// Paints provinces in the extension's colors, each `[r, g, b, a]`.
    fn paint(&self, extension: usize, colors: &[(u32, [u8; 4])]) -> Result<(), eu4_harmony_remote::Error>;
    /// Gives the map back.
    fn release_map(&self, extension: usize) -> Result<(), eu4_harmony_remote::Error>;
    /// The mod's text for `key` in the player language, its `$NAME$` places
    /// not yet filled.
    fn text(&self, id: &str, key: &str) -> Option<String>;
    /// The player language's Paradox name.
    fn language(&self) -> String;
    /// Sends an HTTP request the manifest allows; its answer comes later.
    fn fetch(&self, request: fetch::Request) -> Box<dyn Pending<fetch::Answer>>;
    /// A line for the mod's own log.
    fn log(&self, id: &str, line: &str);
    /// A counted handler error after its first few written occurrences. Returns
    /// true when the host summarizes repeated counts in its bounded journal so
    /// [`Extension::session_over`] does not emit a duplicate summary line.
    fn repeated_error(&self, _id: &str, _message: &str) -> bool {
        false
    }
    /// Flushes periodic summaries, or all pending counts at a session change.
    fn maintain_logs(&self, _checkpoint: bool) {}
    /// The number of the session the game is in, for counting errors by
    /// session.
    fn session(&self) -> u64;
    /// The extension's first error of a session: what the player is told.
    fn first_error(&self, extension: &Extension, from: &str, message: &str);
    /// The extension was stopped for `reason`, for the rest of the game.
    fn stopped(&self, extension: &Extension, reason: &str);
}

/// What running a capability came to.
pub enum Called {
    Done(Answered),
    /// Its answer comes later.
    Later(Box<dyn Pending<Answered>>),
}

/// A capability's answer as Lua is to have it, written as JSON in Lua's form
/// (`eu4_harmony_checks::form::as_lua`: an army's ID a number, a date an
/// object of year, month and day); `null` for one with no result. Or why it
/// failed.
pub type Answered = Result<Value, eu4_harmony_remote::Error>;

/// An answer still to come, asked for whenever the host polls its
/// extensions.
pub trait Pending<T>: Send {
    fn poll(&mut self) -> Option<T>;
}

/// A property of an interface element, as `game.ui.<element>` sets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Element {
    Visible(bool),
    Enabled(bool),
    Text(Text),
    Tooltip(Text),
}

/// What a handler gets after `game`.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// Any value JSON can hold; an object of year, month and day is a date.
    Json(Value),
    Text(String),
    Click {
        right: bool,
        shift: bool,
        ctrl: bool,
        /// The row of a list a button of an extension window is in, from 1.
        row: Option<u32>,
    },
}

/// The mod an extension is opened for.
pub struct Mod<'a> {
    pub id: &'a str,
    /// Its name, for the player.
    pub name: &'a str,
    /// Its `eu4-harmony/` folder.
    pub harmony: PathBuf,
    pub permissions: &'a Permissions,
    /// Its data folder, for `files`; none where there is no game's user
    /// folder.
    pub data: Option<PathBuf>,
}

// ---------- extensions ----------

/// One extension.
pub struct Extension {
    index: usize,
    /// Unique among every extension this process opens, for telling which
    /// one runs.
    serial: u64,
    id: String,
    name: String,
    host: Arc<dyn Host>,
    lua: Lua,
    /// Its main thread, a `lua_State*`, on which the error of a task the
    /// host runs is written.
    main: usize,
    handlers: Table,
    /// The windows its `main.lua` returns, laid out.
    windows: Vec<layout::Window>,
    images: Vec<images::Image>,
    /// The `game` every handler gets.
    game: Table,
    /// The prelude's own function that runs a pop-up's button's action.
    press: Function,
    /// The prelude's own function that marks a spawned task's handle
    /// cancelled, for a task dropped with its session.
    dropped: Function,
    /// The sentinel a function that waits returns to its Lua wrapper.
    wait: LuaValue,
    /// What its tasks are to wait for, shared with its sandbox.
    waits: Arc<Mutex<Waits>>,
    state: Mutex<State>,
}

/// What changes as the extension runs.
#[derive(Default)]
struct State {
    stopped: Option<String>,
    /// Every stretch since it was loaded, and those of the session going on.
    all: Times,
    session: Times,
    timeouts: u32,
    /// Errors of this session by where and what, and how often.
    errors: HashMap<String, u32>,
    /// Whether the host took over repeated-error summaries for this session.
    host_summarized: bool,
    /// The session whose first error the player was told of.
    told: Option<u64>,
    /// Tasks waiting for an answer.
    tasks: Vec<Task>,
    /// Spawned tasks cancelled while they ran or were being taken up, by
    /// their handle: dropped at their next wait, or forgotten when they end
    /// first (ADR tasks-are-awaited-like-promises).
    cancelling: HashSet<usize>,
}

/// What tasks are to wait for, by number, until their task takes it as it
/// gives up its turn.
#[derive(Default)]
struct Waits {
    by_number: HashMap<u64, Waiting>,
    next: u64,
}

impl Waits {
    /// Keeps `wait`: the wait sentinel and its number, which the running
    /// task gives up its turn with.
    fn keep(waits: &Mutex<Waits>, sentinel: &LuaValue, wait: Waiting) -> MultiValue {
        let mut waits = waits.lock().unwrap_or_else(|e| e.into_inner());
        waits.next += 1;
        let number = waits.next;
        waits.by_number.insert(number, wait);
        MultiValue::from_iter([sentinel.clone(), LuaValue::Integer(number as i64)])
    }
}

/// A task that waits.
struct Task {
    thread: Thread,
    wait: Waiting,
    /// For the log: the handler it came from.
    from: String,
    /// A spawned task's handle, which `spawn` returned; none for a
    /// handler's.
    handle: Option<Table>,
}

impl Task {
    fn key(&self) -> Option<usize> {
        self.handle.as_ref().map(key)
    }
}

/// A spawned task's handle as [`State::cancelling`] keeps it.
fn key(handle: &Table) -> usize {
    handle.to_pointer() as usize
}

/// What a task waits for: a capability's answer, a response from the
/// network by its deadline, or any of some spawned tasks to end.
enum Waiting {
    Capability(Box<dyn Pending<Answered>>),
    Fetch(Box<dyn Pending<fetch::Answer>>, Instant),
    /// The handles of the tasks awaited; the prelude sets `ended` on one
    /// when its task ends.
    Tasks(Vec<Table>),
}

/// The most passes over the waiting tasks one poll makes: a task that ends
/// in a pass wakes those awaiting it in the next, within the same frame.
const PASSES: usize = 16;

static SERIAL: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// The extension running on this thread, by serial, and the task it
    /// runs, if any.
    static CURRENT: Cell<Option<u64>> = const { Cell::new(None) };
    static TASK: RefCell<Option<Thread>> = const { RefCell::new(None) };
}

/// Whether the extension `serial` runs on this thread now.
fn running(serial: u64) -> bool {
    CURRENT.with(Cell::get) == Some(serial)
}

/// Whether an extension runs on this thread now, so that what it called is
/// running inside its call: an extension is not to be called again before
/// that call is done.
pub fn calling() -> bool {
    CURRENT.with(Cell::get).is_some()
}

/// The task running now on this thread, for the Lua wrapper of a function
/// that waits.
fn current_task() -> Option<Thread> {
    TASK.with(|t| t.borrow().clone())
}

/// Every Lua file under the `lua/` of the mod's `eu4-harmony/` folder
/// `harmony`, in name order, compiled as [`open`] and `require` load them,
/// without running any; and each that does not compile, or read, with why.
pub fn compile(harmony: &std::path::Path) -> (Vec<PathBuf>, Vec<String>) {
    fn walk(dir: &std::path::Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|x| x == "lua") {
                found.push(path);
            }
        }
    }
    let modules = harmony.join("lua");
    let mut scripts = Vec::new();
    walk(&modules, &mut scripts);
    scripts.sort();
    let lua = Lua::new();
    let mut problems = Vec::new();
    for path in &scripts {
        let file = path.strip_prefix(&modules).unwrap_or(path).to_string_lossy().replace('\\', "/");
        match std::fs::read(path) {
            Ok(source) => {
                if let Err(e) = sandbox::chunk(&lua, source, &file).into_function() {
                    problems.push(e.to_string());
                }
            }
            Err(e) => problems.push(format!("{file}: {e}")),
        }
    }
    (scripts, problems)
}

/// Opens the extension of `m` as the `index`th of `host`: its Lua state made
/// in the sandbox and its `main.lua` run.
pub fn open(index: usize, m: &Mod, host: Arc<dyn Host>) -> Result<Arc<Extension>, String> {
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let waits = Arc::new(Mutex::new(Waits::default()));
    let opened = sandbox::open(&sandbox::Opening { index, serial, m, host: host.clone(), waits: waits.clone() }).map_err(|e| e.to_string())?;
    let main = m.harmony.join(eu4_harmony_manifest::EXTENSION);
    let source = std::fs::read_to_string(&main).map_err(|e| format!("cannot read {}: {e}", main.display()))?;
    // main.lua runs as a plain call: it cannot wait, and the time limit
    // holds for it too.
    watch::start(opened.main_state, None);
    let ran = sandbox::chunk(&opened.lua, source, "main.lua").eval::<LuaValue>();
    let stopped = watch::end();
    let handlers = match ran.map_err(|e| e.to_string())? {
        LuaValue::Table(t) => t,
        other if stopped => return Err(format!("main.lua {}; it returned {}", watch::STOPPED, other.type_name())),
        other => return Err(format!("main.lua returned {} instead of a table of handlers", other.type_name())),
    };
    let mut windows = match handlers.get::<LuaValue>("windows").map_err(|e| e.to_string())? {
        LuaValue::Nil => Vec::new(),
        declared => {
            let declared = values::to_json(&declared).map_err(|e| format!("main.lua's windows: {e}"))?;
            layout::read(&declared, &|key| host.text(m.id, key).is_some()).map_err(|e| format!("main.lua's {e}"))?
        }
    };
    let images = images::load(m.harmony.parent().unwrap_or(&m.harmony), m.id, &mut windows)?;
    let extension = Arc::new(Extension {
        index,
        serial,
        id: m.id.to_string(),
        name: m.name.to_string(),
        host,
        lua: opened.lua,
        main: opened.main_state as usize,
        handlers,
        windows,
        images,
        game: opened.game,
        press: opened.press,
        dropped: opened.dropped,
        wait: opened.wait,
        waits,
        state: Mutex::new(State::default()),
    });
    // spawn reaches the extension through its state.
    extension.lua.set_app_data(Arc::downgrade(&extension));
    Ok(extension)
}

impl Extension {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The mod's name, for the player.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Why the extension was stopped, if it was.
    pub fn stopped(&self) -> Option<String> {
        self.state().stopped.clone()
    }

    /// The windows its `main.lua` returns, laid out, in name order.
    pub fn windows(&self) -> &[layout::Window] {
        &self.windows
    }

    /// Validated entry images, kept alive with the extension's declaration.
    pub fn images(&self) -> &[images::Image] {
        &self.images
    }

    /// Its Lua state, for tests.
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// How many tasks wait.
    pub fn waiting(&self) -> usize {
        self.state().tasks.len()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn log(&self, line: &str) {
        self.host.log(&self.id, line);
    }
}

// ---------- capabilities ----------

/// Runs a capability of the catalogue for the extension, from its Lua
/// wrapper: its answer as Lua values; for a game's refusal `nil, err`; for
/// one that waits, the wait sentinel and the number its task waits on.
/// Called wrongly (outside a handler, arguments of the wrong kind) it is an
/// error.
fn call(lua: &Lua, at: &sandbox::Calling, name: &str, args: MultiValue) -> mlua::Result<MultiValue> {
    if !running(at.serial) {
        return Err(mlua::Error::runtime(format!("game.{name}: the game is reached only from a handler, at a safe point, not at the top of main.lua")));
    }
    let request = sandbox::request(name, args).map_err(|e| mlua::Error::runtime(format!("game.{name}: {e}")))?;
    let request = eu4_harmony_remote::request_from_fields(request).map_err(|e| mlua::Error::runtime(format!("game.{name}: {}", e.message)))?;
    match at.host.run(at.index, request) {
        Called::Done(response) => answer(lua, name, response),
        Called::Later(pending) => Ok(Waits::keep(&at.waits, &at.wait, Waiting::Capability(pending))),
    }
}

/// Sends `http.fetch`'s request for the extension, whose manifest lists
/// `hosts`: the wait sentinel and the number its task waits on. A request
/// the manifest does not allow, or written wrongly, is an error.
fn fetch(at: &sandbox::Calling, hosts: &[String], url: &str, options: Option<Table>) -> mlua::Result<MultiValue> {
    let refuse = |e: String| mlua::Error::runtime(format!("http.fetch: {e}"));
    if hosts.is_empty() {
        return Err(sandbox::undeclared("http.fetch", "network", "network = [\"api.example.com\"]"));
    }
    let target = fetch::target(url).map_err(refuse)?;
    if !fetch::listed(&target, hosts) {
        return Err(refuse(format!("{url} goes to a host the manifest's network permission does not list ({})", hosts.join(", "))));
    }
    let request = fetch::request(target, options).map_err(refuse)?;
    if !running(at.serial) {
        return Err(refuse("a request is sent only from a handler or a task".into()));
    }
    let deadline = Instant::now() + request.timeout;
    Ok(Waits::keep(&at.waits, &at.wait, Waiting::Fetch(at.host.fetch(request), deadline)))
}

/// A capability's answer as Lua values: the result (`true` for one with
/// none), or `nil, err` for a refusal; an error for a request the game could
/// not read.
fn answer(lua: &Lua, name: &str, answered: Answered) -> mlua::Result<MultiValue> {
    match answered {
        Ok(Value::Null) => Ok(MultiValue::from_iter([LuaValue::Boolean(true)])),
        Ok(json) => Ok(MultiValue::from_iter([values::to_lua(lua, &json)?])),
        Err(e) if matches!(e.code, Code::BadRequest | Code::Unavailable) => Err(mlua::Error::runtime(format!("game.{name}: {}", e.message))),
        Err(e) => sandbox::failure(lua, e.code.name(), &e.message),
    }
}

// ---------- running ----------

/// What a stretch of a task came to.
enum Stretch {
    Done,
    /// It waits for the answer with this number.
    Waits(u64),
    Failed(String),
}

impl Extension {
    /// Runs one stretch of `thread` with `args`, as the running task.
    fn stretch(&self, thread: &Thread, args: MultiValue) -> Stretch {
        let previous = CURRENT.with(|c| c.replace(Some(self.serial)));
        let previous_task = TASK.with(|t| t.replace(Some(thread.clone())));
        // A task `spawn` runs has its error written on the task that spawned
        // it, which is watched already.
        watch::start(thread.to_pointer(), previous.is_none().then_some(self.main as *const std::ffi::c_void));
        let started = Instant::now();
        let result = thread.resume::<MultiValue>(args);
        let elapsed = started.elapsed();
        let timed_out = watch::end();
        CURRENT.with(|c| c.set(previous));
        TASK.with(|t| t.replace(previous_task));
        self.count(elapsed, timed_out);
        match result {
            Err(e) => Stretch::Failed(e.to_string()),
            Ok(_) if thread.status() != ThreadStatus::Resumable => Stretch::Done,
            Ok(values) => match values.iter().collect::<Vec<_>>().as_slice() {
                [first, LuaValue::Integer(number)] if **first == self.wait => Stretch::Waits(*number as u64),
                _ => Stretch::Failed("yielded outside a coroutine of the mod's own; a handler gives up its turn only by waiting for a capability".into()),
            },
        }
    }

    /// Times a stretch, and counts stretches stopped in a row.
    fn count(&self, elapsed: Duration, timed_out: bool) {
        let mut state = self.state();
        state.all.add(elapsed);
        state.session.add(elapsed);
        state.timeouts = if timed_out { state.timeouts + 1 } else { 0 };
        drop(state);
        if elapsed > SLOW {
            self.log(&format!("slow stretch: {} ms", elapsed.as_millis()));
        }
    }

    /// Whether it has the handler `name` and is not stopped.
    pub fn has_handler(&self, name: &str) -> bool {
        self.state().stopped.is_none() && matches!(self.handlers.get::<LuaValue>(name), Ok(LuaValue::Function(_)))
    }

    /// Runs the handler `name` as a new task, with `args` after `game`. A
    /// handler the extension does not have, or a stopped extension, is not
    /// called.
    pub fn handle(&self, name: &str, args: &[Arg]) {
        if self.state().stopped.is_some() {
            return;
        }
        let Ok(LuaValue::Function(handler)) = self.handlers.get::<LuaValue>(name) else { return };
        let started = (|| {
            let thread = self.lua.create_thread(handler)?;
            let mut all = MultiValue::from_iter([LuaValue::Table(self.game.clone())]);
            for arg in args {
                all.push_back(self.arg(arg)?);
            }
            Ok::<_, mlua::Error>((thread, all))
        })();
        match started {
            Ok((thread, all)) => {
                let outcome = self.stretch(&thread, all);
                self.after(name, thread, outcome, None);
            }
            Err(e) => self.error(name, &e.to_string()),
        }
    }

    /// Runs the action of the button `button` (from 1, left to right) of its
    /// pop-up `popup` as a new task, which closes the pop-up after it unless
    /// the action answers `{ close = false }`.
    pub fn press_popup(&self, popup: u32, button: u32) {
        if self.state().stopped.is_some() {
            return;
        }
        const FROM: &str = "a pop-up's button";
        match self.lua.create_thread(self.press.clone()) {
            Ok(thread) => {
                let outcome = self.stretch(&thread, MultiValue::from_iter([LuaValue::Integer(popup.into()), LuaValue::Integer(button.into())]));
                self.after(FROM, thread, outcome, None);
            }
            Err(e) => self.error(FROM, &e.to_string()),
        }
    }

    /// One argument as Lua has it.
    fn arg(&self, arg: &Arg) -> mlua::Result<LuaValue> {
        Ok(match arg {
            Arg::Json(json) => values::to_lua(&self.lua, json)?,
            Arg::Text(text) => LuaValue::String(self.lua.create_string(text)?),
            Arg::Click { right, shift, ctrl, row } => {
                let t = self.lua.create_table()?;
                t.set("right", *right)?;
                t.set("shift", *shift)?;
                t.set("ctrl", *ctrl)?;
                t.set("row", *row)?;
                LuaValue::Table(t)
            }
        })
    }

    /// Starts `runner`, the prelude's wrapper of the function `spawn` was
    /// given, as a task of its own whose handle is `handle`.
    fn spawn(&self, handle: Table, runner: Function) -> mlua::Result<()> {
        let thread = self.lua.create_thread(runner)?;
        // spawn is called from a running stretch: the running task's state
        // is put back once the spawned one gives up its turn.
        let outcome = self.stretch(&thread, MultiValue::new());
        self.after("spawn", thread, outcome, Some(handle));
        Ok(())
    }

    /// Cancels the spawned task whose handle is `handle`, which the prelude
    /// has marked cancelled already: a task that waits is dropped now; one
    /// that runs, or is being taken up, at its next wait.
    fn cancel(&self, handle: &Table) {
        let key = key(handle);
        let mut state = self.state();
        match state.tasks.iter().position(|t| t.key() == Some(key)) {
            Some(i) => drop(state.tasks.remove(i)),
            None => drop(state.cancelling.insert(key)),
        }
    }

    /// Whether the task was cancelled since it was last taken up; it is
    /// forgotten as cancelled once this has said so.
    fn cancelled(&self, task: &Task) -> bool {
        task.key().is_some_and(|key| self.state().cancelling.remove(&key))
    }

    /// What follows a stretch: the task kept while it waits, unless it was
    /// cancelled; an error logged; the extension stopped after the time
    /// limit too often.
    fn after(&self, from: &str, thread: Thread, outcome: Stretch, handle: Option<Table>) {
        match outcome {
            Stretch::Done => {
                if let Some(handle) = &handle {
                    self.state().cancelling.remove(&key(handle));
                }
            }
            Stretch::Waits(number) => {
                let wait = self.waits.lock().unwrap_or_else(|e| e.into_inner()).by_number.remove(&number);
                match wait {
                    Some(wait) => {
                        let task = Task { thread, wait, from: from.to_string(), handle };
                        if !self.cancelled(&task) {
                            self.state().tasks.push(task);
                        }
                    }
                    None => self.error(from, "waited for an answer nobody asked for"),
                }
            }
            Stretch::Failed(message) => {
                if let Some(handle) = &handle {
                    self.state().cancelling.remove(&key(handle));
                }
                self.error(from, &message);
            }
        }
        if self.state().timeouts >= TIMEOUTS {
            self.stop(&format!("its handlers ran past the time limit ({} s) {TIMEOUTS} times in a row", watch::LIMIT.as_secs()));
        }
    }

    /// Takes up every waiting task whose answer has come, and those
    /// awaiting a task that ended meanwhile, within the same poll.
    pub fn poll(&self) {
        for _ in 0..PASSES {
            if !self.pass() {
                break;
            }
        }
    }

    /// One pass over the waiting tasks: whether it took any up.
    fn pass(&self) -> bool {
        let tasks = std::mem::take(&mut self.state().tasks);
        let mut still = Vec::new();
        let mut took = false;
        for mut task in tasks {
            if self.state().stopped.is_some() {
                break;
            }
            if self.cancelled(&task) {
                continue;
            }
            let came = match &mut task.wait {
                Waiting::Capability(pending) => pending.poll().map(|response| answer(&self.lua, &task.from, response)),
                Waiting::Fetch(pending, deadline) => match pending.poll() {
                    Some(response) => Some(fetch::answer(&self.lua, response)),
                    None if Instant::now() >= *deadline => Some(fetch::answer(&self.lua, Err(fetch::Failure::Timeout))),
                    None => None,
                },
                Waiting::Tasks(handles) => handles.iter().any(|h| h.raw_get::<bool>("ended").unwrap_or(false)).then(|| Ok(MultiValue::new())),
            };
            match came {
                None => still.push(task),
                Some(answer) => {
                    took = true;
                    // An answer that cannot be read is the task's error.
                    let outcome = match answer {
                        Ok(values) => self.stretch(&task.thread, values),
                        Err(e) => Stretch::Failed(e.to_string()),
                    };
                    let Task { thread, from, handle, .. } = task;
                    self.after(&from, thread, outcome, handle);
                }
            }
        }
        // A task cancelled after it was passed over in this pass goes now.
        still.retain(|task| !self.cancelled(task));
        // Stopped on the way, it keeps no task: they would never run again.
        let mut state = self.state();
        if state.stopped.is_none() {
            still.append(&mut state.tasks);
            state.tasks = still;
        }
        took
    }

    /// An error in a handler: logged, the same one only counted after its
    /// first few, and the player told of the session's first.
    fn error(&self, from: &str, message: &str) {
        let key = format!("{from}: {message}");
        let session = self.host.session();
        let (count, tell) = {
            let mut state = self.state();
            let count = state.errors.entry(key.clone()).or_insert(0);
            *count += 1;
            let count = *count;
            let tell = state.told != Some(session);
            state.told = Some(session);
            (count, tell)
        };
        if count <= WRITTEN {
            self.log(&format!("error in {key}"));
        } else if self.host.repeated_error(self.id(), &key) {
            self.state().host_summarized = true;
        }
        if count == WRITTEN {
            self.log(&format!("(the same error at the same place is counted from now on: {key})"));
        }
        if tell {
            self.host.first_error(self, from, message);
        }
    }

    /// Stops the extension for the rest of the game.
    fn stop(&self, reason: &str) {
        {
            let mut state = self.state();
            if state.stopped.is_some() {
                return;
            }
            state.stopped = Some(reason.to_string());
            state.tasks.clear();
            state.cancelling.clear();
        }
        self.waits.lock().unwrap_or_else(|e| e.into_inner()).by_number.clear();
        self.log(&format!("stopped: {reason}"));
        self.host.stopped(self, reason);
    }

    /// The session ended: its waiting tasks are dropped, a spawned one's
    /// handle marked cancelled, and any error counts not already summarized by
    /// the host's journal go to the log.
    pub fn session_over(&self) {
        let (dropped, counted) = {
            let mut state = self.state();
            let dropped = std::mem::take(&mut state.tasks);
            state.cancelling.clear();
            let host_summarized = std::mem::replace(&mut state.host_summarized, false);
            let counted: Vec<(String, u32)> = state.errors.drain().filter(|(_, n)| !host_summarized && *n > WRITTEN).collect();
            (dropped, counted)
        };
        self.waits.lock().unwrap_or_else(|e| e.into_inner()).by_number.clear();
        if !dropped.is_empty() {
            self.log(&format!("the session ended: {} waiting task(s) dropped", dropped.len()));
        }
        for handle in dropped.into_iter().filter_map(|t| t.handle) {
            if let Err(e) = self.dropped.call::<()>(handle) {
                self.log(&format!("a dropped task's handle: {e}"));
            }
        }
        for (key, n) in counted {
            self.log(&format!("error {n} times this session: {key}"));
        }
    }

    /// The extension's save data, from its `save_data`
    /// (ADR save-data-is-asked-for-by-save-data), as a plain call that cannot
    /// wait: `None` when it has no `save_data` or is stopped; `Some(Ok(None))`
    /// when it keeps nothing; an error when `save_data` fails or returns what
    /// JSON cannot hold, which is logged as other errors.
    pub fn save_data(&self) -> Option<Result<Option<Value>, String>> {
        if self.state().stopped.is_some() {
            return None;
        }
        let Ok(LuaValue::Function(save_data)) = self.handlers.get::<LuaValue>("save_data") else { return None };
        let previous = CURRENT.with(|c| c.replace(Some(self.serial)));
        watch::start(self.main as *const std::ffi::c_void, None);
        let started = Instant::now();
        let returned = save_data.call::<LuaValue>(self.game.clone());
        let elapsed = started.elapsed();
        let timed_out = watch::end();
        CURRENT.with(|c| c.set(previous));
        self.count(elapsed, timed_out);
        let value = returned.map_err(|err| err.to_string()).and_then(|v| values::to_json(&v)).and_then(|v| {
            eu4_harmony_checks::save_data::size(v.to_string().len()).map_err(|r| r.message)?;
            Ok(v)
        });
        Some(match value {
            Ok(Value::Null) => Ok(None),
            Ok(v) => Ok(Some(v)),
            Err(message) => {
                let message = format!("{message}; no data is kept for the mod");
                self.error("save_data", &message);
                Err(message)
            }
        })
    }

    /// Its stretches, the time spent in them, and whether it was stopped, as
    /// the startup report writes them.
    pub fn timing(&self) -> String {
        let state = self.state();
        let mut line = format!("{}: {}", self.id, state.all.summary());
        if let Some(reason) = &state.stopped {
            line.push_str(&format!("; STOPPED: {reason}"));
        }
        line
    }

    /// The stretches since this was last asked, which the host asks at a
    /// session's start and end.
    pub fn take_session_times(&self) -> Times {
        std::mem::take(&mut self.state().session)
    }
}

/// The extension a Lua state belongs to, for its globals.
fn extension_of(lua: &Lua) -> Option<Arc<Extension>> {
    lua.app_data_ref::<Weak<Extension>>().and_then(|weak| weak.upgrade())
}

#[cfg(test)]
mod tests;
