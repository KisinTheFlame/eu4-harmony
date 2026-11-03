//! `eu4-harmony test`: runs a mod's `eu4-harmony/tests/*.lua` against the
//! fake game ([`crate::fake`]), with the same Lua host and sandbox as the
//! game. A test file registers its tests with `test(name, function)`; each
//! test opens the mod with `open()`, which runs its `main.lua` afresh, and
//! drives it through the handle `open` returns (`eu4-harmony skills testing`
//! lists what it can do). A test fails when it raises an error, an `assert`
//! included.
use crate::fake::{check, code_named, Answer, Fake};
use eu4_harmony_lua::{fetch, values, Arg, Extension, Mod};
use eu4_harmony_manifest::Manifest;
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
    root: PathBuf,
    manifest: Manifest,
    name: String,
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
    let opened = Arc::new(Opened { root: root.to_path_buf(), manifest, name, scratch: scratch.clone() });
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
    let shown = file.strip_prefix(&opened.root).unwrap_or(file).to_string_lossy().replace('\\', "/");
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
/// answers its functions gave not of their capability's shape, which fail
/// it.
#[derive(Default)]
struct Notes(Vec<String>, Vec<String>);

/// The tests' own Lua state: Lua's string, table, math and utf8, with
/// `test`, `open`, `print`, and `require` for helper modules in
/// `eu4-harmony/tests/`.
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
        let tests = opened.root.join("eu4-harmony").join(TESTS);
        globals.set("__modules", lua.create_table()?)?;
        globals.set(
            "require",
            lua.create_function(move |lua, name: String| {
                let modules: Table = lua.globals().get("__modules")?;
                if let Ok(module @ (LuaValue::Table(_) | LuaValue::Function(_) | LuaValue::Boolean(_))) = modules.get::<LuaValue>(name.as_str()) {
                    return Ok(module);
                }
                if name.is_empty() || !name.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
                    return Err(mlua::Error::runtime(format!("require {name:?}: a module is named by words between dots")));
                }
                let file = tests.join(format!("{}.lua", name.replace('.', "/")));
                let source = std::fs::read_to_string(&file).map_err(|e| mlua::Error::runtime(format!("require {name:?}: {}: {e}", file.display())))?;
                let module = lua.load(source).set_name(format!("@{}.lua", name.replace('.', "/"))).set_mode(mlua::ChunkMode::Text).call::<LuaValue>(())?;
                let module = if module.is_nil() { LuaValue::Boolean(true) } else { module };
                modules.set(name.as_str(), module.clone())?;
                Ok(module)
            })?,
        )?;
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
fn open(lua: &Lua, at: &Opened) -> Result<TestMod, String> {
    let harmony = at.root.join("eu4-harmony");
    let (texts, _) = eu4_harmony_loc::load_dir(&harmony.join("localisation"));
    let fake = Arc::new(Fake::new(texts, lua));
    let n = OPENED.fetch_add(1, Ordering::Relaxed);
    let m = Mod {
        id: &at.manifest.id,
        name: &at.name,
        harmony,
        permissions: &at.manifest.permissions,
        data: Some(at.scratch.join(n.to_string()).join(&at.manifest.id)),
    };
    let extension = eu4_harmony_lua::open(0, &m, fake.clone())?;
    Ok(TestMod { extension, fake })
}

/// A mod a test opened, and its fake game.
struct TestMod {
    extension: Arc<Extension>,
    fake: Arc<Fake>,
}

impl TestMod {
    /// Notes what the mod was left with, for the test's report.
    fn note(&self, lua: &Lua) {
        let store = self.fake.store();
        let mut notes = Vec::new();
        for name in &store.unanswered {
            let note = format!("{name} had no answer");
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
        if let Some(mut kept) = lua.app_data_mut::<Notes>() {
            for note in notes {
                if !kept.0.contains(&note) {
                    kept.0.push(note);
                }
            }
            for why in &store.misshapen {
                if !kept.1.contains(why) {
                    kept.1.push(why.clone());
                }
            }
        }
    }

    fn handle(&self, lua: &Lua, name: &str, args: &[Arg]) {
        self.extension.handle(name, args);
        self.note(lua);
    }
}

fn json(value: &LuaValue) -> mlua::Result<Value> {
    values::to_json(value).map_err(mlua::Error::runtime)
}

/// Where the test called from: `file:line: `.
fn here(lua: &Lua) -> String {
    lua.inspect_stack(1, |d| format!("{}:{}: ", d.source().short_src.as_deref().unwrap_or("?"), d.current_line().unwrap_or(0))).unwrap_or_default()
}

/// `answer` held to `capability`'s result, as the test gave it.
fn shaped(lua: &Lua, capability: &str, answer: &LuaValue) -> mlua::Result<Value> {
    let json = json(answer)?;
    check(capability, &json).map_err(|why| mlua::Error::runtime(format!("{}{why}", here(lua))))?;
    Ok(json)
}

fn code(name: &str) -> mlua::Result<eu4_harmony_remote::Code> {
    code_named(name).ok_or_else(|| mlua::Error::runtime(format!("no error code {name}; eu4-harmony wiki errors lists them")))
}

impl UserData for TestMod {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // What the fake game answers.
        methods.add_method("answer", |lua, this, (capability, answer): (String, LuaValue)| {
            let answer = match answer {
                LuaValue::Function(f) if eu4_harmony_remote::result_shape(&capability).is_some() => Answer::Function(f),
                LuaValue::Function(_) => return Err(mlua::Error::runtime(format!("{}there is no capability {capability}", here(lua)))),
                other => Answer::Value(shaped(lua, &capability, &other)?),
            };
            this.fake.store().answers.insert(capability, answer);
            Ok(())
        });
        methods.add_method("refuse", |_, this, (capability, name, message): (String, String, Option<String>)| {
            this.fake.store().answers.insert(capability, Answer::Refusal(code(&name)?, message.unwrap_or_default()));
            Ok(())
        });
        methods.add_method("later", |_, this, capability: String| {
            this.fake.store().answers.insert(capability, Answer::Later);
            Ok(())
        });
        methods.add_method("reply", |lua, this, (capability, answer): (String, LuaValue)| reply(this, &capability, Ok(shaped(lua, &capability, &answer)?)));
        methods.add_method("reply_refusal", |_, this, (capability, name, message): (String, String, Option<String>)| {
            reply(this, &capability, Err(eu4_harmony_remote::Error::new(code(&name)?, message.unwrap_or_default())))
        });
        methods.add_method("respond", |_, this, (n, response): (usize, Table)| {
            let headers: Vec<(String, String)> = match response.get::<Option<Table>>("headers")? {
                Some(t) => t.pairs::<String, String>().map(|p| p.map(|(k, v)| (k.to_ascii_lowercase(), v))).collect::<mlua::Result<_>>()?,
                None => Vec::new(),
            };
            let body = response.get::<Option<mlua::String>>("body")?.map(|b| b.as_bytes().to_vec()).unwrap_or_default();
            let answer = fetch::Response { status: response.get::<Option<u32>>("status")?.unwrap_or(200), headers, body };
            respond(this, n, Ok(answer))
        });
        methods.add_method("fail", |_, this, (n, name, message): (usize, String, Option<String>)| {
            let failure = match name.as_str() {
                "timeout" => fetch::Failure::Timeout,
                "too_large" => fetch::Failure::TooLarge,
                "network" => fetch::Failure::Network(message.unwrap_or_else(|| "the host cannot be reached".into())),
                other => return Err(mlua::Error::runtime(format!("a request fails with timeout, network or too_large, not {other}"))),
            };
            respond(this, n, Err(failure))
        });
        methods.add_method("world", |lua, this, world: LuaValue| {
            let world = crate::world::World::new(json(&world)?).map_err(|e| mlua::Error::runtime(format!("{}{e}", here(lua))))?;
            this.fake.store().world = Some(world);
            Ok(())
        });
        methods.add_method("language", |_, this, name: String| {
            let language = eu4_harmony_loc::Language::from_id(&name).ok_or_else(|| mlua::Error::runtime(format!("no language {name}")))?;
            *this.fake.language.lock().unwrap_or_else(|e| e.into_inner()) = language;
            Ok(())
        });

        // Events, as the game raises them.
        methods.add_method("session", |lua, this, saved: LuaValue| {
            this.handle(lua, "on_session", &[Arg::Json(json(&saved)?)]);
            Ok(())
        });
        methods.add_method("session_ended", |lua, this, reason: Option<String>| {
            this.handle(lua, "on_session_ended", &[Arg::Text(reason.unwrap_or_else(|| "return".into()))]);
            this.extension.session_over();
            Ok(())
        });
        methods.add_method("day", |lua, this, date: LuaValue| {
            let text = match &date {
                LuaValue::String(s) => s.to_str()?.to_string(),
                LuaValue::Table(t) => format!("{}.{}.{}", t.get::<i64>("year")?, t.get::<i64>("month")?, t.get::<i64>("day")?),
                other => return Err(mlua::Error::runtime(format!("a date is \"1444.11.12\" or {{ year, month, day }}, not a {}", other.type_name()))),
            };
            let date = eu4_harmony_checks::Date::parse(&text).ok_or_else(|| mlua::Error::runtime(format!("{text} is not a date")))?;
            this.handle(lua, "on_day", &[Arg::Date(date)]);
            Ok(())
        });
        methods.add_method("poll", |lua, this, ()| {
            this.extension.poll();
            this.note(lua);
            Ok(())
        });
        methods.add_method("frame", |lua, this, ()| {
            this.extension.poll();
            this.handle(lua, "on_frame", &[]);
            Ok(())
        });
        methods.add_method("button", |lua, this, (name, click): (String, Option<Table>)| {
            let flag = |key: &str| -> mlua::Result<bool> { Ok(click.as_ref().map(|t| t.get::<Option<bool>>(key)).transpose()?.flatten().unwrap_or(false)) };
            let how = Arg::Click { right: flag("right")?, shift: flag("shift")?, ctrl: flag("ctrl")? };
            this.handle(lua, "on_button", &[Arg::Text(name), how]);
            Ok(())
        });
        methods.add_method("army_ordered", |lua, this, army: i64| {
            this.handle(lua, "on_army_ordered", &[Arg::Integer(army)]);
            Ok(())
        });
        methods.add_method("army_defeated", |lua, this, army: i64| {
            this.handle(lua, "on_army_defeated", &[Arg::Integer(army)]);
            Ok(())
        });
        methods.add_method("map_lost", |lua, this, ()| {
            this.handle(lua, "on_map_lost", &[]);
            Ok(())
        });
        methods.add_method("save", |lua, this, ()| match this.extension.on_save() {
            None | Some(Ok(None)) => Ok((LuaValue::Nil, LuaValue::Nil)),
            Some(Ok(Some(data))) => Ok((values::to_lua(lua, &data)?, LuaValue::Nil)),
            Some(Err(message)) => Ok((LuaValue::Nil, LuaValue::String(lua.create_string(message)?))),
        });

        // What the mod did.
        methods.add_method("log", |lua, this, ()| lua.create_sequence_from(this.fake.store().log.clone()));
        methods.add_method("errors", |lua, this, ()| {
            let errors: Vec<String> = this.fake.store().log.iter().filter(|l| l.starts_with("error in ")).cloned().collect();
            lua.create_sequence_from(errors)
        });
        methods.add_method("calls", |lua, this, capability: Option<String>| {
            let calls = lua.create_table()?;
            for call in this.fake.store().calls.iter() {
                if capability.as_deref().is_none_or(|c| call.get("capability").and_then(Value::as_str) == Some(c)) {
                    calls.push(values::to_lua(lua, call)?)?;
                }
            }
            Ok(calls)
        });
        methods.add_method("element", |lua, this, name: String| {
            let element = lua.create_table()?;
            if let Some(properties) = this.fake.store().elements.get(&name) {
                for (property, value) in properties {
                    element.set(*property, values::to_lua(lua, value)?)?;
                }
            }
            Ok(element)
        });
        methods.add_method("map", |lua, this, ()| match &this.fake.store().map {
            None => Ok(LuaValue::Nil),
            Some(colors) => {
                let map = lua.create_table()?;
                for (province, [r, g, b, a]) in colors {
                    let color = lua.create_table()?;
                    color.set("r", *r)?;
                    color.set("g", *g)?;
                    color.set("b", *b)?;
                    color.set("a", *a)?;
                    map.set(*province, color)?;
                }
                Ok(LuaValue::Table(map))
            }
        });
        methods.add_method("requests", |lua, this, ()| {
            let list = lua.create_table()?;
            for (request, _) in this.fake.store().requests.iter() {
                let t = lua.create_table()?;
                t.set("url", request.target.url())?;
                t.set("method", request.method.as_str())?;
                let headers = lua.create_table()?;
                for (name, value) in &request.headers {
                    headers.set(name.as_str(), value.as_str())?;
                }
                t.set("headers", headers)?;
                t.set("body", lua.create_string(&request.body)?)?;
                t.set("timeout", request.timeout.as_secs_f64())?;
                list.push(t)?;
            }
            Ok(list)
        });
        methods.add_method("waiting", |_, this, ()| Ok(this.extension.waiting()));
        methods.add_method("told", |lua, this, ()| lua.create_sequence_from(this.fake.store().told.clone()));
        methods.add_method("stopped", |_, this, ()| Ok(this.extension.stopped()));
    }
}

/// Gives the oldest answer of `capability` still to come.
fn reply(this: &TestMod, capability: &str, answer: eu4_harmony_lua::Answered) -> mlua::Result<()> {
    let mut store = this.fake.store();
    let Some(at) = store.later.iter().position(|(name, _)| name == capability) else {
        return Err(mlua::Error::runtime(format!("no answer of {capability} is still to come; mod:later(\"{capability}\") makes it come later")));
    };
    let (_, slot) = store.later.remove(at);
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(answer);
    Ok(())
}

/// Gives the `n`th request (from 1) its answer.
fn respond(this: &TestMod, n: usize, answer: fetch::Answer) -> mlua::Result<()> {
    let store = this.fake.store();
    let Some((_, slot)) = n.checked_sub(1).and_then(|i| store.requests.get(i)) else {
        return Err(mlua::Error::runtime(format!("there is no request {n}; the mod sent {}", store.requests.len())));
    };
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(answer);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let manifest = Manifest::parse("id = \"g\"\napi_version = \"0.2\"").unwrap();
        let opened = Arc::new(Opened { root: dir.clone(), manifest, name: "G".into(), scratch: dir.join("scratch") });
        let lua = harness(&opened).unwrap();
        for method in named {
            let kind: String = lua.load(format!("return type(open().{method})")).eval().unwrap();
            assert_eq!(kind, "function", "the testing guide names mod:{method}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
