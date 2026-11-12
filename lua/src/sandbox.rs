//! An extension's Lua state: the sandbox (ADR extensions-are-lua-in-a-sandbox)
//! and what the host puts in it. Lua's own string, table, math, utf8 and
//! coroutine libraries, the base library without loading files or bytecode,
//! `os.clock`; the framework's globals `log`, `json`, `mod_files`, `text`, and
//! the tasks' `spawn`, `async`, `await_all` and `await_any`, which the prelude
//! builds, and `require` for the mod's own modules; the sensitive capabilities
//! (ADR sensitive-capabilities-are-declared) `os.time` and `os.date`, `files`
//! and `http`, which raise an error where the manifest does not declare them;
//! and `game`.
use super::{
    files,
    values::{self, date_text},
    windows, Element, Host, Mod, Waits,
};
use eu4_harmony_remote::{capabilities, shape::Shape, Capability, Kind};
use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Table, Thread, Value as LuaValue, Variadic};
use serde_json::{Map, Number, Value};
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// A new extension's state, with `game` and the sentinel a capability that
/// waits returns to its wrapper.
pub struct Opened {
    pub lua: Lua,
    pub game: Table,
    /// The prelude's own function that runs a pop-up's button's action.
    pub press: Function,
    /// The prelude's own function that marks a dropped task's handle
    /// cancelled.
    pub dropped: Function,
    pub wait: LuaValue,
    /// The state's main thread, for the time limit on plain calls.
    pub main_state: *const c_void,
}

/// The most memory an extension's Lua state may hold. The state lives in
/// the game's process, and what it holds the game cannot have: past this an
/// allocation fails with Lua's memory error, an error as any other
/// (ADR an-error-does-not-stop-an-extension), long before the game's own
/// allocations would. It is far more than a mod's tables of provinces,
/// countries and routes need.
pub const MEMORY: usize = 256 * 1024 * 1024;

/// The registry's name for the modules `require` has loaded.
const LOADED: &str = "eu4harmony.loaded";

/// What a new extension's state is made for.
pub struct Opening<'a> {
    pub index: usize,
    pub serial: u64,
    pub m: &'a Mod<'a>,
    pub host: Arc<dyn Host>,
    /// What its tasks wait for.
    pub waits: Arc<Mutex<Waits>>,
}

/// What a function the sandbox gives the extension reaches it by.
pub struct Calling {
    pub index: usize,
    pub serial: u64,
    pub host: Arc<dyn Host>,
    pub waits: Arc<Mutex<Waits>>,
    /// The sentinel a function that waits returns to its wrapper.
    pub wait: LuaValue,
}

/// The state of a new extension, as `at` says.
pub fn open(at: &Opening) -> mlua::Result<Opened> {
    let (index, serial, id, harmony, permissions) = (at.index, at.serial, at.m.id, at.m.harmony.as_path(), at.m.permissions);
    let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE | StdLib::OS;
    let lua = Lua::new_with(libs, LuaOptions::default())?;
    lua.set_memory_limit(MEMORY)?;
    let globals = lua.globals();

    // os: the clock the extension spends, and the real time when declared.
    let full: Table = globals.get("os")?;
    let os = lua.create_table()?;
    os.set("clock", full.get::<Function>("clock")?)?;
    for name in ["time", "date"] {
        let function = match permissions.clock {
            true => full.get::<Function>(name)?,
            false => lua.create_function(move |_, _: MultiValue| Err::<(), _>(undeclared(&format!("os.{name}"), "clock", "clock = true")))?,
        };
        os.set(name, function)?;
    }
    globals.set("os", os)?;

    // log and print: the extension's own log.
    let (mod_id, host) = (id.to_string(), at.host.clone());
    let log = lua.create_function(move |lua, args: Variadic<LuaValue>| {
        let tostring: Function = lua.globals().get("tostring")?;
        let words = args.iter().map(|a| tostring.call::<String>(a.clone())).collect::<mlua::Result<Vec<_>>>()?;
        host.log(&mod_id, &words.join(" "));
        Ok(())
    })?;
    globals.set("print", log.clone())?;
    globals.set("log", log)?;

    // json: as extension save data is written.
    let json = lua.create_table()?;
    json.set(
        "encode",
        lua.create_function(|_, value: LuaValue| {
            values::to_json(&value).map(|v| v.to_string()).map_err(|e| mlua::Error::runtime(format!("json.encode: {e}")))
        })?,
    )?;
    json.set(
        "decode",
        lua.create_function(|lua, text: mlua::String| {
            let parsed: Value = serde_json::from_slice(&text.as_bytes()).map_err(|e| mlua::Error::runtime(format!("json.decode: {e}")))?;
            values::to_lua(lua, &parsed)
        })?,
    )?;
    globals.set("json", json)?;

    // mod_files: the mod's own files, read only.
    let root = harmony.to_path_buf();
    let mod_files = lua.create_table()?;
    mod_files.set(
        "read",
        lua.create_function(move |lua, path: String| {
            let file = within(&root, &path).map_err(|e| mlua::Error::runtime(format!("mod_files.read: {e}")))?;
            match std::fs::read(&file) {
                Ok(bytes) => Ok(MultiValue::from_iter([LuaValue::String(lua.create_string(bytes)?)])),
                Err(e) => failure(lua, "not_found", &format!("{path}: {e}")),
            }
        })?,
    )?;
    globals.set("mod_files", mod_files)?;

    // text: the mod's localisation in the player language.
    let text = lua.create_table()?;
    let (mod_id, host) = (id.to_string(), at.host.clone());
    text.set(
        "get",
        lua.create_function(move |lua, (key, values): (String, Option<Table>)| {
            let Some(raw) = host.text(&mod_id, &key) else {
                return failure(lua, "not_found", &eu4_harmony_checks::texts::not_found(&key).message);
            };
            let mut args = Vec::new();
            if let Some(values) = values {
                let tostring: Function = lua.globals().get("tostring")?;
                for pair in values.pairs::<String, LuaValue>() {
                    let (name, value) = pair?;
                    args.push((name, tostring.call::<String>(value)?));
                }
            }
            let args: Vec<(&str, &str)> = args.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
            Ok(MultiValue::from_iter([LuaValue::String(lua.create_string(eu4_harmony_loc::fill(&raw, &args))?)]))
        })?,
    )?;
    let host = at.host.clone();
    text.set("language", lua.create_function(move |_, ()| Ok(host.language()))?)?;
    globals.set("text", text)?;

    // require: the mod's own modules, by dotted name.
    lua.set_named_registry_value(LOADED, lua.create_table()?)?;
    let modules = harmony.join("lua");
    globals.set(
        "require",
        lua.create_function(move |lua, name: String| {
            let loaded: Table = lua.named_registry_value(LOADED)?;
            if let Ok(module) = loaded.get::<LuaValue>(name.as_str()) {
                if !module.is_nil() {
                    return Ok(module);
                }
            }
            let file = module_file(&modules, &name).map_err(|e| mlua::Error::runtime(format!("require {name:?}: {e}")))?;
            let source = std::fs::read_to_string(&file).map_err(|e| mlua::Error::runtime(format!("require {name:?}: {e}")))?;
            let module = chunk(lua, source, &format!("{}.lua", name.replace('.', "/"))).call::<LuaValue>(name.as_str())?;
            let module = if module.is_nil() { LuaValue::Boolean(true) } else { module };
            loaded.set(name.as_str(), module.clone())?;
            Ok(module)
        })?,
    )?;

    // spawn, async, await_all and await_any, which the prelude builds on
    // these: a task of its own and its handle, cancelling it, and waiting for
    // tasks to end (ADR tasks-are-awaited-like-promises).
    let start = lua.create_function(move |lua, (handle, runner): (Table, Function)| {
        if !super::running(serial) {
            return Err(mlua::Error::runtime("spawn: a task is started only from a handler or a task, not at the top of main.lua"));
        }
        match super::extension_of(lua) {
            Some(e) => e.spawn(handle, runner),
            None => Err(mlua::Error::runtime("spawn: the extension is not loaded")),
        }
    })?;
    let cancel = lua.create_function(|lua, handle: Table| {
        if let Some(e) = super::extension_of(lua) {
            e.cancel(&handle);
        }
        Ok(())
    })?;

    // files: the extension's data folder.
    globals.set("files", files::table(&lua, at.m.data.clone(), permissions.files)?)?;

    // What the functions that reach the host share, with the sentinel a
    // function that waits returns to its wrapper.
    let wait = LuaValue::Table(lua.create_table()?);
    let calling = Arc::new(Calling { index, serial, host: at.host.clone(), waits: at.waits.clone(), wait: wait.clone() });

    // http.fetch, which the prelude wraps: it waits.
    let (hosts, at_fetch) = (permissions.network.clone(), calling.clone());
    let fetch = lua.create_function(move |_, (url, options): (String, Option<Table>)| super::fetch(&at_fetch, &hosts, &url, options))?;

    // game, built by the prelude around the host's functions.
    let at_call = calling.clone();
    let call = lua.create_function(move |lua, (name, args): (String, MultiValue)| super::call(lua, &at_call, &name, args))?;
    let task = lua.create_function(|_, ()| Ok(super::current_task()))?;
    let names = lua.create_sequence_from(extension_capabilities().map(|c| c.name()))?;
    let at_element = calling.clone();
    let set_element =
        lua.create_function(move |lua, (element, property, value): (String, String, LuaValue)| set_element(lua, &at_element, &element, &property, value))?;
    // game.window and game.popup, which the prelude wraps.
    let at_window = calling.clone();
    let window = lua.create_function(move |lua, (op, window, widget, property, value): (String, String, Option<String>, Option<String>, LuaValue)| {
        window_change(lua, &at_window, &op, &window, widget.as_deref(), property.as_deref(), &value)
    })?;
    let (at_popup, popup_id) = (calling.clone(), id.to_string());
    let popup = lua.create_function(move |lua, spec: Table| show_popup(lua, &at_popup, &popup_id, &spec))?;
    let at_close = calling.clone();
    let close_popup = lua.create_function(move |_, popup: u32| {
        if !super::running(at_close.serial) {
            return Err(mlua::Error::runtime("closing a pop-up: the game is reached only from a handler, not at the top of main.lua"));
        }
        at_close.host.close_popup(at_close.index, popup).map_err(|e| mlua::Error::runtime(format!("closing a pop-up: {e}")))
    })?;
    // The coroutine of the mod's own that runs, for the time limit.
    let resuming = lua.create_function(|_, co: Thread| {
        super::watch::resume(co.to_pointer());
        Ok(())
    })?;
    let resumed = lua.create_function(|_, co: Thread| {
        super::watch::resumed(co.to_pointer());
        Ok(())
    })?;
    // Waiting for any of some tasks to end: the wait sentinel and its number.
    let at_await = calling.clone();
    let awaiting = lua.create_function(move |_, handles: Vec<Table>| {
        if !super::running(at_await.serial) {
            return Err(mlua::Error::runtime("a task is awaited only from a handler or a task"));
        }
        Ok(Waits::keep(&at_await.waits, &at_await.wait, super::Waiting::Tasks(handles)))
    })?;
    let at_tooltip = calling.clone();
    let tooltip = lua.create_function(move |lua, (window, widget): (Option<String>, String)| {
        let text = at_tooltip.host.tooltip(at_tooltip.index, window.as_deref(), &widget).map_err(mlua::Error::runtime)?;
        text.to_lua(lua)
    })?;
    let (game, press, dropped): (Table, Function, Function) = lua.load(include_str!("prelude.lua")).set_name("@prelude").call((
        call,
        wait.clone(),
        task,
        names,
        set_element,
        fetch,
        resuming,
        resumed,
        window,
        popup,
        close_popup,
        start,
        cancel,
        awaiting,
        tooltip,
    ))?;
    let map = lua.create_table()?;
    let at_paint = calling.clone();
    map.set("paint", lua.create_function(move |lua, colors: Table| paint(lua, &at_paint, colors))?)?;
    let at_release = calling;
    map.set(
        "release",
        lua.create_function(move |lua, ()| {
            if !super::running(at_release.serial) {
                return Err(mlua::Error::runtime("game.map.release: the game is reached only from a handler, not at the top of main.lua"));
            }
            match at_release.host.release_map(at_release.index) {
                Ok(()) => Ok(MultiValue::from_iter([LuaValue::Boolean(true)])),
                Err(e) => failure(lua, e.code.name(), &e.message),
            }
        })?,
    )?;
    game.set("map", map)?;
    let main_state = main_state(&lua);
    Ok(Opened { lua, game, press, dropped, wait, main_state })
}

/// The state's main thread, a `lua_State*`.
pub fn main_state(lua: &Lua) -> *const c_void {
    let mut found = std::ptr::null();
    let _ = unsafe { lua.exec_raw::<()>((), |state| found = state as *const c_void) };
    found
}

/// The capabilities extensions have, each named by its group and action.
fn extension_capabilities() -> impl Iterator<Item = Capability> {
    capabilities().into_iter().filter(for_extensions)
}

fn for_extensions(c: &Capability) -> bool {
    c.extensions && !c.action.is_empty() && c.group != "text"
}

/// `nil, err` for a failure, as capabilities answer one.
pub(super) fn failure(lua: &Lua, code: &str, message: &str) -> mlua::Result<MultiValue> {
    let err = lua.create_table()?;
    err.set("code", code)?;
    err.set("message", message)?;
    Ok(MultiValue::from_iter([LuaValue::Nil, LuaValue::Table(err)]))
}

/// The error for a sensitive capability `what` used though the manifest
/// does not declare `permission`, which `example` declares.
pub(super) fn undeclared(what: &str, permission: &str, example: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "{what}: the manifest does not declare the {permission} permission (permissions = {{ {example} }} in eu4-harmony/manifest.toml)"
    ))
}

/// A path under `root`: relative, with `/`, no `..`, drive or leading `/`,
/// and each part a name Windows can give a file.
pub(super) fn within(root: &Path, path: &str) -> Result<PathBuf, String> {
    let parts: Vec<&str> = path.split('/').collect();
    if path.is_empty() || !parts.iter().all(|p| file_name(p)) {
        return Err(format!("{path:?} is not a relative path of file names with / between them"));
    }
    Ok(parts.iter().fold(root.to_path_buf(), |at, part| at.join(part)))
}

/// Whether Windows can give a file this name, and it names no device.
fn file_name(name: &str) -> bool {
    const DEVICES: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6",
        "lpt7", "lpt8", "lpt9",
    ];
    let stem = name.split('.').next().unwrap_or_default().trim_end().to_ascii_lowercase();
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.chars().any(|c| c.is_control() || r#"<>:"\|?*"#.contains(c))
        && !DEVICES.contains(&stem.as_str())
}

/// The Lua file `file` of the mod's `lua/` (`range/bfs.lua`), as the host
/// loads it: as text, never bytecode, named by that path.
pub(crate) fn chunk<'a>(lua: &'a Lua, source: impl mlua::AsChunk + 'a, file: &str) -> mlua::Chunk<'a> {
    lua.load(source).set_name(format!("@{file}")).set_mode(mlua::ChunkMode::Text)
}

/// The file of the module `name` (`range.bfs` is `range/bfs.lua`) under the
/// mod's `lua/`.
fn module_file(modules: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || !name.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return Err("a module is named by words of letters, digits and _ between dots".into());
    }
    within(modules, &format!("{}.lua", name.replace('.', "/")))
}

/// `game.ui.<element>.<property> = value`.
fn set_element(lua: &Lua, at: &Calling, element: &str, property: &str, value: LuaValue) -> mlua::Result<()> {
    if !super::running(at.serial) {
        return Err(mlua::Error::runtime(format!("game.ui.{element}: the game is reached only from a handler, not at the top of main.lua")));
    }
    let text = |v: &LuaValue| match v {
        LuaValue::String(s) => Ok(s.to_str()?.to_string()),
        LuaValue::Nil => Ok(String::new()),
        other => Err(mlua::Error::runtime(format!("game.ui.{element}.{property} takes a string, not a {}", other.type_name()))),
    };
    let flag = |v: &LuaValue| match v {
        LuaValue::Boolean(b) => Ok(*b),
        other => Err(mlua::Error::runtime(format!("game.ui.{element}.{property} takes true or false, not a {}", other.type_name()))),
    };
    let value = match property {
        "visible" => Element::Visible(flag(&value)?),
        "enabled" => Element::Enabled(flag(&value)?),
        "text" => Element::Text(text(&value)?),
        "tooltip" => {
            let text = crate::Text::from_lua(value)?;
            check_text(lua, at, &text)?;
            Element::Tooltip(text)
        }
        other => return Err(mlua::Error::runtime(format!("game.ui.{element}.{other}: an element has visible, enabled, text and tooltip"))),
    };
    at.host.set_element(at.index, element, value).map_err(|e| mlua::Error::runtime(format!("game.ui.{element}.{property}: {e}")))
}

fn check_text(lua: &Lua, at: &Calling, text: &crate::Text) -> mlua::Result<()> {
    let extension = super::extension_of(lua).ok_or_else(|| mlua::Error::runtime("the extension is not loaded"))?;
    text.check(&|key| at.host.text(extension.id(), key).is_some()).map_err(mlua::Error::runtime)
}

/// `game.window.open(name)`, `game.window.close(name)` and
/// `game.window.<window>.<widget>.<property> = value`, as `op` says.
fn window_change(lua: &Lua, at: &Calling, op: &str, name: &str, widget: Option<&str>, property: Option<&str>, value: &LuaValue) -> mlua::Result<()> {
    let what = match (op, widget) {
        ("set", Some(widget)) => format!("game.window.{name}.{widget}"),
        _ => format!("game.window.{op}"),
    };
    if !super::running(at.serial) {
        return Err(mlua::Error::runtime(format!("{what}: the game is reached only from a handler, not at the top of main.lua")));
    }
    let Some(extension) = super::extension_of(lua) else { return Err(mlua::Error::runtime(format!("{what}: the extension is not loaded"))) };
    let declared = extension.windows();
    let change = match (op, widget, property) {
        ("open", _, _) => windows::open_or_close(declared, true, name),
        ("close", _, _) => windows::open_or_close(declared, false, name),
        (_, Some(widget), Some(property)) => windows::set(declared, name, widget, property, value),
        _ => Err("set as game.window.<window>.<widget>.<property> = value".to_string()),
    }
    .map_err(mlua::Error::runtime)?;
    let check = |property: &windows::Property| -> mlua::Result<()> {
        if let windows::Property::Tooltip(text) = property {
            check_text(lua, at, text)?;
        }
        Ok(())
    };
    match &change {
        windows::WindowChange::Set { value, .. } => check(value)?,
        windows::WindowChange::Rows { rows, .. } => {
            for (_, property) in rows.iter().flatten() {
                check(property)?;
            }
        }
        _ => {}
    }
    at.host.window(at.index, change).map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))
}

/// `game.popup { ... }`: the pop-up shown, its number, which the prelude
/// keeps its buttons' actions by.
fn show_popup(lua: &Lua, at: &Calling, id: &str, spec: &Table) -> mlua::Result<u32> {
    if !super::running(at.serial) {
        return Err(mlua::Error::runtime("game.popup: a pop-up is shown only from a handler or a task"));
    }
    let words = |key: &str, values: &[(String, String)]| {
        let raw = at.host.text(id, key)?;
        let values: Vec<(&str, &str)> = values.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
        Some(eu4_harmony_loc::fill(&raw, &values))
    };
    let popup = windows::popup(spec, &words, lua).map_err(mlua::Error::runtime)?;
    at.host.popup(at.index, popup).map_err(|e| mlua::Error::runtime(format!("game.popup: {e}")))
}

/// `game.map.paint({ [province] = 0xRRGGBB or { r, g, b, a } })`.
fn paint(lua: &Lua, at: &Calling, colors: Table) -> mlua::Result<MultiValue> {
    let mut list = Vec::new();
    for pair in colors.pairs::<i64, LuaValue>() {
        let (province, color) = pair?;
        let rgba = match color {
            LuaValue::Integer(rgb) if (0..=0xFF_FFFF).contains(&rgb) => [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 0xFF],
            LuaValue::Table(t) => {
                let part = |k: &str, default: Option<u8>| -> mlua::Result<u8> {
                    match (t.get::<Option<i64>>(k)?, default) {
                        (Some(v), _) if (0..=255).contains(&v) => Ok(v as u8),
                        (None, Some(d)) => Ok(d),
                        _ => Err(mlua::Error::runtime(format!("game.map.paint: {k} is from 0 to 255"))),
                    }
                };
                [part("r", None)?, part("g", None)?, part("b", None)?, part("a", Some(0xFF))?]
            }
            other => return Err(mlua::Error::runtime(format!("game.map.paint: a color is 0xRRGGBB or {{ r, g, b, a }}, not a {}", other.type_name()))),
        };
        let province = u32::try_from(province).map_err(|_| mlua::Error::runtime(format!("game.map.paint: {province} is no province")))?;
        list.push((province, rgba));
    }
    if !super::running(at.serial) {
        return Err(mlua::Error::runtime("game.map.paint: the game is reached only from a handler, not at the top of main.lua"));
    }
    match at.host.paint(at.index, &list) {
        Ok(()) => Ok(MultiValue::from_iter([LuaValue::Boolean(true)])),
        Err(e) => failure(lua, e.code.name(), &e.message),
    }
}

/// A capability's request as JSON, from its Lua arguments: the required
/// ones in order, then the optional ones in a table.
pub fn request(name: &str, args: MultiValue) -> Result<Map<String, Value>, String> {
    let Some(capability) = eu4_harmony_remote::capability(name).filter(|c| for_extensions(c)) else {
        return Err("no such capability for extensions".into());
    };
    let required: Vec<_> = capability.args.iter().filter(|a| !a.optional).collect();
    let optional: Vec<_> = capability.args.iter().filter(|a| a.optional).collect();
    let mut given: Vec<LuaValue> = args.into_iter().collect();
    let options = if given.len() == required.len() + 1 && !optional.is_empty() {
        match given.pop() {
            Some(LuaValue::Table(t)) => Some(t),
            Some(other) => return Err(format!("its options are a table, not a {}", other.type_name())),
            None => None,
        }
    } else {
        None
    };
    if given.len() != required.len() {
        let names: Vec<&str> = required.iter().map(|a| a.name).collect();
        let more = if optional.is_empty() { String::new() } else { ", then a table of options".to_string() };
        return Err(format!("takes {} ({}){more}; got {} value(s)", required.len(), names.join(", "), given.len()));
    }
    let mut fields = Map::new();
    fields.insert("capability".into(), Value::String(name.into()));
    for (arg, value) in required.iter().zip(&given) {
        fields.insert(arg.name.into(), arg_json(arg.kind, value).map_err(|e| format!("{}: {e}", arg.name))?);
    }
    if let Some(options) = options {
        for pair in options.pairs::<String, LuaValue>() {
            let (key, value) = pair.map_err(|e| e.to_string())?;
            let Some(arg) = optional.iter().find(|a| a.name == key) else {
                return Err(format!("has no option {key}"));
            };
            fields.insert(arg.name.into(), arg_json(arg.kind, &value).map_err(|e| format!("{key}: {e}"))?);
        }
    }
    Ok(fields)
}

/// `word` after "a", or "an" before a vowel.
fn article(word: &str) -> String {
    let an = word.starts_with(['a', 'e', 'i', 'o', 'u']);
    format!("{} {word}", if an { "an" } else { "a" })
}

/// One argument as JSON, as its kind's shape says Lua passes it. An army is
/// the one kind whose JSON its shape does not say: Lua passes its ID as a
/// number or as text, and JSON writes it as text.
fn arg_json(kind: Kind, value: &LuaValue) -> Result<Value, String> {
    let wrong = || format!("{} such as {}, not {}", article(kind.name()), kind.example(), article(value.type_name()));
    let json = match kind {
        Kind::Army => army(value),
        Kind::Armies => sequence(value, |id| Ok(army(id)))?,
        _ => shaped(&kind.shape(), value)?,
    };
    json.ok_or_else(wrong)
}

/// `value` as JSON, when it has `shape`; an error for a table JSON cannot
/// write.
fn shaped(shape: &Shape, value: &LuaValue) -> Result<Option<Value>, String> {
    Ok(match (shape, value) {
        (Shape::Integer, _) => integer(value).map(Value::from),
        (Shape::Number, LuaValue::Integer(i)) => Some(Value::from(*i)),
        (Shape::Number, LuaValue::Number(n)) => Number::from_f64(*n).map(Value::Number),
        (Shape::Boolean, LuaValue::Boolean(b)) => Some(Value::Bool(*b)),
        (Shape::Text | Shape::Words(_), _) => string(value).map(Value::String),
        (Shape::Date, _) => date_text(value).map(Value::String),
        (Shape::Optional(_), LuaValue::Nil) => Some(Value::Null),
        (Shape::Optional(inner), _) => shaped(inner, value)?,
        (Shape::List(inner), _) => sequence(value, |item| shaped(inner, item))?,
        (Shape::Either(shapes), _) => {
            for shape in shapes {
                if let Some(json) = shaped(shape, value)? {
                    return Ok(Some(json));
                }
            }
            None
        }
        (Shape::Object { .. } | Shape::Map(_), LuaValue::Table(_)) | (Shape::Any, _) => Some(values::to_json(value)?),
        _ => None,
    })
}

/// A table whose keys are 1 to its length, as a JSON array of its values,
/// each as `item` writes it.
fn sequence(value: &LuaValue, item: impl Fn(&LuaValue) -> Result<Option<Value>, String>) -> Result<Option<Value>, String> {
    let LuaValue::Table(t) = value else { return Ok(None) };
    if t.pairs::<LuaValue, LuaValue>().count() != t.raw_len() {
        return Ok(None);
    }
    let mut items = Vec::new();
    for value in t.clone().sequence_values::<LuaValue>() {
        match item(&value.map_err(|e| e.to_string())?)? {
            Some(json) => items.push(json),
            None => return Ok(None),
        }
    }
    Ok(Some(Value::Array(items)))
}

/// An army's ID, a number or its decimal text, as JSON's text.
fn army(value: &LuaValue) -> Option<Value> {
    integer(value).map(|i| i.to_string()).or_else(|| string(value)).map(Value::String)
}

fn integer(value: &LuaValue) -> Option<i64> {
    match value {
        LuaValue::Integer(i) => Some(*i),
        LuaValue::Number(n) if n.fract() == 0.0 => Some(*n as i64),
        _ => None,
    }
}

fn string(value: &LuaValue) -> Option<String> {
    match value {
        LuaValue::String(s) => s.to_str().ok().map(|s| s.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_under_their_root() {
        let root = Path::new(r"C:\mod\eu4-harmony");
        assert_eq!(within(root, "data/units.json").unwrap(), root.join("data").join("units.json"));
        for bad in ["", "../x", "data/../../x", "/x", r"data\x", "C:/x", "data//x", "./x", "nul", "con.txt", "a/aux", "x.", "x ", "a?b", "a*", "a|b"] {
            assert!(within(root, bad).is_err(), "{bad}");
        }
        assert_eq!(module_file(Path::new("lua"), "range.bfs").unwrap(), Path::new("lua").join("range").join("bfs.lua"));
        assert!(module_file(Path::new("lua"), "range..bfs").is_err() && module_file(Path::new("lua"), "a/b").is_err());
    }

    #[test]
    fn a_request_takes_required_arguments_in_order_and_options_in_a_table() {
        let lua = Lua::new();
        let args = |code: &str| lua.load(code).eval::<MultiValue>().unwrap();
        let request = |name: &str, args: MultiValue| request(name, args).map(Value::Object);
        assert_eq!(
            request("army.move", args("return 545460846646, 151")).unwrap(),
            serde_json::json!({"capability": "army.move", "army": "545460846646", "province": 151})
        );
        assert_eq!(
            request("diplomacy.declare_war", args("return 'ENG', 'cb_core', { province = 167, call_allies = { 'DUL', 'CND' } }")).unwrap(),
            serde_json::json!({"capability": "diplomacy.declare_war", "country": "ENG", "casus_belli": "cb_core", "province": 167, "call_allies": ["DUL", "CND"]})
        );
        assert_eq!(request("camera.look_at", args("return { province = 151 }")).unwrap(), serde_json::json!({"capability": "camera.look_at", "province": 151}));
        assert!(request("army.move", args("return 1")).unwrap_err().contains("takes 2 (army, province)"));
        assert!(request("army.move", args("return 1, 'x'")).unwrap_err().contains("province: a province such as 151, not a string"));
        assert!(request("loan.repay", args("return 'x'")).unwrap_err().contains("loan: an integer such as 3, not a string"));
        assert!(request("building.build", args("return 1, 2")).unwrap_err().contains("building: a text such as text, not an integer"));
        assert!(request("camera.look_at", args("return { height = 3 }")).unwrap_err().contains("no option height"));
        assert!(request("menu.load", args("return 'x'")).unwrap_err().contains("no such capability for extensions"));
    }

    #[test]
    fn every_kind_takes_what_its_shape_says_lua_passes() {
        let lua = Lua::new();
        let json = |kind: Kind, code: &str| arg_json(kind, &lua.load(code).eval::<LuaValue>().unwrap());
        assert_eq!(json(Kind::Army, "return 545460846646").unwrap(), serde_json::json!("545460846646"));
        assert_eq!(json(Kind::Army, "return '545460846646'").unwrap(), serde_json::json!("545460846646"));
        assert_eq!(json(Kind::Armies, "return { 12, '34' }").unwrap(), serde_json::json!(["12", "34"]));
        assert_eq!(json(Kind::Province, "return 151.0").unwrap(), serde_json::json!(151));
        assert_eq!(json(Kind::Date, "return { year = 1444, month = 11, day = 11 }").unwrap(), serde_json::json!("1444.11.11"));
        assert_eq!(json(Kind::Date, "return '1444.11.11'").unwrap(), serde_json::json!("1444.11.11"));
        assert_eq!(json(Kind::Allies, "return 'all'").unwrap(), serde_json::json!("all"));
        assert_eq!(json(Kind::Allies, "return { 'DUL', 'CND' }").unwrap(), serde_json::json!(["DUL", "CND"]));
        assert_eq!(json(Kind::Provinces, "return {}").unwrap(), serde_json::json!([]));
        assert_eq!(json(Kind::UnitKind, "return 'infantry'").unwrap(), serde_json::json!("infantry"));
        assert_eq!(
            json(Kind::Terms, "return { { term = 'demand_provinces', province = 236 } }").unwrap(),
            serde_json::json!([{"term": "demand_provinces", "province": 236}])
        );
        for (kind, bad, wrong) in [
            (Kind::Province, "return 1.5", "a province such as 151, not a number"),
            (Kind::Boolean, "return 1", "a boolean such as true, not an integer"),
            (Kind::Countries, "return { 'FRA', 2 }", "a countries such as FRA,ENG, not a table"),
            (Kind::Allies, "return { 'DUL', true }", "an allies such as DUL,CND, not a table"),
            (Kind::Armies, "return { 12, {} }", "an armies such as 4294967377,4294967378, not a table"),
            (Kind::Provinces, "return { 151, x = 2 }", "a provinces such as 151,152, not a table"),
            (Kind::Terms, "return { term = 'humiliate' }", "a terms such as [{\"term\":\"humiliate\"}], not a table"),
            (Kind::Terms, "return 'humiliate'", "a terms such as [{\"term\":\"humiliate\"}], not a string"),
        ] {
            assert_eq!(json(kind, bad).unwrap_err(), wrong, "{bad}");
        }
    }
}
