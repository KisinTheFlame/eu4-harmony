//! Loads extension DLLs, implements the capability table they call, and
//! dispatches events to them at safe points.
//!
//! Each callback runs through the SEH boundary. An extension that returns an
//! error, panics or faults is stopped: it is never called again (its DLL stays
//! loaded), its UI elements are hidden and its map coloring is released.
use crate::{
    game::{self, events, fail, orders::Orders, world, GResult},
    lang, loc, map,
    memory::Memory,
    mods::DiscoveredMod,
    notice::Message,
    saves, text, ui,
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

struct Runtime {
    geography: world::Geography,
    orders: Orders,
    session: u64,
    last_day: Option<(u64, i32)>,
}

static MEMORY: OnceLock<Memory> = OnceLock::new();
static EXTENSIONS: Mutex<Vec<Loaded>> = Mutex::new(Vec::new());
static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);
static LOG_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

thread_local! {
    /// Extension whose callback is running on this thread.
    static CURRENT: Cell<Option<usize>> = const { Cell::new(None) };
}

fn with_runtime<T>(f: impl FnOnce(&mut Runtime) -> T) -> T {
    let mut guard = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(|| Runtime { geography: Default::default(), orders: Default::default(), session: 0, last_day: None }))
}

fn extensions() -> std::sync::MutexGuard<'static, Vec<Loaded>> {
    EXTENSIONS.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------- host functions ----------

unsafe fn host<'a>(ptr: *mut c_void) -> &'a Host {
    &*(ptr as *const Host)
}

/// Runs a capability: checks it is called from this extension's callback on
/// the game thread, and records the error message for `last_error`.
fn capability(ptr: *mut c_void, f: impl FnOnce(&Host, &Memory) -> GResult<()>) -> i32 {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let h = unsafe { host(ptr) };
        if CURRENT.with(Cell::get) != Some(h.index) || crate::thread_id() != crate::main_thread() {
            return (h, Err(game::Fail { code: abi::WRONG_CONTEXT, message: "called outside this extension's callback".into() }));
        }
        let Some(m) = MEMORY.get() else {
            return (h, Err(game::Fail { code: abi::UNAVAILABLE, message: "game capabilities unavailable".into() }));
        };
        (h, f(h, m))
    }));
    match outcome {
        Ok((_, Ok(()))) => abi::OK,
        Ok((h, Err(e))) => {
            *h.last_error.lock().unwrap_or_else(|p| p.into_inner()) = e.message;
            e.code
        }
        Err(_) => abi::INTERNAL,
    }
}

/// Copies `items` into a caller buffer with the two-call protocol.
unsafe fn list_out<T: Copy>(items: &[T], buf: *mut T, cap: usize, len: *mut usize) -> GResult<()> {
    if len.is_null() {
        return fail(abi::INVALID_ARGUMENT, "null length pointer");
    }
    *len = items.len();
    if items.len() > cap {
        return fail(abi::BUFFER_TOO_SMALL, "buffer too small");
    }
    if !items.is_empty() {
        if buf.is_null() {
            return fail(abi::INVALID_ARGUMENT, "null buffer");
        }
        std::ptr::copy_nonoverlapping(items.as_ptr(), buf, items.len());
    }
    Ok(())
}

unsafe fn out<T>(ptr: *mut T, value: T) -> GResult<()> {
    if ptr.is_null() {
        return fail(abi::INVALID_ARGUMENT, "null output pointer");
    }
    *ptr = value;
    Ok(())
}

unsafe fn utf8<'a>(ptr: *const u8, len: usize) -> GResult<&'a str> {
    if len == 0 {
        return Ok("");
    }
    if ptr.is_null() {
        return fail(abi::INVALID_ARGUMENT, "null text");
    }
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).or_else(|_| fail(abi::INVALID_ARGUMENT, "text is not UTF-8"))
}

fn encode(h: &Host, text: &str) -> Vec<u8> {
    let (bytes, lossy) = text::encode(text, text::plugin_loaded());
    if lossy {
        extension_log(h, &format!("some characters of {text:?} cannot be displayed and were replaced by '?'"));
    }
    bytes
}

unsafe extern "C" fn h_log(ptr: *mut c_void, text: *const u8, len: usize) {
    let _ = std::panic::catch_unwind(|| {
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
    std::panic::catch_unwind(|| {
        let h = host(ptr);
        let message = h.last_error.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !buf.is_null() {
            let n = message.len().min(cap);
            std::ptr::copy_nonoverlapping(message.as_ptr(), buf, n);
        }
        message.len()
    })
    .unwrap_or(0)
}

unsafe extern "C" fn h_session(ptr: *mut c_void, o: *mut abi::Session) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        out(
            o,
            abi::Session {
                session: events::session(),
                date: ctx.date,
                paused: game::paused(m, &ctx)? as u8,
                visibility_current: events::visibility_current(m, &ctx) as u8,
                _reserved: 0,
                player: ctx.player.0,
            },
        )
    })
}

unsafe extern "C" fn h_country_tag(ptr: *mut c_void, country: u64, o: *mut [u8; 3]) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        let tag = game::Tag(country);
        game::country(m, &ctx, tag)?;
        out(o, tag.letters())
    })
}

unsafe extern "C" fn h_countries_hostile(ptr: *mut c_void, a: u64, b: u64, o: *mut u8) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        out(o, world::hostile(m, &ctx, game::Tag(a), game::Tag(b))? as u8)
    })
}

unsafe extern "C" fn h_provinces(ptr: *mut c_void, buf: *mut abi::Province, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        let visible = events::visibility_current(m, &ctx);
        let provinces = with_runtime(|rt| rt.geography.provinces(m, &ctx, visible))?;
        list_out(&provinces, buf, cap, len)
    })
}

unsafe extern "C" fn h_province_neighbors(ptr: *mut c_void, id: u32, buf: *mut abi::Neighbor, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        let neighbors = with_runtime(|rt| rt.geography.neighbors(m, &ctx, id))?;
        list_out(&neighbors, buf, cap, len)
    })
}

fn army_out(m: &Memory, ctx: &game::Ctx, h: &Host, address: usize, id: u64) -> GResult<abi::Army> {
    let mut record = world::army_record(m, &game::special_countries(m)?, address)?;
    if record.flags & abi::ARMY_LAND != 0 {
        let path = game::orders::read_path(m, address + 0x100)?;
        record.route_len = path.ids.len() as u32;
        record.destination = path.ids.last().copied().unwrap_or(0);
        record.route_source = with_runtime(|rt| rt.orders.route_source(m, address, id, h.index))?;
    }
    let _ = ctx;
    Ok(record)
}

unsafe extern "C" fn h_army(ptr: *mut c_void, id: u64, o: *mut abi::Army) -> i32 {
    capability(ptr, |h, m| {
        let ctx = game::context(m)?;
        let Some((_, address, _)) = world::own_roster(m, &ctx)?.into_iter().find(|(i, _, _)| *i == id) else {
            return fail(abi::NOT_FOUND, format!("no own army {id}"));
        };
        out(o, army_out(m, &ctx, h, address, id)?)
    })
}

unsafe extern "C" fn h_own_armies(ptr: *mut c_void, buf: *mut abi::Army, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |h, m| {
        let ctx = game::context(m)?;
        let armies = world::own_roster(m, &ctx)?
            .into_iter()
            .map(|(id, address, _)| army_out(m, &ctx, h, address, id))
            .collect::<GResult<Vec<_>>>()?;
        list_out(&armies, buf, cap, len)
    })
}

unsafe extern "C" fn h_province_armies(ptr: *mut c_void, province: u32, buf: *mut abi::Army, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        if !events::visibility_current(m, &ctx) {
            return fail(abi::NOT_READY, "visibility is not current yet");
        }
        let address = with_runtime(|rt| rt.geography.province_address(m, &ctx, province))?;
        let armies: Vec<_> = world::province_armies(m, &ctx, address)?.into_iter().map(|(a, _)| a).collect();
        list_out(&armies, buf, cap, len)
    })
}

unsafe extern "C" fn h_army_route(ptr: *mut c_void, id: u64, buf: *mut u32, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        let address = world::own_land_army(m, &ctx, id)?;
        list_out(&game::orders::read_path(m, address + 0x100)?.ids, buf, cap, len)
    })
}

unsafe extern "C" fn h_army_can_enter(ptr: *mut c_void, id: u64, province: u32, o: *mut u8) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        let army = world::own_land_army(m, &ctx, id)?;
        let address = with_runtime(|rt| rt.geography.province_address(m, &ctx, province))?;
        out(o, world::can_enter(m, army, address)? as u8)
    })
}

unsafe extern "C" fn h_army_strait_open(ptr: *mut c_void, id: u64, from: u32, to: u32, o: *mut u8) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        let army = world::own_land_army(m, &ctx, id)?;
        out(o, with_runtime(|rt| rt.geography.strait_open(m, &ctx, army, from, to))? as u8)
    })
}

unsafe extern "C" fn h_army_can_attack(ptr: *mut c_void, id: u64, target: u64, province: u32, o: *mut u8) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        if !events::visibility_current(m, &ctx) {
            return fail(abi::NOT_READY, "visibility is not current yet");
        }
        let army = world::own_land_army(m, &ctx, id)?;
        let address = with_runtime(|rt| rt.geography.province_address(m, &ctx, province))?;
        let Some((_, unit)) = world::province_armies(m, &ctx, address)?.into_iter().find(|(a, _)| a.id == target) else {
            return fail(abi::NOT_FOUND, format!("no visible army {target} in {province}"));
        };
        out(o, (unit != army && world::can_attack(m, army, unit)?) as u8)
    })
}

unsafe extern "C" fn h_find_route(ptr: *mut c_void, id: u64, goal: u32, buf: *mut u32, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        let army = world::own_land_army(m, &ctx, id)?;
        let route = with_runtime(|rt| {
            let Runtime { geography, orders, .. } = rt;
            orders.find_route(m, &ctx, geography, army, goal)
        })?;
        list_out(&route, buf, cap, len)
    })
}

unsafe extern "C" fn h_army_move(ptr: *mut c_void, id: u64, goal: u32) -> i32 {
    capability(ptr, |h, m| {
        let ctx = game::context(m)?;
        let army = world::own_land_army(m, &ctx, id)?;
        with_runtime(|rt| {
            let Runtime { geography, orders, .. } = rt;
            orders.move_army(m, &ctx, geography, army, id, goal, h.index)
        })
    })
}

unsafe extern "C" fn h_army_stop(ptr: *mut c_void, id: u64) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        let army = world::own_land_army(m, &ctx, id)?;
        with_runtime(|rt| rt.orders.stop_army(m, &ctx, army, id))
    })
}

unsafe extern "C" fn h_ui_selected_army(ptr: *mut c_void, o: *mut u64) -> i32 {
    capability(ptr, |_, m| {
        let ctx = game::context(m)?;
        out(o, ui::selected_army(m, &ctx)?)
    })
}

unsafe extern "C" fn h_ui_set_visible(ptr: *mut c_void, name: *const u8, name_len: usize, visible: u8) -> i32 {
    capability(ptr, |h, _| ui::set_visible(h.index, utf8(name, name_len)?, visible != 0))
}

unsafe extern "C" fn h_ui_set_enabled(ptr: *mut c_void, name: *const u8, name_len: usize, enabled: u8) -> i32 {
    capability(ptr, |h, _| ui::set_enabled(h.index, utf8(name, name_len)?, enabled != 0))
}

unsafe extern "C" fn h_ui_set_text(ptr: *mut c_void, name: *const u8, name_len: usize, t: *const u8, t_len: usize) -> i32 {
    capability(ptr, |h, _| {
        let text = encode(h, utf8(t, t_len)?);
        ui::set_text(h.index, utf8(name, name_len)?, text)
    })
}

unsafe extern "C" fn h_ui_set_tooltip(ptr: *mut c_void, name: *const u8, name_len: usize, t: *const u8, t_len: usize) -> i32 {
    capability(ptr, |h, _| {
        let text = encode(h, utf8(t, t_len)?);
        ui::set_tooltip(h.index, utf8(name, name_len)?, text)
    })
}

unsafe extern "C" fn h_map_paint(ptr: *mut c_void, colors: *const abi::MapColor, count: usize) -> i32 {
    capability(ptr, |h, m| {
        if colors.is_null() || count == 0 {
            return fail(abi::INVALID_ARGUMENT, "no colors");
        }
        let ctx = game::context(m)?;
        map::paint(m, &ctx, h.index, std::slice::from_raw_parts(colors, count))
    })
}

unsafe extern "C" fn h_map_clear(ptr: *mut c_void) -> i32 {
    capability(ptr, |h, m| {
        let ctx = game::context(m)?;
        map::clear(m, &ctx, h.index)
    })
}

unsafe extern "C" fn h_save_data_get(ptr: *mut c_void, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |h, _| {
        let bytes = saves::get(&h.id).map(|v| v.to_string().into_bytes()).unwrap_or_default();
        list_out(&bytes, buf, cap, len)
    })
}

unsafe extern "C" fn h_save_data_set(ptr: *mut c_void, json: *const u8, len: usize) -> i32 {
    capability(ptr, |h, _| {
        let text = utf8(json, len)?;
        if text.is_empty() {
            saves::set(&h.id, None);
            return Ok(());
        }
        let value: serde_json::Value = serde_json::from_str(text).or_else(|e| fail(abi::INVALID_ARGUMENT, format!("not JSON: {e}")))?;
        saves::set(&h.id, Some(value));
        Ok(())
    })
}

unsafe extern "C" fn h_language(ptr: *mut c_void, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |_, _| list_out(lang::player().id().as_bytes(), buf, cap, len))
}

unsafe extern "C" fn h_text(ptr: *mut c_void, key: *const u8, key_len: usize, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
    capability(ptr, |h, _| {
        let key = utf8(key, key_len)?;
        let Some(text) = loc::mod_text(&h.id, key) else {
            return fail(abi::NOT_FOUND, format!("no localisation text for {key}"));
        };
        list_out(text.as_bytes(), buf, cap, len)
    })
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
pub fn load(memory: Option<Memory>, mods: &[DiscoveredMod], log_dir: &Path) -> Vec<LoadOutcome> {
    if let Some(m) = memory {
        let _ = MEMORY.set(m);
    }
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
    let symbol = format!("{}\0", abi::ENTRY_SYMBOL);
    let entry = unsafe { GetProcAddress(module, symbol.as_ptr()) }.ok_or_else(|| format!("{} has no {} export", path.display(), abi::ENTRY_SYMBOL))?;
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
        if let Some(m) = MEMORY.get() {
            map::release(m, index);
        }
        crate::notice::post(&format!("stopped:{id}"), Message::new("EU4HARMONY_STOPPED").arg("MOD", name).arg("DETAIL", reason));
    }
}

fn count() -> usize {
    extensions().len()
}

/// Safe-point driver, run after the map manager's frame.
pub fn frame() {
    let Some(m) = MEMORY.get().copied() else { return };
    let Some(state) = events::begin_frame(&m) else { return };
    let new_session = with_runtime(|rt| {
        if rt.session == state.session {
            return false;
        }
        rt.session = state.session;
        rt.geography.clear();
        rt.orders.clear();
        rt.last_day = None;
        true
    });
    let n = count();
    if new_session {
        map::reset_session(&m, &state.ctx);
        let loaded = saves::take_loaded();
        log!("session {} started (from a loaded save: {})", state.session, loaded.is_some());
        saves::replace_all(loaded.unwrap_or_default());
        for i in 0..n {
            call(i, "on_session", |v| v.on_session.map(|f| f as usize), &[]);
        }
        refresh_report(true);
    }
    for change in state.changes {
        match change {
            events::Change::Ordered(id) => {
                with_runtime(|rt| rt.orders.revoke(id));
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
    let today = (state.session, state.ctx.date);
    if state.visibility_current && with_runtime(|rt| rt.last_day.replace(today)) != Some(today) {
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
