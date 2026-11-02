//! An extension's Lua state: the sandbox (ADR 0024) and what the host puts
//! in it. Lua's own string, table, math, utf8 and coroutine libraries, the
//! base library without loading files or bytecode, `os.clock` (and
//! `os.time` and `os.date` when the manifest declares `clock`); the
//! framework's globals `log`, `json`, `mod_files`, `text` and `spawn`, and
//! `require` for the mod's own modules; and `game`.
use super::values::{self, date_text};
use crate::{
    game::{interface, map_coloring, Caller},
    lang,
    mods::Permissions,
};
use eu4_harmony_remote::{capabilities, Capability, Kind};
use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Table, Value as LuaValue, Variadic};
use serde_json::{Map, Number, Value};
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
};

/// A new extension's state, with `game` and the sentinel a capability that
/// waits returns to its wrapper.
pub struct Opened {
    pub lua: Lua,
    pub game: Table,
    pub wait: LuaValue,
    /// The state's main thread, for the time limit on plain calls.
    pub main_state: *const c_void,
}

/// The registry's name for the modules `require` has loaded.
const LOADED: &str = "eu4harmony.loaded";

/// The state of the extension `index` of the mod `id`, whose files are in
/// `harmony` (its `eu4-harmony/` folder).
pub fn open(index: usize, id: &str, harmony: &Path, permissions: &Permissions) -> mlua::Result<Opened> {
    let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE | StdLib::OS;
    let lua = Lua::new_with(libs, LuaOptions::default())?;
    let globals = lua.globals();

    // os: the clock the extension spends, and the real time when declared.
    let full: Table = globals.get("os")?;
    let os = lua.create_table()?;
    os.set("clock", full.get::<Function>("clock")?)?;
    if permissions.clock {
        os.set("time", full.get::<Function>("time")?)?;
        os.set("date", full.get::<Function>("date")?)?;
    }
    globals.set("os", os)?;

    // log and print: the extension's own log.
    let mod_id = id.to_string();
    let log = lua.create_function(move |lua, args: Variadic<LuaValue>| {
        let tostring: Function = lua.globals().get("tostring")?;
        let words = args.iter().map(|a| tostring.call::<String>(a.clone())).collect::<mlua::Result<Vec<_>>>()?;
        super::extension_log(index, &mod_id, &words.join(" "));
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
    let mod_id = id.to_string();
    text.set(
        "get",
        lua.create_function(move |lua, (key, values): (String, Option<Table>)| {
            let Some(raw) = crate::loc::mod_text(&mod_id, &key) else {
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
    text.set("language", lua.create_function(|_, ()| Ok(lang::player().id()))?)?;
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
            let chunk = format!("@{}.lua", name.replace('.', "/"));
            let module = lua.load(source).set_name(chunk).set_mode(mlua::ChunkMode::Text).call::<LuaValue>(name.as_str())?;
            let module = if module.is_nil() { LuaValue::Boolean(true) } else { module };
            loaded.set(name.as_str(), module.clone())?;
            Ok(module)
        })?,
    )?;

    // spawn: a task of its own (ADR 0030).
    globals.set(
        "spawn",
        lua.create_function(move |_, function: Function| {
            if super::CURRENT.with(std::cell::Cell::get) != Some(index) {
                return Err(mlua::Error::runtime("spawn: a task is started only from a handler, not at the top of main.lua"));
            }
            match super::extension(index) {
                Some(e) => e.spawn(function),
                None => Err(mlua::Error::runtime("spawn: the extension is not loaded")),
            }
        })?,
    )?;

    // game, built by the prelude around the host's functions.
    let wait = LuaValue::Table(lua.create_table()?);
    let call = lua.create_function(move |lua, (name, args): (String, MultiValue)| super::call(lua, index, &name, args))?;
    let task = lua.create_function(|_, ()| Ok(super::current_task()))?;
    let names = lua.create_sequence_from(extension_capabilities().map(|c| c.name()))?;
    let set_element = lua.create_function(move |_, (element, property, value): (String, String, LuaValue)| set_element(index, &element, &property, value))?;
    let game: Table = lua.load(include_str!("prelude.lua")).set_name("@prelude").call((call, wait.clone(), task, names, set_element))?;
    let map = lua.create_table()?;
    map.set("paint", lua.create_function(move |lua, colors: Table| paint(lua, index, colors))?)?;
    map.set(
        "release",
        lua.create_function(move |lua, ()| match map_coloring::clear(Caller::Extension(index)) {
            Ok(()) => Ok(MultiValue::from_iter([LuaValue::Boolean(true)])),
            Err(e) => failure(lua, status(e.status), &e.message),
        })?,
    )?;
    game.set("map", map)?;
    let main_state = main_state(&lua);
    Ok(Opened { lua, game, wait, main_state })
}

/// The state's main thread, a `lua_State*`.
pub fn main_state(lua: &Lua) -> *const c_void {
    let mut found = std::ptr::null();
    let _ = unsafe { lua.exec_raw::<()>((), |state| found = state as *const c_void) };
    found
}

/// The capabilities extensions have, each named by its group and action.
fn extension_capabilities() -> impl Iterator<Item = Capability> {
    capabilities().into_iter().filter(|c| c.extensions && !c.action.is_empty() && c.group != "text")
}

/// `nil, err` for a failure, as capabilities answer one.
fn failure(lua: &Lua, code: &str, message: &str) -> mlua::Result<MultiValue> {
    let err = lua.create_table()?;
    err.set("code", code)?;
    err.set("message", message)?;
    Ok(MultiValue::from_iter([LuaValue::Nil, LuaValue::Table(err)]))
}

fn status(status: crate::game::Status) -> &'static str {
    use crate::game::Status;
    match status {
        Status::NotInGame => "not_in_game",
        Status::NotReady => "not_ready",
        Status::NotFound => "not_found",
        Status::Rejected => "rejected",
        Status::InvalidArgument => "invalid_argument",
        Status::Unavailable => "unavailable",
        Status::WrongContext | Status::Internal => "internal",
    }
}

/// A path under `root`: relative, with `/`, no `..`, drive or leading `/`.
fn within(root: &Path, path: &str) -> Result<PathBuf, String> {
    let parts: Vec<&str> = path.split('/').collect();
    let bad = path.is_empty() || path.contains('\\') || path.contains(':') || parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..");
    if bad {
        return Err(format!("{path:?} is not a relative path with / between its parts"));
    }
    Ok(parts.iter().fold(root.to_path_buf(), |at, part| at.join(part)))
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
fn set_element(index: usize, element: &str, property: &str, value: LuaValue) -> mlua::Result<()> {
    if super::CURRENT.with(std::cell::Cell::get) != Some(index) {
        return Err(mlua::Error::runtime(format!("game.ui.{element}: the game is reached only from a handler, not at the top of main.lua")));
    }
    let caller = Caller::Extension(index);
    let text = |v: &LuaValue| match v {
        LuaValue::String(s) => Ok(s.to_str()?.to_string()),
        LuaValue::Nil => Ok(String::new()),
        other => Err(mlua::Error::runtime(format!("game.ui.{element}.{property} takes a string, not a {}", other.type_name()))),
    };
    let flag = |v: &LuaValue| match v {
        LuaValue::Boolean(b) => Ok(*b),
        other => Err(mlua::Error::runtime(format!("game.ui.{element}.{property} takes true or false, not a {}", other.type_name()))),
    };
    let result = match property {
        "visible" => interface::set_visible(caller, element, flag(&value)?),
        "enabled" => interface::set_enabled(caller, element, flag(&value)?),
        "text" => interface::set_text(caller, element, &text(&value)?),
        "tooltip" => interface::set_tooltip(caller, element, &text(&value)?),
        other => return Err(mlua::Error::runtime(format!("game.ui.{element}.{other}: an element has visible, enabled, text and tooltip"))),
    };
    result.map_err(|e| mlua::Error::runtime(format!("game.ui.{element}.{property}: {}", e.message)))
}

/// `game.map.paint({ [province] = 0xRRGGBB or { r, g, b, a } })`.
fn paint(lua: &Lua, index: usize, colors: Table) -> mlua::Result<MultiValue> {
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
        list.push(map_coloring::MapColor { province, rgba });
    }
    if super::CURRENT.with(std::cell::Cell::get) != Some(index) {
        return Err(mlua::Error::runtime("game.map.paint: the game is reached only from a handler, not at the top of main.lua"));
    }
    match map_coloring::paint(Caller::Extension(index), &list) {
        Ok(()) => Ok(MultiValue::from_iter([LuaValue::Boolean(true)])),
        Err(e) => failure(lua, status(e.status), &e.message),
    }
}

/// A capability's request as JSON, from its Lua arguments: the required
/// ones in order, then the optional ones in a table.
pub fn request(name: &str, args: MultiValue) -> Result<Value, String> {
    let Some(capability) = extension_capabilities().find(|c| c.name() == name) else {
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
    Ok(Value::Object(fields))
}

/// One argument as JSON, as its kind is written there.
fn arg_json(kind: Kind, value: &LuaValue) -> Result<Value, String> {
    let wrong = || format!("a {} such as {}, not a {}", kind.name(), kind.example(), value.type_name());
    let integer = |v: &LuaValue| match v {
        LuaValue::Integer(i) => Some(*i),
        LuaValue::Number(n) if n.fract() == 0.0 => Some(*n as i64),
        _ => None,
    };
    let string = |v: &LuaValue| match v {
        LuaValue::String(s) => s.to_str().ok().map(|s| s.to_string()),
        _ => None,
    };
    match kind {
        Kind::Province | Kind::Integer => integer(value).map(|i| Value::Number(Number::from(i))).ok_or_else(wrong),
        Kind::Army => match (integer(value), string(value)) {
            (Some(i), _) => Ok(Value::String(i.to_string())),
            (None, Some(s)) => Ok(Value::String(s)),
            _ => Err(wrong()),
        },
        Kind::Boolean => match value {
            LuaValue::Boolean(b) => Ok(Value::Bool(*b)),
            _ => Err(wrong()),
        },
        Kind::Date => date_text(value).map(Value::String).ok_or_else(wrong),
        Kind::Country | Kind::Text | Kind::UnitKind => string(value).map(Value::String).ok_or_else(wrong),
        Kind::Allies => match value {
            LuaValue::String(_) => string(value).map(Value::String).ok_or_else(wrong),
            LuaValue::Table(t) => {
                let tags = t.clone().sequence_values::<String>().collect::<mlua::Result<Vec<_>>>().map_err(|e| e.to_string())?;
                Ok(Value::Array(tags.into_iter().map(Value::String).collect()))
            }
            _ => Err(wrong()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_under_their_root() {
        let root = Path::new(r"C:\mod\eu4-harmony");
        assert_eq!(within(root, "data/units.json").unwrap(), root.join("data").join("units.json"));
        for bad in ["", "../x", "data/../../x", "/x", r"data\x", "C:/x", "data//x", "./x"] {
            assert!(within(root, bad).is_err(), "{bad}");
        }
        assert_eq!(module_file(Path::new("lua"), "range.bfs").unwrap(), Path::new("lua").join("range").join("bfs.lua"));
        assert!(module_file(Path::new("lua"), "range..bfs").is_err() && module_file(Path::new("lua"), "a/b").is_err());
    }

    #[test]
    fn a_request_takes_required_arguments_in_order_and_options_in_a_table() {
        let lua = Lua::new();
        let args = |code: &str| lua.load(code).eval::<MultiValue>().unwrap();
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
        assert!(request("camera.look_at", args("return { height = 3 }")).unwrap_err().contains("no option height"));
        assert!(request("menu.load", args("return 'x'")).unwrap_err().contains("no such capability for extensions"));
    }
}
