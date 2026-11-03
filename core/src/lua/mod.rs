//! The core's side of the Lua host (`eu4_harmony_lua`, ADR 0024): loads
//! every mod's extension at startup on the live host, the running game, and
//! calls its handlers at safe points (`crate::safe_point`).
//!
//! The live host reaches the capability catalogue through remote control's
//! groups, called as the extension (`crate::remote::run_for`), so both sides
//! of the catalogue share one implementation (ADR 0026); interface elements,
//! the map, texts and the player language through the core's own modules;
//! the network through [`http`]. Each extension's log is
//! `eu4-harmony/logs/<id>.log`, the last start's kept beside it.
mod http;

use crate::{
    game::{self, events, interface, map_coloring, Caller},
    map,
    mods::DiscoveredMod,
    notice::Message,
    remote::{self, Wait},
    saves, ui,
};
use eu4_harmony_lua::{fetch, Arg, Called, Element, Extension, Host, Mod, Pending};
use eu4_harmony_remote::{Error, Request, Response};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

static EXTENSIONS: OnceLock<Vec<Arc<Extension>>> = OnceLock::new();

fn extensions() -> &'static [Arc<Extension>] {
    EXTENSIONS.get().map_or(&[], Vec::as_slice)
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
}

/// A capability's answer still to come in remote control.
struct Later(Caller, Box<dyn Wait>);

impl Pending<Response> for Later {
    fn poll(&mut self) -> Option<Response> {
        remote::poll_for(self.0, self.1.as_mut())
    }
}

impl Host for Live {
    fn run(&self, extension: usize, request: Request) -> Called {
        let caller = Caller::Extension(extension);
        match remote::run_for(caller, request) {
            remote::Outcome::Done(response) => Called::Done(response),
            remote::Outcome::Later(wait) => Called::Later(Box::new(Later(caller, wait))),
        }
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
        events::session()
    }

    fn first_error(&self, extension: &Extension, from: &str, message: &str) {
        // The player is told in a game the framework runs in.
        if crate::memory().is_some() {
            let session = events::session();
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
    let host: Arc<dyn Host> = Arc::new(Live { logs: log_dir.to_path_buf(), open: Mutex::new(HashMap::new()) });
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

// ---------- dispatch ----------

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
                    let saved = saves::get(e.id()).unwrap_or(serde_json::Value::Null);
                    e.handle("on_session", &[Arg::Json(saved)]);
                }
                refresh_report(true);
            }
            game::Heard::Change(events::Change::Ordered(id)) => all.iter().for_each(|e| e.handle("on_army_ordered", &[Arg::Integer(id as i64)])),
            game::Heard::Change(events::Change::Defeated(id)) => all.iter().for_each(|e| e.handle("on_army_defeated", &[Arg::Integer(id as i64)])),
            game::Heard::Day(date) => all.iter().for_each(|e| e.handle("on_day", &[Arg::Date(eu4_harmony_checks::Date(date))])),
        }
    }
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
    for e in all {
        e.handle("on_frame", &[]);
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
        e.handle("on_session_ended", &[Arg::Text(word.into())]);
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
        match e.on_save() {
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
