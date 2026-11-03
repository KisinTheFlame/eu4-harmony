//! The Lua host (ADR 0024): runs each mod's extension, its Lua scripts, in a
//! Lua state of its own, and calls its handlers at safe points
//! (`crate::safe_point`).
//!
//! An extension's `main.lua` runs once, at startup, in the sandbox, and
//! returns its handlers. Every handler call runs as a task (ADR 0030), which
//! may wait for a capability's answer and is taken up again at a later safe
//! point. `game`, the same table for every handler, reaches the capability
//! catalogue through remote control's groups, called as the extension
//! (`crate::remote::run_for`), so both sides of the catalogue share one
//! implementation (ADR 0026). An error ends the call and is logged, and the
//! extension is called again (ADR 0027); three stretches in a row stopped by
//! the time limit ([`watch`]) stop it. The sensitive capabilities
//! (ADR 0025) are [`files`] and [`fetch`], with the clock in the sandbox.
mod fetch;
mod files;
mod sandbox;
pub mod values;
mod watch;

use crate::{
    game::{self, events, Caller},
    map,
    mods::DiscoveredMod,
    notice::Message,
    remote::{self, Wait},
    saves, ui,
};
use eu4_harmony_remote::{Answer, Code};
use mlua::{Function, Lua, MultiValue, Table, Thread, ThreadStatus, Value as LuaValue};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    fs::{File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

/// A stretch slower than this is logged.
const SLOW: Duration = Duration::from_millis(5);

/// The same error at the same place is written in full this many times in a
/// session, then only counted.
const WRITTEN: u32 = 3;

/// Stretches stopped by the time limit in a row that stop the extension.
const TIMEOUTS: u32 = 3;

/// One extension.
pub(crate) struct Extension {
    index: usize,
    id: String,
    /// The mod's name, for the player.
    name: String,
    lua: Lua,
    handlers: Table,
    /// The `game` every handler gets.
    game: Table,
    /// The sentinel a capability that waits returns, to its Lua wrapper.
    wait: LuaValue,
    /// What its tasks are to wait for, shared with its sandbox's `http`.
    waits: Arc<Mutex<Waits>>,
    state: Mutex<State>,
    log: Mutex<Option<File>>,
}

/// What changes as the extension runs.
#[derive(Default)]
struct State {
    stopped: Option<String>,
    stretches: u64,
    slow: u64,
    total: Duration,
    longest: Duration,
    timeouts: u32,
    /// Errors of this session by where and what, and how often.
    errors: HashMap<String, u32>,
    /// The session whose first error the player was told of.
    told: Option<u64>,
    /// Tasks waiting for an answer.
    tasks: Vec<Task>,
}

/// What tasks are to wait for, by number, until their task takes it as it
/// gives up its turn.
#[derive(Default)]
pub(crate) struct Waits {
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
}

/// What a task waits for: a capability's answer, or a response from the
/// network.
pub(crate) enum Waiting {
    Capability(Box<dyn Wait>),
    Fetch(fetch::Pending),
}

static EXTENSIONS: OnceLock<Vec<Arc<Extension>>> = OnceLock::new();
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
/// The folder that holds every extension's data folder.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

thread_local! {
    /// The extension running on this thread, and the task it runs, if any.
    static CURRENT: Cell<Option<usize>> = const { Cell::new(None) };
    static TASK: RefCell<Option<Thread>> = const { RefCell::new(None) };
}

fn extensions() -> &'static [Arc<Extension>] {
    EXTENSIONS.get().map_or(&[], Vec::as_slice)
}

/// The id of the mod whose extension has this index.
pub fn mod_id(index: usize) -> Option<String> {
    extensions().get(index).map(|e| e.id.clone())
}

/// The ids of the mods whose extensions loaded.
pub fn mod_ids() -> Vec<String> {
    extensions().iter().map(|e| e.id.clone()).collect()
}

/// Maps discovery-order mod indices to extension indices.
pub fn extension_indices(mods: &[DiscoveredMod]) -> BTreeMap<usize, usize> {
    let list = extensions();
    mods.iter().enumerate().filter_map(|(i, m)| list.iter().find(|e| e.id == m.manifest.id).map(|e| (i, e.index))).collect()
}

// ---------- loading ----------

/// What loading one mod's extension came to.
pub struct LoadOutcome {
    /// The mod takes part: its extension loaded, or it has none.
    pub included: bool,
    pub status: String,
}

/// Loads every enabled mod's extension, in playset order. Once, at startup,
/// on the game thread. Logs go to `log_dir`; `data_dir` holds the data
/// folders.
pub fn load(mods: &[DiscoveredMod], log_dir: &Path, data_dir: &Path) -> Vec<LoadOutcome> {
    let _ = LOG_DIR.set(log_dir.to_path_buf());
    let _ = DATA_DIR.set(data_dir.to_path_buf());
    let supported = crate::memory().is_some();
    let mut loaded = Vec::new();
    let mut outcomes = Vec::new();
    for m in mods {
        let Some(entry) = &m.layout.extension else {
            outcomes.push(LoadOutcome { included: true, status: "no extension".into() });
            continue;
        };
        let result = if !supported {
            Err("the game executable is not supported, so no extension can run".to_string())
        } else if !m.manifest.built_for(eu4_harmony_remote::API_VERSION) {
            Err(format!("written for api_version {}, this EU4 Harmony provides {}", m.manifest.api_version, eu4_harmony_remote::API_VERSION))
        } else {
            open(loaded.len(), m)
        };
        match result {
            Ok(extension) => {
                log!("extension {:?} loaded", m.manifest.id);
                outcomes.push(LoadOutcome { included: true, status: format!("extension {entry} loaded") });
                loaded.push(Arc::new(extension));
            }
            Err(e) => {
                log!("extension {:?} not loaded: {e}", m.manifest.id);
                outcomes.push(LoadOutcome { included: false, status: format!("extension NOT loaded: {e}") });
                // An unsupported executable has its own notice.
                if supported {
                    crate::notice::post(&format!("load:{}", m.manifest.id), not_loaded(m, &e));
                }
            }
        }
    }
    let _ = EXTENSIONS.set(loaded);
    outcomes
}

/// What the player is told about an extension that was not loaded. A
/// version mismatch is the one they can fix, so it is spelled out.
fn not_loaded(m: &DiscoveredMod, e: &str) -> Message {
    let provided = eu4_harmony_remote::API_VERSION;
    if !m.manifest.built_for(provided) {
        return Message::new("EU4HARMONY_API_MISMATCH").arg("MOD", &m.descriptor.name).arg("BUILT", &m.manifest.api_version).arg("PROVIDED", provided);
    }
    Message::new("EU4HARMONY_NOT_LOADED").arg("MOD", &m.descriptor.name).arg("DETAIL", e)
}

/// The extension of `m`, as the `index`th: its state made and its
/// `main.lua` run.
fn open(index: usize, m: &DiscoveredMod) -> Result<Extension, String> {
    // The last start's log is kept beside it; this start's begins before
    // main.lua runs, which may write to it already.
    if let Some(dir) = LOG_DIR.get() {
        let path = log_file(dir, &m.manifest.id);
        if path.exists() {
            let _ = std::fs::rename(&path, path.with_extension("prev.log"));
        }
    }
    let lua_dir = m.dir.join("eu4-harmony").join("lua");
    let data = DATA_DIR.get().map(|d| d.join(&m.manifest.id));
    let waits = Arc::new(Mutex::new(Waits::default()));
    let opened = sandbox::open(index, &m.manifest.id, &m.dir.join("eu4-harmony"), data, &m.manifest.permissions, waits.clone()).map_err(|e| e.to_string())?;
    let main = lua_dir.join("main.lua");
    let source = std::fs::read_to_string(&main).map_err(|e| format!("cannot read {}: {e}", main.display()))?;
    // main.lua runs as a plain call: it cannot wait, and the time limit
    // holds for it too.
    watch::start(opened.main_state);
    let ran = opened.lua.load(source).set_name("@main.lua").set_mode(mlua::ChunkMode::Text).eval::<LuaValue>();
    let stopped = watch::end();
    let handlers = match ran.map_err(|e| e.to_string())? {
        LuaValue::Table(t) => t,
        other if stopped => return Err(format!("main.lua {}; it returned {}", watch::STOPPED, other.type_name())),
        other => return Err(format!("main.lua returned {} instead of a table of handlers", other.type_name())),
    };
    Ok(Extension {
        index,
        id: m.manifest.id.clone(),
        name: m.descriptor.name.clone(),
        lua: opened.lua,
        handlers,
        game: opened.game,
        wait: opened.wait,
        waits,
        state: Mutex::new(State::default()),
        log: Mutex::new(None),
    })
}

// ---------- the extension's log ----------

impl Extension {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Appends a line to the extension's own log, `<id>.log`.
    fn write_log(&self, message: &str) {
        let mut file = self.log.lock().unwrap_or_else(|e| e.into_inner());
        if file.is_none() {
            if let Some(dir) = LOG_DIR.get() {
                *file = OpenOptions::new().create(true).append(true).open(log_file(dir, &self.id)).ok();
            }
        }
        if let Some(f) = file.as_mut() {
            let _ = writeln!(f, "{message}");
            let _ = f.flush();
        }
    }
}

/// The log of the mod `id`'s extension in the logs folder `dir`.
fn log_file(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.log"))
}

/// Appends a line to the log of the extension with this index, from its
/// `log` and `print`.
pub(crate) fn extension_log(index: usize, id: &str, message: &str) {
    match extensions().get(index) {
        Some(e) => e.write_log(message),
        // While main.lua runs, before the extension is in the list.
        None => {
            if let Some(dir) = LOG_DIR.get() {
                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file(dir, id)) {
                    let _ = writeln!(f, "{message}");
                }
            }
        }
    }
}

// ---------- capabilities ----------

/// Runs a capability of the catalogue for the extension `index`, from its
/// Lua wrapper: its answer as Lua values; for a game's refusal `nil, err`;
/// for one that waits, the wait sentinel and the number its task waits on.
/// Called wrongly (outside a handler, arguments of the wrong kind) it is an
/// error.
pub(crate) fn call(lua: &Lua, index: usize, name: &str, args: MultiValue) -> mlua::Result<MultiValue> {
    if CURRENT.with(Cell::get) != Some(index) {
        return Err(mlua::Error::runtime(format!("game.{name}: the game is reached only from a handler, at a safe point, not at the top of main.lua")));
    }
    let request = sandbox::request(name, args).map_err(|e| mlua::Error::runtime(format!("game.{name}: {e}")))?;
    let request = eu4_harmony_remote::request_from_json(&request.to_string()).map_err(|e| mlua::Error::runtime(format!("game.{name}: {}", e.message)))?;
    match remote::run_for(Caller::Extension(index), request) {
        remote::Outcome::Done(response) => answer(lua, name, response),
        remote::Outcome::Later(wait) => {
            let extension = &extensions()[index];
            Ok(Waits::keep(&extension.waits, &extension.wait, Waiting::Capability(wait)))
        }
    }
}

/// Sends `http.fetch`'s request for the extension `index`, whose manifest
/// lists `hosts`: the wait sentinel and the number its task waits on, kept
/// in `waits`. A request the manifest does not allow, or written wrongly, is
/// an error.
pub(crate) fn fetch(index: usize, hosts: &[String], waits: &Mutex<Waits>, sentinel: &LuaValue, url: &str, options: Option<Table>) -> mlua::Result<MultiValue> {
    let refuse = |e: String| mlua::Error::runtime(format!("http.fetch: {e}"));
    if hosts.is_empty() {
        return Err(sandbox::undeclared("http.fetch", "network", "network = [\"api.example.com\"]"));
    }
    let target = fetch::target(url).map_err(refuse)?;
    if !fetch::listed(&target, hosts) {
        return Err(refuse(format!("{url} goes to a host the manifest's network permission does not list ({})", hosts.join(", "))));
    }
    let request = fetch::request(options).map_err(refuse)?;
    if CURRENT.with(Cell::get) != Some(index) {
        return Err(refuse("a request is sent only from a handler or a task".into()));
    }
    Ok(Waits::keep(waits, sentinel, Waiting::Fetch(fetch::start(target, request))))
}

/// A capability's response as Lua values: the result (`true` for one with
/// none), or `nil, err` for a refusal; an error for a request the game could
/// not read.
fn answer(lua: &Lua, name: &str, response: eu4_harmony_remote::Response) -> mlua::Result<MultiValue> {
    match response {
        Ok(answer) => {
            let json = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(&answer)).map_err(mlua::Error::external)?;
            let value = match (&answer, json) {
                (_, serde_json::Value::Null) => LuaValue::Boolean(true),
                (Answer::Describe(_), _) => LuaValue::Nil,
                (_, json) => values::to_lua(lua, &json)?,
            };
            Ok(MultiValue::from_iter([value]))
        }
        Err(e) if matches!(e.code, Code::BadRequest | Code::Unavailable) => Err(mlua::Error::runtime(format!("game.{name}: {}", e.message))),
        Err(e) => {
            let err = lua.create_table()?;
            err.set("code", e.code.name())?;
            err.set("message", e.message)?;
            Ok(MultiValue::from_iter([LuaValue::Nil, LuaValue::Table(err)]))
        }
    }
}

/// The task running now on this thread, for the Lua wrapper of a capability
/// that waits.
pub(crate) fn current_task() -> Option<Thread> {
    TASK.with(|t| t.borrow().clone())
}

// ---------- running ----------

/// What a stretch of a task came to.
enum Stretch {
    Done,
    /// It waits for the capability answer with this number.
    Waits(u64),
    Failed(String),
}

impl Extension {
    fn caller(&self) -> Caller {
        Caller::Extension(self.index)
    }

    fn waits(&self) -> std::sync::MutexGuard<'_, Waits> {
        self.waits.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs one stretch of `thread` with `args`, as the running task.
    fn stretch(&self, thread: &Thread, args: MultiValue) -> Stretch {
        let previous = CURRENT.with(|c| c.replace(Some(self.index)));
        let previous_task = TASK.with(|t| t.replace(Some(thread.clone())));
        watch::start(thread.to_pointer());
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
        state.stretches += 1;
        state.total += elapsed;
        state.longest = state.longest.max(elapsed);
        if elapsed > SLOW {
            state.slow += 1;
        }
        state.timeouts = if timed_out { state.timeouts + 1 } else { 0 };
        drop(state);
        if elapsed > SLOW {
            self.write_log(&format!("slow stretch: {} ms", elapsed.as_millis()));
        }
    }

    /// Runs a handler as a new task, with `args` after `game`.
    fn handle(&self, name: &str, args: impl FnOnce(&Lua) -> mlua::Result<MultiValue>) {
        if self.state().stopped.is_some() {
            return;
        }
        let Ok(LuaValue::Function(handler)) = self.handlers.get::<LuaValue>(name) else { return };
        let started = (|| {
            let thread = self.lua.create_thread(handler)?;
            let mut all = MultiValue::from_iter([LuaValue::Table(self.game.clone())]);
            all.extend(args(&self.lua)?);
            Ok::<_, mlua::Error>((thread, all))
        })();
        match started {
            Ok((thread, all)) => {
                let outcome = self.stretch(&thread, all);
                self.after(name, thread, outcome);
            }
            Err(e) => self.error(name, &e.to_string()),
        }
    }

    /// Starts `function` as a task of its own, from `spawn`.
    pub(crate) fn spawn(&self, function: Function) -> mlua::Result<()> {
        let thread = self.lua.create_thread(function)?;
        // spawn is called from a running stretch: the running task's state
        // is put back once the spawned one gives up its turn.
        let outcome = self.stretch(&thread, MultiValue::new());
        self.after("spawn", thread, outcome);
        Ok(())
    }

    /// What follows a stretch: the task kept while it waits, an error
    /// logged, the extension stopped after the time limit too often.
    fn after(&self, from: &str, thread: Thread, outcome: Stretch) {
        match outcome {
            Stretch::Done => {}
            Stretch::Waits(number) => {
                let wait = self.waits().by_number.remove(&number);
                match wait {
                    Some(wait) => self.state().tasks.push(Task { thread, wait, from: from.to_string() }),
                    None => self.error(from, "waited for an answer nobody asked for"),
                }
            }
            Stretch::Failed(message) => self.error(from, &message),
        }
        if self.state().timeouts >= TIMEOUTS {
            self.stop(&format!("its handlers ran past the time limit ({} s) {TIMEOUTS} times in a row", watch::LIMIT.as_secs()));
        }
    }

    /// Takes up every waiting task whose answer has come.
    fn poll(&self) {
        let tasks = std::mem::take(&mut self.state().tasks);
        let mut still = Vec::new();
        for mut task in tasks {
            if self.state().stopped.is_some() {
                break;
            }
            let came = match &mut task.wait {
                Waiting::Capability(wait) => remote::poll_for(self.caller(), wait.as_mut()).map(|response| answer(&self.lua, &task.from, response)),
                Waiting::Fetch(pending) => pending.poll().map(|response| fetch::answer(&self.lua, response)),
            };
            match came {
                None => still.push(task),
                Some(answer) => {
                    // An answer that cannot be read is the task's error.
                    let outcome = match answer {
                        Ok(values) => self.stretch(&task.thread, values),
                        Err(e) => Stretch::Failed(e.to_string()),
                    };
                    let from = task.from.clone();
                    self.after(&from, task.thread, outcome);
                }
            }
        }
        let mut state = self.state();
        still.append(&mut state.tasks);
        state.tasks = still;
    }

    /// An error in a handler: logged, the same one only counted after its
    /// first few, and the player told of the session's first.
    fn error(&self, from: &str, message: &str) {
        let key = format!("{from}: {message}");
        let session = events::session();
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
            self.write_log(&format!("error in {key}"));
        }
        if count == WRITTEN {
            self.write_log(&format!("(the same error at the same place is counted from now on, and its count written when the session ends: {key})"));
        }
        // The player is told in a game the framework runs in.
        if tell && crate::memory().is_some() {
            log!("extension {:?}: error in {from}: {message}", self.id);
            crate::notice::post(&format!("errors:{}:{session}", self.id), Message::new("EU4HARMONY_ERRORS").arg("MOD", &self.name));
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
        }
        self.waits().by_number.clear();
        log!("extension {:?} stopped: {reason}", self.id);
        self.write_log(&format!("stopped: {reason}"));
        ui::hide_extension(self.index);
        if let Some(m) = crate::memory() {
            map::release(m, self.index);
            crate::notice::post(&format!("stopped:{}", self.id), Message::new("EU4HARMONY_STOPPED").arg("MOD", &self.name).arg("DETAIL", reason));
        }
    }

    /// The session ended: its waiting tasks are dropped, and the counts of
    /// errors seen more often than they were written go to the log.
    fn session_over(&self) {
        let (dropped, counted) = {
            let mut state = self.state();
            let dropped = state.tasks.len();
            state.tasks.clear();
            self.waits().by_number.clear();
            let counted: Vec<(String, u32)> = state.errors.drain().filter(|(_, n)| *n > WRITTEN).collect();
            (dropped, counted)
        };
        if dropped > 0 {
            self.write_log(&format!("the session ended: {dropped} waiting task(s) dropped"));
        }
        for (key, n) in counted {
            self.write_log(&format!("error {n} times this session: {key}"));
        }
    }
}

// ---------- dispatch ----------

/// A click as handlers get it.
fn click_table(lua: &Lua, click: &ui::Click) -> mlua::Result<LuaValue> {
    let t = lua.create_table()?;
    t.set("right", click.click == ui::CLICK_RIGHT)?;
    t.set("shift", click.modifiers & ui::MODIFIER_SHIFT != 0)?;
    t.set("ctrl", click.modifiers & ui::MODIFIER_CTRL != 0)?;
    Ok(LuaValue::Table(t))
}

fn army(id: u64) -> impl FnOnce(&Lua) -> mlua::Result<MultiValue> {
    move |_| Ok(MultiValue::from_iter([LuaValue::Integer(id as i64)]))
}

fn none(_: &Lua) -> mlua::Result<MultiValue> {
    Ok(MultiValue::new())
}

/// Dispatches a game's safe frame (`crate::safe_point`) to every extension:
/// waiting tasks whose answer came, what happened in its order
/// ([`game::Frame::heard`]), what concerns one extension (its map coloring
/// taken over, its buttons clicked), then the frame.
pub fn frame(state: &game::Frame) {
    let all = extensions();
    for e in all {
        e.poll();
    }
    for heard in state.heard() {
        match heard {
            game::Heard::Session(session) => {
                let loaded = saves::take_loaded();
                log!("session {session} started (from a loaded save: {})", loaded.is_some());
                saves::replace_all(loaded.unwrap_or_default());
                for e in all {
                    let saved = saves::get(&e.id);
                    e.handle("on_session", |lua| Ok(MultiValue::from_iter([saved.as_ref().map_or(Ok(LuaValue::Nil), |v| values::to_lua(lua, v))?])));
                }
                refresh_report(true);
            }
            game::Heard::Change(events::Change::Ordered(id)) => all.iter().for_each(|e| e.handle("on_army_ordered", army(id))),
            game::Heard::Change(events::Change::Defeated(id)) => all.iter().for_each(|e| e.handle("on_army_defeated", army(id))),
            game::Heard::Day(date) => {
                let date = eu4_harmony_checks::Date(date);
                for e in all {
                    e.handle("on_day", |lua| {
                        let json = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(date)).map_err(mlua::Error::external)?;
                        Ok(MultiValue::from_iter([values::to_lua(lua, &json)?]))
                    });
                }
            }
        }
    }
    for lost in map::take_lost() {
        if let Some(e) = all.get(lost) {
            e.handle("on_map_lost", none);
        }
    }
    for click in ui::take_clicks() {
        if let Some(e) = all.get(click.extension) {
            let name = click.name.clone();
            e.handle("on_button", |lua| Ok(MultiValue::from_iter([LuaValue::String(lua.create_string(&name)?), click_table(lua, &click)?])));
        }
    }
    for e in all {
        e.handle("on_frame", none);
    }
    refresh_report(false);
}

/// The session is being left, for `reason` (`SESSION_ENDED_*`): every
/// extension hears it while the session's state is still the game's, on the
/// main thread, before anything of it is saved or taken down.
pub fn session_ended(reason: u32) {
    log!("session ending (reason {reason})");
    crate::remote::session_ended(reason);
    let word = match reason {
        SESSION_ENDED_LOAD => "load",
        SESSION_ENDED_NEW_GAME => "new_game",
        _ => "return",
    };
    for e in extensions() {
        e.handle("on_session_ended", |lua| Ok(MultiValue::from_iter([LuaValue::String(lua.create_string(word)?)])));
        e.session_over();
    }
}

/// Why a session is left: for the main menu.
pub const SESSION_ENDED_RETURN: u32 = 0;
/// For a save loaded from the session.
pub const SESSION_ENDED_LOAD: u32 = 1;
/// For a new game started from the session.
pub const SESSION_ENDED_NEW_GAME: u32 = 2;

/// Every extension's save data, from its `on_save` (ADR 0028), as the game
/// writes its world, on the main thread. One whose `on_save` fails or
/// returns what JSON cannot hold has none in this save.
pub fn save_data() {
    for e in extensions() {
        if e.state().stopped.is_some() {
            continue;
        }
        let Ok(LuaValue::Function(on_save)) = e.handlers.get::<LuaValue>("on_save") else { continue };
        let previous = CURRENT.with(|c| c.replace(Some(e.index)));
        let main = sandbox::main_state(&e.lua);
        watch::start(main);
        let started = Instant::now();
        let returned = on_save.call::<LuaValue>(e.game.clone());
        let elapsed = started.elapsed();
        let timed_out = watch::end();
        CURRENT.with(|c| c.set(previous));
        e.count(elapsed, timed_out);
        let value = returned.map_err(|err| err.to_string()).and_then(|v| values::to_json(&v)).and_then(|v| {
            eu4_harmony_checks::save_data::size(v.to_string().len()).map_err(|r| r.message)?;
            Ok(v)
        });
        match value {
            Ok(serde_json::Value::Null) => saves::set(&e.id, None),
            Ok(v) => saves::set(&e.id, Some(v)),
            Err(message) => {
                saves::set(&e.id, None);
                e.error("on_save", &format!("{message}; this save holds no data for the mod"));
            }
        }
    }
}

/// Rewrites the startup report at session starts and every few minutes.
fn refresh_report(now: bool) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if now || last.is_none_or(|t| t.elapsed() > Duration::from_secs(300)) {
        *last = Some(Instant::now());
        drop(last);
        crate::report::write();
    }
}

/// One line per extension: stretches, the time spent in them, and whether
/// it was stopped.
pub fn timing_lines() -> Vec<String> {
    extensions()
        .iter()
        .map(|e| {
            let state = e.state();
            let mut line = format!(
                "{}: {} stretch(es), {} ms in total, longest {} ms, {} over {} ms",
                e.id,
                state.stretches,
                state.total.as_millis(),
                state.longest.as_millis(),
                state.slow,
                SLOW.as_millis()
            );
            if let Some(reason) = &state.stopped {
                line.push_str(&format!("; STOPPED: {reason}"));
            }
            line
        })
        .collect()
}

/// The extension with this index, for its globals.
pub(crate) fn extension(index: usize) -> Option<&'static Arc<Extension>> {
    extensions().get(index)
}

#[cfg(test)]
mod tests;
