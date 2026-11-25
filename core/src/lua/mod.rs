//! The core's side of the Lua host (`eu4_harmony_lua`,
//! ADR extensions-are-lua-in-a-sandbox): loads every mod's extension at startup
//! on the live host, the running game, and calls its handlers at safe points
//! (`crate::safe_point`).
//!
//! The live host reaches the capability catalogue as the extension, through
//! the seam remote control's engine reaches it through as the player
//! (`crate::catalogue::Catalogue`), so both share one implementation
//! (ADR one-capability-catalogue);
//! interface elements, the map, texts and the player language through the
//! core's own modules; the network through [`http`]. Each extension's log is
//! `eu4-harmony/logs/extension-<id>.log`, with bounded older segments beside it.
mod http;

use crate::{
    catalogue::{self, Catalogue, Place, Wait},
    game::{self, map_coloring, Caller},
    map,
    mods::DiscoveredMod,
    notice::{Message, Repeat},
    saves, ui,
};
use eu4_harmony_lua::{
    dispatch::{self, Heard},
    fetch, Answered, Arg, Called, Element, Extension, Host, Mod, Pending, Popup, WindowChange,
};
use eu4_harmony_remote::extensions::args;
use eu4_harmony_remote::{Code, Error, Request, Response};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Single runtime coordinator owning loaded extensions, their launch metadata,
/// the shared host, active session clocks, and startup-report throttling.
static RUNTIME: ExtensionRuntime = ExtensionRuntime::new();

#[derive(Clone)]
struct Opened {
    id: String,
    name: String,
    dir: PathBuf,
    permissions: eu4_harmony_manifest::Permissions,
    data: PathBuf,
}

struct Loaded {
    opened: Opened,
    extension: Arc<Extension>,
}

struct RuntimeState {
    host: Option<Arc<dyn Host>>,
    loaded: Vec<Loaded>,
    session_clocks: Option<Clocks>,
    last_report: Option<Instant>,
}

struct ExtensionRuntime {
    state: Mutex<RuntimeState>,
}

impl ExtensionRuntime {
    const fn new() -> Self {
        Self { state: Mutex::new(RuntimeState { host: None, loaded: Vec::new(), session_clocks: None, last_report: None }) }
    }

    fn extensions(&self) -> Vec<Arc<Extension>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).loaded.iter().map(|l| l.extension.clone()).collect()
    }

    fn mod_id(&self, index: usize) -> Option<String> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).loaded.get(index).map(|l| l.extension.id().to_string())
    }

    fn mod_ids(&self) -> Vec<String> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).loaded.iter().map(|l| l.extension.id().to_string()).collect()
    }

    fn extension_indices(&self, mods: &[DiscoveredMod]) -> BTreeMap<usize, usize> {
        let list = self.extensions();
        mods.iter().enumerate().filter_map(|(i, m)| list.iter().find(|e| e.id() == m.manifest.id).map(|e| (i, e.index()))).collect()
    }

    fn maintain_logs(&self, checkpoint: bool) {
        let host = self.state.lock().unwrap_or_else(|e| e.into_inner()).host.clone();
        if let Some(host) = host {
            host.maintain_logs(checkpoint);
        }
    }

    fn load<E: RuntimeEnv>(&self, env: &E, mods: &[DiscoveredMod], host: Arc<dyn Host>, data_dir: &Path) -> Vec<LoadOutcome> {
        let host = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.host.get_or_insert(host).clone()
        };
        let supported = env.supported();
        let mut loaded = Vec::new();
        let mut outcomes = Vec::new();
        for m in mods {
            let Some(entry) = &m.layout.extension else {
                outcomes.push(LoadOutcome { included: true, status: "no extension".into() });
                continue;
            };
            let result = if !supported {
                Err("the game executable is not supported, so no extension can run".to_string())
            } else if let Some(why) = m.manifest.unsupported(eu4_harmony_remote::API_VERSION) {
                Err(why)
            } else {
                // The shared journal opens on the first log call, before main.lua
                // can write; leases prevent another process rotating this run.
                crate::log::event(format_args!(
                    "extension loading: id={} version={} interface={}",
                    m.manifest.id,
                    m.descriptor.version.as_deref().unwrap_or("not declared"),
                    m.manifest.api_version
                ));
                let opening = Mod {
                    id: &m.manifest.id,
                    name: &m.descriptor.name,
                    harmony: m.dir.join("eu4-harmony"),
                    permissions: &m.manifest.permissions,
                    data: Some(data_dir.join(&m.manifest.id)),
                };
                eu4_harmony_lua::open(loaded.len(), &opening, host.clone())
            };
            match result {
                Ok(extension) => {
                    log!("extension {:?} loaded", m.manifest.id);
                    env.declare_windows(&extension);
                    let opened = Opened {
                        id: m.manifest.id.clone(),
                        name: m.descriptor.name.clone(),
                        dir: m.dir.clone(),
                        permissions: m.manifest.permissions.clone(),
                        data: data_dir.join(&m.manifest.id),
                    };
                    outcomes.push(LoadOutcome { included: true, status: format!("extension {entry} loaded") });
                    loaded.push(Loaded { opened, extension });
                }
                Err(e) => {
                    log!("extension {:?} not loaded: {e}", m.manifest.id);
                    outcomes.push(LoadOutcome { included: false, status: format!("extension NOT loaded: {e}") });
                    // An unsupported executable has its own notice.
                    if supported {
                        env.post_notice(&format!("load:{}", m.manifest.id), Repeat::Run, not_loaded(m, &e));
                    }
                }
            }
        }
        self.state.lock().unwrap_or_else(|e| e.into_inner()).loaded = loaded;
        outcomes
    }

    fn reload<E: RuntimeEnv>(&self, env: &E, id: &str) -> game::GResult<Reloaded> {
        let refuse = |status, message: String| Err(game::Fail { status, message });
        let (host, index, opened, old) = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(host) = state.host.clone() else {
                return refuse(game::Status::NotFound, format!("no extension of a mod {id:?} is loaded"));
            };
            let Some((i, entry)) = state.loaded.iter().enumerate().find(|(_, l)| l.opened.id == id) else {
                return refuse(game::Status::NotFound, format!("no extension of a mod {id:?} is loaded"));
            };
            (host, i, entry.opened.clone(), entry.extension.clone())
        };
        let harmony = opened.dir.join("eu4-harmony");
        let rejected = |message: String| game::Fail { status: game::Status::Rejected, message };
        let manifest = eu4_harmony_manifest::HarmonyMod::read(&opened.dir).map_err(rejected)?.manifest;
        if manifest.id != id {
            return Err(rejected(format!("the manifest's id is now {:?}; a changed id takes a restart of the game", manifest.id)));
        }
        if manifest.permissions != opened.permissions {
            return Err(rejected(
                "the manifest's permissions changed; the player saw them as they were, and they change only with a restart of the game".into(),
            ));
        }
        if let Some(why) = manifest.unsupported(eu4_harmony_remote::API_VERSION) {
            return Err(rejected(why));
        }
        // The new texts first: the new code's windows are checked against them.
        let (problems, earlier) = env.reload_loc(id, &harmony);
        let opening = Mod { id, name: &opened.name, harmony: harmony.clone(), permissions: &opened.permissions, data: Some(opened.data.clone()) };
        let new = match eu4_harmony_lua::open(index, &opening, host.clone()) {
            Ok(new) => new,
            Err(e) => {
                env.restore_loc(id, earlier);
                return Err(rejected(format!("the new code did not load, and the old goes on: {e}")));
            }
        };
        if new.images() != old.images() {
            env.restore_loc(id, earlier);
            return Err(rejected("entry images changed; their native resources take effect at the next start, and the old extension goes on".into()));
        }
        let session = env.session_going_on();
        let outcome = eu4_harmony_lua::reload_handoff(&old, &new, host.as_ref(), session);
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(slot) = state.loaded.get_mut(index).filter(|l| l.opened.id == id) else {
                return refuse(game::Status::NotFound, format!("no extension of a mod {id:?} is loaded"));
            };
            slot.extension = new.clone();
        }
        for problem in problems {
            log!("extension {id:?} reloaded: localisation/{problem}");
        }
        log!("extension {id:?} reloaded{}", outcome.lost.as_ref().map(|l| format!("; its state was not carried: {l}")).unwrap_or_default());
        env.saves_reloaded(id, outcome.saved.clone());
        if session {
            new.handle("on_session", &[Arg::Json(outcome.saved.unwrap_or(serde_json::Value::Null))]);
        }
        self.refresh_report(env, true);
        Ok(Reloaded { session, lost: outcome.lost })
    }

    fn frame<E: RuntimeEnv>(&self, env: &E, heard: &[game::Heard]) {
        let all = self.extensions();
        let heard: Vec<Heard> = heard.iter().map(|h| Heard::of(&h.signal)).collect();
        let mut feed = Fed { runtime: self, env, began: false };
        dispatch::frame(&all, &heard, &mut feed);
        let began = feed.began;
        env.extensions_listen(dispatch::listened(&all).into_iter().collect());
        env.windows_frame();
        self.refresh_report(env, began);
    }

    fn tell_ended<E: RuntimeEnv>(&self, env: &E, end: crate::sessions::End) {
        let all = self.extensions();
        dispatch::hear(&all, &Heard::of(&end.signal()), &mut Fed { runtime: self, env, began: false });
        env.windows_session_ended();
        let Some(began) = self.state.lock().unwrap_or_else(|e| e.into_inner()).session_clocks.take() else { return };
        let now = env.clocks_now();
        let (wall, main, game) = (now.wall.saturating_duration_since(began.wall), now.main.saturating_sub(began.main), now.game.saturating_sub(began.game));
        let share = |part: Duration, of: Duration| if of.is_zero() { 0.0 } else { part.as_secs_f64() / of.as_secs_f64() * 100.0 };
        for e in all {
            let t = e.take_session_times();
            log!(
                "{}, the session: {}; {:.2}% of its {:.1} s, {:.2}% of the main thread's {:.1} s of CPU, {:.2}% of the game's {:.1} s of CPU",
                e.id(),
                t.summary(),
                share(t.total, wall),
                wall.as_secs_f64(),
                share(t.total, main),
                main.as_secs_f64(),
                share(t.total, game),
                game.as_secs_f64()
            );
        }
    }

    fn collect_save_data(&self) -> Vec<(String, Option<serde_json::Value>)> {
        self.extensions()
            .into_iter()
            .map(|e| {
                let value = match e.save_data() {
                    Some(Ok(data)) => data,
                    None | Some(Err(_)) => None,
                };
                (e.id().to_string(), value)
            })
            .collect()
    }

    fn refresh_report<E: RuntimeEnv>(&self, env: &E, now: bool) {
        let current = env.now();
        let due = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if now || state.last_report.is_none_or(|t| current.saturating_duration_since(t) > Duration::from_secs(300)) {
                state.last_report = Some(current);
                true
            } else {
                false
            }
        };
        if due {
            env.write_report();
        }
    }

    fn timing_lines(&self) -> Vec<String> {
        self.extensions().iter().map(|e| e.timing()).collect()
    }
}

/// Core side-effect boundary used by [`ExtensionRuntime`], implemented by
/// [`CoreEnv`] in production and faked in tests.
trait RuntimeEnv {
    type LocBackup;

    fn supported(&self) -> bool;
    fn declare_windows(&self, extension: &Extension);
    fn post_notice(&self, key: &str, repeat: Repeat, message: Message);
    fn reload_loc(&self, id: &str, harmony: &Path) -> (Vec<String>, Self::LocBackup);
    fn restore_loc(&self, id: &str, earlier: Self::LocBackup);
    fn session_going_on(&self) -> bool;
    fn saves_begin_session(&self) -> bool;
    fn saves_get(&self, id: &str) -> Option<serde_json::Value>;
    fn saves_reloaded(&self, id: &str, saved: Option<serde_json::Value>);
    fn map_take_lost(&self) -> Vec<usize>;
    fn take_clicks(&self) -> Vec<dispatch::Click>;
    fn take_windows_seen(&self) -> Vec<dispatch::WindowSeen>;
    fn take_popups_pressed(&self) -> Vec<dispatch::PopupPressed>;
    fn extensions_listen(&self, kinds: BTreeSet<eu4_harmony_remote::signals::Kind>);
    fn windows_frame(&self);
    fn windows_session_ended(&self);
    fn write_report(&self);
    fn clocks_now(&self) -> Clocks;
    fn now(&self) -> Instant;
}

struct CoreEnv;

impl RuntimeEnv for CoreEnv {
    type LocBackup = crate::loc::Earlier;

    fn supported(&self) -> bool {
        crate::memory().is_some()
    }

    fn declare_windows(&self, extension: &Extension) {
        declare_windows(extension);
    }

    fn post_notice(&self, key: &str, repeat: Repeat, message: Message) {
        crate::notice::post(key, repeat, message);
    }

    fn reload_loc(&self, id: &str, harmony: &Path) -> (Vec<String>, Self::LocBackup) {
        crate::loc::reload_mod(id, harmony)
    }

    fn restore_loc(&self, id: &str, earlier: Self::LocBackup) {
        crate::loc::restore_mod(id, earlier);
    }

    fn session_going_on(&self) -> bool {
        crate::sessions::going_on()
    }

    fn saves_begin_session(&self) -> bool {
        saves::begin_session()
    }

    fn saves_get(&self, id: &str) -> Option<serde_json::Value> {
        saves::get(id)
    }

    fn saves_reloaded(&self, id: &str, saved: Option<serde_json::Value>) {
        saves::reloaded(id, saved);
    }

    fn map_take_lost(&self) -> Vec<usize> {
        map::take_lost()
    }

    fn take_clicks(&self) -> Vec<dispatch::Click> {
        let click = |c: ui::Click| dispatch::Click {
            extension: c.extension,
            button: c.name,
            right: c.click == ui::CLICK_RIGHT,
            shift: c.modifiers & ui::MODIFIER_SHIFT != 0,
            ctrl: c.modifiers & ui::MODIFIER_CTRL != 0,
            row: None,
        };
        let mut clicks: Vec<dispatch::Click> = ui::take_clicks().into_iter().map(click).collect();
        clicks.extend(crate::windows::take_clicks());
        clicks
    }

    fn take_windows_seen(&self) -> Vec<dispatch::WindowSeen> {
        crate::windows::take_seen()
    }

    fn take_popups_pressed(&self) -> Vec<dispatch::PopupPressed> {
        crate::windows::take_popups_pressed()
    }

    fn extensions_listen(&self, kinds: BTreeSet<eu4_harmony_remote::signals::Kind>) {
        game::signals::extensions_listen(kinds);
    }

    fn windows_frame(&self) {
        crate::windows::frame();
    }

    fn windows_session_ended(&self) {
        crate::windows::session_ended();
    }

    fn write_report(&self) {
        crate::report::write();
    }

    fn clocks_now(&self) -> Clocks {
        Clocks::now()
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The id of the mod whose extension has this index.
pub fn mod_id(index: usize) -> Option<String> {
    RUNTIME.mod_id(index)
}

/// The ids of the mods whose extensions loaded.
pub fn mod_ids() -> Vec<String> {
    RUNTIME.mod_ids()
}

/// Maps discovery-order mod indices to extension indices.
pub fn extension_indices(mods: &[DiscoveredMod]) -> BTreeMap<usize, usize> {
    RUNTIME.extension_indices(mods)
}

// ---------- the live host ----------

/// The running game, as extensions reach it.
struct Live {
    /// The logs folder, and each extension's log once it is open.
    logs: PathBuf,
    open: Mutex<HashMap<String, ExtensionLog>>,
    reach: Reach,
}

struct ExtensionLog {
    file: Option<eu4_harmony_logging::Journal>,
    retry: Instant,
    unconfirmed: u64,
}

/// How the live host reaches the capability catalogue as an extension: hands
/// `with` the catalogue as `Caller`, the capability layer itself in the game.
type Reach = fn(caller: Caller, with: &mut dyn FnMut(&mut dyn Catalogue));

fn layer(caller: Caller, with: &mut dyn FnMut(&mut dyn Catalogue)) {
    with(&mut catalogue::Live(caller));
}

/// A capability's answer still to come, polled at a game's safe frame.
struct Later(Reach, Caller, Box<dyn Wait>);

impl Pending<Answered> for Later {
    fn poll(&mut self) -> Option<Answered> {
        let Self(reach, caller, wait) = self;
        let mut answer = None;
        reach(*caller, &mut |c| c.at(None, Instant::now(), &Place::Game, &mut |at| answer = wait.poll(at)));
        answer.map(lua_form)
    }
}

/// A response as the Lua host takes it: the answer written in Lua's form.
fn lua_form(response: Response) -> Answered {
    let answer = response?;
    eu4_harmony_checks::form::as_lua(|| serde_json::to_value(&answer)).map_err(|e| Error::new(Code::Internal, format!("the answer cannot be written: {e}")))
}

impl Live {
    fn with_journal(&self, id: &str, write: impl FnOnce(&mut eu4_harmony_logging::Journal) -> std::io::Result<()>) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let state = open.entry(id.to_string()).or_insert_with(|| ExtensionLog { file: None, retry: Instant::now(), unconfirmed: 0 });
        if state.file.is_none() && Instant::now() >= state.retry {
            match eu4_harmony_logging::Journal::open(&self.logs, &eu4_harmony_logging::extension_source(id)) {
                Ok(mut file) => {
                    if state.unconfirmed != 0
                        && file
                            .write(
                                eu4_harmony_logging::Kind::Event,
                                &format!("extension log recovered; {} earlier records were not confirmed persisted", state.unconfirmed),
                            )
                            .is_ok()
                    {
                        state.unconfirmed = 0;
                    }
                    state.file = Some(file);
                }
                Err(error) => {
                    state.retry = Instant::now() + Duration::from_secs(60);
                    log!("cannot open extension log {id:?}: {error}");
                }
            }
        }
        let written = state.file.as_mut().is_some_and(|file| write(file).is_ok());
        if !written {
            state.unconfirmed = state.unconfirmed.saturating_add(1);
        }
    }
}

impl Host for Live {
    fn run(&self, extension: usize, request: Request) -> Called {
        let caller = Caller::Extension(extension);
        let (mut request, mut called) = (Some(request), None);
        (self.reach)(caller, &mut |c| {
            let Some(request) = request.take() else { return };
            // Extensions give no input, which only remote clients have.
            called = Some(match c.run(request, None, Instant::now()) {
                catalogue::Outcome::Done(response) | catalogue::Outcome::Held(response, _) => Called::Done(lua_form(response)),
                catalogue::Outcome::Later(wait) => Called::Later(Box::new(Later(self.reach, caller, wait))),
            })
        });
        called.unwrap_or_else(|| Called::Done(Err(Error::new(Code::Internal, "the capability catalogue was not reached"))))
    }

    fn set_element(&self, extension: usize, element: &str, value: Element) -> Result<(), String> {
        ui::set_element(extension, element, value)
    }

    fn window(&self, extension: usize, change: WindowChange) -> Result<(), String> {
        crate::windows::change(extension, change)
    }

    fn tooltip(&self, extension: usize, window: Option<&str>, widget: &str) -> Result<eu4_harmony_lua::Text, String> {
        match window {
            Some(window) => crate::windows::tooltip(extension, window, widget),
            None => ui::tooltip(extension, widget),
        }
    }

    fn popup(&self, extension: usize, popup: Popup) -> Result<u32, String> {
        crate::windows::popup(extension, popup)
    }

    fn close_popup(&self, extension: usize, popup: u32) -> Result<(), String> {
        crate::windows::close_popup(extension, popup)
    }

    fn paint(&self, extension: usize, colors: &[(u32, [u8; 4])]) -> Result<(), Error> {
        let colors: Vec<_> = colors.iter().map(|&(province, rgba)| map_coloring::MapColor { province, rgba }).collect();
        map_coloring::paint(Caller::Extension(extension), &colors).map_err(Error::from)
    }

    fn release_map(&self, extension: usize) -> Result<(), Error> {
        map_coloring::clear(Caller::Extension(extension)).map_err(Error::from)
    }

    fn text(&self, id: &str, key: &str) -> Option<String> {
        crate::loc::mod_text(id, key)
    }

    fn language(&self) -> String {
        crate::lang::player().id().to_string()
    }

    fn fetch(&self, request: fetch::Request) -> Box<dyn Pending<fetch::Answer>> {
        http::start(request)
    }

    fn log(&self, id: &str, line: &str) {
        let kind = match eu4_harmony_logging::kind(line) {
            eu4_harmony_logging::Kind::Critical => eu4_harmony_logging::Kind::Critical,
            _ => eu4_harmony_logging::Kind::Event,
        };
        self.with_journal(id, |file| file.write(kind, line));
    }

    fn repeated_error(&self, id: &str, message: &str) -> bool {
        self.with_journal(id, |file| file.count_repeated(&format!("error in {message}")));
        true
    }

    fn maintain_logs(&self, checkpoint: bool) {
        if let Ok(mut open) = self.open.try_lock() {
            for (id, state) in open.iter_mut() {
                if let Some(file) = state.file.as_mut() {
                    let _ = if checkpoint { file.checkpoint() } else { file.maintain() };
                }
                if checkpoint && state.unconfirmed != 0 {
                    log!("extension log {id:?}: {} records not confirmed persisted", state.unconfirmed);
                }
            }
        }
    }

    fn session(&self) -> u64 {
        crate::sessions::number()
    }

    fn first_error(&self, extension: &Extension, from: &str, message: &str) {
        // The player is told in a game the framework runs in.
        if crate::memory().is_some() {
            log!("extension {:?}: error in {from}: {message}", extension.id());
            crate::notice::post(&format!("errors:{}", extension.id()), Repeat::Session, Message::new("EU4HARMONY_ERRORS").arg("MOD", extension.name()));
        }
    }

    fn stopped(&self, extension: &Extension, reason: &str) {
        log!("extension {:?} stopped: {reason}", extension.id());
        ui::hide_extension(extension.index());
        crate::windows::forget(extension.index());
        if let Some(m) = crate::memory() {
            map::release(m, extension.index());
            crate::notice::post(
                &format!("stopped:{}", extension.id()),
                Repeat::Run,
                Message::new("EU4HARMONY_STOPPED").arg("MOD", extension.name()).arg("DETAIL", reason),
            );
        }
    }

    fn reload_reset(&self, extension: usize, new: &Extension) {
        ui::forget_extension(extension);
        declare_windows(new);
        if let Some(m) = crate::memory() {
            map::release(m, extension);
        }
    }
}

/// Log maintenance shares the safe frame's clock gate with the core. It
/// never waits for an extension that is writing a line on another thread.
pub(crate) fn maintain_logs(checkpoint: bool) {
    RUNTIME.maintain_logs(checkpoint);
}

#[cfg(test)]
fn log_file(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{}.log", eu4_harmony_logging::extension_source(id)))
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
    let host: Arc<dyn Host> = Arc::new(Live { logs: log_dir.to_path_buf(), open: Mutex::new(HashMap::new()), reach: layer });
    RUNTIME.load(&CoreEnv, mods, host, data_dir)
}

/// The windows an extension's main.lua returns, for the framework to build.
fn declare_windows(extension: &Extension) {
    crate::windows::declare(extension.index(), extension.id(), extension.windows(), extension.images());
}

/// What an extension reload came to.
#[derive(Debug)]
pub struct Reloaded {
    /// A session was going on, so the new code's `on_session` ran.
    pub session: bool,
    /// Why the old code's save data did not reach it, when it did not.
    pub lost: Option<String>,
}

/// Reloads the extension of the mod `id` from its files, in place
/// (cli/skills/mod.md): the new code is opened first, and only when
/// it loads is the old one asked for its save data and replaced. On the
/// main thread, between frames.
pub fn reload(_: Caller, args::Reload { id }: args::Reload) -> game::GResult<Reloaded> {
    if crate::thread_id() != crate::main_thread() {
        return Err(game::Fail { status: game::Status::WrongContext, message: "called off the game's main thread".into() });
    }
    RUNTIME.reload(&CoreEnv, &id)
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

// ---------- dispatch ----------

/// The extensions, as they hear the sessions' ends and the game's safe frames
/// (`crate::safe_point`).
pub(crate) struct Extensions;

impl crate::safe_point::Listener for Extensions {
    fn name(&self) -> &'static str {
        "extensions"
    }

    /// Within an extension's own call, no extension is called again: one
    /// would run inside itself, its waiting tasks dropped under the task
    /// running.
    fn can_hear(&self) -> bool {
        !eu4_harmony_lua::calling()
    }

    /// Every extension hears it, on the main thread, before anything of the
    /// session is taken down: once leaving is certain, after the ironman exit
    /// save, when there is one, was written. Heard late, after the extension's
    /// call it ended in, the session's state is gone by then, as what ended
    /// it ran inside that call.
    fn ended(&self, end: crate::sessions::End) {
        RUNTIME.tell_ended(&CoreEnv, end);
    }

    fn frame(&self, _: u64, heard: &[game::Heard]) {
        RUNTIME.frame(&CoreEnv, heard);
    }
}

/// What the running game gives a frame: the extension save data of the save
/// a session began from, the maps taken over and the buttons clicked.
struct Fed<'a, E: RuntimeEnv> {
    runtime: &'a ExtensionRuntime,
    env: &'a E,
    /// A session began in this frame.
    began: bool,
}

impl<E: RuntimeEnv> dispatch::Feed for Fed<'_, E> {
    fn session_begins(&mut self, session: u64) {
        let from_save = self.env.saves_begin_session();
        log!("session {session} started (from a loaded save: {from_save})");
        self.runtime.state.lock().unwrap_or_else(|e| e.into_inner()).session_clocks = Some(self.env.clocks_now());
        self.began = true;
    }

    fn saved(&mut self, id: &str) -> Option<serde_json::Value> {
        self.env.saves_get(id)
    }

    fn map_lost(&mut self) -> Vec<usize> {
        self.env.map_take_lost()
    }

    fn clicks(&mut self) -> Vec<dispatch::Click> {
        self.env.take_clicks()
    }

    fn windows(&mut self) -> Vec<dispatch::WindowSeen> {
        self.env.take_windows_seen()
    }

    fn popups_pressed(&mut self) -> Vec<dispatch::PopupPressed> {
        self.env.take_popups_pressed()
    }
}

/// A session's clocks: the time, and the CPU time the game's main thread and
/// the whole game have had, to set what extensions took against.
struct Clocks {
    wall: Instant,
    main: Duration,
    game: Duration,
}

impl Clocks {
    /// Read on the main thread, which sessions start and end on.
    fn now() -> Self {
        use windows_sys::Win32::{
            Foundation::FILETIME,
            System::Threading::{GetCurrentProcess, GetCurrentThread, GetProcessTimes, GetThreadTimes},
        };
        let zero = || FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let time = |t: FILETIME| Duration::from_nanos((u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime)) * 100);
        let (mut created, mut exited, mut kernel, mut user) = (zero(), zero(), zero(), zero());
        let main = match unsafe { GetThreadTimes(GetCurrentThread(), &mut created, &mut exited, &mut kernel, &mut user) } {
            0 => Duration::ZERO,
            _ => time(kernel) + time(user),
        };
        let game = match unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) } {
            0 => Duration::ZERO,
            _ => time(kernel) + time(user),
        };
        Clocks { wall: Instant::now(), main, game }
    }
}

/// Polls every live extension's `save_data` handler on the main thread
/// (ADR save-data-is-asked-for-by-save-data). An extension that is stopped, has
/// no `save_data`, returns `nil`, or fails contributes `None`.
pub(crate) fn collect_save_data() -> Vec<(String, Option<serde_json::Value>)> {
    RUNTIME.collect_save_data()
}

/// One line per extension: stretches, the time spent in them, and whether
/// it was stopped.
pub fn timing_lines() -> Vec<String> {
    RUNTIME.timing_lines()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host that only keeps the extensions' log lines.
    #[derive(Default)]
    struct Quiet(Mutex<Vec<String>>);

    struct Never;
    impl Pending<fetch::Answer> for Never {
        fn poll(&mut self) -> Option<fetch::Answer> {
            None
        }
    }

    impl Host for Quiet {
        fn run(&self, _: usize, _: Request) -> Called {
            Called::Done(Err(Error::new(Code::NotInGame, "no game in a test")))
        }
        fn set_element(&self, _: usize, _: &str, _: Element) -> Result<(), String> {
            Ok(())
        }
        fn window(&self, _: usize, _: WindowChange) -> Result<(), String> {
            Ok(())
        }
        fn tooltip(&self, _: usize, _: Option<&str>, _: &str) -> Result<eu4_harmony_lua::Text, String> {
            Ok(Default::default())
        }
        fn popup(&self, _: usize, _: Popup) -> Result<u32, String> {
            Err("no game in a test".into())
        }
        fn close_popup(&self, _: usize, _: u32) -> Result<(), String> {
            Ok(())
        }
        fn paint(&self, _: usize, _: &[(u32, [u8; 4])]) -> Result<(), Error> {
            Ok(())
        }
        fn release_map(&self, _: usize) -> Result<(), Error> {
            Ok(())
        }
        fn text(&self, _: &str, _: &str) -> Option<String> {
            None
        }
        fn language(&self) -> String {
            "english".into()
        }
        fn fetch(&self, _: fetch::Request) -> Box<dyn Pending<fetch::Answer>> {
            Box::new(Never)
        }
        fn log(&self, _: &str, line: &str) {
            self.0.lock().unwrap().push(line.to_string());
        }
        fn session(&self) -> u64 {
            1
        }
        fn first_error(&self, _: &Extension, _: &str, _: &str) {}
        fn stopped(&self, _: &Extension, _: &str) {}
    }

    #[derive(Default)]
    struct ScriptedCatalogue {
        calls: Vec<String>,
    }

    impl Catalogue for ScriptedCatalogue {
        fn run(&mut self, request: Request, _: Option<&catalogue::signals::Buffer>, _: Instant) -> catalogue::Outcome {
            use eu4_harmony_remote::{
                armies::{Army, ArmyAnswer, ArmyKind, ArmyRequest, Units},
                session::{Session, SessionAnswer, SessionRequest},
                Answer, ArmyId, CountryTag, Date, ProvinceId,
            };
            let response = match request {
                Request::Army(ArmyRequest::Get { army: ArmyId(id) }) => {
                    self.calls.push(format!("army {id}"));
                    if id == 4_294_967_377 {
                        Ok(Answer::Army(ArmyAnswer::Get(Army {
                            id: ArmyId(id),
                            owner: CountryTag(*b"TUR"),
                            kind: ArmyKind::Army,
                            province: Some(ProvinceId(149)),
                            regiments: 12,
                            blocked: Vec::new(),
                            route: Vec::new(),
                            route_by: None,
                            aboard: None,
                            carrying: 0,
                            units: Units { infantry: 12, ..Default::default() },
                            strength: 12.0,
                            max_strength: 12.0,
                            morale: 3.0,
                            max_morale: 3.0,
                            leader: None,
                            maintenance: Some(6.0),
                        })))
                    } else {
                        Err(Error::new(Code::NotFound, format!("no own army {id}")))
                    }
                }
                Request::Session(SessionRequest::Get {}) => {
                    self.calls.push("session".into());
                    Ok(Answer::Session(SessionAnswer::Get(Session { number: 2, player: Some(CountryTag(*b"TUR")), observing: false, date: Date(0x035D_7710) })))
                }
                other => Err(Error::new(Code::Internal, format!("{} is not scripted in this test", other.name()))),
            };
            catalogue::Outcome::Done(response)
        }

        fn at(&mut self, signals: Option<&catalogue::signals::Buffer>, now: Instant, place: &Place, polls: &mut dyn FnMut(&mut catalogue::Poll<'_>)) {
            polls(&mut catalogue::Poll::new(signals, now, place, None));
        }
    }

    thread_local! {
        static GAME: std::cell::RefCell<ScriptedCatalogue> = std::cell::RefCell::default();
    }

    /// The catalogue over the scripted test backend, as any extension calls it.
    fn fake_game(_: Caller, with: &mut dyn FnMut(&mut dyn Catalogue)) {
        GAME.with(|game| with(&mut *game.borrow_mut()));
    }

    #[test]
    fn an_extension_s_call_reaches_the_catalogue_and_answers_in_lua_s_form() {
        let dir = std::env::temp_dir().join(format!("eu4h-lua-catalogue-{}", std::process::id()));
        let harmony = dir.join("eu4-harmony");
        std::fs::create_dir_all(harmony.join("lua")).unwrap();
        let main = r#"
            return {
              on_frame = function(game)
                local army = game.army.get(4294967377)
                local session = game.session.get()
                local missing, why = game.army.get(1)
                local date = session.date
                log(math.type(army.id) .. " " .. army.id .. " " .. date.year .. "." .. date.month .. "." .. date.day .. " " .. tostring(missing) .. " " .. why.code)
              end,
            }
        "#;
        std::fs::write(harmony.join("lua/main.lua"), main).unwrap();
        let host = Arc::new(Live { logs: dir.clone(), open: Mutex::new(HashMap::new()), reach: fake_game });
        let permissions = eu4_harmony_manifest::Permissions::default();
        let m = Mod { id: "calling", name: "Calling", harmony, permissions: &permissions, data: Some(dir.join("data")) };
        let extension = eu4_harmony_lua::open(0, &m, host.clone()).unwrap();
        extension.handle("on_frame", &[]);
        drop(host);
        // An army's ID an integer, a date its year, month and day, and a
        // refusal nil and why. On a busy machine the host also logs the
        // handler's stretch as slow, which is not the extension's line.
        let source = eu4_harmony_logging::extension_source("calling");
        let logged = std::fs::read_to_string(log_file(&dir, "calling")).unwrap();
        let logged: Vec<&str> =
            logged.lines().filter_map(|line| eu4_harmony_logging::record_message(line, &source)).filter(|line| !line.starts_with("slow stretch:")).collect();
        assert_eq!(logged, ["integer 4294967377 1444.11.11 nil not_found"]);
        GAME.with(|game| assert_eq!(game.borrow().calls, ["army 4294967377", "session", "army 1"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handler_errors_write_three_times_in_full_and_summarize_remaining_counts_once() {
        let dir = std::env::temp_dir().join(format!("eu4h-handler-error-dedup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let harmony = dir.join("eu4-harmony");
        std::fs::create_dir_all(harmony.join("lua")).unwrap();
        std::fs::write(harmony.join("lua/main.lua"), "return { on_day = function(game) error('boom') end }").unwrap();
        let logs = dir.join("logs");
        let host = Arc::new(Live { logs: logs.clone(), open: Mutex::new(HashMap::new()), reach: fake_game });
        let permissions = eu4_harmony_manifest::Permissions::default();
        let m = Mod { id: "failing", name: "Failing", harmony, permissions: &permissions, data: None };
        let extension = eu4_harmony_lua::open(0, &m, host.clone()).unwrap();
        for _ in 0..6 {
            extension.handle("on_day", &[]);
        }
        extension.session_over();
        host.maintain_logs(true);
        let source = eu4_harmony_logging::extension_source("failing");
        let messages = eu4_harmony_logging::SourceMark::default().messages_since(&logs, &source);
        let full = messages.iter().filter(|m| m.starts_with("error in on_day:") && m.contains("boom")).count();
        let summaries: Vec<&String> = messages.iter().filter(|m| m.starts_with("repeated error: count=3 ") && m.contains("error in on_day:")).collect();
        let duplicate_session_lines = messages.iter().filter(|m| m.contains("times this session:")).count();
        assert_eq!(full, 3, "{messages:?}");
        assert_eq!(summaries.len(), 1, "{messages:?}");
        assert_eq!(duplicate_session_lines, 0, "{messages:?}");
        drop(extension);
        drop(host);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unavailable_extension_log_backs_off_and_reports_unconfirmed_records_on_recovery() {
        let dir = std::env::temp_dir().join(format!("eu4h-extension-log-recovery-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let logs = dir.join("logs");
        std::fs::write(&logs, "not a folder").unwrap();
        let host = Live { logs: logs.clone(), open: Mutex::new(HashMap::new()), reach: fake_game };
        host.log("recovery", "first");
        host.log("recovery", "second");
        {
            let open = host.open.lock().unwrap();
            let log = open.get("recovery").unwrap();
            assert!(log.file.is_none());
            assert_eq!(log.unconfirmed, 2);
            assert!(log.retry > Instant::now());
        }
        std::fs::remove_file(&logs).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        host.open.lock().unwrap().get_mut("recovery").unwrap().retry = Instant::now();
        host.log("recovery", "working again");
        let text = std::fs::read_to_string(log_file(&logs, "recovery")).unwrap();
        assert!(text.contains("2 earlier records were not confirmed persisted"));
        assert!(text.contains("working again"));
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    struct FakeEnv {
        supported: bool,
        session_going_on: std::sync::atomic::AtomicBool,
        now: Mutex<Instant>,
        saved_in: Mutex<HashMap<String, serde_json::Value>>,
        declared: Mutex<Vec<String>>,
        notices: Mutex<Vec<(String, String)>>,
        loc_reloads: Mutex<Vec<String>>,
        loc_restores: Mutex<Vec<(String, u32)>>,
        saves_reloaded: Mutex<Vec<(String, Option<serde_json::Value>)>>,
        listened: Mutex<Vec<BTreeSet<eu4_harmony_remote::signals::Kind>>>,
        windows_frames: std::sync::atomic::AtomicUsize,
        windows_ended: std::sync::atomic::AtomicUsize,
        reports: std::sync::atomic::AtomicUsize,
        begins: std::sync::atomic::AtomicUsize,
    }

    impl FakeEnv {
        fn new() -> Self {
            Self {
                supported: true,
                session_going_on: std::sync::atomic::AtomicBool::new(false),
                now: Mutex::new(Instant::now()),
                saved_in: Mutex::new(HashMap::new()),
                declared: Mutex::new(Vec::new()),
                notices: Mutex::new(Vec::new()),
                loc_reloads: Mutex::new(Vec::new()),
                loc_restores: Mutex::new(Vec::new()),
                saves_reloaded: Mutex::new(Vec::new()),
                listened: Mutex::new(Vec::new()),
                windows_frames: std::sync::atomic::AtomicUsize::new(0),
                windows_ended: std::sync::atomic::AtomicUsize::new(0),
                reports: std::sync::atomic::AtomicUsize::new(0),
                begins: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    impl RuntimeEnv for FakeEnv {
        type LocBackup = u32;

        fn supported(&self) -> bool {
            self.supported
        }

        fn declare_windows(&self, extension: &Extension) {
            self.declared.lock().unwrap().push(extension.id().to_string());
        }

        fn post_notice(&self, key: &str, _: Repeat, message: Message) {
            self.notices.lock().unwrap().push((key.to_string(), format!("{message:?}")));
        }

        fn reload_loc(&self, id: &str, _: &Path) -> (Vec<String>, Self::LocBackup) {
            let mut reloads = self.loc_reloads.lock().unwrap();
            reloads.push(id.to_string());
            (Vec::new(), reloads.len() as u32)
        }

        fn restore_loc(&self, id: &str, earlier: Self::LocBackup) {
            self.loc_restores.lock().unwrap().push((id.to_string(), earlier));
        }

        fn session_going_on(&self) -> bool {
            self.session_going_on.load(std::sync::atomic::Ordering::Relaxed)
        }

        fn saves_begin_session(&self) -> bool {
            self.begins.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            !self.saved_in.lock().unwrap().is_empty()
        }

        fn saves_get(&self, id: &str) -> Option<serde_json::Value> {
            self.saved_in.lock().unwrap().get(id).cloned()
        }

        fn saves_reloaded(&self, id: &str, saved: Option<serde_json::Value>) {
            self.saves_reloaded.lock().unwrap().push((id.to_string(), saved));
        }

        fn map_take_lost(&self) -> Vec<usize> {
            Vec::new()
        }

        fn take_clicks(&self) -> Vec<dispatch::Click> {
            Vec::new()
        }

        fn take_windows_seen(&self) -> Vec<dispatch::WindowSeen> {
            Vec::new()
        }

        fn take_popups_pressed(&self) -> Vec<dispatch::PopupPressed> {
            Vec::new()
        }

        fn extensions_listen(&self, kinds: BTreeSet<eu4_harmony_remote::signals::Kind>) {
            self.listened.lock().unwrap().push(kinds);
        }

        fn windows_frame(&self) {
            self.windows_frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        fn windows_session_ended(&self) {
            self.windows_ended.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        fn write_report(&self) {
            self.reports.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        fn clocks_now(&self) -> Clocks {
            Clocks { wall: *self.now.lock().unwrap(), main: Duration::ZERO, game: Duration::ZERO }
        }

        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }
    }

    fn temp_mod(tag: &str, id: &str, api_version: &str, lua: Option<&str>) -> (PathBuf, DiscoveredMod) {
        let dir = std::env::temp_dir().join(format!("eu4h-lua-rt-{tag}-{id}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let harmony = dir.join("eu4-harmony");
        std::fs::create_dir_all(harmony.join("lua")).unwrap();
        std::fs::write(harmony.join("manifest.toml"), format!("id = \"{id}\"\napi_version = \"{api_version}\"")).unwrap();
        if let Some(code) = lua {
            std::fs::write(harmony.join("lua/main.lua"), code).unwrap();
        }
        let discovered = DiscoveredMod {
            descriptor_file: dir.join(format!("{id}.mod")),
            descriptor: crate::mods::Descriptor { name: id.to_string(), version: Some("1.0".into()), path: Some(dir.clone()), archive: None },
            dir: dir.clone(),
            manifest: eu4_harmony_manifest::HarmonyMod::read(&dir).unwrap().manifest,
            layout: eu4_harmony_manifest::Layout { extension: lua.map(|_| "lua/main.lua".into()), ui: Vec::new(), misplaced: Vec::new() },
        };
        (dir, discovered)
    }

    #[test]
    fn runtime_load_handles_playset_order_version_mismatch_and_unsupported_game() {
        let (dir_a, mod_a) = temp_mod("load", "alpha", eu4_harmony_remote::API_VERSION, Some("return {}"));
        let (dir_b, mod_b) = temp_mod("load", "beta", eu4_harmony_remote::API_VERSION, None);
        let (dir_c, mod_c) = temp_mod("load", "gamma", "99.0", Some("return {}"));
        let (dir_d, mod_d) = temp_mod("load", "delta", eu4_harmony_remote::API_VERSION, Some("return {"));
        let data = std::env::temp_dir().join(format!("eu4h-lua-rt-load-data-{}", std::process::id()));

        let runtime = ExtensionRuntime::new();
        let env = FakeEnv::new();
        let host = Arc::new(Quiet::default());
        let mods = [mod_a, mod_b, mod_c, mod_d];
        let outcomes = runtime.load(&env, &mods, host.clone(), &data);
        assert_eq!(outcomes.iter().map(|o| o.included).collect::<Vec<_>>(), [true, true, false, false]);
        assert_eq!(runtime.mod_ids(), ["alpha"]);
        assert_eq!(runtime.mod_id(0).as_deref(), Some("alpha"));
        assert_eq!(runtime.extension_indices(&mods).get(&0), Some(&0));
        assert_eq!(*env.declared.lock().unwrap(), ["alpha"]);
        let notices = env.notices.lock().unwrap();
        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0].0, "load:gamma");
        assert!(notices[0].1.contains("EU4HARMONY_API_MISMATCH"), "{}", notices[0].1);
        assert_eq!(notices[1].0, "load:delta");
        assert!(notices[1].1.contains("EU4HARMONY_NOT_LOADED"), "{}", notices[1].1);

        // Unsupported executable skips loading extensions without posting per-mod load notices.
        let unsupported = ExtensionRuntime::new();
        let mut unsupported_env = FakeEnv::new();
        unsupported_env.supported = false;
        let outcomes = unsupported.load(&unsupported_env, &mods[..2], host, &data);
        assert_eq!(outcomes.iter().map(|o| o.included).collect::<Vec<_>>(), [false, true]);
        assert!(unsupported_env.notices.lock().unwrap().is_empty());

        for d in [dir_a, dir_b, dir_c, dir_d, data] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_reload_carries_the_save_data_and_rolls_back_failed_or_changed_code() {
        let (dir, m) = temp_mod("reload", "reloaded", eu4_harmony_remote::API_VERSION, Some("return { save_data = function(game) return { count = 41 } end }"));
        let harmony = dir.join("eu4-harmony");
        let write = |file: &str, text: &str| std::fs::write(harmony.join(file), text).unwrap();
        let manifest = format!("id = \"reloaded\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION);
        let data = dir.join("data");

        let runtime = ExtensionRuntime::new();
        let env = FakeEnv::new();
        let host = Arc::new(Quiet::default());
        let outcomes = runtime.load(&env, &[m], host.clone(), &data);
        assert!(outcomes[0].included);
        let first = runtime.extensions()[0].clone();

        // Reload in session: carries save_data, runs on_session, updates report and saves.
        env.session_going_on.store(true, std::sync::atomic::Ordering::Relaxed);
        write("lua/main.lua", "return { on_session = function(game, saved) log('v2 got ' .. tostring(saved and saved.count)) end }");
        let r = runtime.reload(&env, "reloaded").unwrap();
        assert!(r.session && r.lost.is_none());
        let second = runtime.extensions()[0].clone();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(host.0.lock().unwrap().iter().any(|l| l == "v2 got 41"), "{:?}", host.0.lock().unwrap());
        assert_eq!(env.saves_reloaded.lock().unwrap().len(), 1);
        assert_eq!(env.reports.load(std::sync::atomic::Ordering::Relaxed), 1);

        // Reload out of session: replaces extension without running on_session.
        env.session_going_on.store(false, std::sync::atomic::Ordering::Relaxed);
        write("lua/main.lua", "return { on_session = function() log('should not run') end }");
        let r = runtime.reload(&env, "reloaded").unwrap();
        assert!(!r.session);
        let third = runtime.extensions()[0].clone();
        assert!(!Arc::ptr_eq(&second, &third));
        assert!(!host.0.lock().unwrap().iter().any(|l| l == "should not run"));

        // Compile failure restores localisation and keeps the live extension.
        write("lua/main.lua", "return {");
        let refused = runtime.reload(&env, "reloaded").unwrap_err();
        assert_eq!(refused.status, game::Status::Rejected);
        assert!(refused.message.contains("the new code did not load, and the old goes on"), "{}", refused.message);
        assert_eq!(env.loc_restores.lock().unwrap().len(), 1);
        assert!(Arc::ptr_eq(&runtime.extensions()[0], &third));

        // Changed manifest permissions or id are refused before touching localisation.
        let loc_before = env.loc_reloads.lock().unwrap().len();
        write("lua/main.lua", "return {}");
        write("manifest.toml", &format!("{manifest}\npermissions = {{ clock = true }}"));
        assert!(runtime.reload(&env, "reloaded").unwrap_err().message.contains("permissions changed"));
        write("manifest.toml", &format!("id = \"renamed\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION));
        assert!(runtime.reload(&env, "reloaded").unwrap_err().message.contains("changed id"));
        assert_eq!(env.loc_reloads.lock().unwrap().len(), loc_before);
        assert!(Arc::ptr_eq(&runtime.extensions()[0], &third));
        assert_eq!(runtime.reload(&env, "other").unwrap_err().status, game::Status::NotFound);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runtime_frame_session_end_and_save_collection_stay_isolated() {
        let code = r#"
            return {
              on_session = function(game, saved) log('session:' .. tostring(saved and saved.n)) end,
              on_day = function(game) log('day') end,
              on_session_ended = function(game) log('ended') end,
              save_data = function(game) return { n = 9 } end,
            }
        "#;
        let (dir, m) = temp_mod("frame", "framed", eu4_harmony_remote::API_VERSION, Some(code));
        let data = dir.join("data");
        let runtime = ExtensionRuntime::new();
        let env = FakeEnv::new();
        env.saved_in.lock().unwrap().insert("framed".into(), serde_json::json!({"n": 7}));
        let host = Arc::new(Quiet::default());
        assert!(runtime.load(&env, &[m], host.clone(), &data)[0].included);

        let begin = vec![game::Heard { signal: eu4_harmony_remote::signals::Signal::Session { session: 1 }, seen: true }];
        runtime.frame(&env, &begin);
        assert_eq!(env.begins.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(env.windows_frames.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(env.reports.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(runtime.state.lock().unwrap().session_clocks.is_some());
        assert!(host.0.lock().unwrap().iter().any(|l| l == "session:7"), "{:?}", host.0.lock().unwrap());

        // Report refresh throttles until 300s have elapsed.
        runtime.frame(&env, &[]);
        assert_eq!(env.reports.load(std::sync::atomic::Ordering::Relaxed), 1);
        *env.now.lock().unwrap() += Duration::from_secs(301);
        runtime.frame(&env, &[]);
        assert_eq!(env.reports.load(std::sync::atomic::Ordering::Relaxed), 2);

        // Save collection polls live extensions.
        assert_eq!(runtime.collect_save_data(), vec![("framed".into(), Some(serde_json::json!({"n": 9})))]);

        // Session end dispatches handler, notifies windows, and drains clocks.
        runtime.tell_ended(&env, crate::sessions::End::Return);
        assert_eq!(env.windows_ended.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(runtime.state.lock().unwrap().session_clocks.is_none());
        assert!(host.0.lock().unwrap().iter().any(|l| l == "ended"), "{:?}", host.0.lock().unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
