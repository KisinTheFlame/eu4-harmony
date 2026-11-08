//! The Lua host with a mod written to a temporary folder and a host that
//! records what the extension does: the sandbox, handlers and tasks, errors
//! and the time limit, and capabilities answering as they do outside a game.
use super::*;
use eu4_harmony_remote::Error;

/// Where a test puts a request's answer.
type Answered = Arc<Mutex<Option<fetch::Answer>>>;

/// A host with no game: capabilities answer `not_in_game`, requests wait
/// until a test answers them, and the rest is recorded.
#[derive(Default)]
struct TestHost {
    log: Mutex<Vec<String>>,
    elements: Mutex<Vec<(String, Element)>>,
    requests: Mutex<Vec<(fetch::Request, Answered)>>,
    told: Mutex<Vec<String>>,
    /// For each capability run, whether it ran inside an extension's call.
    inside: Mutex<Vec<bool>>,
    /// The mod's texts by key.
    texts: Vec<(&'static str, &'static str)>,
    windows: Mutex<Vec<WindowChange>>,
    /// The pop-ups shown, each with the slot its answer is put in.
    popups: Mutex<Vec<(Popup, PopupAnswer)>>,
}

/// Where a test puts a pop-up's answer.
type PopupAnswer = Arc<Mutex<Option<super::Answered>>>;

/// A pop-up's answer, once a test gives it.
struct PopupSlot(PopupAnswer);

impl Pending<super::Answered> for PopupSlot {
    fn poll(&mut self) -> Option<super::Answered> {
        self.0.lock().unwrap().take()
    }
}

/// A request's answer, once a test gives it.
struct Slot(Answered);

impl Pending<fetch::Answer> for Slot {
    fn poll(&mut self) -> Option<fetch::Answer> {
        self.0.lock().unwrap().take()
    }
}

impl Host for TestHost {
    fn run(&self, _: usize, _: Request) -> Called {
        self.inside.lock().unwrap().push(calling());
        Called::Done(Err(Error::new(Code::NotInGame, "no game in a test")))
    }
    fn set_element(&self, _: usize, element: &str, value: Element) -> Result<(), String> {
        self.elements.lock().unwrap().push((element.to_string(), value));
        Ok(())
    }
    fn window(&self, _: usize, change: WindowChange) -> Result<(), String> {
        self.windows.lock().unwrap().push(change);
        Ok(())
    }
    fn popup(&self, _: usize, popup: Popup) -> Result<Box<dyn Pending<super::Answered>>, String> {
        let slot = Arc::new(Mutex::new(None));
        self.popups.lock().unwrap().push((popup, slot.clone()));
        Ok(Box::new(PopupSlot(slot)))
    }
    fn close_popup(&self, _: usize, id: &str) -> Result<(), String> {
        for (popup, slot) in self.popups.lock().unwrap().iter() {
            if popup.id.as_deref() == Some(id) {
                *slot.lock().unwrap() = Some(Ok(Value::String("closed".into())));
            }
        }
        Ok(())
    }
    fn paint(&self, _: usize, _: &[(u32, [u8; 4])]) -> Result<(), Error> {
        Ok(())
    }
    fn release_map(&self, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn text(&self, _: &str, key: &str) -> Option<String> {
        self.texts.iter().find(|(k, _)| *k == key).map(|(_, t)| t.to_string())
    }
    fn language(&self) -> String {
        "english".into()
    }
    fn fetch(&self, request: fetch::Request) -> Box<dyn Pending<fetch::Answer>> {
        let slot = Arc::new(Mutex::new(None));
        self.requests.lock().unwrap().push((request, slot.clone()));
        Box::new(Slot(slot))
    }
    fn log(&self, _: &str, line: &str) {
        self.log.lock().unwrap().push(line.to_string());
    }
    fn session(&self) -> u64 {
        1
    }
    fn first_error(&self, extension: &Extension, from: &str, _: &str) {
        self.told.lock().unwrap().push(format!("{}: {from}", extension.id()));
    }
    fn stopped(&self, extension: &Extension, reason: &str) {
        self.told.lock().unwrap().push(format!("{} stopped: {reason}", extension.id()));
    }
}

/// A mod in its own folder, with `main.lua` and the other files given, and
/// its data folder beside it.
fn write_mod(name: &str, main: &str, others: &[(&str, &str)]) -> (PathBuf, eu4_harmony_manifest::Manifest) {
    let dir = std::env::temp_dir().join(format!("eu4h-lua-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (file, text) in [("eu4-harmony/lua/main.lua", main)].iter().chain(others) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let manifest_text = others.iter().find(|(f, _)| *f == "eu4-harmony/manifest.toml").map_or("id = \"t\"\napi_version = \"0.3\"", |(_, t)| *t);
    (dir, eu4_harmony_manifest::Manifest::parse(manifest_text).unwrap())
}

/// The mod's extension, opened on a host of its own.
fn load_with(name: &str, main: &str, others: &[(&str, &str)]) -> Result<(Arc<Extension>, Arc<TestHost>), String> {
    let (dir, manifest) = write_mod(name, main, others);
    let host = Arc::new(TestHost::default());
    let m = Mod { id: &manifest.id, name: "T", harmony: dir.join("eu4-harmony"), permissions: &manifest.permissions, data: Some(dir.join("data")) };
    open(0, &m, host.clone()).map(|e| (e, host))
}

fn load_one(name: &str, main: &str, others: &[(&str, &str)]) -> Result<Arc<Extension>, String> {
    load_with(name, main, others).map(|(e, _)| e)
}

/// The value of the global `name` once `handler` ran.
fn after(e: &Extension, handler: &str, name: &str) -> LuaValue {
    e.handle(handler, &[]);
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
        -- Text loaded without an environment sees the globals; with one, only it.
        seen[#seen + 1] = "text=" .. load("return type(string)")() .. "," .. tostring(load("return x", "x", "t", { x = 1 })())
        SEEN = table.concat(seen, " ")
        return {}
        "#,
        &[],
    )
    .unwrap();
    assert_eq!(
        text(e.lua.globals().get("SEEN").unwrap()),
        "io=nil debug=nil package=nil dofile=nil loadfile=nil collectgarbage=function string.dump=nil os.clock=function os.time=function os.execute=nil bytecode=nil text=table,1"
    );
}

#[test]
fn the_clock_tells_the_time_only_when_declared() {
    let manifest = "id = \"t\"\napi_version = \"0.3\"\npermissions = { clock = true }";
    let e = load_one("clock", "TIME = math.type(os.time()) .. ' ' .. os.date('%Y'):len() return {}", &[("eu4-harmony/manifest.toml", manifest)]).unwrap();
    assert_eq!(text(e.lua.globals().get("TIME").unwrap()), "integer 4");
    let undeclared =
        load_one("no-clock", "WHY = tostring(select(2, pcall(os.time))) .. ' | ' .. tostring(select(2, pcall(os.date, '%Y'))) return {}", &[]).unwrap();
    let why = text(undeclared.lua.globals().get("WHY").unwrap());
    assert!(why.contains("os.time: the manifest does not declare the clock permission (permissions = { clock = true }"), "{why}");
    assert!(why.contains("os.date: the manifest does not declare the clock permission"), "{why}");
}

#[test]
fn files_keep_the_extension_s_data_in_its_own_folder() {
    let manifest = "id = \"keeper\"\napi_version = \"0.3\"\npermissions = { files = true }";
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
    let folder = std::env::temp_dir().join(format!("eu4h-lua-data-{}", std::process::id())).join("data");
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
    let manifest = "id = \"t\"\napi_version = \"0.3\"\npermissions = { network = [\"api.example.com\"] }";
    let main = r#"
        return {
          on_frame = function(game)
            local response, err = http.fetch("https://api.example.com/score?x=1", {
              method = "post", headers = { ["Content-Type"] = "application/json" }, body = '{"a":1}', timeout = 5,
            })
            GOT = response.status .. " " .. response.headers["x-rank"] .. " " .. response.body
          end,
        }
        "#;
    let (e, host) = load_with("fetch", main, &[("eu4-harmony/manifest.toml", manifest)]).unwrap();
    e.handle("on_frame", &[]);
    assert_eq!(e.waiting(), 1, "{:?}", errors(&e));
    let (request, slot) = host.requests.lock().unwrap().pop().unwrap();
    assert_eq!(
        (request.target.url(), request.method.as_str(), request.body.as_slice()),
        ("https://api.example.com:443/score?x=1".to_string(), "POST", &b"{\"a\":1}"[..])
    );
    assert_eq!(request.headers, [("Content-Type".to_string(), "application/json".to_string())]);
    // Until its answer comes, the task waits.
    e.poll();
    assert_eq!(e.waiting(), 1);
    *slot.lock().unwrap() = Some(Ok(fetch::Response { status: 201, headers: vec![("x-rank".into(), "3".into())], body: b"hello".to_vec() }));
    e.poll();
    assert_eq!(text(e.lua.globals().get("GOT").unwrap()), "201 3 hello", "{:?}", errors(&e));
    assert_eq!(e.waiting(), 0);
}

#[test]
fn a_request_with_no_answer_in_time_fails_with_timeout() {
    let manifest = "id = \"t\"\napi_version = \"0.3\"\npermissions = { network = [\"api.example.com\"] }";
    let main = "return { on_frame = function(game) local r, err = http.fetch('https://api.example.com/', { timeout = 0.3 }) GOT = tostring(r) .. ' ' .. err.code end }";
    let e = load_one("fetch-timeout", main, &[("eu4-harmony/manifest.toml", manifest)]).unwrap();
    e.handle("on_frame", &[]);
    assert_eq!(text(poll_until(&e, "GOT")), "nil timeout");
}

#[test]
fn a_request_the_manifest_does_not_allow_is_an_error() {
    let manifest = "id = \"t\"\napi_version = \"0.3\"\npermissions = { network = [\"api.example.com\"] }";
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
    assert!(e.state().tasks.is_empty() && e.waits.lock().unwrap().by_number.is_empty());

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
fn a_capability_runs_inside_its_extension_s_call() {
    // The host is told so, as what the capability makes happen (a console
    // command loading a save, say) must not call extensions again within.
    let (e, host) = load_with("inside", "return { on_frame = function(game) game.session.get() end }", &[]).unwrap();
    assert!(!calling());
    e.handle("on_frame", &[]);
    assert!(!calling());
    assert_eq!(*host.inside.lock().unwrap(), [true]);
}

#[test]
fn an_error_ends_the_call_and_the_next_event_calls_again() {
    let e = load_one("errors", "return { on_frame = function(game) CALLS = (CALLS or 0) + 1 error('boom') end }", &[]).unwrap();
    for _ in 0..5 {
        e.handle("on_frame", &[]);
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
    // From a handler, the function runs at once.
    assert_eq!(after(&e, "on_frame", "AFTER"), LuaValue::Boolean(true), "{:?}", errors(&e));
}

#[test]
fn an_element_is_set_and_never_read() {
    let e = load_one("element", "return { on_frame = function(game) local _ = game.ui.button.visible end }", &[]).unwrap();
    e.handle("on_frame", &[]);
    assert!(errors(&e).iter().any(|k| k.contains("game.ui.button.visible is set, not read")), "{:?}", errors(&e));
}

#[test]
fn the_time_limit_stops_a_handler_and_three_in_a_row_the_extension() {
    let e = load_one("forever", "return { on_frame = function(game) while true do end end }", &[]).unwrap();
    let started = Instant::now();
    e.handle("on_frame", &[]);
    assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    assert!(errors(&e).iter().any(|k| k.contains(watch::STOPPED)), "{:?}", errors(&e));
    assert_eq!(e.state().timeouts, 1);
    e.handle("on_frame", &[]);
    e.handle("on_frame", &[]);
    assert!(e.state().stopped.as_deref().is_some_and(|r| r.contains("time limit")));
    // A stopped extension is not called again.
    let before = e.state().all.count;
    e.handle("on_frame", &[]);
    assert_eq!(e.state().all.count, before);
}

/// Whether `e`'s handler `name`, run on a thread of its own, returns within
/// a few seconds: a handler the time limit does not stop runs on there
/// instead of hanging the tests.
fn returns_in_time(e: &Arc<Extension>, name: &str) -> bool {
    let (e, name, (returned, came)) = (e.clone(), name.to_string(), std::sync::mpsc::channel());
    std::thread::spawn(move || {
        e.handle(&name, &[]);
        let _ = returned.send(());
    });
    came.recv_timeout(Duration::from_secs(5)).is_ok()
}

#[test]
fn running_out_of_the_extension_s_memory_is_an_error() {
    let e = load_one(
        "memory",
        r#"
        return {
          on_big = function(game) BIG = #string.rep("x", 300 * 1024 * 1024) end,
          on_grow = function(game)
            local kept = {}
            for i = 1, 1e6 do kept[i] = string.rep("x", 1024 * 1024) end
          end,
          on_day = function(game) DAY = #string.rep("x", 1024 * 1024) end,
        }
        "#,
        &[],
    )
    .unwrap();
    let out_of_memory = |handler: &str| errors(&e).iter().any(|k| k.starts_with(handler) && k.contains("not enough memory"));
    e.handle("on_big", &[]);
    assert!(out_of_memory("on_big"), "{:?}", errors(&e));
    assert_eq!(e.lua.globals().get::<LuaValue>("BIG").unwrap(), LuaValue::Nil);
    // Growing to the limit a little at a time, under a lower limit to be
    // quick: the memory a test fills is new to the process, and slow.
    assert_eq!(e.lua.set_memory_limit(32 * 1024 * 1024).unwrap(), sandbox::MEMORY);
    e.handle("on_grow", &[]);
    assert!(out_of_memory("on_grow"), "{:?}", errors(&e));
    assert!(e.lua.used_memory() <= 32 * 1024 * 1024, "{}", e.lua.used_memory());
    // Once what held it is gone, the memory is the extension's again.
    assert_eq!(after(&e, "on_day", "DAY"), LuaValue::Integer(1024 * 1024), "{:?}", errors(&e));
    assert!(e.state().stopped.is_none());
}

#[test]
fn a_stopped_extension_keeps_no_waiting_tasks() {
    let manifest = "id = \"t\"\napi_version = \"0.3\"\npermissions = { network = [\"api.example.com\"] }";
    let main = r#"
        return {
          on_wait = function(game) http.fetch("https://api.example.com/never") end,
          on_spin = function(game) http.fetch("https://api.example.com/spin") while true do end end,
        }
        "#;
    let (e, host) = load_with("stopped-tasks", main, &[("eu4-harmony/manifest.toml", manifest)]).unwrap();
    e.handle("on_wait", &[]);
    for _ in 0..TIMEOUTS {
        e.handle("on_spin", &[]);
    }
    assert_eq!(e.waiting(), 1 + TIMEOUTS as usize, "{:?}", errors(&e));
    for (request, slot) in host.requests.lock().unwrap().iter() {
        if request.target.url().ends_with("/spin") {
            *slot.lock().unwrap() = Some(Ok(fetch::Response { status: 200, headers: vec![], body: vec![] }));
        }
    }
    // The task still waiting is looked at before the ones whose answers
    // came, which run past the time limit until the extension is stopped.
    e.poll();
    assert!(e.state().stopped.is_some());
    assert_eq!(e.waiting(), 0);
}

#[test]
fn the_time_limit_is_not_caught_by_pcall() {
    let e = load_one(
        "pcall-forever",
        r#"
        return {
          on_frame = function(game) while true do pcall(function() while true do end end) end end,
          on_day = function(game) DAY = true end,
        }
        "#,
        &[],
    )
    .unwrap();
    assert!(returns_in_time(&e, "on_frame"));
    assert!(errors(&e).iter().any(|k| k.contains(watch::STOPPED)), "{:?}", errors(&e));
    assert_eq!(e.state().timeouts, 1);
    // Once the stretch has ended, the next one runs as before.
    assert_eq!(after(&e, "on_day", "DAY"), LuaValue::Boolean(true), "{:?}", errors(&e));
}

#[test]
fn the_time_limit_holds_in_a_coroutine_of_the_mod_s_own() {
    let e = load_one(
        "coroutine-forever",
        r#"
        local spin = coroutine.create(function() coroutine.yield() while true do end end)
        return {
          on_wrap = function(game) coroutine.wrap(function() while true do end end)() end,
          on_resume = function(game) coroutine.resume(spin) coroutine.resume(spin) end,
          -- The error the coroutine ends with is caught, and the task goes on.
          on_caught = function(game)
            coroutine.resume(coroutine.create(function() while true do end end))
            while true do end
          end,
        }
        "#,
        &[],
    )
    .unwrap();
    for handler in ["on_wrap", "on_resume", "on_caught"] {
        assert!(returns_in_time(&e, handler), "{handler}");
        assert!(errors(&e).iter().any(|k| k.starts_with(handler) && k.contains(watch::STOPPED)), "{handler}: {:?}", errors(&e));
    }
    assert!(e.state().stopped.is_some());
}

#[test]
fn the_time_limit_holds_while_a_task_s_error_is_written() {
    let e = load_one(
        "tostring-forever",
        "return { on_frame = function(game) error(setmetatable({}, { __tostring = function() while true do end end })) end }",
        &[],
    )
    .unwrap();
    assert!(returns_in_time(&e, "on_frame"));
    assert!(errors(&e).iter().any(|k| k.contains(watch::STOPPED)), "{:?}", errors(&e));
}

#[test]
fn coroutines_of_the_mod_s_own_yield_resume_and_fail_as_lua_s_do() {
    let e = load_one(
        "coroutine-lua",
        r##"
        return {
          on_frame = function(game)
            local co = coroutine.wrap(function(a) local b = coroutine.yield(a + 1) return b * 2 end)
            local seen = { co(1), co(10) }
            local c = coroutine.create(function(...) coroutine.yield(select("#", ...)) error({ code = 7 }) end)
            seen[#seen + 1] = select(2, coroutine.resume(c, nil, nil))
            local ok, err = coroutine.resume(c)
            seen[#seen + 1] = tostring(ok) .. " " .. err.code .. " " .. coroutine.status(c)
            seen[#seen + 1] = select(2, coroutine.resume(c))
            seen[#seen + 1] = select(2, pcall(coroutine.wrap(function() error("boom") end)))
            seen[#seen + 1] = select(2, pcall(coroutine.resume, 5))
            SEEN = table.concat(seen, " | ")
          end,
        }
        "##,
        &[],
    )
    .unwrap();
    let seen = text(after(&e, "on_frame", "SEEN"));
    let parts: Vec<&str> = seen.split(" | ").collect();
    assert_eq!(parts[..5], ["2", "20", "2", "false 7 dead", "cannot resume dead coroutine"], "{seen}");
    assert!(parts[5].ends_with("boom") && parts[5].contains("main.lua"), "{seen}");
    assert!(parts[6].contains("bad argument #1") && parts[6].contains("thread expected, got number"), "{seen}");
}

#[test]
fn a_yield_of_the_mod_s_own_outside_its_coroutines_is_an_error() {
    let e = load_one("yield", "return { on_frame = function(game) coroutine.yield(1) end }", &[]).unwrap();
    e.handle("on_frame", &[]);
    assert!(errors(&e).iter().any(|k| k.contains("yielded outside a coroutine")), "{:?}", errors(&e));
    // Its own coroutines yield as Lua's do.
    let own =
        load_one("own-coroutine", "return { on_frame = function(game) local c = coroutine.wrap(function() coroutine.yield(7) end) SEVEN = c() end }", &[])
            .unwrap();
    assert_eq!(after(&own, "on_frame", "SEVEN"), LuaValue::Integer(7));
}

#[test]
fn the_sandbox_has_every_global_the_catalogue_describes_and_no_other() {
    use eu4_harmony_remote::lua::{FUNCTIONS, GLOBALS};
    use std::collections::BTreeSet;
    const LUA_S_OWN: &[&str] = &[
        "_G",
        "_VERSION",
        "assert",
        "collectgarbage",
        "error",
        "getmetatable",
        "ipairs",
        "load",
        "next",
        "pairs",
        "pcall",
        "rawequal",
        "rawget",
        "rawlen",
        "rawset",
        "select",
        "setmetatable",
        "tonumber",
        "tostring",
        "type",
        "warn",
        "xpcall",
        "string",
        "table",
        "math",
        "utf8",
        "coroutine",
        "os",
    ];
    let e = load_one("described", "return { on_frame = function(game) GAME = game end }", &[]).unwrap();
    let keys = |t: &Table| -> BTreeSet<String> { t.pairs::<String, LuaValue>().map(|p| p.unwrap().0).collect() };
    let globals: BTreeSet<String> = keys(&e.lua.globals()).into_iter().filter(|k| !LUA_S_OWN.contains(&k.as_str()) && k != "GAME").collect();
    assert_eq!(globals, GLOBALS.iter().map(|g| g.to_string()).collect::<BTreeSet<_>>());
    // Each global table holds the functions described under it.
    let under = |table: &str| -> BTreeSet<String> {
        FUNCTIONS.iter().filter_map(|f| f.name.strip_prefix(&format!("{table}.")).filter(|rest| !rest.contains('.')).map(String::from)).collect()
    };
    for table in GLOBALS {
        if let Ok(t) = e.lua.globals().get::<Table>(*table) {
            assert_eq!(keys(&t), under(table), "{table}");
        }
    }
    let os: Table = e.lua.globals().get("os").unwrap();
    assert_eq!(keys(&os), under("os"));
    let game: Table = after(&e, "on_frame", "GAME").as_table().cloned().unwrap();
    assert_eq!(keys(&game.get::<Table>("map").unwrap()), under("game.map"));
}

/// A feed that records when it is asked, and gives one mod save data.
#[derive(Default)]
struct TestFeed {
    asked: Vec<String>,
    lost: Vec<usize>,
    clicks: Vec<dispatch::Click>,
    windows: Vec<dispatch::WindowSeen>,
}

impl dispatch::Feed for TestFeed {
    fn session_begins(&mut self, session: u64) {
        self.asked.push(format!("session {session}"));
    }
    fn saved(&mut self, id: &str) -> Option<Value> {
        (id == "first").then(|| serde_json::json!({ "n": 7 }))
    }
    fn map_lost(&mut self) -> Vec<usize> {
        self.asked.push("map_lost".into());
        std::mem::take(&mut self.lost)
    }
    fn clicks(&mut self) -> Vec<dispatch::Click> {
        self.asked.push("clicks".into());
        std::mem::take(&mut self.clicks)
    }
    fn windows(&mut self) -> Vec<dispatch::WindowSeen> {
        self.asked.push("windows".into());
        std::mem::take(&mut self.windows)
    }
}

#[test]
fn a_frame_reaches_every_extension_in_its_order() {
    let main = r#"
        local function say(...) log(table.concat({ ... }, " ")) end
        return {
          on_session = function(game, saved) say(ID, "session", tostring(saved and saved.n)) end,
          on_day = function(game, date) say(ID, "day", date.year .. "." .. date.month .. "." .. date.day) end,
          on_month = function(game, date) say(ID, "month", date.month) end,
          on_army_ordered = function(game, army) say(ID, "ordered", math.type(army), army) end,
          on_battle = function(game, battle) say(ID, "battle", battle.winner, #battle.attackers) end,
          on_session_ended = function(game, reason) say(ID, "ended", reason) end,
          on_map_lost = function(game) say(ID, "map_lost") end,
          on_button = function(game, name, click) say(ID, "button", name, tostring(click.right), tostring(click.row)) end,
          on_window = function(game, name, open) say(ID, "window", name, tostring(open)) end,
          on_frame = function(game) say(ID, "frame") end,
        }
    "#;
    let host = Arc::new(TestHost::default());
    let mut opened = Vec::new();
    for (index, id) in ["first", "second"].into_iter().enumerate() {
        let (dir, manifest) = write_mod(&format!("dispatch-{id}"), &format!("ID = {id:?}\n{main}"), &[]);
        let m = Mod { id, name: id, harmony: dir.join("eu4-harmony"), permissions: &manifest.permissions, data: None };
        opened.push(open(index, &m, host.clone()).unwrap());
    }
    use eu4_harmony_remote::{events::*, ArmyId, CountryTag, Date, ProvinceId};
    let date = Date::parse("1444.12.1").unwrap();
    let tag = |t: &str| serde_json::from_value::<CountryTag>(serde_json::json!(t)).unwrap();
    let heard: Vec<dispatch::Heard> = [
        Event::Session { session: 3 },
        Event::ArmyOrdered { army: ArmyId(4294967377) },
        Event::Battle {
            province: ProvinceId(151),
            naval: false,
            attackers: vec![tag("TUR")],
            defenders: vec![tag("BYZ")],
            winner: Winner::Attackers,
            attacker_losses: 100,
            defender_losses: 900,
        },
        Event::Day { date },
        Event::Month { date },
    ]
    .iter()
    .map(dispatch::Heard::of)
    .collect();
    let mut feed = TestFeed {
        lost: vec![1],
        clicks: vec![dispatch::Click { extension: 0, button: "go".into(), right: true, shift: false, ctrl: false, row: None }],
        windows: vec![dispatch::WindowSeen { extension: 1, window: "reforms".into(), open: true }],
        ..Default::default()
    };
    dispatch::frame(&opened, &heard, &mut feed);
    dispatch::hear(&opened, &dispatch::Heard::of(&Event::SessionEnded { reason: SessionEnd::Load }), &mut feed);
    let log: Vec<String> = host.log.lock().unwrap().iter().filter(|l| !l.starts_with("slow stretch:")).cloned().collect();
    assert_eq!(
        log,
        [
            "first session 7",
            "second session nil",
            "first ordered integer 4294967377",
            "second ordered integer 4294967377",
            "first battle attackers 1",
            "second battle attackers 1",
            "first day 1444.12.1",
            "second day 1444.12.1",
            "first month 12",
            "second month 12",
            "second map_lost",
            "first button go true nil",
            "second window reforms true",
            "first frame",
            "second frame",
            "first ended load",
            "second ended load",
        ]
    );
    // The map and the clicks are asked for when their turn comes, after
    // what was heard.
    assert_eq!(feed.asked, ["session 3", "map_lost", "clicks", "windows"]);
    let listened: Vec<&str> = dispatch::listened(&opened).iter().map(|k| k.name()).collect();
    assert_eq!(listened, ["session", "session_ended", "day", "month", "army_ordered", "battle"]);
}

#[test]
fn an_event_a_test_raises_is_held_to_its_shape() {
    use eu4_harmony_remote::events::Kind;
    use serde_json::json;
    let heard = dispatch::Heard::new(Kind::WarDeclared, json!({ "war": "The War", "attacker": "FRA", "defender": "ENG" })).unwrap();
    assert_eq!(heard.field("attacker"), Some(&json!("FRA")));
    // An optional field may be left out; one the event requires may not.
    assert!(dispatch::Heard::new(Kind::ProvinceOwnerChanged, json!({ "province": 1 })).is_ok());
    let missing = dispatch::Heard::new(Kind::WarDeclared, json!({ "war": "The War" })).unwrap_err();
    assert!(missing.contains("attacker, defender"), "{missing}");
    let wrong = dispatch::Heard::new(Kind::Day, json!({ "date": "1444.11.11" })).unwrap_err();
    assert!(wrong.contains("a date"), "{wrong}");
    let unknown = dispatch::Heard::new(Kind::ArmyDefeated, json!({ "army": 1, "fleet": 2 })).unwrap_err();
    assert!(unknown.contains("fleet is no field"), "{unknown}");
    assert!(dispatch::Heard::new(Kind::Day, json!([1])).is_err());
}

/// A mod's extension with windows, on a host that has the texts given.
fn load_texts(name: &str, main: &str, texts: &[(&'static str, &'static str)]) -> Result<(Arc<Extension>, Arc<TestHost>), String> {
    let (dir, manifest) = write_mod(name, main, &[]);
    let host = Arc::new(TestHost { texts: texts.to_vec(), ..TestHost::default() });
    let m = Mod { id: &manifest.id, name: "T", harmony: dir.join("eu4-harmony"), permissions: &manifest.permissions, data: None };
    open(0, &m, host.clone()).map(|e| (e, host))
}

#[test]
fn an_extension_sets_its_windows_and_waits_for_a_popup() {
    let main = r#"
        return {
          windows = {
            reforms = { size = { 300, 200 }, title = "TITLE", items = {
              { type = "text", name = "status", size = { 200, 20 } },
            }},
          },
          on_frame = function(game)
            game.window.open("reforms")
            game.window.reforms.status.text = "Ready"
            local answer = game.popup { title = "TITLE", text = "ASK", values = { N = 2 }, agree = "YES", decline = "NO", id = "ask" }
            log("answered " .. tostring(answer))
          end,
          on_day = function(game)
            game.window.reforms.status.frame = 2
          end,
        }
    "#;
    let texts = [("TITLE", "Reforms"), ("ASK", "Take $N$?"), ("YES", "Yes"), ("NO", "No")];
    let (e, host) = load_texts("windows", main, &texts).unwrap();
    assert_eq!(e.windows().len(), 1);
    e.handle("on_frame", &[]);
    assert_eq!(
        *host.windows.lock().unwrap(),
        [WindowChange::Open("reforms".into()), WindowChange::Set { window: "reforms".into(), widget: "status".into(), value: Property::Text("Ready".into()) },]
    );
    {
        let popups = host.popups.lock().unwrap();
        assert_eq!(
            popups[0].0,
            Popup {
                id: Some("ask".into()),
                header: None,
                title: "Reforms".into(),
                text: "Take 2?".into(),
                agree: Some("Yes".into()),
                decline: Some("No".into()),
                goto: None,
                shields: [None, None],
            }
        );
        *popups[0].1.lock().unwrap() = Some(Ok(Value::from("decline")));
    }
    assert_eq!(e.waiting(), 1);
    e.poll();
    assert!(host.log.lock().unwrap().iter().any(|l| l == "answered decline"), "{:?}", host.log.lock().unwrap());
    // A property the game widget does not have is the handler's error.
    e.handle("on_day", &[]);
    let errors = errors(&e);
    assert!(errors.len() == 1 && errors[0].starts_with("on_day: runtime error: game.window.reforms.status.frame: a text has visible, text"), "{errors:?}");
    // A layout that is wrong, or a text the mod lacks, keeps it from loading.
    let wrong = load_texts("windows-wrong", "return { windows = { w = { size = { 100, 100 }, title = \"NONE\" } } }", &texts).err().unwrap();
    assert!(wrong.contains("main.lua's windows.w: title: the mod has no text \"NONE\""), "{wrong}");
}
