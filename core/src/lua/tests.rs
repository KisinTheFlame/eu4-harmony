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
        descriptor: Descriptor { name: "T".into(), version: None, path: Some(dir.clone()), archive: None },
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
        "io=nil debug=nil package=nil dofile=nil loadfile=nil collectgarbage=function string.dump=nil os.clock=function os.time=function os.execute=nil bytecode=nil"
    );
}

#[test]
fn the_clock_tells_the_time_only_when_declared() {
    let manifest = "id = \"t\"\napi_version = \"0.2\"\npermissions = { clock = true }";
    let e = load_one("clock", "TIME = math.type(os.time()) .. ' ' .. os.date('%Y'):len() return {}", &[("eu4-harmony/manifest.toml", manifest)]).unwrap();
    assert_eq!(text(e.lua.globals().get("TIME").unwrap()), "integer 4");
    let undeclared =
        load_one("no-clock", "WHY = tostring(select(2, pcall(os.time))) .. ' | ' .. tostring(select(2, pcall(os.date, '%Y'))) return {}", &[]).unwrap();
    let why = text(undeclared.lua.globals().get("WHY").unwrap());
    assert!(why.contains("os.time: the manifest does not declare the clock permission (permissions = { clock = true }"), "{why}");
    assert!(why.contains("os.date: the manifest does not declare the clock permission"), "{why}");
}

/// The folder data folders are made in, in a test.
fn data_dir() -> PathBuf {
    DATA_DIR.get_or_init(|| std::env::temp_dir().join(format!("eu4h-lua-data-{}", std::process::id()))).clone()
}

#[test]
fn files_keep_the_extension_s_data_in_its_own_folder() {
    let folder = data_dir().join("keeper");
    let _ = std::fs::remove_dir_all(&folder);
    let manifest = "id = \"keeper\"\napi_version = \"0.2\"\npermissions = { files = true }";
    let e = load_one(
        "data",
        r#"
        local seen = {}
        seen[#seen + 1] = table.concat(files.list(), ",")
        local missing, err = files.read("cache/routes.json")
        seen[#seen + 1] = tostring(missing) .. " " .. err.code
        seen[#seen + 1] = tostring(files.write("cache/routes.json", json.encode({ 151, 152 })))
        files.write("cache/routes.json", "[151]")
        files.write("notes.txt", "x")
        seen[#seen + 1] = files.read("cache/routes.json")
        seen[#seen + 1] = table.concat(files.list(), ",") .. " " .. table.concat(files.list("cache"), ",")
        seen[#seen + 1] = tostring(files.remove("cache/routes.json")) .. " " .. select(2, files.remove("cache/routes.json")).code
        seen[#seen + 1] = tostring(files.remove("cache")) .. " " .. table.concat(files.list(), ",")
        seen[#seen + 1] = tostring(select(2, pcall(files.write, "../escape.txt", "x")))
        seen[#seen + 1] = tostring(select(2, pcall(files.read, "nul")))
        SEEN = table.concat(seen, " | ")
        return {}
        "#,
        &[("eu4-harmony/manifest.toml", manifest)],
    )
    .unwrap();
    let seen = text(e.lua.globals().get("SEEN").unwrap());
    let parts: Vec<&str> = seen.split(" | ").collect();
    assert_eq!(parts[..7], ["", "nil not_found", "true", "[151]", "cache/,notes.txt routes.json", "true not_found", "true notes.txt"], "{seen}");
    assert!(parts[7].contains("not a relative path of file names") && parts[8].contains("not a relative path of file names"), "{seen}");
    assert_eq!(std::fs::read_to_string(folder.join("notes.txt")).unwrap(), "x");
    let _ = std::fs::remove_dir_all(&folder);

    let undeclared = load_one("no-data", "WHY = select(2, pcall(files.read, 'x')) return {}", &[]).unwrap();
    let why = text(undeclared.lua.globals().get("WHY").unwrap());
    assert!(why.contains("files.read: the manifest does not declare the files permission (permissions = { files = true }"), "{why}");
}

/// A server on this machine that answers one request with `answer` (or
/// never, for none) and tells what it was asked.
fn serve_once(answer: Option<&'static str>) -> (u16, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tell, asked) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        while let Ok(n) = stream.read(&mut buffer) {
            request.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&request).to_string();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let length = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()));
                if body.len() >= length.unwrap_or(0) {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        let _ = tell.send(String::from_utf8_lossy(&request).to_string());
        match answer {
            Some(answer) => {
                let _ = stream.write_all(answer.as_bytes());
            }
            None => std::thread::sleep(Duration::from_secs(5)),
        }
    });
    (port, asked)
}

/// Polls `e`'s waiting tasks until the global `name` is set.
fn poll_until(e: &Extension, name: &str) -> LuaValue {
    let started = Instant::now();
    loop {
        e.poll();
        let value: LuaValue = e.lua.globals().get(name).unwrap();
        if !value.is_nil() || started.elapsed() > Duration::from_secs(10) {
            return value;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_request_waits_for_its_response_and_the_task_goes_on() {
    let (port, asked) =
        serve_once(Some("HTTP/1.1 201 Created\r\nX-Rank: 3\r\nSet-Cookie: a=1\r\nset-cookie: b=2\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"));
    let manifest = format!("id = \"t\"\napi_version = \"0.2\"\npermissions = {{ network = [\"127.0.0.1:{port}\"] }}");
    let main = format!(
        r#"
        return {{
          on_frame = function(game)
            local response, err = http.fetch("http://127.0.0.1:{port}/score?x=1", {{
              method = "post", headers = {{ ["Content-Type"] = "application/json" }}, body = '{{"a":1}}', timeout = 5,
            }})
            GOT = response.status .. " " .. response.headers["x-rank"] .. " " .. response.headers["set-cookie"] .. " " .. response.body
          end,
        }}
        "#
    );
    let e = load_one("fetch", &main, &[("eu4-harmony/manifest.toml", &manifest)]).unwrap();
    e.handle("on_frame", none);
    assert_eq!(e.state().tasks.len(), 1, "{:?}", errors(&e));
    assert_eq!(text(poll_until(&e, "GOT")), "201 3 a=1, b=2 hello", "{:?}", errors(&e));
    let request = asked.recv().unwrap();
    assert!(request.starts_with("POST /score?x=1 HTTP/1.1\r\n"), "{request}");
    // Header names are sent in lowercase, which HTTP allows.
    assert!(request.to_ascii_lowercase().contains("content-type: application/json\r\n") && request.ends_with("{\"a\":1}"), "{request}");
    assert!(e.state().tasks.is_empty());
}

#[test]
fn a_request_with_no_answer_in_time_fails_with_timeout() {
    let (port, _asked) = serve_once(None);
    let manifest = format!("id = \"t\"\napi_version = \"0.2\"\npermissions = {{ network = [\"127.0.0.1:{port}\"] }}");
    let main = format!("return {{ on_frame = function(game) local r, err = http.fetch('http://127.0.0.1:{port}/', {{ timeout = 0.3 }}) GOT = tostring(r) .. ' ' .. err.code end }}");
    let e = load_one("fetch-timeout", &main, &[("eu4-harmony/manifest.toml", &manifest)]).unwrap();
    e.handle("on_frame", none);
    assert_eq!(text(poll_until(&e, "GOT")), "nil timeout");
}

#[test]
fn a_request_the_manifest_does_not_allow_is_an_error() {
    let manifest = "id = \"t\"\napi_version = \"0.2\"\npermissions = { network = [\"api.example.com\"] }";
    let e = load_one(
        "fetch-refused",
        r#"
        TOP = select(2, pcall(http.fetch, "https://api.example.com/"))
        return {
          on_frame = function(game)
            local seen = {}
            for _, call in ipairs({
              { "https://example.com/" }, { "https://api.example.com:8443/" }, { "ftp://api.example.com/" },
              { "https://api.example.com/", { timeout = 301 } }, { "https://api.example.com/", { retries = 2 } },
            }) do
              seen[#seen + 1] = tostring(select(2, pcall(http.fetch, call[1], call[2])))
            end
            SEEN = table.concat(seen, " | ")
          end,
        }
        "#,
        &[("eu4-harmony/manifest.toml", manifest)],
    )
    .unwrap();
    assert!(text(e.lua.globals().get("TOP").unwrap()).contains("http.fetch waits for its answer, which it cannot do here"));
    let seen = text(after(&e, "on_frame", "SEEN"));
    let parts: Vec<&str> = seen.split(" | ").collect();
    assert!(parts[0].contains("does not list (api.example.com)") && parts[1].contains("does not list"), "{seen}");
    assert!(parts[2].contains("begins with http:// or https://"), "{seen}");
    assert!(parts[3].contains("timeout is more than 0 and at most 300 seconds") && parts[4].contains("has no option retries"), "{seen}");
    assert!(e.state().tasks.is_empty() && e.waits().by_number.is_empty());

    let undeclared = load_one("no-network", "return { on_frame = function(game) WHY = select(2, pcall(http.fetch, 'https://x.com/')) end }", &[]).unwrap();
    let why = text(after(&undeclared, "on_frame", "WHY"));
    assert!(why.contains("http.fetch: the manifest does not declare the network permission"), "{why}");
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

#[test]
fn a_redirect_is_a_response_and_not_followed() {
    let (port, _asked) = serve_once(Some("HTTP/1.1 302 Found\r\nLocation: http://example.com/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"));
    let manifest = format!("id = \"t\"\napi_version = \"0.2\"\npermissions = {{ network = [\"127.0.0.1:{port}\"] }}");
    let main =
        format!("return {{ on_frame = function(game) local r = http.fetch('http://127.0.0.1:{port}/') GOT = r.status .. ' ' .. r.headers.location end }}");
    let e = load_one("fetch-redirect", &main, &[("eu4-harmony/manifest.toml", &manifest)]).unwrap();
    e.handle("on_frame", none);
    assert_eq!(text(poll_until(&e, "GOT")), "302 http://example.com/", "{:?}", errors(&e));
}
