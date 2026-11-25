//! `eu4-harmony test`: runs a mod's `eu4-harmony/tests/*.lua` against the
//! fake game ([`crate::fake`]), with the same Lua host and sandbox as the
//! game. A test file registers its tests with `test(name, function)`; each
//! test opens the mod with `open()`, which runs its `main.lua` afresh, and
//! drives it through the handle `open` returns (`eu4-harmony skills testing`
//! lists what it can do). A test fails when it raises an error, an `assert`
//! included.
use crate::fake::{ClickOptions, FakeMod, ModSource, Properties};
use eu4_harmony_lua::{fetch, values};
use eu4_harmony_manifest::Manifest;
use eu4_harmony_remote::Date;
use mlua::{Function, Lua, LuaOptions, StdLib, Table, UserData, UserDataMethods, Value as LuaValue, Variadic};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

/// Where the tests are, in a mod's `eu4-harmony/`.
pub const TESTS: &str = "tests";

/// What a run came to.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub passed: usize,
    pub failed: usize,
    /// What was printed: each file, each test and its notes.
    pub report: String,
}

/// The mod the tests open.
struct Opened {
    source: Arc<ModSource>,
    /// The folder the tests' data folders are made in.
    scratch: PathBuf,
}

/// Runs every test of the mod at `root`.
pub fn run(root: &Path, manifest: Manifest, name: String) -> Result<Outcome, String> {
    let dir = root.join("eu4-harmony").join(TESTS);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}; tests go in eu4-harmony/{TESTS}/<name>.lua", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lua"))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("no .lua file in {}", dir.display()));
    }
    let scratch = std::env::temp_dir().join(format!("eu4-harmony-test-{}", std::process::id()));
    let source = Arc::new(ModSource { root: root.to_path_buf(), manifest, name });
    let opened = Arc::new(Opened { source, scratch: scratch.clone() });
    let mut outcome = Outcome::default();
    for file in files {
        run_file(&opened, &file, &mut outcome);
    }
    let _ = std::fs::remove_dir_all(scratch);
    outcome.report.push_str(&format!("{} passed, {} failed\n", outcome.passed, outcome.failed));
    Ok(outcome)
}

/// Runs one test file: its top level, which registers its tests, then each
/// test in the order it was registered.
fn run_file(opened: &Arc<Opened>, file: &Path, outcome: &mut Outcome) {
    let shown = file.strip_prefix(&opened.source.root).unwrap_or(file).to_string_lossy().replace('\\', "/");
    outcome.report.push_str(&format!("{shown}\n"));
    let tests = match harness(opened).and_then(|lua| load(&lua, file).map(|t| (lua, t))) {
        Ok(found) => found,
        Err(e) => {
            outcome.failed += 1;
            outcome.report.push_str(&format!("  FAILED  (the file) {e}\n"));
            return;
        }
    };
    let (lua, registered) = tests;
    for (name, function) in registered {
        let mut result = function.call::<()>(()).map_err(|e| e.to_string());
        // What the mods this test opened were left with.
        let (notes, misshapen) = lua.app_data_ref::<Notes>().map(|n| (n.0.clone(), n.1.clone())).unwrap_or_default();
        lua.set_app_data(Notes::default());
        if let (Ok(()), Some(first)) = (&result, misshapen.first()) {
            result = Err(first.clone());
        }
        match result {
            Ok(()) => {
                outcome.passed += 1;
                outcome.report.push_str(&format!("  ok      {name}\n"));
            }
            Err(e) => {
                outcome.failed += 1;
                let message = e.lines().next().unwrap_or_default().to_string();
                outcome.report.push_str(&format!("  FAILED  {name}\n          {message}\n"));
            }
        }
        for note in notes {
            outcome.report.push_str(&format!("          note: {note}\n"));
        }
    }
}

/// Notes about the mods a test opened: capabilities it left unanswered; and
/// answers its functions gave not of their capability's shape, which fail it.
#[derive(Default)]
struct Notes(Vec<String>, Vec<String>);

/// The tests' own Lua state: Lua's string, table, math and utf8, with
/// `test`, `open`, `print`, and `require` for helper modules in
/// `eu4-harmony/tests/` and the mod's own modules in `eu4-harmony/lua/`.
fn harness(opened: &Arc<Opened>) -> Result<Lua, String> {
    let lua = Lua::new_with(StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8, LuaOptions::default()).map_err(|e| e.to_string())?;
    let setup = || -> mlua::Result<()> {
        let globals = lua.globals();
        globals.set("__tests", lua.create_table()?)?;
        globals.set(
            "test",
            lua.create_function(|lua, (name, function): (String, Function)| {
                let tests: Table = lua.globals().get("__tests")?;
                tests.push(lua.create_sequence_from([LuaValue::String(lua.create_string(&name)?), LuaValue::Function(function)])?)
            })?,
        )?;
        let at = opened.clone();
        globals.set("open", lua.create_function(move |lua, ()| open(lua, &at).map_err(mlua::Error::runtime))?)?;
        // A helper of the tests', or else one of the mod's own modules, such as
        // a decision core to test on its own.
        let folders = vec![opened.source.root.join("eu4-harmony").join(TESTS), opened.source.root.join("eu4-harmony").join("lua")];
        eu4_harmony_lua::install_require(&lua, folders)?;
        globals.set(
            "print",
            lua.create_function(|lua, args: Variadic<LuaValue>| {
                let tostring: Function = lua.globals().get("tostring")?;
                let words = args.iter().map(|a| tostring.call::<String>(a.clone())).collect::<mlua::Result<Vec<_>>>()?;
                println!("          {}", words.join(" "));
                Ok(())
            })?,
        )?;
        Ok(())
    };
    setup().map_err(|e| e.to_string())?;
    lua.set_app_data(Notes::default());
    Ok(lua)
}

/// Runs a test file's top level; its tests, by name, as it registered them.
fn load(lua: &Lua, file: &Path) -> Result<Vec<(String, Function)>, String> {
    let source = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let name = format!("@{}", file.file_name().unwrap_or_default().to_string_lossy());
    lua.load(source).set_name(name).set_mode(mlua::ChunkMode::Text).exec().map_err(|e| e.to_string())?;
    let tests: Table = lua.globals().get("__tests").map_err(|e| e.to_string())?;
    let mut found = Vec::new();
    for entry in tests.sequence_values::<Table>() {
        let entry = entry.map_err(|e| e.to_string())?;
        found.push((entry.get::<String>(1).map_err(|e| e.to_string())?, entry.get::<Function>(2).map_err(|e| e.to_string())?));
    }
    if found.is_empty() {
        return Err("it registers no test: test(\"what it checks\", function() ... end)".into());
    }
    Ok(found)
}

static OPENED: AtomicUsize = AtomicUsize::new(0);

/// The mod opened afresh on a fake game of its own, as the handle tests
/// drive it with.
fn open(lua: &Lua, at: &Arc<Opened>) -> Result<TestMod, String> {
    let n = OPENED.fetch_add(1, Ordering::Relaxed);
    let data = at.scratch.join(n.to_string()).join(&at.source.manifest.id);
    Ok(TestMod(FakeMod::open(at.source.clone(), data, Some(lua))?))
}

/// Thin Lua `UserData` handle projecting [`FakeMod`] into `eu4-harmony test`.
struct TestMod(FakeMod);

impl TestMod {
    /// Moves any newly recorded fake-game notes into the active test's report.
    fn record_notes(&self, lua: &Lua) {
        let notes = self.0.take_notes();
        if let Some(mut kept) = lua.app_data_mut::<Notes>() {
            for name in notes.unanswered {
                let note = format!("{name} had no answer");
                if !kept.0.contains(&note) {
                    kept.0.push(note);
                }
            }
            for why in notes.misshapen {
                if !kept.1.contains(&why) {
                    kept.1.push(why);
                }
            }
        }
    }
}

fn json(value: &LuaValue) -> mlua::Result<Value> {
    values::to_json(value).map_err(mlua::Error::runtime)
}

/// Where the test called from: `file:line: `.
fn here(lua: &Lua) -> String {
    lua.inspect_stack(1, |d| format!("{}:{}: ", d.source().short_src.as_deref().unwrap_or("?"), d.current_line().unwrap_or(0))).unwrap_or_default()
}

fn at(lua: &Lua, message: String) -> mlua::Error {
    mlua::Error::runtime(format!("{}{message}", here(lua)))
}

fn click_options(click: Option<Table>, with_row: bool) -> mlua::Result<ClickOptions> {
    let flag = |key: &str| -> mlua::Result<bool> { Ok(click.as_ref().map(|t| t.get::<Option<bool>>(key)).transpose()?.flatten().unwrap_or(false)) };
    let row = if with_row { click.as_ref().map(|t| t.get::<Option<u32>>("row")).transpose()?.flatten() } else { None };
    Ok(ClickOptions { right: flag("right")?, shift: flag("shift")?, ctrl: flag("ctrl")?, row })
}

fn properties_table(lua: &Lua, properties: &Properties) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for (name, value) in properties {
        t.set(*name, values::to_lua(lua, value)?)?;
    }
    Ok(t)
}

impl UserData for TestMod {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // What the fake game answers.
        methods.add_method("answer", |lua, this, (capability, answer): (String, LuaValue)| {
            match answer {
                LuaValue::Function(f) => this.0.answer_function(&capability, f),
                other => this.0.answer(&capability, json(&other)?),
            }
            .map_err(|e| at(lua, e))
        });
        methods.add_method("refuse", |_, this, (capability, name, message): (String, String, Option<String>)| {
            this.0.refuse(&capability, &name, message).map_err(mlua::Error::runtime)
        });
        methods.add_method("later", |_, this, capability: String| {
            this.0.later(&capability);
            Ok(())
        });
        methods.add_method("reply", |lua, this, (capability, answer): (String, LuaValue)| this.0.reply(&capability, json(&answer)?).map_err(|e| at(lua, e)));
        methods.add_method("reply_refusal", |_, this, (capability, name, message): (String, String, Option<String>)| {
            this.0.reply_refusal(&capability, &name, message).map_err(mlua::Error::runtime)
        });
        methods.add_method("respond", |_, this, (n, response): (usize, Table)| {
            let headers: Vec<(String, String)> = match response.get::<Option<Table>>("headers")? {
                Some(t) => t.pairs::<String, String>().map(|p| p.map(|(k, v)| (k.to_ascii_lowercase(), v))).collect::<mlua::Result<_>>()?,
                None => Vec::new(),
            };
            let body = response.get::<Option<mlua::String>>("body")?.map(|b| b.as_bytes().to_vec()).unwrap_or_default();
            let answer = fetch::Response { status: response.get::<Option<u32>>("status")?.unwrap_or(200), headers, body };
            this.0.respond(n, answer).map_err(mlua::Error::runtime)
        });
        methods.add_method("fail", |_, this, (n, name, message): (usize, String, Option<String>)| {
            this.0.fail_request(n, &name, message).map_err(mlua::Error::runtime)
        });
        methods.add_method("world", |lua, this, world: LuaValue| this.0.set_world(json(&world)?).map_err(|e| at(lua, e)));
        methods.add_method("language", |_, this, name: String| this.0.set_language(&name).map_err(mlua::Error::runtime));

        // Signals, as the game raises them (`eu4_harmony_lua::dispatch`).
        methods.add_method("session", |lua, this, saved: LuaValue| {
            this.0.session(Some(json(&saved)?));
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("session_ended", |lua, this, reason: Option<String>| {
            this.0.session_ended(reason.as_deref()).map_err(|e| at(lua, e))?;
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("day", |lua, this, date: LuaValue| {
            let text = match &date {
                LuaValue::String(s) => s.to_str()?.to_string(),
                LuaValue::Table(t) => format!("{}.{}.{}", t.get::<i64>("year")?, t.get::<i64>("month")?, t.get::<i64>("day")?),
                other => return Err(mlua::Error::runtime(format!("a date is \"1444.11.12\" or {{ year, month, day }}, not a {}", other.type_name()))),
            };
            let date = Date::parse(&text).ok_or_else(|| mlua::Error::runtime(format!("{text} is not a date")))?;
            this.0.day(date);
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("raise", |lua, this, (kind, fields): (String, Option<LuaValue>)| {
            let fields = fields.map(|f| json(&f)).transpose()?.unwrap_or_else(|| Value::Object(Default::default()));
            this.0.raise(&kind, fields).map_err(|e| at(lua, e))?;
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("poll", |lua, this, ()| {
            this.0.poll();
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("frame", |lua, this, ()| {
            this.0.frame();
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("button", |lua, this, (name, click): (String, Option<Table>)| {
            this.0.button(&name, click_options(click, false)?).map_err(|e| at(lua, e))?;
            this.record_notes(lua);
            Ok(())
        });

        // The mod's windows and pop-ups, as the player uses them.
        methods.add_method("press", |lua, this, (window, widget, click): (String, String, Option<Table>)| {
            this.0.press(&window, &widget, click_options(click, true)?).map_err(|e| at(lua, e))?;
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("entry", |lua, this, window: String| {
            this.0.entry(&window).map_err(|e| at(lua, e))?;
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("close", |lua, this, window: String| {
            this.0.close(&window).map_err(|e| at(lua, e))?;
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("popups", |lua, this, ()| {
            let list = lua.create_table()?;
            for p in this.0.popups() {
                let t = lua.create_table()?;
                t.set("popup", p.number)?;
                t.set("title", p.title)?;
                t.set("text", p.text)?;
                t.set("header", p.header)?;
                t.set("buttons", lua.create_sequence_from(p.buttons)?)?;
                list.push(t)?;
            }
            Ok(list)
        });
        methods.add_method("answer_popup", |lua, this, (number, button): (u32, u32)| {
            this.0.answer_popup(number, button).map_err(|e| at(lua, e))?;
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("army_ordered", |lua, this, army: u64| {
            this.0.army_ordered(army);
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("army_defeated", |lua, this, army: u64| {
            this.0.army_defeated(army);
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("map_lost", |lua, this, ()| {
            this.0.map_lost();
            this.record_notes(lua);
            Ok(())
        });
        methods.add_method("save", |lua, this, ()| {
            let saved = this.0.save();
            this.record_notes(lua);
            match saved {
                Ok(None) => Ok((LuaValue::Nil, LuaValue::Nil)),
                Ok(Some(data)) => Ok((values::to_lua(lua, &data)?, LuaValue::Nil)),
                Err(message) => Ok((LuaValue::Nil, LuaValue::String(lua.create_string(message)?))),
            }
        });
        methods.add_method("reload", |lua, this, ()| {
            let report = this.0.reload().map_err(|e| at(lua, e))?;
            this.record_notes(lua);
            let result = lua.create_table()?;
            result.set("session", report.session)?;
            result.set("saved_kept", report.saved_kept)?;
            result.set("lost", report.lost)?;
            Ok(result)
        });

        // What the mod did.
        methods.add_method("log", |lua, this, ()| lua.create_sequence_from(this.0.log()));
        methods.add_method("errors", |lua, this, ()| lua.create_sequence_from(this.0.errors()));
        methods.add_method("calls", |lua, this, capability: Option<String>| {
            let calls = lua.create_table()?;
            for call in this.0.calls(capability.as_deref()) {
                calls.push(values::to_lua(lua, &call)?)?;
            }
            Ok(calls)
        });
        methods.add_method("element", |lua, this, name: String| properties_table(lua, &this.0.element(&name)));
        methods.add_method("window", |lua, this, name: String| {
            let window = this.0.window(&name).map_err(|e| at(lua, e))?;
            let t = lua.create_table()?;
            for (widget, p) in &window.widgets {
                t.set(widget.as_str(), properties_table(lua, p)?)?;
            }
            for (list, rows) in &window.rows {
                let widget: Table = match t.get::<Option<Table>>(list.as_str())? {
                    Some(w) => w,
                    None => lua.create_table()?,
                };
                let all = lua.create_table()?;
                for row in rows {
                    let r = lua.create_table()?;
                    for (name, p) in row {
                        r.set(name.as_str(), properties_table(lua, p)?)?;
                    }
                    all.push(r)?;
                }
                widget.set("rows", all)?;
                t.set(list.as_str(), widget)?;
            }
            t.set("open", window.open)?;
            Ok(t)
        });
        methods.add_method("map", |lua, this, ()| match this.0.map() {
            None => Ok(LuaValue::Nil),
            Some(colors) => {
                let map = lua.create_table()?;
                for (province, [r, g, b, a]) in colors {
                    let color = lua.create_table()?;
                    color.set("r", r)?;
                    color.set("g", g)?;
                    color.set("b", b)?;
                    color.set("a", a)?;
                    map.set(province, color)?;
                }
                Ok(LuaValue::Table(map))
            }
        });
        methods.add_method("requests", |lua, this, ()| {
            let list = lua.create_table()?;
            for request in this.0.requests() {
                let t = lua.create_table()?;
                t.set("url", request.url)?;
                t.set("method", request.method)?;
                let headers = lua.create_table()?;
                for (name, value) in request.headers {
                    headers.set(name, value)?;
                }
                t.set("headers", headers)?;
                t.set("body", lua.create_string(&request.body)?)?;
                t.set("timeout", request.timeout)?;
                list.push(t)?;
            }
            Ok(list)
        });
        methods.add_method("waiting", |_, this, ()| Ok(this.0.waiting()));
        methods.add_method("told", |lua, this, ()| lua.create_sequence_from(this.0.told()));
        methods.add_method("stopped", |_, this, ()| Ok(this.0.stopped()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test raises any signal, held to its shape, and a day raises its
    /// month and its year as the game does.
    #[test]
    fn a_test_raises_every_signal_as_the_game_does() {
        let dir = std::env::temp_dir().join(format!("eu4h-test-signals-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |file: &str, text: &str| {
            let path = dir.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "lua/main.lua",
            r#"
            local function say(...) log(table.concat({ ... }, " ")) end
            return {
              on_session = function(game, saved) say("session", tostring(saved and saved.n)) end,
              on_day = function(game, date) say("day", date.month .. "." .. date.day) end,
              on_month = function(game, date) say("month", date.month) end,
              on_year = function(game, date) say("year", date.year) end,
              on_war_declared = function(game, war, attacker, defender) say("war", war, attacker, defender) end,
              on_battle = function(game, battle) say("battle", battle.province, battle.winner) end,
              on_army_ordered = function(game, army) say("ordered", math.type(army), army) end,
              on_session_ended = function(game, reason) say("ended", reason) end,
            }
            "#,
        );
        write(
            "tests/signals_test.lua",
            r#"
            test("signals", function()
              local mod = open()
              mod:session({ n = 2 })
              mod:day("1444.12.31")
              mod:day("1445.1.1")
              mod:raise("war_declared", { war = "Castilian Conquest", attacker = "CAS", defender = "GRA" })
              mod:raise("battle", { province = 151, naval = false, attackers = { "TUR" }, defenders = { "BYZ" }, winner = "attackers", attacker_losses = 1, defender_losses = 2 })
              mod:raise("day", { date = { year = 1445, month = 2, day = 1 } })
              mod:army_ordered(4294967377)
              mod:session_ended("load")
              mod:session()
              mod:day("1445.3.1")
              local log = table.concat(mod:log(), "|")
              assert(log == "session 2|day 12.31|day 1.1|month 1|year 1445|war Castilian Conquest CAS GRA|battle 151 attackers|day 2.1|month 2|ordered integer 4294967377|ended load|session nil|day 3.1", log)
            end)
            test("a misshapen signal", function()
              local mod = open()
              local ok, why = pcall(mod.raise, mod, "war_declared", { war = "x" })
              assert(not ok and tostring(why):find("has no attacker, defender"), tostring(why))
              ok, why = pcall(mod.raise, mod, "no_such_signal", {})
              assert(not ok and tostring(why):find("there is no signal no_such_signal"), tostring(why))
              ok, why = pcall(mod.session_ended, mod, "quit")
              assert(not ok and tostring(why):find("return, load, new_game or country"), tostring(why))
            end)
            "#,
        );
        let manifest = Manifest::parse(&format!("id = \"e\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION)).unwrap();
        let outcome = run(&dir, manifest, "E".into()).unwrap();
        assert_eq!((outcome.passed, outcome.failed), (2, 0), "{}", outcome.report);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The fake game keeps the mod's windows and pop-ups by the game's
    /// rules: they open only in a session, at most so many pop-ups are open,
    /// and the session's end closes them all.
    #[test]
    fn the_fake_game_keeps_windows_and_pop_ups_as_the_game_does() {
        let dir = std::env::temp_dir().join(format!("eu4h-test-windows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |file: &str, text: &str| {
            let path = dir.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("localisation/w_l_english.yml", "\u{feff}l_english:\n W_TITLE:0 \"Asked\"\n");
        write(
            "lua/main.lua",
            r#"
            local function say(...) log(table.concat({ ... }, " ")) end
            local function try(what, f, ...)
              local ok, why = pcall(f, ...)
              say(what, ok and "ok" or tostring(why))
            end
            return {
              windows = { reforms = { size = { 300, 200 } }, electors = { size = { 300, 200 } } },
              on_war_declared = function(game, war)
                if war == "open" then
                  try("open", game.window.open, "reforms")
                  try("popup", game.popup, { title = "W_TITLE", text = "W_TITLE" })
                elseif war == "flood" then
                  for _ = 1, 32 do game.popup { title = "W_TITLE", text = "W_TITLE" } end
                  try("33rd", game.popup, { title = "W_TITLE", text = "W_TITLE" })
                end
              end,
            }
            "#,
        );
        write(
            "tests/windows_test.lua",
            r#"
            test("windows", function()
              local mod = open()
              mod:raise("war_declared", { war = "open", attacker = "CAS", defender = "GRA" })
              assert(not mod:window("reforms").open and #mod:popups() == 0)
              mod:session()
              mod:raise("war_declared", { war = "open", attacker = "CAS", defender = "GRA" })
              mod:frame()
              assert(mod:window("reforms").open and #mod:popups() == 1)
              local ok, why = pcall(mod.answer_popup, mod, mod:popups()[1].popup, 2)
              assert(not ok and tostring(why):find("has buttons 1 to 1"), tostring(why))
              mod:answer_popup(mod:popups()[1].popup, 1)
              assert(#mod:popups() == 0, "a button with no action closes it")
              mod:raise("war_declared", { war = "flood", attacker = "CAS", defender = "GRA" })
              assert(#mod:popups() == 32)
              mod:session_ended()
              assert(not mod:window("reforms").open and #mod:popups() == 0, "the session's end closes them all")
              local log = mod:log()
              assert(#log == 5, table.concat(log, "|"))
              assert(log[1]:find("^open .*a window opens only in a game"), log[1])
              assert(log[2]:find("^popup .*a pop%-up is shown only in a game"), log[2])
              assert(log[3] == "open ok" and log[4] == "popup ok", table.concat(log, "|"))
              assert(log[5]:find("^33rd .*32 pop%-ups are open already"), log[5])
            end)
            "#,
        );
        let manifest = Manifest::parse(&format!("id = \"w\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION)).unwrap();
        let outcome = run(&dir, manifest, "W".into()).unwrap();
        assert_eq!((outcome.passed, outcome.failed), (1, 0), "{}", outcome.report);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn fake_game_resolves_defaults_entry_numbers_language_switches_and_refuses_disabled_or_hidden_presses() {
        let dir = std::env::temp_dir().join(format!("eu4h-test-resolved-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |file: &str, text: &str| {
            let path = dir.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("localisation/u_l_english.yml", "\u{feff}l_english:\n U_STATUS:0 \"Count $N$\"\n U_BTN:0 \"Act\"\n U_TIP:0 \"Ready\"\n");
        write("localisation/u_l_simp_chinese.yml", "\u{feff}l_simp_chinese:\n U_STATUS:0 \"计数 $N$\"\n U_BTN:0 \"执行\"\n U_TIP:0 \"就绪\"\n");
        write("ui/single_unitpanel/buttons/toggle.gui", "guiButtonType = { name = \"toggle\" }\ninstantTextBoxType = { name = \"note\" }\n");
        write(
            "lua/main.lua",
            r#"
            return {
              windows = {
                tools = {
                  size = { 320, 320 },
                  entry = { sprite = "GFX_tools", format = "percent", decimals = 1, value = 0.125, tooltip = { key = "U_TIP" } },
                  items = {
                    { type = "text", name = "status", size = { 200, 20 }, text = { key = "U_STATUS", values = { N = 4 } } },
                    { type = "button", name = "act", text = { key = "U_BTN" } },
                    { type = "list", name = "items", size = { 260, 120 }, row = { size = { 240, 64 }, items = {
                      { type = "button", name = "pick", text = { key = "U_BTN" } },
                      { type = "text", name = "label", size = { 100, 20 }, text = { key = "U_STATUS", values = { N = 9 } } },
                    }}},
                  },
                },
              },
              on_session = function(game)
                game.ui.toggle.text = { key = "U_STATUS", values = { N = 7 } }
                game.ui.toggle.tooltip = { key = "U_TIP" }
                game.ui.toggle.enabled = false
                local bad_ok, bad_err = pcall(function() game.ui.missing.visible = false end)
                assert(not bad_ok and tostring(bad_err):find("no element \"missing\" declared"), tostring(bad_err))
                game.window.tools.items.rows = { { pick = false }, {} }
              end,
              on_button = function(game, name, click)
                log(name .. ":" .. tostring(click.row))
                if name == "tools.act" then
                  game.window.tools.act.enabled = false
                  game.window.tools.items.visible = false
                  game.window.tools.entry.enabled = false
                end
              end,
            }
            "#,
        );
        write(
            "tests/resolved_test.lua",
            r#"
            test("resolved views and pressability", function()
              local mod = open()
              local initial = mod:window("tools")
              assert(not initial.open and initial.status.text == "Count 4" and initial.act.enabled == true)
              assert(initial.entry.value == 0.125 and initial.entry.text == "12.5%" and initial.entry.tooltip == "Ready")

              mod:session()
              assert(mod:element("toggle").text == "Count 7" and mod:element("toggle").tooltip == "Ready")
              local ok, why = pcall(mod.button, mod, "toggle")
              assert(not ok and tostring(why):find("disabled"), tostring(why))
              ok, why = pcall(mod.button, mod, "note")
              assert(not ok and tostring(why):find("not a button"), tostring(why))
              ok, why = pcall(mod.button, mod, "missing")
              assert(not ok and tostring(why):find("no element \"missing\" declared"), tostring(why))

              mod:language("simp_chinese")
              assert(mod:element("toggle").text == "计数 7" and mod:element("toggle").tooltip == "就绪")
              assert(mod:window("tools").status.text == "计数 4" and mod:window("tools").entry.tooltip == "就绪")
              assert(mod:window("tools").items.rows[2].label.text == "计数 9")

              mod:entry("tools")
              assert(mod:window("tools").open)
              ok, why = pcall(mod.press, mod, "tools", "pick", { row = 1 })
              assert(not ok and tostring(why):find("row 1 is disabled"), tostring(why))
              mod:press("tools", "pick", { row = 2 })
              mod:press("tools", "act")

              ok, why = pcall(mod.press, mod, "tools", "act")
              assert(not ok and tostring(why):find("tools%.act is disabled"), tostring(why))
              ok, why = pcall(mod.press, mod, "tools", "pick", { row = 2 })
              assert(not ok and tostring(why):find("list tools%.items is hidden"), tostring(why))
              ok, why = pcall(mod.entry, mod, "tools")
              assert(not ok and tostring(why):find("entry button of \"tools\" is disabled"), tostring(why))
              assert(table.concat(mod:log(), "|") == "tools.pick:2|tools.act:nil")
            end)
            "#,
        );
        let manifest = Manifest::parse(&format!("id = \"u\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION)).unwrap();
        let outcome = run(&dir, manifest, "U".into()).unwrap();
        assert_eq!((outcome.passed, outcome.failed), (1, 0), "{}", outcome.report);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Tasks run in the fake game as in the game, and game.time.wait answers
    /// once the days a test raises reach its date.
    #[test]
    fn tasks_and_waits_for_days_run_as_in_the_game() {
        let dir = std::env::temp_dir().join(format!("eu4h-test-tasks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |file: &str, text: &str| {
            let path = dir.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "lua/main.lua",
            r#"
            local check_later = async(function(game, days)
              local date = game.time.wait(days)
              return date.month .. "." .. date.day
            end)
            local pending
            return {
              on_button = function(game, name)
                if name == "later" then
                  log("on " .. check_later(game, 3):await())
                elseif name == "cancel" then
                  pending = spawn(function() game.time.wait(1) log("not cancelled") end)
                  pending:cancel()
                else
                  local date, err = game.time.wait(0)
                  log(tostring(date) .. " " .. err.message)
                end
              end,
            }
            "#,
        );
        write(
            "tests/tasks_test.lua",
            r#"
            test("a wait for days", function()
              local mod = open()
              mod:session()
              mod:day("1444.11.11")
              mod:button("later")
              mod:day("1444.11.12")
              mod:day("1444.11.13")
              assert(#mod:log() == 0, table.concat(mod:log(), "|"))
              mod:day("1444.11.14")
              mod:button("cancel")
              mod:day("1444.11.15")
              mod:button("zero")
              local log = table.concat(mod:log(), "|")
              assert(log:find("^on 11.14|nil ") and log:find("days is from 1 to 36500, not 0") and not log:find("not cancelled"), log)
            end)
            "#,
        );
        let manifest = Manifest::parse(&format!("id = \"w\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION)).unwrap();
        let outcome = run(&dir, manifest, "W".into()).unwrap();
        assert_eq!((outcome.passed, outcome.failed), (1, 0), "{}", outcome.report);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_world_keeps_the_flags_and_variables_a_mod_sets() {
        let dir = std::env::temp_dir().join(format!("eu4h-test-script-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |file: &str, text: &str| {
            let path = dir.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "lua/main.lua",
            r#"
            return {
              on_button = function(game, name)
                local me = { country = "CAS" }
                game.script.set_flag(me, "met")
                game.script.set_variable({ province = 1 }, "count", 2.0005)
                local flag = game.script.flag(me, "met")
                local none = game.script.flag({ global = true }, "met")
                local var = game.script.variable({ province = 1 }, "count")
                local _, err = game.script.variable({ ruler = "CAS" }, "count")
                game.script.clear_flag(me, "met")
                log(tostring(flag.set) .. " " .. flag.day.year .. " " .. tostring(none.set) .. " " .. var.value .. " " .. err.code
                  .. " " .. tostring(game.script.flag(me, "met").set))
              end,
            }
            "#,
        );
        write(
            "tests/script_test.lua",
            r#"
            test("flags and variables", function()
              local mod = open()
              mod:world{ player = "CAS", provinces = { { id = 1, owner = "CAS" } } }
              mod:session()
              mod:button("go")
              local log = table.concat(mod:log(), "|")
              assert(log == "true 1444 false 2.001 invalid_argument false", log)
            end)
            "#,
        );
        let manifest = Manifest::parse(&format!("id = \"s\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION)).unwrap();
        let outcome = run(&dir, manifest, "S".into()).unwrap();
        assert_eq!((outcome.passed, outcome.failed), (1, 0), "{}", outcome.report);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Every `mod:<method>` the testing guide names is one a test has.
    #[test]
    fn the_testing_guide_names_only_methods_there_are() {
        let guide = include_str!("../skills/testing.md");
        let named: std::collections::BTreeSet<&str> =
            guide.split("mod:").skip(1).map(|rest| rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).next().unwrap_or_default()).collect();
        let dir = std::env::temp_dir().join(format!("eu4h-test-guide-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("eu4-harmony/lua")).unwrap();
        std::fs::write(dir.join("eu4-harmony/lua/main.lua"), "return {}").unwrap();
        let manifest = Manifest::parse("id = \"g\"\napi_version = \"0.3\"").unwrap();
        let source = Arc::new(ModSource { root: dir.clone(), manifest, name: "G".into() });
        let opened = Arc::new(Opened { source, scratch: dir.join("scratch") });
        let lua = harness(&opened).unwrap();
        for method in named {
            let kind: String = lua.load(format!("return type(open().{method})")).eval().unwrap();
            assert_eq!(kind, "function", "the testing guide names mod:{method}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn session_calendar_tracks_first_error_world_dates_and_module_cache() {
        let dir = std::env::temp_dir().join(format!("eu4h-test-cal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |file: &str, text: &str| {
            let path = dir.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("lua/cached.lua", "local name = ...\n_G.loads = (_G.loads or 0) + 1\nreturn false\n");
        write("tests/helpers/echo.lua", "local name = ...\nreturn name .. \":\" .. tostring(_G.loads)\n");
        write(
            "lua/main.lua",
            r#"
            return {
              on_button = function(game, name)
                if name == "boom" then error("kaboom") end
                local s = game.session.get()
                local me = { country = "CAS" }
                game.script.set_flag(me, "seen")
                local f = game.script.flag(me, "seen")
                log("ok:" .. s.number .. " " .. s.date.year .. "." .. s.date.month .. "." .. s.date.day .. " " .. f.day.year .. "." .. f.day.month .. "." .. f.day.day)
              end,
            }
            "#,
        );
        write(
            "tests/cal_test.lua",
            r#"
            test("require caches scalars and passes module name", function()
              assert(require("cached") == false)
              assert(require("cached") == false)
              assert(_G.loads == 1)
              assert(require("helpers.echo") == "helpers.echo:1")
            end)

            test("first_error repeats across sessions and world dates advance", function()
              local mod = open()
              mod:world{ player = "CAS", date = "1444.11.11", provinces = { { id = 1, owner = "CAS" } } }
              mod:button("boom")
              mod:button("boom")
              mod:session()
              mod:button("boom")
              mod:button("ok")
              mod:day("1445.3.15")
              mod:button("ok")
              mod:session_ended()
              mod:session()
              mod:button("boom")
              mod:button("ok")
              assert(#mod:told() == 3, "pre-session, session 1, session 2: " .. #mod:told())
              assert(#mod:errors() == 4, #mod:errors())
              local ok_lines = {}
              for _, line in ipairs(mod:log()) do
                if line:sub(1, 3) == "ok:" then ok_lines[#ok_lines + 1] = line:sub(4) end
              end
              local log = table.concat(ok_lines, "|")
              assert(log == "1 1444.11.11 1444.11.11|1 1445.3.15 1445.3.15|2 1444.11.11 1444.11.11", log)
            end)
            "#,
        );
        let manifest = Manifest::parse(&format!("id = \"cal\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION)).unwrap();
        let outcome = run(&dir, manifest, "Cal".into()).unwrap();
        assert_eq!((outcome.passed, outcome.failed), (2, 0), "{}", outcome.report);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reload_carries_save_data_skips_stopped_and_keeps_old_extension_on_error() {
        let dir = std::env::temp_dir().join(format!("eu4h-test-reload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |file: &str, text: &str| {
            let path = dir.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("manifest.toml", &format!("id = \"rel\"\napi_version = \"{}\"\n", eu4_harmony_remote::API_VERSION));
        write("localisation/rel_l_english.yml", "l_english:\n REL_LABEL:0 \"Old\"\n");
        write(
            "lua/main.lua",
            r#"
            local state = 0
            return {
              on_session = function(game, saved)
                state = saved and saved.n or 1
                game.ui.go.text = text.get("REL_LABEL") .. ":" .. state
              end,
              on_button = function(game, name)
                if name == "inc" then state = state + 1 end
              end,
              save_data = function() return { n = state } end,
            }
            "#,
        );
        let manifest = Manifest::parse(&format!("id = \"rel\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION)).unwrap();
        let source = Arc::new(ModSource { root: dir.clone(), manifest, name: "Rel".into() });
        let opened = Arc::new(Opened { source, scratch: dir.join("scratch") });
        let lua = harness(&opened).unwrap();
        lua.load(
            r#"
            mod = open()
            mod:session()
            mod:button("inc")
            "#,
        )
        .exec()
        .unwrap();

        // Update localisation + code cleanly, reload preserves save_data and new text.
        write("localisation/rel_l_english.yml", "l_english:\n REL_LABEL:0 \"New\"\n");
        lua.load(
            r#"
            local r = mod:reload()
            assert(r.session == true and r.saved_kept == true and r.lost == nil)
            assert(mod:element("go").text == "New:2", mod:element("go").text)
            "#,
        )
        .exec()
        .unwrap();

        // Broken main.lua fails reload while leaving the running extension intact.
        write("lua/main.lua", "return { on_button = ");
        lua.load(
            r#"
            local ok = pcall(function() mod:reload() end)
            assert(not ok)
            assert(mod:element("go").text == "New:2")
            "#,
        )
        .exec()
        .unwrap();

        let _ = std::fs::remove_dir_all(dir);
    }
}
