//! The Lua host with a mod written to a temporary folder and no game: the
//! sandbox, handlers and tasks, errors and the time limit, and capabilities
//! answering as they do outside a game.
use super::*;
use crate::mods::{Descriptor, Manifest};

/// A mod in its own folder, with `main.lua` and the other files given.
fn write_mod(name: &str, main: &str, others: &[(&str, &str)]) -> DiscoveredMod {
    let dir = std::env::temp_dir().join(format!("eu4h-lua-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (file, text) in [("eu4-harmony/lua/main.lua", main)].iter().chain(others) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let manifest_text = others.iter().find(|(f, _)| *f == "eu4-harmony/manifest.toml").map_or("id = \"t\"\napi_version = \"0.2\"", |(_, t)| *t);
    DiscoveredMod {
        descriptor_file: dir.join("t.mod"),
        descriptor: Descriptor { name: "T".into(), path: Some(dir.clone()), archive: None },
        layout: eu4_harmony_manifest::layout(&dir.join("eu4-harmony")),
        manifest: Manifest::parse(manifest_text).unwrap(),
        dir,
    }
}

fn load_one(name: &str, main: &str, others: &[(&str, &str)]) -> Result<Extension, String> {
    open(0, &write_mod(name, main, others))
}

/// The value of the global `name` once `handler` ran.
fn after(e: &Extension, handler: &str, name: &str) -> LuaValue {
    e.handle(handler, none);
    e.lua.globals().get(name).unwrap()
}

fn text(value: LuaValue) -> String {
    match value {
        LuaValue::String(s) => s.to_str().unwrap().to_string(),
        other => format!("{other:?}"),
    }
}

fn errors(e: &Extension) -> Vec<String> {
    let mut keys: Vec<String> = e.state().errors.keys().cloned().collect();
    keys.sort();
    keys
}

#[test]
fn the_sandbox_has_lua_s_own_libraries_and_nothing_of_the_system() {
    let e = load_one(
        "sandbox",
        r#"
        local seen = {}
        for _, name in ipairs({ "io", "debug", "package", "dofile", "loadfile", "collectgarbage" }) do
          seen[#seen + 1] = name .. "=" .. type(_G[name])
        end
        seen[#seen + 1] = "string.dump=" .. type(string.dump)
        seen[#seen + 1] = "os.clock=" .. type(os.clock) .. " os.time=" .. type(os.time) .. " os.execute=" .. type(os.execute)
        seen[#seen + 1] = "bytecode=" .. tostring((load("\27Lua", "x")))
        SEEN = table.concat(seen, " ")
        return {}
        "#,
        &[],
    )
    .unwrap();
    assert_eq!(
        text(e.lua.globals().get("SEEN").unwrap()),
        "io=nil debug=nil package=nil dofile=nil loadfile=nil collectgarbage=function string.dump=nil os.clock=function os.time=nil os.execute=nil bytecode=nil"
    );
}

#[test]
fn the_clock_is_there_when_declared() {
    let manifest = "id = \"t\"\napi_version = \"0.2\"\npermissions = { clock = true }";
    let e = load_one("clock", "TIME = type(os.time) .. type(os.date) return {}", &[("eu4-harmony/manifest.toml", manifest)]).unwrap();
    assert_eq!(text(e.lua.globals().get("TIME").unwrap()), "functionfunction");
}

#[test]
fn require_loads_the_mod_s_own_modules_once() {
    let e = load_one(
        "require",
        r#"
        local a = require("range.bfs")
        local b = require("range.bfs")
        SAME = a == b and a.depth == 5
        BAD = select(2, pcall(require, "../main"))
        return {}
        "#,
        &[("eu4-harmony/lua/range/bfs.lua", "LOADS = (LOADS or 0) + 1 return { depth = 5 }")],
    )
    .unwrap();
    let globals = e.lua.globals();
    assert_eq!((globals.get::<bool>("SAME").unwrap(), globals.get::<i64>("LOADS").unwrap()), (true, 1));
    assert!(text(globals.get("BAD").unwrap()).contains("words of letters, digits and _ between dots"));
}

#[test]
fn mod_files_and_json_read_what_the_mod_ships() {
    let e = load_one(
        "files",
        r#"
        local units = json.decode(mod_files.read("data/units.json"))
        UNITS = units.infantry .. " " .. json.encode({ ids = { 545460846646 } })
        local missing, err = mod_files.read("data/none.json")
        MISSING = tostring(missing) .. " " .. err.code
        ESCAPE = select(2, pcall(mod_files.read, "../manifest.toml"))
        return {}
        "#,
        &[("eu4-harmony/data/units.json", r#"{"infantry": 12}"#)],
    )
    .unwrap();
    let globals = e.lua.globals();
    assert_eq!(text(globals.get("UNITS").unwrap()), r#"12 {"ids":[545460846646]}"#);
    assert_eq!(text(globals.get("MISSING").unwrap()), "nil not_found");
    assert!(text(globals.get("ESCAPE").unwrap()).contains("not a relative path"));
}

#[test]
fn main_lua_returns_its_handlers_and_reaches_no_game() {
    assert!(load_one("nothing", "return 5", &[]).err().unwrap().contains("returned integer instead of a table of handlers"));
    assert!(load_one("broken", "return {", &[]).err().unwrap().contains("main.lua"));
    let e = load_one("top", "OK, WHY = pcall(function() return game end) return {}", &[]).unwrap();
    assert_eq!(e.lua.globals().get::<LuaValue>("WHY").unwrap(), LuaValue::Nil);
    let top = load_one("top-call", "return { on_frame = function(game) GAME = game end }", &[]).unwrap();
    let game: Table = after(&top, "on_frame", "GAME").as_table().cloned().unwrap();
    // Called from outside a handler, a capability is an error.
    let session: Table = game.get("session").unwrap();
    let get: Function = session.get("get").unwrap();
    assert!(get.call::<LuaValue>(()).unwrap_err().to_string().contains("only from a handler"));
}

#[test]
fn a_capability_outside_a_game_answers_nil_and_why() {
    let e = load_one(
        "capability",
        r#"
        return {
          on_frame = function(game)
            local session, err = game.session.get()
            ANSWER = tostring(session) .. " " .. err.code
            WRONG = select(2, pcall(game.army.move, 1, "x"))
          end,
        }
        "#,
        &[],
    )
    .unwrap();
    // Off the game thread, with no game: refused, never an error.
    let answer = text(after(&e, "on_frame", "ANSWER"));
    assert!(answer == "nil not_in_game" || answer == "nil internal", "{answer}");
    assert!(text(e.lua.globals().get("WRONG").unwrap()).contains("province: a province such as 151, not a string"));
}

#[test]
fn an_error_ends_the_call_and_the_next_event_calls_again() {
    let e = load_one("errors", "return { on_frame = function(game) CALLS = (CALLS or 0) + 1 error('boom') end }", &[]).unwrap();
    for _ in 0..5 {
        e.handle("on_frame", none);
    }
    assert_eq!(e.lua.globals().get::<i64>("CALLS").unwrap(), 5);
    let keys = errors(&e);
    assert_eq!(keys.len(), 1, "{keys:?}");
    assert!(keys[0].starts_with("on_frame: ") && keys[0].contains("boom"), "{keys:?}");
    assert_eq!(e.state().errors[&keys[0]], 5);
    assert!(e.state().stopped.is_none());
    e.session_over();
    assert!(e.state().errors.is_empty());
}

#[test]
fn spawn_starts_a_task_only_from_a_handler() {
    let e = load_one(
        "spawn",
        r#"
        TOP = select(2, pcall(spawn, function() end))
        return { on_frame = function(game) spawn(function() SPAWNED = true end) AFTER = SPAWNED end }
        "#,
        &[],
    )
    .unwrap();
    assert!(text(e.lua.globals().get("TOP").unwrap()).contains("only from a handler"));
    // The extension is not in the host's list in a test, so spawn says so;
    // a loaded one runs the function at once.
    e.handle("on_frame", none);
    assert!(errors(&e).iter().any(|k| k.contains("spawn: the extension is not loaded")), "{:?}", errors(&e));
}

#[test]
fn an_element_is_set_and_never_read() {
    let e = load_one("element", "return { on_frame = function(game) local _ = game.ui.button.visible end }", &[]).unwrap();
    e.handle("on_frame", none);
    assert!(errors(&e).iter().any(|k| k.contains("game.ui.button.visible is set, not read")), "{:?}", errors(&e));
}

#[test]
fn the_time_limit_stops_a_handler_and_three_in_a_row_the_extension() {
    let e = load_one("forever", "return { on_frame = function(game) while true do end end }", &[]).unwrap();
    let started = Instant::now();
    e.handle("on_frame", none);
    assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    assert!(errors(&e).iter().any(|k| k.contains(watch::STOPPED)), "{:?}", errors(&e));
    assert_eq!(e.state().timeouts, 1);
    e.handle("on_frame", none);
    e.handle("on_frame", none);
    assert!(e.state().stopped.as_deref().is_some_and(|r| r.contains("time limit")));
    // A stopped extension is not called again.
    let before = e.state().stretches;
    e.handle("on_frame", none);
    assert_eq!(e.state().stretches, before);
}

#[test]
fn a_yield_of_the_mod_s_own_outside_its_coroutines_is_an_error() {
    let e = load_one("yield", "return { on_frame = function(game) coroutine.yield(1) end }", &[]).unwrap();
    e.handle("on_frame", none);
    assert!(errors(&e).iter().any(|k| k.contains("yielded outside a coroutine")), "{:?}", errors(&e));
    // Its own coroutines yield as Lua's do.
    let own =
        load_one("own-coroutine", "return { on_frame = function(game) local c = coroutine.wrap(function() coroutine.yield(7) end) SEVEN = c() end }", &[])
            .unwrap();
    assert_eq!(after(&own, "on_frame", "SEVEN"), LuaValue::Integer(7));
}
