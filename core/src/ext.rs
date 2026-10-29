//! Loads extension DLLs, implements the capability table they call over the
//! capability layer (`crate::game`), and dispatches events to them at safe
//! points.
//!
//! Each callback runs through the SEH boundary. An extension that returns an
//! error, panics or faults is stopped: it is never called again (its DLL stays
//! loaded), its UI elements are hidden and its map coloring is released.
use crate::{
    game::{
        self,
        armies::{self, RouteSource},
        events, interface,
        map_coloring::{self, MapColor},
        countries, provinces, save_data, texts, time, Caller, Status, Tag,
    },
    map,
    memory::Memory,
    mods::DiscoveredMod,
    notice::Message,
    saves, ui,
};
use eu4_harmony_abi as abi;
use std::{
    cell::Cell,
    collections::BTreeMap,
    ffi::c_void,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};

/// A single callback slower than this is logged.
const SLOW: Duration = Duration::from_millis(5);

/// Per-extension context behind the opaque `host` pointer.
struct Host {
    index: usize,
    id: String,
    /// The mod's name, for the player.
    name: String,
    last_error: Mutex<String>,
    log: Mutex<Option<File>>,
}

struct Loaded {
    host: Box<Host>,
    _host_api: Box<abi::HostApi>,
    game_api: Box<abi::GameApi>,
    vtable: abi::ExtensionVTable,
    stopped: Option<String>,
    calls: u64,
    slow_calls: u64,
    total: Duration,
    max: Duration,
}
// The raw pointers in the tables are only used on the game thread.
unsafe impl Send for Loaded {}

static EXTENSIONS: Mutex<Vec<Loaded>> = Mutex::new(Vec::new());
static LOG_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

thread_local! {
    /// Extension whose callback is running on this thread.
    static CURRENT: Cell<Option<usize>> = const { Cell::new(None) };
}

fn extensions() -> std::sync::MutexGuard<'static, Vec<Loaded>> {
    EXTENSIONS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The id of the mod whose extension has this index.
pub fn mod_id(index: usize) -> Option<String> {
    extensions().get(index).map(|l| l.host.id.clone())
}

// ---------- host functions ----------
//
// Each converts between the ABI and the capability layer (`crate::game`),
// where the capabilities' logic is.

unsafe fn host<'a>(ptr: *mut c_void) -> &'a Host {
    &*(ptr as *const Host)
}

/// A failed host function: an ABI status code and a message for `last_error`.
struct Failure {
    code: i32,
    message: String,
}
type HResult<T> = std::result::Result<T, Failure>;

impl From<game::Fail> for Failure {
    fn from(fail: game::Fail) -> Self {
        let code = match fail.status {
            Status::NotInGame => abi::NOT_IN_GAME,
            Status::NotReady => abi::NOT_READY,
            Status::NotFound => abi::NOT_FOUND,
            Status::Rejected => abi::REJECTED,
            Status::InvalidArgument => abi::INVALID_ARGUMENT,
            Status::Unavailable => abi::UNAVAILABLE,
            Status::WrongContext => abi::WRONG_CONTEXT,
            Status::Internal => abi::INTERNAL,
        };
        Self { code, message: fail.message }
    }
}

fn failure<T>(code: i32, message: impl Into<String>) -> HResult<T> {
    Err(Failure { code, message: message.into() })
}

/// Runs a capability: checks it is called from this extension's callback on
/// the game thread, and records the error message for `last_error`. A panic
/// here is in framework code and ends the game (ADR 0012).
fn capability(ptr: *mut c_void, f: impl FnOnce(Caller) -> HResult<()>) -> i32 {
    let (h, outcome) = crate::guarded("extension capability", || {
        let h = unsafe { host(ptr) };
        if CURRENT.with(Cell::get) != Some(h.index) || crate::thread_id() != crate::main_thread() {
            return (h, failure(abi::WRONG_CONTEXT, "called outside this extension's callback"));
        }
        if crate::memory().is_none() {
            return (h, failure(abi::UNAVAILABLE, "game capabilities unavailable"));
        }
        (h, f(Caller::Extension(h.index)))
    });
    match outcome {
        Ok(()) => abi::OK,
        Err(e) => {
            *h.last_error.lock().unwrap_or_else(|p| p.into_inner()) = e.message;
            e.code
        }
    }
}

/// Copies `items` into a caller buffer with the two-call protocol.
unsafe fn list_out<T: Copy>(items: &[T], buf: *mut T, cap: usize, len: *mut usize) -> HResult<()> {
    if len.is_null() {
        return failure(abi::INVALID_ARGUMENT, "null length pointer");
    }
    *len = items.len();
    if items.len() > cap {
        return failure(abi::BUFFER_TOO_SMALL, "buffer too small");
    }
    if !items.is_empty() {
        if buf.is_null() {
            return failure(abi::INVALID_ARGUMENT, "null buffer");
        }
        std::ptr::copy_nonoverlapping(items.as_ptr(), buf, items.len());
    }
    Ok(())
}

unsafe fn out<T>(ptr: *mut T, value: T) -> HResult<()> {
    if ptr.is_null() {
        return failure(abi::INVALID_ARGUMENT, "null output pointer");
    }
    *ptr = value;
    Ok(())
}

unsafe fn utf8<'a>(ptr: *const u8, len: usize) -> HResult<&'a str> {
    if len == 0 {
        return Ok("");
    }
    if ptr.is_null() {
        return failure(abi::INVALID_ARGUMENT, "null text");
    }
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).or_else(|_| failure(abi::INVALID_ARGUMENT, "text is not UTF-8"))
}

fn province_out(p: &provinces::Province) -> abi::Province {
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    abi::Province {
        id: p.id,
        flags: flag(p.land, abi::PROVINCE_LAND) | flag(p.lake, abi::PROVINCE_LAKE) | flag(p.impassable, abi::PROVINCE_IMPASSABLE) | flag(p.visible, abi::PROVINCE_VISIBLE),
        owner: p.owner.map_or(0, |t| t.0),
        controller: p.controller.map_or(0, |t| t.0),
    }
}

fn army_out(a: &armies::Army) -> abi::Army {
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    abi::Army {
        id: a.id,
        owner: a.owner.0,
        province: a.province,
        flags: flag(a.land, abi::ARMY_LAND)
            | flag(a.in_battle, abi::ARMY_IN_BATTLE)
            | flag(a.sieging, abi::ARMY_SIEGING)
            | flag(a.retreating, abi::ARMY_RETREATING)
            | flag(a.exiled, abi::ARMY_EXILED)
            | flag(a.movement_locked, abi::ARMY_MOVEMENT_LOCKED)
            | flag(a.attached, abi::ARMY_ATTACHED)
            | flag(a.has_followers, abi::ARMY_HAS_FOLLOWERS)
            | flag(a.busy, abi::ARMY_BUSY),
        regiments: a.regiments,
        route_len: a.route_len,
        destination: a.destination.unwrap_or(0),
        route_source: match a.route_source {
            RouteSource::None => abi::ROUTE_NONE,
            RouteSource::External => abi::ROUTE_EXTERNAL,
            RouteSource::Own => abi::ROUTE_OWN,
            RouteSource::OtherExtension => abi::ROUTE_OTHER_EXTENSION,
        },
    }
}

unsafe extern "C" fn h_log(ptr: *mut c_void, text: *const u8, len: usize) {
    crate::guarded("extension log", || {
        let h = host(ptr);
        let message = match utf8(text, len) {
            Ok(s) => s.to_string(),
            Err(_) => String::from_utf8_lossy(std::slice::from_raw_parts(text, len)).into_owned(),
        };
        extension_log(h, &message);
    });
}

fn extension_log(h: &Host, message: &str) {
    let mut file = h.log.lock().unwrap_or_else(|e| e.into_inner());
    if file.is_none() {
        if let Some(dir) = LOG_DIR.get() {
            let path = dir.join(format!("{}.log", h.id));
            if path.exists() {
                let _ = std::fs::rename(&path, path.with_extension("prev.log"));
            }
            *file = OpenOptions::new().create(true).write(true).truncate(true).open(path).ok();
        }
    }
    if let Some(f) = file.as_mut() {
        let _ = writeln!(f, "{message}");
        let _ = f.flush();
    }
}

unsafe extern "C" fn h_last_error(ptr: *mut c_void, buf: *mut u8, cap: usize) -> usize {
    crate::guarded("extension last error", || {
        let h = host(ptr);
        let message = h.last_error.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !buf.is_null() {
            let n = message.len().min(cap);
            std::ptr::copy_nonoverlapping(message.as_ptr(), buf, n);
        }
        message.len()
    })
}

unsafe extern "C" fn h_session(ptr: *mut c_void, o: *mut abi::Session) -> i32 {
    capability(ptr, |caller| {
        let s = game::session::session(caller)?;
        out(
            o,
            abi::Session {
                session: s.session,
                date: s.date,
                paused: s.paused as u8,
                visibility_current: s.visibility_current as u8,
                _reserved: 0,
                player: s.player.0,
            },
        )
    })
}

unsafe extern "C" fn h_country_tag(ptr: *mut c_void, country: u64, o: *mut [u8; 3]) -> i32 {
    capability(ptr, |caller| out(o, game::countries::tag(caller, Tag(country))?))
}

unsafe extern "C" fn h_countries_hostile(ptr: *mut c_void, a: u64, b: u64, o: *mut u8) -> i32 {
    capability(ptr, |caller| out(o, game::countries::hostile(caller, Tag(a), Tag(b))? as u8))
}

unsafe extern "C" fn h_provinces(ptr: *mut c_void, buf: *mut abi::Province, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| {
        let provinces: Vec<_> = provinces::provinces(caller)?.iter().map(province_out).collect();
        list_out(&provinces, buf, cap, len)
    })
}

unsafe extern "C" fn h_province_neighbors(ptr: *mut c_void, id: u32, buf: *mut abi::Neighbor, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| {
        let neighbors: Vec<_> = provinces::neighbors(caller, id)?
            .iter()
            .map(|n| abi::Neighbor { to: n.to, flags: abi::NEIGHBOR_LAND | if n.strait { abi::NEIGHBOR_STRAIT } else { 0 } })
            .collect();
        list_out(&neighbors, buf, cap, len)
    })
}

unsafe extern "C" fn h_army(ptr: *mut c_void, id: u64, o: *mut abi::Army) -> i32 {
    capability(ptr, |caller| out(o, army_out(&armies::army(caller, id)?)))
}

unsafe extern "C" fn h_own_armies(ptr: *mut c_void, buf: *mut abi::Army, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| {
        let armies: Vec<_> = armies::own_armies(caller)?.iter().map(army_out).collect();
        list_out(&armies, buf, cap, len)
    })
}

unsafe extern "C" fn h_province_armies(ptr: *mut c_void, province: u32, buf: *mut abi::Army, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| {
        let armies: Vec<_> = armies::in_province(caller, province)?.iter().map(army_out).collect();
        list_out(&armies, buf, cap, len)
    })
}

unsafe extern "C" fn h_army_route(ptr: *mut c_void, id: u64, buf: *mut u32, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| list_out(&armies::route(caller, id)?, buf, cap, len))
}

unsafe extern "C" fn h_army_can_enter(ptr: *mut c_void, id: u64, province: u32, o: *mut u8) -> i32 {
    capability(ptr, |caller| out(o, armies::can_enter(caller, id, province)? as u8))
}

unsafe extern "C" fn h_army_strait_open(ptr: *mut c_void, id: u64, from: u32, to: u32, o: *mut u8) -> i32 {
    capability(ptr, |caller| out(o, armies::strait_open(caller, id, from, to)? as u8))
}

unsafe extern "C" fn h_army_can_attack(ptr: *mut c_void, id: u64, target: u64, province: u32, o: *mut u8) -> i32 {
    capability(ptr, |caller| out(o, armies::can_attack(caller, id, target, province)? as u8))
}

unsafe extern "C" fn h_find_route(ptr: *mut c_void, id: u64, goal: u32, buf: *mut u32, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| list_out(&armies::find_route(caller, id, goal)?, buf, cap, len))
}

unsafe extern "C" fn h_army_move(ptr: *mut c_void, id: u64, goal: u32) -> i32 {
    capability(ptr, |caller| Ok(armies::move_to(caller, id, goal)?))
}

unsafe extern "C" fn h_army_stop(ptr: *mut c_void, id: u64) -> i32 {
    capability(ptr, |caller| Ok(armies::stop(caller, id)?))
}

unsafe extern "C" fn h_ui_selected_army(ptr: *mut c_void, o: *mut u64) -> i32 {
    capability(ptr, |caller| out(o, armies::selected(caller)?))
}

unsafe extern "C" fn h_ui_set_visible(ptr: *mut c_void, name: *const u8, name_len: usize, visible: u8) -> i32 {
    capability(ptr, |caller| Ok(interface::set_visible(caller, utf8(name, name_len)?, visible != 0)?))
}

unsafe extern "C" fn h_ui_set_enabled(ptr: *mut c_void, name: *const u8, name_len: usize, enabled: u8) -> i32 {
    capability(ptr, |caller| Ok(interface::set_enabled(caller, utf8(name, name_len)?, enabled != 0)?))
}

unsafe extern "C" fn h_ui_set_text(ptr: *mut c_void, name: *const u8, name_len: usize, t: *const u8, t_len: usize) -> i32 {
    capability(ptr, |caller| {
        let text = utf8(t, t_len)?;
        Ok(interface::set_text(caller, utf8(name, name_len)?, text)?)
    })
}

unsafe extern "C" fn h_ui_set_tooltip(ptr: *mut c_void, name: *const u8, name_len: usize, t: *const u8, t_len: usize) -> i32 {
    capability(ptr, |caller| {
        let text = utf8(t, t_len)?;
        Ok(interface::set_tooltip(caller, utf8(name, name_len)?, text)?)
    })
}

unsafe extern "C" fn h_map_paint(ptr: *mut c_void, colors: *const abi::MapColor, count: usize) -> i32 {
    capability(ptr, |caller| {
        if colors.is_null() || count == 0 {
            return failure(abi::INVALID_ARGUMENT, "no colors");
        }
        let colors: Vec<_> = std::slice::from_raw_parts(colors, count).iter().map(|c| MapColor { province: c.province, rgba: c.rgba }).collect();
        Ok(map_coloring::paint(caller, &colors)?)
    })
}

unsafe extern "C" fn h_map_clear(ptr: *mut c_void) -> i32 {
    capability(ptr, |caller| Ok(map_coloring::clear(caller)?))
}

unsafe extern "C" fn h_save_data_get(ptr: *mut c_void, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| {
        let bytes = save_data::get(caller)?.map(|v| v.to_string().into_bytes()).unwrap_or_default();
        list_out(&bytes, buf, cap, len)
    })
}

unsafe extern "C" fn h_save_data_set(ptr: *mut c_void, json: *const u8, len: usize) -> i32 {
    capability(ptr, |caller| {
        let text = utf8(json, len)?;
        let value = if text.is_empty() {
            None
        } else {
            Some(serde_json::from_str(text).or_else(|e| failure(abi::INVALID_ARGUMENT, format!("not JSON: {e}")))?)
        };
        Ok(save_data::set(caller, value)?)
    })
}

unsafe extern "C" fn h_language(ptr: *mut c_void, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| list_out(texts::language(caller)?.as_bytes(), buf, cap, len))
}

unsafe extern "C" fn h_text(ptr: *mut c_void, key: *const u8, key_len: usize, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| list_out(texts::text(caller, utf8(key, key_len)?)?.as_bytes(), buf, cap, len))
}

fn camera_out(c: game::camera::Camera) -> abi::Camera {
    abi::Camera { x: c.x, y: c.y, height: c.height, province: c.province.unwrap_or(0), min_height: c.min_height, max_height: c.max_height }
}

/// The camera after a move, for an `out` that may be null.
unsafe fn moved_out(o: *mut abi::Camera, c: game::camera::Camera) -> HResult<()> {
    if !o.is_null() {
        *o = camera_out(c);
    }
    Ok(())
}

unsafe extern "C" fn h_camera(ptr: *mut c_void, o: *mut abi::Camera) -> i32 {
    capability(ptr, |caller| out(o, camera_out(game::camera::camera(caller)?)))
}

unsafe extern "C" fn h_camera_look_at_province(ptr: *mut c_void, province: u32, o: *mut abi::Camera) -> i32 {
    capability(ptr, |caller| moved_out(o, game::camera::look_at_province(caller, province)?))
}

unsafe extern "C" fn h_camera_look_at(ptr: *mut c_void, x: u32, y: u32, o: *mut abi::Camera) -> i32 {
    capability(ptr, |caller| moved_out(o, game::camera::look_at(caller, x, y)?))
}

unsafe extern "C" fn h_camera_zoom(ptr: *mut c_void, height: u32, o: *mut abi::Camera) -> i32 {
    capability(ptr, |caller| moved_out(o, game::camera::zoom(caller, height)?))
}

/// A fixed-point number of the game's, kept in thousandths, as the number it
/// stands for in a snapshot's JSON (ADR 0018).
fn thousandths(value: i32) -> f64 {
    value as f64 / 1000.0
}

/// A province's snapshot as the JSON `abi::GameApi::province` describes.
fn province_json(s: &provinces::Snapshot) -> serde_json::Value {
    let country = |t: Option<Tag>| t.map(|t| t.0);
    serde_json::json!({
        "id": s.id,
        "name": s.name,
        "owner": country(s.owner),
        "controller": country(s.controller),
        "development": {"tax": thousandths(s.development.tax), "production": thousandths(s.development.production), "manpower": thousandths(s.development.manpower)},
        "terrain": s.terrain,
        "fort": {"level": s.fort.level, "mothballed": s.fort.mothballed},
        "buildings": s.buildings,
        "recruit": s.recruit.iter().map(|r| serde_json::json!({"kind": r.kind.key(), "unit_type": r.unit_type, "allowed": r.allowed, "reasons": r.reasons})).collect::<Vec<_>>(),
    })
}

unsafe extern "C" fn h_province(ptr: *mut c_void, id: u32, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| list_out(province_json(&provinces::province(caller, id)?).to_string().as_bytes(), buf, cap, len))
}

unsafe extern "C" fn h_console_run(ptr: *mut c_void, command: *const u8, command_len: usize, printed: Option<abi::TextSink>, user: *mut c_void) -> i32 {
    capability(ptr, |caller| {
        let text = game::console::run(caller, utf8(command, command_len)?)?;
        if let Some(printed) = printed {
            printed(user, text.as_ptr(), text.len());
        }
        Ok(())
    })
}

/// A country's snapshot as the JSON `abi::GameApi::country` describes.
fn country_json(c: &countries::Country) -> serde_json::Value {
    let ids = |tags: &[Tag]| tags.iter().map(|t| t.0).collect::<Vec<_>>();
    serde_json::json!({
        "id": c.tag.0,
        "treasury": thousandths(c.treasury),
        "manpower": c.manpower,
        "max_manpower": c.max_manpower,
        "stability": thousandths(c.stability),
        "prestige": thousandths(c.prestige),
        "power": {"adm": c.power[0], "dip": c.power[1], "mil": c.power[2]},
        "wars": c.wars.iter().map(|w| serde_json::json!({"name": w.name, "attackers": ids(&w.attackers), "defenders": ids(&w.defenders)})).collect::<Vec<_>>(),
        "relations": c.relations.iter().map(|r| serde_json::json!({"kind": r.kind.key(), "country": r.country.0})).collect::<Vec<_>>(),
    })
}

unsafe extern "C" fn h_country(ptr: *mut c_void, country: u64, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |caller| list_out(country_json(&countries::snapshot(caller, Tag(country))?).to_string().as_bytes(), buf, cap, len))
}

unsafe extern "C" fn h_time_speed(ptr: *mut c_void, o: *mut u32) -> i32 {
    capability(ptr, |caller| out(o, time::time(caller)?.speed))
}

unsafe extern "C" fn h_time_set_speed(ptr: *mut c_void, speed: u32) -> i32 {
    capability(ptr, |caller| Ok(time::set_speed(caller, speed)?))
}

unsafe extern "C" fn h_time_pause(ptr: *mut c_void) -> i32 {
    capability(ptr, |caller| Ok(time::pause(caller)?))
}

unsafe extern "C" fn h_time_resume(ptr: *mut c_void) -> i32 {
    capability(ptr, |caller| Ok(time::resume(caller)?))
}

fn game_api(host: *mut c_void) -> abi::GameApi {
    abi::GameApi {
        size: std::mem::size_of::<abi::GameApi>(),
        host,
        last_error: h_last_error,
        session: h_session,
        country_tag: h_country_tag,
        countries_hostile: h_countries_hostile,
        provinces: h_provinces,
        province_neighbors: h_province_neighbors,
        army: h_army,
        own_armies: h_own_armies,
        province_armies: h_province_armies,
        army_route: h_army_route,
        army_can_enter: h_army_can_enter,
        army_strait_open: h_army_strait_open,
        army_can_attack: h_army_can_attack,
        find_route: h_find_route,
        army_move: h_army_move,
        army_stop: h_army_stop,
        ui_selected_army: h_ui_selected_army,
        ui_set_visible: h_ui_set_visible,
        ui_set_enabled: h_ui_set_enabled,
        ui_set_text: h_ui_set_text,
        ui_set_tooltip: h_ui_set_tooltip,
        map_paint: h_map_paint,
        map_clear: h_map_clear,
        save_data_get: h_save_data_get,
        save_data_set: h_save_data_set,
        language: h_language,
        text: h_text,
        camera: h_camera,
        camera_look_at_province: h_camera_look_at_province,
        camera_look_at: h_camera_look_at,
        camera_zoom: h_camera_zoom,
        province: h_province,
        console_run: h_console_run,
        country: h_country,
        time_speed: h_time_speed,
        time_set_speed: h_time_set_speed,
        time_pause: h_time_pause,
        time_resume: h_time_resume,
    }
}

// ---------- loading ----------

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain([0]).collect()
}

/// What loading one mod's extension came to.
pub struct LoadOutcome {
    /// The mod takes part: its extension loaded, or it declares none.
    pub included: bool,
    pub status: String,
}

/// Loads every enabled mod's extension, in playset order.
pub fn load(mods: &[DiscoveredMod], log_dir: &Path) -> Vec<LoadOutcome> {
    let memory = crate::memory().copied();
    let _ = LOG_DIR.set(log_dir.to_path_buf());
    let mut outcomes = Vec::new();
    for m in mods {
        let Some(relative) = &m.manifest.extension else {
            outcomes.push(LoadOutcome { included: true, status: "no extension".into() });
            continue;
        };
        match load_one(memory, m, relative) {
            Ok(()) => {
                log!("extension {:?} loaded", m.manifest.id);
                outcomes.push(LoadOutcome { included: true, status: format!("extension {relative} loaded") });
            }
            Err(e) => {
                log!("extension {:?} not loaded: {e}", m.manifest.id);
                outcomes.push(LoadOutcome { included: false, status: format!("extension NOT loaded: {e}") });
                // An unsupported executable has its own notice.
                if memory.is_some() {
                    crate::notice::post(&format!("load:{}", m.manifest.id), not_loaded(m, &e));
                }
            }
        }
    }
    outcomes
}

/// What the player is told about an extension that was not loaded. An API
/// mismatch is the one they can fix, so it is spelled out.
fn not_loaded(m: &DiscoveredMod, e: &str) -> Message {
    if m.manifest.api_version != abi::API_VERSION {
        return Message::new("EU4HARMONY_API_MISMATCH").arg("MOD", &m.descriptor.name).arg("BUILT", &m.manifest.api_version).arg("PROVIDED", abi::API_VERSION);
    }
    Message::new("EU4HARMONY_NOT_LOADED").arg("MOD", &m.descriptor.name).arg("DETAIL", e)
}

fn load_one(memory: Option<Memory>, m: &DiscoveredMod, relative: &str) -> crate::Result<()> {
    if memory.is_none() {
        return Err("the game executable is not supported, so no extension can run".into());
    }
    if m.manifest.api_version != abi::API_VERSION {
        return Err(format!("built for api_version {}, this EU4 Harmony provides {}", m.manifest.api_version, abi::API_VERSION));
    }
    let path = m.dir.join("eu4-harmony").join(relative);
    let module = unsafe { LoadLibraryExW(wide(&path).as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
    if module.is_null() {
        return Err(format!("cannot load {}: {}", path.display(), std::io::Error::last_os_error()));
    }
    let export = |name: &str| unsafe { GetProcAddress(module, format!("{name}\0").as_ptr()) }.ok_or_else(|| format!("{} has no {name} export", path.display()));
    // The ABI the DLL was built for, which its manifest may not keep up with.
    let built = Memory { base: 0 }.u32(export(abi::ABI_SYMBOL)? as usize)?;
    if built != abi::ABI_VERSION {
        return Err(format!("built for ABI version {built}, this EU4 Harmony speaks version {}", abi::ABI_VERSION));
    }
    let entry = export(abi::ENTRY_SYMBOL)?;
    let mut list = extensions();
    let index = list.len();
    let host = Box::new(Host { index, id: m.manifest.id.clone(), name: m.descriptor.name.clone(), last_error: Mutex::new(String::new()), log: Mutex::new(None) });
    let host_ptr = &*host as *const Host as *mut c_void;
    let host_api = Box::new(abi::HostApi { size: std::mem::size_of::<abi::HostApi>(), abi_version: abi::ABI_VERSION, host: host_ptr, log: h_log });
    let game_api = Box::new(game_api(host_ptr));
    let mut vtable = abi::ExtensionVTable {
        size: std::mem::size_of::<abi::ExtensionVTable>(),
        user: std::ptr::null_mut(),
        on_session: None,
        on_frame: None,
        on_day: None,
        on_button: None,
        on_army_ordered: None,
        on_army_defeated: None,
        on_map_lost: None,
    };
    let code = Memory { base: 0 }.call(entry as usize, &[&*host_api as *const abi::HostApi as usize, &mut vtable as *mut _ as usize])?;
    if code as u32 as i32 != abi::OK {
        return Err(format!("entry point returned {}", code as u32 as i32));
    }
    list.push(Loaded { host, _host_api: host_api, game_api, vtable, stopped: None, calls: 0, slow_calls: 0, total: Duration::ZERO, max: Duration::ZERO });
    Ok(())
}

/// Maps discovery-order mod indices to extension indices.
pub fn extension_indices(mods: &[DiscoveredMod]) -> BTreeMap<usize, usize> {
    let list = extensions();
    mods.iter().enumerate().filter_map(|(i, m)| list.iter().find(|l| l.host.id == m.manifest.id).map(|l| (i, l.host.index))).collect()
}

// ---------- dispatch ----------

/// Calls one extension callback (a function pointer and its integer
/// arguments, `user` and the game table first) and stops the extension on failure.
fn call(index: usize, name: &str, pick: impl Fn(&abi::ExtensionVTable) -> Option<usize>, args: &[usize]) {
    let (target, user, game) = {
        let list = extensions();
        let Some(l) = list.get(index) else { return };
        if l.stopped.is_some() {
            return;
        }
        let Some(target) = pick(&l.vtable) else { return };
        (target, l.vtable.user as usize, &*l.game_api as *const abi::GameApi as usize)
    };
    let mut full = vec![user, game];
    full.extend_from_slice(args);
    CURRENT.with(|c| c.set(Some(index)));
    let started = Instant::now();
    let result = Memory { base: 0 }.call(target, &full);
    let elapsed = started.elapsed();
    CURRENT.with(|c| c.set(None));
    let failure = match result {
        Ok(code) if code as u32 as i32 == abi::OK => None,
        Ok(code) => Some(format!("{name} returned {}", code as u32 as i32)),
        Err(e) => Some(format!("{name} crashed: {e}")),
    };
    let (id, name) = {
        let mut list = extensions();
        let l = &mut list[index];
        l.calls += 1;
        l.total += elapsed;
        l.max = l.max.max(elapsed);
        if elapsed > SLOW {
            l.slow_calls += 1;
            extension_log(&l.host, &format!("slow callback {name}: {} ms", elapsed.as_millis()));
        }
        if let Some(reason) = &failure {
            l.stopped = Some(reason.clone());
        }
        (l.host.id.clone(), l.host.name.clone())
    };
    if let Some(reason) = failure {
        log!("extension {id:?} stopped: {reason}");
        ui::hide_extension(index);
        if let Some(m) = crate::memory() {
            map::release(m, index);
        }
        crate::notice::post(&format!("stopped:{id}"), Message::new("EU4HARMONY_STOPPED").arg("MOD", name).arg("DETAIL", reason));
    }
}

fn count() -> usize {
    extensions().len()
}

/// Dispatches a game's safe frame (`crate::safe_point`) to every extension.
pub fn frame(state: &game::Frame) {
    let n = count();
    if state.new_session {
        let loaded = saves::take_loaded();
        log!("session {} started (from a loaded save: {})", state.session, loaded.is_some());
        saves::replace_all(loaded.unwrap_or_default());
        for i in 0..n {
            call(i, "on_session", |v| v.on_session.map(|f| f as usize), &[]);
        }
        refresh_report(true);
    }
    for &change in &state.changes {
        match change {
            events::Change::Ordered(id) => {
                for i in 0..n {
                    call(i, "on_army_ordered", |v| v.on_army_ordered.map(|f| f as usize), &[id as usize]);
                }
            }
            events::Change::Defeated(id) => {
                for i in 0..n {
                    call(i, "on_army_defeated", |v| v.on_army_defeated.map(|f| f as usize), &[id as usize]);
                }
            }
        }
    }
    for lost in map::take_lost() {
        call(lost, "on_map_lost", |v| v.on_map_lost.map(|f| f as usize), &[]);
    }
    for click in ui::take_clicks() {
        let name = click.name;
        call(click.extension, "on_button", |v| v.on_button.map(|f| f as usize), &[name.as_ptr() as usize, name.len(), click.click as usize, click.modifiers as usize]);
    }
    if state.new_day {
        for i in 0..n {
            call(i, "on_day", |v| v.on_day.map(|f| f as usize), &[state.ctx.date as u32 as usize]);
        }
    }
    for i in 0..n {
        call(i, "on_frame", |v| v.on_frame.map(|f| f as usize), &[]);
    }
    refresh_report(false);
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

/// One line per extension: calls, time spent in them, and whether it was stopped.
pub fn timing_lines() -> Vec<String> {
    extensions()
        .iter()
        .map(|l| {
            let mut line = format!(
                "{}: {} call(s), {} ms in total, longest {} ms, {} over {} ms",
                l.host.id,
                l.calls,
                l.total.as_millis(),
                l.max.as_millis(),
                l.slow_calls,
                SLOW.as_millis()
            );
            if let Some(reason) = &l.stopped {
                line.push_str(&format!("; STOPPED: {reason}"));
            }
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{countries::RelationKind, provinces::UnitKind};

    /// The snapshots the SDK's tests read (ADR 0018).
    const PROVINCE_JSON: &str = include_str!("../../abi/json/province.json");
    const COUNTRY_JSON: &str = include_str!("../../abi/json/country.json");

    /// Constantinople as the engine's fake game has it, every kind of unit
    /// in it, is written as the same JSON value as the file holds: the same
    /// members, holding the same values, each number of the same sort
    /// (`10.0`, not `10`). Only the spaces and the order of the members may
    /// differ, which no JSON reader sees.
    #[test]
    fn a_province_snapshot_is_written_as_the_abi_says() {
        let snapshot = crate::remote::fake::constantinople();
        assert_eq!(snapshot.recruit.iter().map(|r| r.kind).collect::<Vec<_>>(), UnitKind::ALL);
        assert_eq!(province_json(&snapshot), serde_json::from_str::<serde_json::Value>(PROVINCE_JSON).unwrap());
    }

    /// Wallachia as the engine's fake game has it, a relation of every kind
    /// and a war on either side, is written as the same JSON value as the
    /// file holds, as a province's is.
    #[test]
    fn a_country_snapshot_is_written_as_the_abi_says() {
        let snapshot = crate::remote::fake::wallachia();
        assert_eq!(snapshot.relations.iter().map(|r| r.kind).collect::<Vec<_>>(), RelationKind::ALL);
        assert_eq!(country_json(&snapshot), serde_json::from_str::<serde_json::Value>(COUNTRY_JSON).unwrap());
    }
}
