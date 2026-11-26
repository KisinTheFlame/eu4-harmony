//! The core's side of the Lua host (`eu4_harmony_lua`,
//! ADR extensions-are-lua-in-a-sandbox): loads every mod's extension at startup
//! on [`LuaHost`], the running game, and calls its handlers at safe points
//! (`crate::safe_point`).
//!
//! [`LuaHost`] reaches the capability catalogue as the extension through one
//! seam (`crate::capabilities::catalogue::Catalogue`), shared with remote
//! control's engine (ADR one-capability-catalogue); interface elements, the
//! map, texts, persistence, and notices go through a single [`Surface`] port.
//! Each extension's log is `eu4-harmony/logs/extension-<id>.log`, with bounded
//! older segments beside it.
mod http;

use crate::{
    base::mods::DiscoveredMod,
    base::notice::{Message, Repeat},
    capabilities::catalogue::{self, Catalogue, Place, Wait},
    capabilities::game::map,
    capabilities::game::{self, map_coloring, Caller},
    interface::ui,
    session::saves,
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
/// the unified [`LuaHost`], active session clocks, and startup-report throttling.
static RUNTIME: ExtensionRuntime<CoreSurface> = ExtensionRuntime::new();

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

struct RuntimeState<S: Surface> {
    host: Option<Arc<LuaHost<S>>>,
    loaded: Vec<Loaded>,
    session_clocks: Option<Clocks>,
    last_report: Option<Instant>,
}

/// Inputs collected once per safe frame from the game's UI and map state.
#[derive(Default)]
struct FrameInputs {
    map_lost: Vec<usize>,
    clicks: Vec<dispatch::Click>,
    windows: Vec<dispatch::WindowSeen>,
    popups_pressed: Vec<dispatch::PopupPressed>,
}

/// Unified host-environment seam between [`ExtensionRuntime`] / [`LuaHost`]
/// and the rest of the core. Production uses [`CoreSurface`]; tests supply an
/// in-process implementation without `thread_local!` globals or a second fake `Host`.
trait Surface: Send + Sync + 'static {
    type LocBackup;

    fn supported(&self) -> bool;
    fn with_catalogue(&self, caller: Caller, f: &mut dyn FnMut(&mut dyn Catalogue));
    fn session(&self) -> (u64, bool);

    fn saves_begin_session(&self) -> bool;
    fn saves_get(&self, id: &str) -> Option<serde_json::Value>;
    fn saves_reloaded(&self, id: &str, saved: Option<serde_json::Value>);

    fn mod_text(&self, id: &str, key: &str) -> Option<String>;
    fn language(&self) -> String;
    fn reload_loc(&self, id: &str, harmony: &Path) -> (Vec<String>, Self::LocBackup);
    fn restore_loc(&self, id: &str, earlier: Self::LocBackup);

    fn declare_windows(&self, extension: &Extension);
    fn set_element(&self, extension: usize, element: &str, value: Element) -> Result<(), String>;
    fn window(&self, extension: usize, change: WindowChange) -> Result<(), String>;
    fn tooltip(&self, extension: usize, window: Option<&str>, widget: &str) -> Result<eu4_harmony_lua::Text, String>;
    fn popup(&self, extension: usize, popup: Popup) -> Result<u32, String>;
    fn close_popup(&self, extension: usize, popup: u32) -> Result<(), String>;
    fn paint(&self, extension: usize, colors: &[(u32, [u8; 4])]) -> Result<(), Error>;
    fn release_map(&self, extension: usize) -> Result<(), Error>;
    /// Cleans up UI/window/map resources for `extension`: when `reloaded` is
    /// `Some(new)`, forgets old UI and declares `new`'s windows; when `None`
    /// (extension stopped), hides UI and forgets all windows.
    fn cleanup_extension(&self, extension: usize, reloaded: Option<&Extension>);

    fn take_frame_inputs(&self) -> FrameInputs;
    fn finish_frame(&self, listened: BTreeSet<eu4_harmony_remote::signals::Kind>);
    fn windows_session_ended(&self);

    fn post_notice(&self, key: &str, repeat: Repeat, message: Message);
    fn write_report(&self);
    fn clocks_now(&self) -> Clocks;
    fn now(&self) -> Instant;
}

struct CoreSurface;

impl Surface for CoreSurface {
    type LocBackup = crate::base::loc::Earlier;

    fn supported(&self) -> bool {
        crate::native::memory().is_some()
    }

    fn with_catalogue(&self, caller: Caller, f: &mut dyn FnMut(&mut dyn Catalogue)) {
        f(&mut catalogue::Live(caller));
    }

    fn session(&self) -> (u64, bool) {
        (crate::session::sessions::number(), crate::session::sessions::going_on())
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

    fn mod_text(&self, id: &str, key: &str) -> Option<String> {
        crate::base::loc::mod_text(id, key)
    }

    fn language(&self) -> String {
        crate::base::lang::player().id().to_string()
    }

    fn reload_loc(&self, id: &str, harmony: &Path) -> (Vec<String>, Self::LocBackup) {
        crate::base::loc::reload_mod(id, harmony)
    }

    fn restore_loc(&self, id: &str, earlier: Self::LocBackup) {
        crate::base::loc::restore_mod(id, earlier);
    }

    fn declare_windows(&self, extension: &Extension) {
        declare_windows(extension);
    }

    fn set_element(&self, extension: usize, element: &str, value: Element) -> Result<(), String> {
        ui::set_element(extension, element, value)
    }

    fn window(&self, extension: usize, change: WindowChange) -> Result<(), String> {
        crate::interface::windows::change(extension, change)
    }

    fn tooltip(&self, extension: usize, window: Option<&str>, widget: &str) -> Result<eu4_harmony_lua::Text, String> {
        match window {
            Some(window) => crate::interface::windows::tooltip(extension, window, widget),
            None => ui::tooltip(extension, widget),
        }
    }

    fn popup(&self, extension: usize, popup: Popup) -> Result<u32, String> {
        crate::interface::windows::popup(extension, popup)
    }

    fn close_popup(&self, extension: usize, popup: u32) -> Result<(), String> {
        crate::interface::windows::close_popup(extension, popup)
    }

    fn paint(&self, extension: usize, colors: &[(u32, [u8; 4])]) -> Result<(), Error> {
        let colors: Vec<_> = colors.iter().map(|&(province, rgba)| map_coloring::MapColor { province, rgba }).collect();
        map_coloring::paint(Caller::Extension(extension), &colors).map_err(Error::from)
    }

    fn release_map(&self, extension: usize) -> Result<(), Error> {
        map_coloring::clear(Caller::Extension(extension)).map_err(Error::from)
    }

    fn cleanup_extension(&self, extension: usize, reloaded: Option<&Extension>) {
        match reloaded {
            Some(new) => {
                ui::forget_extension(extension);
                declare_windows(new);
            }
            None => {
                ui::hide_extension(extension);
                crate::interface::windows::forget(extension);
            }
        }
        if let Some(m) = crate::native::memory() {
            map::release(m, extension);
        }
    }

    fn take_frame_inputs(&self) -> FrameInputs {
        let click = |c: ui::Click| dispatch::Click {
            extension: c.extension,
            button: c.name,
            right: c.click == ui::CLICK_RIGHT,
            shift: c.modifiers & ui::MODIFIER_SHIFT != 0,
            ctrl: c.modifiers & ui::MODIFIER_CTRL != 0,
            row: None,
        };
        let mut clicks: Vec<dispatch::Click> = ui::take_clicks().into_iter().map(click).collect();
        clicks.extend(crate::interface::windows::take_clicks());
        FrameInputs {
            map_lost: map::take_lost(),
            clicks,
            windows: crate::interface::windows::take_seen(),
            popups_pressed: crate::interface::windows::take_popups_pressed(),
        }
    }

    fn finish_frame(&self, listened: BTreeSet<eu4_harmony_remote::signals::Kind>) {
        game::signals::extensions_listen(listened);
        crate::interface::windows::frame();
    }

    fn windows_session_ended(&self) {
        crate::interface::windows::session_ended();
    }

    fn post_notice(&self, key: &str, repeat: Repeat, message: Message) {
        crate::base::notice::post(key, repeat, message);
    }

    fn write_report(&self) {
        crate::base::report::write();
    }

    fn clocks_now(&self) -> Clocks {
        Clocks::now()
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

struct ExtensionRuntime<S: Surface> {
    state: Mutex<RuntimeState<S>>,
}

impl<S: Surface> ExtensionRuntime<S> {
    const fn new() -> Self {
        Self { state: Mutex::new(RuntimeState { host: None, loaded: Vec::new(), session_clocks: None, last_report: None }) }
    }

    fn extensions(&self) -> Vec<Arc<Extension>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).loaded.iter().map(|l| l.extension.clone()).collect()
    }

    fn host(&self) -> Option<Arc<LuaHost<S>>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).host.clone()
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
        if let Some(host) = self.host() {
            host.maintain_logs(checkpoint);
        }
    }

    fn load(&self, surface: Arc<S>, mods: &[DiscoveredMod], log_dir: &Path, data_dir: &Path) -> Vec<LoadOutcome> {
        let host = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.host.get_or_insert_with(|| Arc::new(LuaHost::new(log_dir.to_path_buf(), surface))).clone()
        };
        let host_dyn: Arc<dyn Host> = host.clone();
        let supported = host.surface.supported();
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
                crate::base::log::event(format_args!(
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
                eu4_harmony_lua::open(loaded.len(), &opening, host_dyn.clone())
            };
            match result {
                Ok(extension) => {
                    log!("extension {:?} loaded", m.manifest.id);
                    host.surface.declare_windows(&extension);
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
                        host.surface.post_notice(&format!("load:{}", m.manifest.id), Repeat::Run, not_loaded(m, &e));
                    }
                }
            }
        }
        self.state.lock().unwrap_or_else(|e| e.into_inner()).loaded = loaded;
        outcomes
    }

    fn reload(&self, id: &str) -> game::GResult<Reloaded> {
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
        let host_dyn: Arc<dyn Host> = host.clone();
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
        let (problems, earlier) = host.surface.reload_loc(id, &harmony);
        let opening = Mod { id, name: &opened.name, harmony: harmony.clone(), permissions: &opened.permissions, data: Some(opened.data.clone()) };
        let new = match eu4_harmony_lua::open(index, &opening, host_dyn) {
            Ok(new) => new,
            Err(e) => {
                host.surface.restore_loc(id, earlier);
                return Err(rejected(format!("the new code did not load, and the old goes on: {e}")));
            }
        };
        if new.images() != old.images() {
            host.surface.restore_loc(id, earlier);
            return Err(rejected("entry images changed; their native resources take effect at the next start, and the old extension goes on".into()));
        }
        let session = host.surface.session().1;
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
        host.surface.saves_reloaded(id, outcome.saved.clone());
        if session {
            new.handle("on_session", &[Arg::Json(outcome.saved.unwrap_or(serde_json::Value::Null))]);
        }
        self.refresh_report(&host.surface, true);
        Ok(Reloaded { session, lost: outcome.lost })
    }

    fn frame(&self, heard: &[game::Heard]) {
        let Some(host) = self.host() else { return };
        let all = self.extensions();
        let heard: Vec<Heard> = heard.iter().map(|h| Heard::of(&h.signal)).collect();
        let mut feed = Fed::new(self, &host.surface);
        dispatch::frame(&all, &heard, &mut feed);
        let began = feed.began;
        host.surface.finish_frame(dispatch::listened(&all).into_iter().collect());
        self.refresh_report(&host.surface, began);
    }

    fn tell_ended(&self, end: crate::session::sessions::End) {
        let Some(host) = self.host() else { return };
        let all = self.extensions();
        dispatch::hear(&all, &Heard::of(&end.signal()), &mut Fed::new(self, &host.surface));
        host.surface.windows_session_ended();
        let Some(began) = self.state.lock().unwrap_or_else(|e| e.into_inner()).session_clocks.take() else { return };
        let now = host.surface.clocks_now();
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

    fn refresh_report(&self, surface: &S, now: bool) {
        let current = surface.now();
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
            surface.write_report();
        }
    }

    fn timing_lines(&self) -> Vec<String> {
        self.extensions().iter().map(|e| e.timing()).collect()
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

// ---------- the unified Lua host ----------

/// The running game, as extensions reach it through [`Surface`].
struct LuaHost<S: Surface> {
    /// The logs folder, and each extension's log once it is open.
    logs: PathBuf,
    open: Mutex<HashMap<String, ExtensionLog>>,
    surface: Arc<S>,
}

struct ExtensionLog {
    file: Option<eu4_harmony_logging::Journal>,
    retry: Instant,
    unconfirmed: u64,
}

impl<S: Surface> LuaHost<S> {
    fn new(logs: PathBuf, surface: Arc<S>) -> Self {
        Self { logs, open: Mutex::new(HashMap::new()), surface }
    }

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

/// A capability's answer still to come, polled at a game's safe frame.
struct Later<S: Surface> {
    surface: Arc<S>,
    caller: Caller,
    wait: Box<dyn Wait>,
}

impl<S: Surface> Pending<Answered> for Later<S> {
    fn poll(&mut self) -> Option<Answered> {
        let mut answer = None;
        let now = self.surface.now();
        self.surface.with_catalogue(self.caller, &mut |c| c.at(None, now, &Place::Game, &mut |at| answer = self.wait.poll(at)));
        answer.map(lua_form)
    }
}

/// A response as the Lua host takes it: the answer written in Lua's form.
fn lua_form(response: Response) -> Answered {
    let answer = response?;
    eu4_harmony_checks::form::as_lua(|| serde_json::to_value(&answer)).map_err(|e| Error::new(Code::Internal, format!("the answer cannot be written: {e}")))
}

impl<S: Surface> Host for LuaHost<S> {
    fn run(&self, extension: usize, request: Request) -> Called {
        let caller = Caller::Extension(extension);
        let now = self.surface.now();
        let (mut request, mut called) = (Some(request), None);
        self.surface.with_catalogue(caller, &mut |c| {
            let Some(request) = request.take() else { return };
            // Extensions give no input, which only remote clients have.
            called = Some(match c.run(request, game::As::Game, None, now) {
                catalogue::Outcome::Done(response) | catalogue::Outcome::Held(response, _) => Called::Done(lua_form(response)),
                catalogue::Outcome::Later(wait) => Called::Later(Box::new(Later { surface: self.surface.clone(), caller, wait })),
            });
        });
        called.unwrap_or_else(|| Called::Done(Err(Error::new(Code::Internal, "the capability catalogue was not reached"))))
    }

    fn set_element(&self, extension: usize, element: &str, value: Element) -> Result<(), String> {
        self.surface.set_element(extension, element, value)
    }

    fn window(&self, extension: usize, change: WindowChange) -> Result<(), String> {
        self.surface.window(extension, change)
    }

    fn tooltip(&self, extension: usize, window: Option<&str>, widget: &str) -> Result<eu4_harmony_lua::Text, String> {
        self.surface.tooltip(extension, window, widget)
    }

    fn popup(&self, extension: usize, popup: Popup) -> Result<u32, String> {
        self.surface.popup(extension, popup)
    }

    fn close_popup(&self, extension: usize, popup: u32) -> Result<(), String> {
        self.surface.close_popup(extension, popup)
    }

    fn paint(&self, extension: usize, colors: &[(u32, [u8; 4])]) -> Result<(), Error> {
        self.surface.paint(extension, colors)
    }

    fn release_map(&self, extension: usize) -> Result<(), Error> {
        self.surface.release_map(extension)
    }

    fn text(&self, id: &str, key: &str) -> Option<String> {
        self.surface.mod_text(id, key)
    }

    fn language(&self) -> String {
        self.surface.language()
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
        self.surface.session().0
    }

    fn first_error(&self, extension: &Extension, from: &str, message: &str) {
        // The player is told in a game the framework runs in.
        if self.surface.supported() {
            log!("extension {:?}: error in {from}: {message}", extension.id());
            self.surface.post_notice(&format!("errors:{}", extension.id()), Repeat::Session, Message::new("EU4HARMONY_ERRORS").arg("MOD", extension.name()));
        }
    }

    fn stopped(&self, extension: &Extension, reason: &str) {
        log!("extension {:?} stopped: {reason}", extension.id());
        self.surface.cleanup_extension(extension.index(), None);
        if self.surface.supported() {
            self.surface.post_notice(
                &format!("stopped:{}", extension.id()),
                Repeat::Run,
                Message::new("EU4HARMONY_STOPPED").arg("MOD", extension.name()).arg("DETAIL", reason),
            );
        }
    }

    fn reload_reset(&self, extension: usize, new: &Extension) {
        self.surface.cleanup_extension(extension, Some(new));
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
    RUNTIME.load(Arc::new(CoreSurface), mods, log_dir, data_dir)
}

/// The windows an extension's main.lua returns, for the framework to build.
fn declare_windows(extension: &Extension) {
    crate::interface::windows::declare(extension.index(), extension.id(), extension.windows(), extension.images());
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
    if crate::base::thread_id() != crate::base::main_thread() {
        return Err(game::Fail { status: game::Status::WrongContext, message: "called off the game's main thread".into() });
    }
    RUNTIME.reload(&id)
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

/// Within an extension's own call, no extension is called again: one would
/// run inside itself, its waiting tasks dropped under the task running.
pub(crate) fn can_hear_end() -> bool {
    !eu4_harmony_lua::calling()
}

/// Every extension hears it, on the main thread, before anything of the
/// session is taken down: once leaving is certain, after the ironman exit
/// save, when there is one, was written. Heard late, after the extension's
/// call it ended in, the session's state is gone by then, as what ended
/// it ran inside that call.
pub(crate) fn session_ended(end: crate::session::sessions::End) {
    RUNTIME.tell_ended(end);
}

pub(crate) fn safe_frame(heard: &[game::Heard]) {
    RUNTIME.frame(heard);
}

/// What the running game gives a frame: the extension save data of the save
/// a session began from, the maps taken over and the buttons clicked.
struct Fed<'a, S: Surface> {
    runtime: &'a ExtensionRuntime<S>,
    surface: &'a S,
    inputs: Option<FrameInputs>,
    /// A session began in this frame.
    began: bool,
}

impl<'a, S: Surface> Fed<'a, S> {
    fn new(runtime: &'a ExtensionRuntime<S>, surface: &'a S) -> Self {
        Self { runtime, surface, inputs: None, began: false }
    }

    fn inputs(&mut self) -> &mut FrameInputs {
        self.inputs.get_or_insert_with(|| self.surface.take_frame_inputs())
    }
}

impl<S: Surface> dispatch::Feed for Fed<'_, S> {
    fn session_begins(&mut self, session: u64) {
        let from_save = self.surface.saves_begin_session();
        log!("session {session} started (from a loaded save: {from_save})");
        self.runtime.state.lock().unwrap_or_else(|e| e.into_inner()).session_clocks = Some(self.surface.clocks_now());
        self.began = true;
    }

    fn saved(&mut self, id: &str) -> Option<serde_json::Value> {
        self.surface.saves_get(id)
    }

    fn map_lost(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.inputs().map_lost)
    }

    fn clicks(&mut self) -> Vec<dispatch::Click> {
        std::mem::take(&mut self.inputs().clicks)
    }

    fn windows(&mut self) -> Vec<dispatch::WindowSeen> {
        std::mem::take(&mut self.inputs().windows)
    }

    fn popups_pressed(&mut self) -> Vec<dispatch::PopupPressed> {
        std::mem::take(&mut self.inputs().popups_pressed)
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
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

    #[derive(Default)]
    struct ScriptedCatalogue {
        calls: Vec<String>,
        /// Optional pending wait answered after `polls_until_ready` polls.
        polls_until_ready: usize,
    }

    struct CountdownWait {
        remaining: usize,
        date: eu4_harmony_remote::Date,
    }

    impl Wait for CountdownWait {
        fn poll(&mut self, _: &mut catalogue::Poll<'_>) -> Option<Response> {
            if self.remaining > 0 {
                self.remaining -= 1;
                return None;
            }
            Some(Ok(eu4_harmony_remote::Answer::Time(eu4_harmony_remote::time::TimeAnswer::Wait(self.date))))
        }
    }

    impl Catalogue for ScriptedCatalogue {
        fn run(&mut self, request: Request, _: game::As, _: Option<&catalogue::signals::Buffer>, _: Instant) -> catalogue::Outcome {
            use eu4_harmony_remote::{
                armies::{Army, ArmyAnswer, ArmyKind, ArmyRequest, Units},
                session::{Session, SessionAnswer, SessionRequest},
                time::TimeRequest,
                Answer, ArmyId, CountryTag, Date, ProvinceId,
            };
            match request {
                Request::Time(TimeRequest::Wait { days }) => {
                    self.calls.push(format!("wait {days}"));
                    catalogue::Outcome::Later(Box::new(CountdownWait { remaining: self.polls_until_ready, date: Date(0x035D_7710 + days as i32 * 24) }))
                }
                Request::Army(ArmyRequest::Get { army: ArmyId(id) }) => {
                    self.calls.push(format!("army {id}"));
                    let response = if id == 4_294_967_377 {
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
                    };
                    catalogue::Outcome::Done(response)
                }
                Request::Session(SessionRequest::Get {}) => {
                    self.calls.push("session".into());
                    catalogue::Outcome::Done(Ok(Answer::Session(SessionAnswer::Get(Session {
                        number: 2,
                        player: Some(CountryTag(*b"TUR")),
                        observing: false,
                        date: Date(0x035D_7710),
                    }))))
                }
                other => catalogue::Outcome::Done(Err(Error::new(Code::Internal, format!("{} is not scripted in this test", other.name())))),
            }
        }

        fn at(&mut self, signals: Option<&catalogue::signals::Buffer>, now: Instant, place: &Place, polls: &mut dyn FnMut(&mut catalogue::Poll<'_>)) {
            polls(&mut catalogue::Poll::new(signals, now, place, None));
        }
    }

    /// Unified in-process test [`Surface`]: owns its scripted catalogue,
    /// persistence, localisation, UI/map cleanup trace, notices, and clock
    /// with zero `thread_local!` state.
    struct FakeSurface {
        supported: bool,
        session_number: AtomicU64,
        session_going_on: AtomicBool,
        now: Mutex<Instant>,
        catalogue: Mutex<ScriptedCatalogue>,
        saved_in: Mutex<HashMap<String, serde_json::Value>>,
        declared: Mutex<Vec<String>>,
        cleanups: Mutex<Vec<(usize, Option<String>)>>,
        notices: Mutex<Vec<(String, String)>>,
        loc_reloads: Mutex<Vec<String>>,
        loc_restores: Mutex<Vec<(String, u32)>>,
        saves_reloaded: Mutex<Vec<(String, Option<serde_json::Value>)>>,
        listened: Mutex<Vec<BTreeSet<eu4_harmony_remote::signals::Kind>>>,
        windows_frames: AtomicUsize,
        windows_ended: AtomicUsize,
        reports: AtomicUsize,
        begins: AtomicUsize,
    }

    impl FakeSurface {
        fn new() -> Self {
            Self {
                supported: true,
                session_number: AtomicU64::new(1),
                session_going_on: AtomicBool::new(false),
                now: Mutex::new(Instant::now()),
                catalogue: Mutex::new(ScriptedCatalogue::default()),
                saved_in: Mutex::new(HashMap::new()),
                declared: Mutex::new(Vec::new()),
                cleanups: Mutex::new(Vec::new()),
                notices: Mutex::new(Vec::new()),
                loc_reloads: Mutex::new(Vec::new()),
                loc_restores: Mutex::new(Vec::new()),
                saves_reloaded: Mutex::new(Vec::new()),
                listened: Mutex::new(Vec::new()),
                windows_frames: AtomicUsize::new(0),
                windows_ended: AtomicUsize::new(0),
                reports: AtomicUsize::new(0),
                begins: AtomicUsize::new(0),
            }
        }
    }

    impl Surface for FakeSurface {
        type LocBackup = u32;

        fn supported(&self) -> bool {
            self.supported
        }

        fn with_catalogue(&self, _: Caller, f: &mut dyn FnMut(&mut dyn Catalogue)) {
            let mut cat = self.catalogue.lock().unwrap();
            f(&mut *cat);
        }

        fn session(&self) -> (u64, bool) {
            (self.session_number.load(Ordering::Relaxed), self.session_going_on.load(Ordering::Relaxed))
        }

        fn saves_begin_session(&self) -> bool {
            self.begins.fetch_add(1, Ordering::Relaxed);
            !self.saved_in.lock().unwrap().is_empty()
        }

        fn saves_get(&self, id: &str) -> Option<serde_json::Value> {
            self.saved_in.lock().unwrap().get(id).cloned()
        }

        fn saves_reloaded(&self, id: &str, saved: Option<serde_json::Value>) {
            self.saves_reloaded.lock().unwrap().push((id.to_string(), saved));
        }

        fn mod_text(&self, _: &str, _: &str) -> Option<String> {
            None
        }

        fn language(&self) -> String {
            "english".into()
        }

        fn reload_loc(&self, id: &str, _: &Path) -> (Vec<String>, Self::LocBackup) {
            let mut reloads = self.loc_reloads.lock().unwrap();
            reloads.push(id.to_string());
            (Vec::new(), reloads.len() as u32)
        }

        fn restore_loc(&self, id: &str, earlier: Self::LocBackup) {
            self.loc_restores.lock().unwrap().push((id.to_string(), earlier));
        }

        fn declare_windows(&self, extension: &Extension) {
            self.declared.lock().unwrap().push(extension.id().to_string());
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

        fn cleanup_extension(&self, extension: usize, reloaded: Option<&Extension>) {
            self.cleanups.lock().unwrap().push((extension, reloaded.map(|e| e.id().to_string())));
        }

        fn take_frame_inputs(&self) -> FrameInputs {
            FrameInputs::default()
        }

        fn finish_frame(&self, listened: BTreeSet<eu4_harmony_remote::signals::Kind>) {
            self.listened.lock().unwrap().push(listened);
            self.windows_frames.fetch_add(1, Ordering::Relaxed);
        }

        fn windows_session_ended(&self) {
            self.windows_ended.fetch_add(1, Ordering::Relaxed);
        }

        fn post_notice(&self, key: &str, _: Repeat, message: Message) {
            self.notices.lock().unwrap().push((key.to_string(), format!("{message:?}")));
        }

        fn write_report(&self) {
            self.reports.fetch_add(1, Ordering::Relaxed);
        }

        fn clocks_now(&self) -> Clocks {
            Clocks { wall: *self.now.lock().unwrap(), main: Duration::ZERO, game: Duration::ZERO }
        }

        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }
    }

    fn read_logged_messages(log_dir: &Path, id: &str) -> Vec<String> {
        let source = eu4_harmony_logging::extension_source(id);
        let Ok(logged) = std::fs::read_to_string(log_file(log_dir, id)) else {
            return Vec::new();
        };
        logged
            .lines()
            .filter_map(|line| eu4_harmony_logging::record_message(line, &source))
            .filter(|line| !line.starts_with("slow stretch:"))
            .map(str::to_string)
            .collect()
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
            descriptor: crate::base::mods::Descriptor { name: id.to_string(), version: Some("1.0".into()), path: Some(dir.clone()), archive: None },
            dir: dir.clone(),
            manifest: eu4_harmony_manifest::HarmonyMod::read(&dir).unwrap().manifest,
            layout: eu4_harmony_manifest::Layout { extension: lua.map(|_| "lua/main.lua".into()), ui: Vec::new(), misplaced: Vec::new() },
        };
        (dir, discovered)
    }

    #[test]
    fn an_extension_s_call_reaches_the_catalogue_and_answers_in_lua_s_form() {
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
        let (dir, m) = temp_mod("cat", "calling", eu4_harmony_remote::API_VERSION, Some(main));
        let logs = dir.join("logs");
        let data = dir.join("data");
        let surface = Arc::new(FakeSurface::new());
        let runtime = ExtensionRuntime::new();
        assert!(runtime.load(surface.clone(), &[m], &logs, &data)[0].included);
        runtime.frame(&[]);
        runtime.maintain_logs(true);
        assert_eq!(read_logged_messages(&logs, "calling"), ["integer 4294967377 1444.11.11 nil not_found"]);
        assert_eq!(surface.catalogue.lock().unwrap().calls, ["army 4294967377", "session", "army 1"]);
        drop(runtime);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn later_polls_through_unified_host_across_safe_frames() {
        let main = r#"
            local asked = false
            return {
              on_frame = function(game)
                if not asked then
                  asked = true
                  spawn(function()
                    local date = game.time.wait(2)
                    log("waited:" .. date.year .. "." .. date.month .. "." .. date.day)
                  end)
                end
              end,
            }
        "#;
        let (dir, m) = temp_mod("later", "waiter", eu4_harmony_remote::API_VERSION, Some(main));
        let logs = dir.join("logs");
        let data = dir.join("data");
        let surface = Arc::new(FakeSurface::new());
        surface.catalogue.lock().unwrap().polls_until_ready = 1;
        let runtime = ExtensionRuntime::new();
        assert!(runtime.load(surface.clone(), &[m], &logs, &data)[0].included);

        // Frame 1 spawns the coroutine and yields on `game.time.wait(2)`.
        runtime.frame(&[]);
        runtime.maintain_logs(true);
        assert!(read_logged_messages(&logs, "waiter").is_empty());

        // Frame 2 polls `Later` for the first time; `CountdownWait` still returns `None`.
        runtime.frame(&[]);
        runtime.maintain_logs(true);
        assert!(read_logged_messages(&logs, "waiter").is_empty());

        // Frame 3 polls `Later` again; `CountdownWait` resolves and resumes the coroutine.
        runtime.frame(&[]);
        runtime.maintain_logs(true);
        assert_eq!(read_logged_messages(&logs, "waiter"), ["waited:1444.11.13"]);
        assert_eq!(surface.catalogue.lock().unwrap().calls, ["wait 2"]);

        drop(runtime);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handler_errors_write_three_times_in_full_and_summarize_remaining_counts_once() {
        let (dir, m) = temp_mod("err", "failing", eu4_harmony_remote::API_VERSION, Some("return { on_day = function(game) error('boom') end }"));
        let logs = dir.join("logs");
        let data = dir.join("data");
        let surface = Arc::new(FakeSurface::new());
        let runtime = ExtensionRuntime::new();
        assert!(runtime.load(surface.clone(), &[m], &logs, &data)[0].included);

        let day = [game::Heard { signal: eu4_harmony_remote::signals::Signal::Day { date: eu4_harmony_remote::Date(0x035D_7710) }, seen: true }];
        for _ in 0..6 {
            runtime.frame(&day);
        }
        runtime.tell_ended(crate::session::sessions::End::Return);
        runtime.maintain_logs(true);

        let source = eu4_harmony_logging::extension_source("failing");
        let messages = eu4_harmony_logging::SourceMark::default().messages_since(&logs, &source);
        let full = messages.iter().filter(|m| m.starts_with("error in on_day:") && m.contains("boom")).count();
        let summaries: Vec<&String> = messages.iter().filter(|m| m.starts_with("repeated error: count=3 ") && m.contains("error in on_day:")).collect();
        let duplicate_session_lines = messages.iter().filter(|m| m.contains("times this session:")).count();
        assert_eq!(full, 3, "{messages:?}");
        assert_eq!(summaries.len(), 1, "{messages:?}");
        assert_eq!(duplicate_session_lines, 0, "{messages:?}");
        // First error notified the player once.
        assert!(surface.notices.lock().unwrap().iter().any(|(k, _)| k == "errors:failing"));

        drop(runtime);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn host_stopped_cleans_up_extension_and_posts_stopped_notice() {
        let (dir, m) = temp_mod("stop", "stopper", eu4_harmony_remote::API_VERSION, Some("return {}"));
        let logs = dir.join("logs");
        let data = dir.join("data");
        let surface = Arc::new(FakeSurface::new());
        let runtime = ExtensionRuntime::new();
        assert!(runtime.load(surface.clone(), &[m], &logs, &data)[0].included);
        let host = runtime.host().unwrap();
        let ext = runtime.extensions()[0].clone();

        host.stopped(&ext, "exceeded instruction budget");
        assert_eq!(*surface.cleanups.lock().unwrap(), [(0, None)]);
        let notices = surface.notices.lock().unwrap();
        assert!(notices.iter().any(|(k, msg)| k == "stopped:stopper" && msg.contains("EU4HARMONY_STOPPED")), "{notices:?}");

        drop(runtime);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unavailable_extension_log_backs_off_and_reports_unconfirmed_records_on_recovery() {
        let dir = std::env::temp_dir().join(format!("eu4h-extension-log-recovery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let logs = dir.join("logs");
        std::fs::write(&logs, "not a folder").unwrap();
        let host = LuaHost::new(logs.clone(), Arc::new(FakeSurface::new()));
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

    #[test]
    fn runtime_load_handles_playset_order_version_mismatch_and_unsupported_game() {
        let (dir_a, mod_a) = temp_mod("load", "alpha", eu4_harmony_remote::API_VERSION, Some("return {}"));
        let (dir_b, mod_b) = temp_mod("load", "beta", eu4_harmony_remote::API_VERSION, None);
        let (dir_c, mod_c) = temp_mod("load", "gamma", "99.0", Some("return {}"));
        let (dir_d, mod_d) = temp_mod("load", "delta", eu4_harmony_remote::API_VERSION, Some("return {"));
        let logs = std::env::temp_dir().join(format!("eu4h-lua-rt-load-logs-{}", std::process::id()));
        let data = std::env::temp_dir().join(format!("eu4h-lua-rt-load-data-{}", std::process::id()));

        let runtime = ExtensionRuntime::new();
        let surface = Arc::new(FakeSurface::new());
        let mods = [mod_a, mod_b, mod_c, mod_d];
        let outcomes = runtime.load(surface.clone(), &mods, &logs, &data);
        assert_eq!(outcomes.iter().map(|o| o.included).collect::<Vec<_>>(), [true, true, false, false]);
        assert_eq!(runtime.mod_ids(), ["alpha"]);
        assert_eq!(runtime.mod_id(0).as_deref(), Some("alpha"));
        assert_eq!(runtime.extension_indices(&mods).get(&0), Some(&0));
        assert_eq!(*surface.declared.lock().unwrap(), ["alpha"]);
        let notices = surface.notices.lock().unwrap();
        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0].0, "load:gamma");
        assert!(notices[0].1.contains("EU4HARMONY_API_MISMATCH"), "{}", notices[0].1);
        assert_eq!(notices[1].0, "load:delta");
        assert!(notices[1].1.contains("EU4HARMONY_NOT_LOADED"), "{}", notices[1].1);

        // Unsupported executable skips loading extensions without posting per-mod load notices.
        let unsupported = ExtensionRuntime::new();
        let mut unsupported_surface = FakeSurface::new();
        unsupported_surface.supported = false;
        let unsupported_surface = Arc::new(unsupported_surface);
        let outcomes = unsupported.load(unsupported_surface.clone(), &mods[..2], &logs, &data);
        assert_eq!(outcomes.iter().map(|o| o.included).collect::<Vec<_>>(), [false, true]);
        assert!(unsupported_surface.notices.lock().unwrap().is_empty());

        drop(runtime);
        drop(unsupported);
        for d in [dir_a, dir_b, dir_c, dir_d, logs, data] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_reload_carries_the_save_data_runs_reload_reset_and_rolls_back_failed_or_changed_code() {
        let (dir, m) = temp_mod("reload", "reloaded", eu4_harmony_remote::API_VERSION, Some("return { save_data = function(game) return { count = 41 } end }"));
        let harmony = dir.join("eu4-harmony");
        let write = |file: &str, text: &str| std::fs::write(harmony.join(file), text).unwrap();
        let manifest = format!("id = \"reloaded\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION);
        let logs = dir.join("logs");
        let data = dir.join("data");

        let runtime = ExtensionRuntime::new();
        let surface = Arc::new(FakeSurface::new());
        let outcomes = runtime.load(surface.clone(), &[m], &logs, &data);
        assert!(outcomes[0].included);
        let first = runtime.extensions()[0].clone();

        // Reload in session: carries save_data, triggers reload_reset cleanup, runs on_session, updates report and saves.
        surface.session_going_on.store(true, Ordering::Relaxed);
        write("lua/main.lua", "return { on_session = function(game, saved) log('v2 got ' .. tostring(saved and saved.count)) end }");
        let r = runtime.reload("reloaded").unwrap();
        assert!(r.session && r.lost.is_none());
        let second = runtime.extensions()[0].clone();
        assert!(!Arc::ptr_eq(&first, &second));
        runtime.maintain_logs(true);
        assert!(read_logged_messages(&logs, "reloaded").iter().any(|l| l == "v2 got 41"));
        assert_eq!(*surface.cleanups.lock().unwrap(), [(0, Some("reloaded".to_string()))]);
        assert_eq!(surface.saves_reloaded.lock().unwrap().len(), 1);
        assert_eq!(surface.reports.load(Ordering::Relaxed), 1);

        // Reload out of session: replaces extension without running on_session.
        surface.session_going_on.store(false, Ordering::Relaxed);
        write("lua/main.lua", "return { on_session = function() log('should not run') end }");
        let r = runtime.reload("reloaded").unwrap();
        assert!(!r.session);
        let third = runtime.extensions()[0].clone();
        assert!(!Arc::ptr_eq(&second, &third));
        runtime.maintain_logs(true);
        assert!(!read_logged_messages(&logs, "reloaded").iter().any(|l| l == "should not run"));

        // Compile failure restores localisation and keeps the live extension.
        write("lua/main.lua", "return {");
        let refused = runtime.reload("reloaded").unwrap_err();
        assert_eq!(refused.status, game::Status::Rejected);
        assert!(refused.message.contains("the new code did not load, and the old goes on"), "{}", refused.message);
        assert_eq!(surface.loc_restores.lock().unwrap().len(), 1);
        assert!(Arc::ptr_eq(&runtime.extensions()[0], &third));

        // Changed manifest permissions or id are refused before touching localisation.
        let loc_before = surface.loc_reloads.lock().unwrap().len();
        write("lua/main.lua", "return {}");
        write("manifest.toml", &format!("{manifest}\npermissions = {{ clock = true }}"));
        assert!(runtime.reload("reloaded").unwrap_err().message.contains("permissions changed"));
        write("manifest.toml", &format!("id = \"renamed\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION));
        assert!(runtime.reload("reloaded").unwrap_err().message.contains("changed id"));
        assert_eq!(surface.loc_reloads.lock().unwrap().len(), loc_before);
        assert!(Arc::ptr_eq(&runtime.extensions()[0], &third));
        assert_eq!(runtime.reload("other").unwrap_err().status, game::Status::NotFound);

        drop(runtime);
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
        let logs = dir.join("logs");
        let data = dir.join("data");
        let runtime = ExtensionRuntime::new();
        let surface = Arc::new(FakeSurface::new());
        surface.saved_in.lock().unwrap().insert("framed".into(), serde_json::json!({"n": 7}));
        assert!(runtime.load(surface.clone(), &[m], &logs, &data)[0].included);

        let begin = vec![game::Heard { signal: eu4_harmony_remote::signals::Signal::Session { session: 1 }, seen: true }];
        runtime.frame(&begin);
        assert_eq!(surface.begins.load(Ordering::Relaxed), 1);
        assert_eq!(surface.windows_frames.load(Ordering::Relaxed), 1);
        assert_eq!(surface.reports.load(Ordering::Relaxed), 1);
        runtime.maintain_logs(true);
        assert!(read_logged_messages(&logs, "framed").iter().any(|l| l == "session:7"));

        // Report refresh throttles until 300s have elapsed.
        runtime.frame(&[]);
        assert_eq!(surface.reports.load(Ordering::Relaxed), 1);
        *surface.now.lock().unwrap() += Duration::from_secs(301);
        runtime.frame(&[]);
        assert_eq!(surface.reports.load(Ordering::Relaxed), 2);

        // Save collection polls live extensions.
        assert_eq!(runtime.collect_save_data(), vec![("framed".into(), Some(serde_json::json!({"n": 9})))]);

        // Session end dispatches handler and notifies windows.
        runtime.tell_ended(crate::session::sessions::End::Return);
        assert_eq!(surface.windows_ended.load(Ordering::Relaxed), 1);
        runtime.maintain_logs(true);
        assert!(read_logged_messages(&logs, "framed").iter().any(|l| l == "ended"));

        drop(runtime);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
