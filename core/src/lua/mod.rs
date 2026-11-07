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
//! `eu4-harmony/logs/<id>.log`, the last start's kept beside it.
mod http;

use crate::{
    catalogue::{self, Catalogue, Place, Wait},
    game::{self, interface, map_coloring, Caller},
    map,
    mods::DiscoveredMod,
    notice::Message,
    saves, ui,
};
use eu4_harmony_lua::{fetch, Answered, Arg, Called, Element, Extension, Host, Mod, Pending};
use eu4_harmony_remote::{events::Event, Code, Error, Request, Response};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, RwLock},
    time::{Duration, Instant},
};

/// The extensions loaded, by number; an extension reload replaces one in its
/// place.
static EXTENSIONS: RwLock<Vec<Arc<Extension>>> = RwLock::new(Vec::new());

/// What each extension was opened from, by number, and the host they share,
/// for an extension reload to open it again.
static OPENED: Mutex<Vec<Opened>> = Mutex::new(Vec::new());
static HOST: OnceLock<Arc<dyn Host>> = OnceLock::new();

struct Opened {
    id: String,
    name: String,
    dir: PathBuf,
    permissions: eu4_harmony_manifest::Permissions,
    data: PathBuf,
}

fn extensions() -> Vec<Arc<Extension>> {
    EXTENSIONS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The id of the mod whose extension has this index.
pub fn mod_id(index: usize) -> Option<String> {
    extensions().get(index).map(|e| e.id().to_string())
}

/// The ids of the mods whose extensions loaded.
pub fn mod_ids() -> Vec<String> {
    extensions().iter().map(|e| e.id().to_string()).collect()
}

/// Maps discovery-order mod indices to extension indices.
pub fn extension_indices(mods: &[DiscoveredMod]) -> BTreeMap<usize, usize> {
    let list = extensions();
    mods.iter().enumerate().filter_map(|(i, m)| list.iter().find(|e| e.id() == m.manifest.id).map(|e| (i, e.index()))).collect()
}

// ---------- the live host ----------

/// The running game, as extensions reach it.
struct Live {
    /// The logs folder, and each extension's log once it is open.
    logs: PathBuf,
    open: Mutex<HashMap<String, File>>,
    reach: Reach,
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

impl Host for Live {
    fn run(&self, extension: usize, request: Request) -> Called {
        let caller = Caller::Extension(extension);
        let (mut request, mut called) = (Some(request), None);
        (self.reach)(caller, &mut |c| {
            let Some(request) = request.take() else { return };
            // Extensions give no input, which only remote clients have.
            called = Some(match c.run(request, None, Instant::now()) {
                catalogue::Outcome::Done(response) | catalogue::Outcome::Input(response, _) => Called::Done(lua_form(response)),
                catalogue::Outcome::Later(wait) => Called::Later(Box::new(Later(self.reach, caller, wait))),
            })
        });
        called.unwrap_or_else(|| Called::Done(Err(Error::new(Code::Internal, "the capability catalogue was not reached"))))
    }

    fn set_element(&self, extension: usize, element: &str, value: Element) -> Result<(), String> {
        let caller = Caller::Extension(extension);
        let result = match value {
            Element::Visible(on) => interface::set_visible(caller, element, on),
            Element::Enabled(on) => interface::set_enabled(caller, element, on),
            Element::Text(text) => interface::set_text(caller, element, &text),
            Element::Tooltip(text) => interface::set_tooltip(caller, element, &text),
        };
        result.map_err(|e| e.message)
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
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if !open.contains_key(id) {
            if let Ok(file) = OpenOptions::new().create(true).append(true).open(log_file(&self.logs, id)) {
                open.insert(id.to_string(), file);
            }
        }
        if let Some(file) = open.get_mut(id) {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    }

    fn session(&self) -> u64 {
        crate::sessions::number()
    }

    fn first_error(&self, extension: &Extension, from: &str, message: &str) {
        // The player is told in a game the framework runs in.
        if crate::memory().is_some() {
            let session = crate::sessions::number();
            log!("extension {:?}: error in {from}: {message}", extension.id());
            crate::notice::post(&format!("errors:{}:{session}", extension.id()), Message::new("EU4HARMONY_ERRORS").arg("MOD", extension.name()));
        }
    }

    fn stopped(&self, extension: &Extension, reason: &str) {
        log!("extension {:?} stopped: {reason}", extension.id());
        ui::hide_extension(extension.index());
        if let Some(m) = crate::memory() {
            map::release(m, extension.index());
            crate::notice::post(&format!("stopped:{}", extension.id()), Message::new("EU4HARMONY_STOPPED").arg("MOD", extension.name()).arg("DETAIL", reason));
        }
    }
}

/// The log of the mod `id`'s extension in the logs folder `dir`.
fn log_file(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.log"))
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
    let host: Arc<dyn Host> = HOST.get_or_init(|| Arc::new(Live { logs: log_dir.to_path_buf(), open: Mutex::new(HashMap::new()), reach: layer })).clone();
    let mut opened = Vec::new();
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
            // The last start's log is kept beside it; this start's begins
            // before main.lua runs, which may write to it already.
            let path = log_file(log_dir, &m.manifest.id);
            if path.exists() {
                let _ = std::fs::rename(&path, path.with_extension("prev.log"));
            }
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
                opened.push(Opened {
                    id: m.manifest.id.clone(),
                    name: m.descriptor.name.clone(),
                    dir: m.dir.clone(),
                    permissions: m.manifest.permissions.clone(),
                    data: data_dir.join(&m.manifest.id),
                });
                outcomes.push(LoadOutcome { included: true, status: format!("extension {entry} loaded") });
                loaded.push(extension);
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
    *EXTENSIONS.write().unwrap_or_else(|e| e.into_inner()) = loaded;
    *OPENED.lock().unwrap_or_else(|e| e.into_inner()) = opened;
    outcomes
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
/// (docs/extension-reload.md): the new code is opened first, and only when
/// it loads is the old one asked for its save data and replaced. On the
/// main thread, between frames.
pub fn reload(_: Caller, id: String) -> game::GResult<Reloaded> {
    if crate::thread_id() != crate::main_thread() {
        return Err(game::Fail { status: game::Status::WrongContext, message: "called off the game's main thread".into() });
    }
    reload_now(&id)
}

fn reload_now(id: &str) -> game::GResult<Reloaded> {
    let refuse = |status, message: String| Err(game::Fail { status, message });
    let Some(host) = HOST.get().cloned() else { return refuse(game::Status::NotFound, format!("no extension of a mod {id:?} is loaded")) };
    let (index, name, dir, permissions, data) = {
        let opened = OPENED.lock().unwrap_or_else(|e| e.into_inner());
        let Some((i, o)) = opened.iter().enumerate().find(|(_, o)| o.id == id) else {
            return refuse(game::Status::NotFound, format!("no extension of a mod {id:?} is loaded"));
        };
        (i, o.name.clone(), o.dir.clone(), o.permissions.clone(), o.data.clone())
    };
    let harmony = dir.join("eu4-harmony");
    let manifest_file = harmony.join(eu4_harmony_manifest::FILE);
    let manifest = std::fs::read_to_string(&manifest_file).map_err(|e| e.to_string()).and_then(|t| eu4_harmony_manifest::Manifest::parse(&t));
    let rejected = |message: String| game::Fail { status: game::Status::Rejected, message };
    let manifest = manifest.map_err(|e| rejected(format!("{}: {e}", manifest_file.display())))?;
    if manifest.id != id {
        return Err(rejected(format!("the manifest's id is now {:?}; a changed id takes a restart of the game", manifest.id)));
    }
    if manifest.permissions != permissions {
        return Err(rejected("the manifest's permissions changed; the player saw them as they were, and they change only with a restart of the game".into()));
    }
    if !manifest.built_for(eu4_harmony_remote::API_VERSION) {
        return Err(rejected(format!("written for api_version {}, this EU4 Harmony provides {}", manifest.api_version, eu4_harmony_remote::API_VERSION)));
    }
    let opening = Mod { id, name: &name, harmony: harmony.clone(), permissions: &permissions, data: Some(data) };
    let new = eu4_harmony_lua::open(index, &opening, host).map_err(|e| rejected(format!("the new code did not load, and the old goes on: {e}")))?;
    let Some(old) = extensions().get(index).cloned() else { return refuse(game::Status::NotFound, format!("no extension of a mod {id:?} is loaded")) };
    let session = crate::sessions::going_on();
    // A stopped extension's code is not run again, save_data included.
    let (saved, lost) = match (session, old.stopped()) {
        (false, _) => (None, None),
        (true, Some(reason)) => (None, Some(format!("the old code was stopped ({reason}), so its save_data was not asked"))),
        (true, None) => match old.save_data() {
            Some(Ok(saved)) => (saved, None),
            Some(Err(e)) => (None, Some(format!("save_data failed: {e}"))),
            None => (None, None),
        },
    };
    old.session_over();
    ui::forget_extension(index);
    if let Some(m) = crate::memory() {
        map::release(m, index);
    }
    EXTENSIONS.write().unwrap_or_else(|e| e.into_inner())[index] = new.clone();
    for problem in crate::loc::reload_mod(id, &harmony) {
        log!("extension {id:?} reloaded: localisation/{problem}");
    }
    log!("extension {id:?} reloaded{}", lost.as_ref().map(|l| format!("; its state was not carried: {l}")).unwrap_or_default());
    if session {
        new.handle("on_session", &[Arg::Json(saved.unwrap_or(serde_json::Value::Null))]);
    }
    refresh_report(true);
    Ok(Reloaded { session, lost })
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

/// The handler `event` calls, and its arguments
/// (`eu4_harmony_remote::lua::handler_call`).
fn handler_call(event: &Event) -> Option<(&'static str, Vec<Arg>)> {
    let Some((name, args)) = eu4_harmony_remote::lua::handler_call(event) else {
        log!("no handler for the event {}", event.kind());
        return None;
    };
    Some((name, args.into_iter().map(Arg::Json).collect()))
}

/// Dispatches a game's safe frame (`crate::safe_point`) to every extension:
/// waiting tasks whose answer came, what happened in its order
/// ([`game::Frame::heard`]), what concerns one extension (its map coloring
/// taken over, its buttons clicked), then the frame. A session that ended
/// inside an extension's call is heard first, and at the frame's end.
pub fn frame(state: &game::Frame) {
    ended_within_a_call();
    let all = extensions();
    for e in &all {
        e.poll();
    }
    for heard in state.heard() {
        match heard.event {
            Event::Session { session } => {
                let loaded = saves::take_loaded();
                log!("session {session} started (from a loaded save: {})", loaded.is_some());
                for e in &all {
                    e.take_session_times();
                }
                *SESSION_CLOCKS.lock().unwrap_or_else(|e| e.into_inner()) = Some(Clocks::now());
                saves::replace_all(loaded.unwrap_or_default());
                for e in &all {
                    let saved = saves::get(e.id()).unwrap_or(serde_json::Value::Null);
                    e.handle("on_session", &[Arg::Json(saved)]);
                }
                refresh_report(true);
            }
            // Extensions hear every event, the player's view or not.
            event => {
                if let Some((name, args)) = handler_call(&event) {
                    all.iter().for_each(|e| e.handle(name, &args));
                }
            }
        }
    }
    let listened = eu4_harmony_remote::events::EVENTS.iter().map(|(kind, _)| *kind).filter(|kind| all.iter().any(|e| e.has_handler(&format!("on_{kind}"))));
    game::events::extensions_listen(listened.collect());
    for lost in map::take_lost() {
        if let Some(e) = all.get(lost) {
            e.handle("on_map_lost", &[]);
        }
    }
    for click in ui::take_clicks() {
        if let Some(e) = all.get(click.extension) {
            let how = Arg::Click {
                right: click.click == ui::CLICK_RIGHT,
                shift: click.modifiers & ui::MODIFIER_SHIFT != 0,
                ctrl: click.modifiers & ui::MODIFIER_CTRL != 0,
            };
            e.handle("on_button", &[Arg::Text(click.name.clone()), how]);
        }
    }
    for e in &all {
        e.handle("on_frame", &[]);
    }
    ended_within_a_call();
    refresh_report(false);
}

/// The sessions that ended while an extension was inside a call of its own,
/// as when a console command it ran loaded a save, for extensions to hear
/// once that call is done ([`ended_within_a_call`]).
static ENDED_WITHIN: Mutex<Vec<crate::sessions::End>> = Mutex::new(Vec::new());

/// The session is being left, for `end` (`crate::sessions`): every
/// extension hears it while the session's state is still the game's, on the
/// main thread, before anything of it is taken down: once leaving is
/// certain, after the ironman exit save, when there is one, was written.
///
/// Within an extension's own call, no extension is called again: one would
/// run inside itself, its waiting tasks dropped under the task running.
/// Then they hear it at the next point the framework calls them from, the
/// start or end of a game's safe frame; the session's state is gone by then,
/// as what ended it ran inside that call.
pub fn session_ended(end: crate::sessions::End) {
    if eu4_harmony_lua::calling() {
        log!("extensions hear the session's end once the extension's call it ended in is done");
        ENDED_WITHIN.lock().unwrap_or_else(|e| e.into_inner()).push(end);
        return;
    }
    tell_ended(end);
}

/// Tells extensions of the sessions that ended inside an extension's call.
fn ended_within_a_call() {
    let ended = std::mem::take(&mut *ENDED_WITHIN.lock().unwrap_or_else(|e| e.into_inner()));
    for end in ended {
        tell_ended(end);
    }
}

fn tell_ended(end: crate::sessions::End) {
    let call = handler_call(&end.event());
    for e in extensions() {
        if let Some((name, args)) = &call {
            e.handle(name, args);
        }
        e.session_over();
    }
    let Some(began) = SESSION_CLOCKS.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    let now = Clocks::now();
    let (wall, main, game) = (now.wall - began.wall, now.main.saturating_sub(began.main), now.game.saturating_sub(began.game));
    let share = |part: Duration, of: Duration| if of.is_zero() { 0.0 } else { part.as_secs_f64() / of.as_secs_f64() * 100.0 };
    for e in extensions() {
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

/// A session's clocks: the time, and the CPU time the game's main thread and
/// the whole game have had, to set what extensions took against.
struct Clocks {
    wall: Instant,
    main: Duration,
    game: Duration,
}

/// The clocks when the session going on began.
static SESSION_CLOCKS: Mutex<Option<Clocks>> = Mutex::new(None);

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

/// Every extension's save data, from its `save_data`
/// (ADR save-data-is-asked-for-by-save-data), as the game writes its world, on
/// the main thread. One whose `save_data` fails or returns what JSON cannot
/// hold has none in this save.
pub fn save_data() {
    for e in extensions() {
        match e.save_data() {
            None => {}
            Some(Ok(data)) => saves::set(e.id(), data),
            Some(Err(_)) => saves::set(e.id(), None),
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
    extensions().iter().map(|e| e.timing()).collect()
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

    thread_local! {
        static GAME: std::cell::RefCell<catalogue::fake::Fake> = std::cell::RefCell::default();
    }

    /// The catalogue over the fake game, as any extension calls it.
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
        let logged = std::fs::read_to_string(log_file(&dir, "calling")).unwrap();
        let logged: Vec<&str> = logged.lines().filter(|line| !line.starts_with("slow stretch:")).collect();
        assert_eq!(logged, ["integer 4294967377 1444.11.11 nil not_found"]);
        GAME.with(|game| assert_eq!(game.borrow().calls, ["army 4294967377", "session", "army 1"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reload_carries_the_save_data_and_leaves_failed_code_out() {
        let dir = std::env::temp_dir().join(format!("eu4h-reload-{}", std::process::id()));
        let harmony = dir.join("eu4-harmony");
        std::fs::create_dir_all(harmony.join("lua")).unwrap();
        let write = |file: &str, text: &str| std::fs::write(harmony.join(file), text).unwrap();
        let manifest = format!("id = \"reloaded\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION);
        write("manifest.toml", &manifest);
        write("lua/main.lua", "return { save_data = function(game) return { count = 41 } end }");
        // No other test loads extensions, so the host is this test's.
        let host = Arc::new(Quiet::default());
        assert!(HOST.set(host.clone()).is_ok());
        let permissions = eu4_harmony_manifest::Permissions::default();
        let m = Mod { id: "reloaded", name: "Reloaded", harmony: harmony.clone(), permissions: &permissions, data: Some(dir.join("data")) };
        let first = eu4_harmony_lua::open(0, &m, host.clone()).unwrap();
        *EXTENSIONS.write().unwrap() = vec![first.clone()];
        *OPENED.lock().unwrap() =
            vec![Opened { id: "reloaded".into(), name: "Reloaded".into(), dir: dir.clone(), permissions: permissions.clone(), data: dir.join("data") }];
        crate::sessions::set_going_on(Some(1));

        write("lua/main.lua", "return { on_session = function(game, saved) log('v2 got ' .. tostring(saved and saved.count)) end }");
        let r = reload_now("reloaded").unwrap();
        assert!(r.session && r.lost.is_none());
        let second = extensions()[0].clone();
        assert!(!Arc::ptr_eq(&first, &second));

        // New code that does not compile, or a changed manifest: the old goes on.
        write("lua/main.lua", "return {");
        let refused = reload_now("reloaded").unwrap_err();
        assert_eq!(refused.status, game::Status::Rejected);
        assert!(refused.message.contains("the new code did not load, and the old goes on"), "{}", refused.message);
        write("lua/main.lua", "return {}");
        write("manifest.toml", &format!("{manifest}\npermissions = {{ clock = true }}"));
        assert!(reload_now("reloaded").unwrap_err().message.contains("permissions changed"));
        assert!(Arc::ptr_eq(&extensions()[0], &second));
        assert_eq!(reload_now("other").unwrap_err().status, game::Status::NotFound);

        assert!(host.0.lock().unwrap().iter().any(|l| l == "v2 got 41"), "{:?}", host.0.lock().unwrap());
        crate::sessions::set_going_on(None);
        *EXTENSIONS.write().unwrap() = Vec::new();
        *OPENED.lock().unwrap() = Vec::new();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
