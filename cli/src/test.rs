//! `eu4-harmony test`: runs a mod's `eu4-harmony/tests/*.lua` against the
//! fake game ([`crate::fake`]), with the same Lua host and sandbox as the
//! game. A test file registers its tests with `test(name, function)`; each
//! test opens the mod with `open()`, which runs its `main.lua` afresh, and
//! drives it through the handle `open` returns (`eu4-harmony skills testing`
//! lists what it can do). A test fails when it raises an error, an `assert`
//! included.
use crate::fake::{check, code_named, lua_date, Answer, Fake};
use eu4_harmony_lua::{
    dispatch::{self, Heard},
    fetch, layout, values, Extension, Mod,
};
use eu4_harmony_manifest::Manifest;
use eu4_harmony_remote::{
    signals::{Kind, SessionEnd, Signal},
    ArmyId, Date,
};
use mlua::{Function, Lua, LuaOptions, StdLib, Table, UserData, UserDataMethods, Value as LuaValue, Variadic};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
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
        let folders = [opened.root.join("eu4-harmony").join(TESTS), opened.root.join("eu4-harmony").join("lua")];
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
                let relative = format!("{}.lua", name.replace('.', "/"));
                let Some(file) = folders.iter().map(|f| f.join(&relative)).find(|f| f.is_file()) else {
                    return Err(mlua::Error::runtime(format!("require {name:?}: no {relative} in eu4-harmony/{TESTS}/ or eu4-harmony/lua/")));
                };
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
    fake.store().windows.declare(extension.index(), &at.manifest.id, extension.windows());
    Ok(TestMod { extension, fake, calendar: Mutex::default() })
}

/// A mod a test opened, and its fake game.
struct TestMod {
    extension: Arc<Extension>,
    fake: Arc<Fake>,
    calendar: Mutex<Calendar>,
}

/// Where the fake game's time is, as the signals raised have it.
#[derive(Default)]
struct Calendar {
    /// The number of the session going on; 0 before the first.
    session: u64,
    /// The last day heard in the session.
    day: Option<Date>,
}

/// What the fake game gives a dispatch: the save data a test starts a
/// session with, and at a frame the windows the mod opened or closed itself.
/// Maps and clicks are raised one at a time instead.
struct Fed(Option<Value>, Vec<dispatch::WindowSeen>);

impl dispatch::Feed for Fed {
    fn session_begins(&mut self, _: u64) {}

    fn saved(&mut self, _: &str) -> Option<Value> {
        self.0.take()
    }

    fn map_lost(&mut self) -> Vec<usize> {
        Vec::new()
    }

    fn clicks(&mut self) -> Vec<dispatch::Click> {
        Vec::new()
    }

    fn windows(&mut self) -> Vec<dispatch::WindowSeen> {
        std::mem::take(&mut self.1)
    }

    fn popups_pressed(&mut self) -> Vec<dispatch::PopupPressed> {
        Vec::new()
    }
}

impl TestMod {
    fn calendar(&self) -> std::sync::MutexGuard<'_, Calendar> {
        self.calendar.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The mod hears `heard`, a session's start with `saved`.
    fn hear(&self, lua: &Lua, heard: &Heard, saved: Option<Value>) {
        dispatch::hear(std::slice::from_ref(&self.extension), heard, &mut Fed(saved, Vec::new()));
        self.note(lua);
    }

    /// The mod hears the day `date`, and its month and its year when it is
    /// the first heard in them, as the game raises them; first, as at the
    /// game's frame, its tasks waiting for the day are taken up.
    fn day(&self, lua: &Lua, date: Date) {
        let (month, year) = {
            let mut calendar = self.calendar();
            date.turned(calendar.day.replace(date))
        };
        *self.fake.today.lock().unwrap_or_else(|e| e.into_inner()) = Some(date);
        self.extension.poll();
        self.hear(lua, &Heard::of(&Signal::Day { date }), None);
        if month {
            self.hear(lua, &Heard::of(&Signal::Month { date }), None);
        }
        if year {
            self.hear(lua, &Heard::of(&Signal::Year { date }), None);
        }
    }

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

        // Signals, as the game raises them (`eu4_harmony_lua::dispatch`).
        methods.add_method("session", |lua, this, saved: LuaValue| {
            let saved = json(&saved)?;
            let session = {
                let mut calendar = this.calendar();
                calendar.session += 1;
                calendar.day = None;
                calendar.session
            };
            this.fake.store().going_on = true;
            *this.fake.today.lock().unwrap_or_else(|e| e.into_inner()) = None;
            this.hear(lua, &Heard::of(&Signal::Session { session }), Some(saved));
            Ok(())
        });
        methods.add_method("session_ended", |lua, this, reason: Option<String>| {
            let reason = reason.unwrap_or_else(|| "return".into());
            let reason: SessionEnd = serde_json::from_value(Value::String(reason.clone()))
                .map_err(|_| mlua::Error::runtime(format!("{}a session ends for return, load, new_game or country, not {reason}", here(lua))))?;
            // As in the game: the session is over as the mod hears it end,
            // and its windows and pop-ups close after.
            this.fake.store().going_on = false;
            this.hear(lua, &Heard::of(&Signal::SessionEnded { reason }), None);
            this.fake.store().windows.end_session();
            Ok(())
        });
        methods.add_method("day", |lua, this, date: LuaValue| {
            let text = match &date {
                LuaValue::String(s) => s.to_str()?.to_string(),
                LuaValue::Table(t) => format!("{}.{}.{}", t.get::<i64>("year")?, t.get::<i64>("month")?, t.get::<i64>("day")?),
                other => return Err(mlua::Error::runtime(format!("a date is \"1444.11.12\" or {{ year, month, day }}, not a {}", other.type_name()))),
            };
            let date = Date::parse(&text).ok_or_else(|| mlua::Error::runtime(format!("{text} is not a date")))?;
            this.day(lua, date);
            Ok(())
        });
        methods.add_method("raise", |lua, this, (kind, fields): (String, Option<LuaValue>)| {
            let named = Kind::named(&kind)
                .ok_or_else(|| mlua::Error::runtime(format!("{}there is no signal {kind}; eu4-harmony wiki signals lists them", here(lua))))?;
            if named == Kind::Session {
                return Err(mlua::Error::runtime(format!("{}mod:session(saved) starts a session, with the save data it began from", here(lua))));
            }
            let fields = fields.map(|f| json(&f)).transpose()?.unwrap_or_else(|| Value::Object(Default::default()));
            let heard = Heard::new(named, fields).map_err(|why| mlua::Error::runtime(format!("{}{why}", here(lua))))?;
            match heard.field("date").filter(|_| named == Kind::Day).and_then(lua_date) {
                Some(date) => this.day(lua, date),
                None => this.hear(lua, &heard, None),
            }
            Ok(())
        });
        methods.add_method("poll", |lua, this, ()| {
            this.extension.poll();
            this.note(lua);
            Ok(())
        });
        methods.add_method("frame", |lua, this, ()| {
            let seen = this.fake.store().windows.take_seen();
            dispatch::frame(std::slice::from_ref(&this.extension), &[], &mut Fed(None, seen));
            this.note(lua);
            Ok(())
        });
        methods.add_method("button", |lua, this, (name, click): (String, Option<Table>)| {
            let flag = |key: &str| -> mlua::Result<bool> { Ok(click.as_ref().map(|t| t.get::<Option<bool>>(key)).transpose()?.flatten().unwrap_or(false)) };
            let click = dispatch::Click {
                extension: this.extension.index(),
                button: name,
                right: flag("right")?,
                shift: flag("shift")?,
                ctrl: flag("ctrl")?,
                row: None,
            };
            dispatch::click(std::slice::from_ref(&this.extension), &click);
            this.note(lua);
            Ok(())
        });

        // The mod's windows and pop-ups, as the player uses them.
        methods.add_method("press", |lua, this, (window, widget, click): (String, String, Option<Table>)| {
            let flag = |key: &str| -> mlua::Result<bool> { Ok(click.as_ref().map(|t| t.get::<Option<bool>>(key)).transpose()?.flatten().unwrap_or(false)) };
            let row = click.as_ref().map(|t| t.get::<Option<u32>>("row")).transpose()?.flatten();
            pressable(this, &window, &widget, row).map_err(|e| mlua::Error::runtime(format!("{}{e}", here(lua))))?;
            let click = dispatch::Click {
                extension: this.extension.index(),
                button: format!("{window}.{widget}"),
                right: flag("right")?,
                shift: flag("shift")?,
                ctrl: flag("ctrl")?,
                row,
            };
            dispatch::click(std::slice::from_ref(&this.extension), &click);
            this.note(lua);
            Ok(())
        });
        methods.add_method("entry", |lua, this, window: String| {
            let has_entry = this.extension.windows().iter().any(|w| w.name == window && w.entry.is_some());
            if !has_entry {
                return Err(mlua::Error::runtime(format!("{}the mod's window {window:?} has no entry button", here(lua))));
            }
            let seen = {
                let mut store = this.fake.store();
                let open = store.windows.is_open(this.extension.index(), &window);
                store.windows.show(this.extension.index(), &window, !open)
            };
            for seen in seen {
                dispatch::window(std::slice::from_ref(&this.extension), &seen);
            }
            this.note(lua);
            Ok(())
        });
        methods.add_method("close", |lua, this, window: String| {
            if !this.fake.store().windows.is_open(this.extension.index(), &window) {
                return Err(mlua::Error::runtime(format!("{}the mod's window {window:?} is not open", here(lua))));
            }
            let seen = this.fake.store().windows.show(this.extension.index(), &window, false);
            for seen in seen {
                dispatch::window(std::slice::from_ref(&this.extension), &seen);
            }
            this.note(lua);
            Ok(())
        });
        methods.add_method("popups", |lua, this, ()| {
            let list = lua.create_table()?;
            for p in this.fake.store().windows.popups() {
                let t = lua.create_table()?;
                t.set("popup", p.number)?;
                t.set("title", p.popup.title.as_str())?;
                t.set("text", p.popup.text.as_str())?;
                t.set("header", p.popup.header.clone())?;
                t.set("buttons", lua.create_sequence_from(shown(&p.popup))?)?;
                list.push(t)?;
            }
            Ok(list)
        });
        methods.add_method("answer_popup", |lua, this, (number, button): (u32, u32)| {
            // The button's action runs, as at the game's next frame, and
            // closes the pop-up unless it says not to.
            let pressed = {
                let mut store = this.fake.store();
                store.windows.press(number, button).map_err(|e| mlua::Error::runtime(format!("{}{e}", here(lua))))?;
                store.windows.take_pressed()
            };
            for pressed in pressed {
                dispatch::popup_pressed(std::slice::from_ref(&this.extension), &pressed);
            }
            this.note(lua);
            Ok(())
        });
        methods.add_method("army_ordered", |lua, this, army: u64| {
            this.hear(lua, &Heard::of(&Signal::ArmyOrdered { army: ArmyId(army) }), None);
            Ok(())
        });
        methods.add_method("army_defeated", |lua, this, army: u64| {
            this.hear(lua, &Heard::of(&Signal::ArmyDefeated { army: ArmyId(army) }), None);
            Ok(())
        });
        methods.add_method("map_lost", |lua, this, ()| {
            dispatch::map_lost(std::slice::from_ref(&this.extension), this.extension.index());
            this.note(lua);
            Ok(())
        });
        methods.add_method("save", |lua, this, ()| match this.extension.save_data() {
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
        methods.add_method("window", |lua, this, name: String| {
            if !this.extension.windows().iter().any(|w| w.name == name) {
                return Err(mlua::Error::runtime(format!("{}the mod has no window {name:?}", here(lua))));
            }
            let window =
                this.fake.store().window(this.extension.index(), &name, |key| eu4_harmony_lua::Host::text(this.fake.as_ref(), this.extension.id(), key));
            let t = lua.create_table()?;
            let Some(window) = window else {
                t.set("open", false)?;
                return Ok(t);
            };
            let properties = |p: &BTreeMap<&'static str, Value>| -> mlua::Result<Table> {
                let t = lua.create_table()?;
                for (name, value) in p {
                    t.set(*name, values::to_lua(lua, value)?)?;
                }
                Ok(t)
            };
            for (widget, p) in &window.widgets {
                t.set(widget.as_str(), properties(p)?)?;
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
                        r.set(name.as_str(), properties(p)?)?;
                    }
                    all.push(r)?;
                }
                widget.set("rows", all)?;
                t.set(list.as_str(), widget)?;
            }
            t.set("open", window.open)?;
            Ok(t)
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

/// Whether the player can press `widget` of the mod's `window`, in the
/// list's row `row` when one is given, or why not.
fn pressable(this: &TestMod, window: &str, widget: &str, row: Option<u32>) -> Result<(), String> {
    let Some(layout) = this.extension.windows().iter().find(|w| w.name == window) else { return Err(format!("the mod has no window {window:?}")) };
    if !this.fake.store().windows.is_open(this.extension.index(), window) {
        return Err(format!("the mod's window {window:?} is not open"));
    }
    let button = |w: &layout::Placed| matches!(w.kind, layout::Kind::Button { .. });
    let found = match row {
        None => layout.widget(widget).is_some_and(button),
        Some(n) => {
            let rows = this.fake.store().windows.wanted(this.extension.index(), window).map(|w| w.rows.clone()).unwrap_or_default();
            let in_list = layout.widgets.iter().find_map(|w| match &w.kind {
                layout::Kind::List { row, .. } if row.widget(widget).is_some_and(button) => w.name.clone(),
                _ => None,
            });
            match in_list {
                Some(list) if (1..=rows.get(&list).map_or(0, Vec::len) as u32).contains(&n) => true,
                Some(list) => return Err(format!("the list {list} has no row {n}")),
                None => false,
            }
        }
    };
    if !found {
        return Err(format!("the window {window:?} has no button {widget}{}", if row.is_some() { " in a list's row" } else { "" }));
    }
    Ok(())
}

/// A pop-up's buttons' words as they stand, left to right, as remote
/// control's popup.list gives them: the game's OK when the mod gave none.
fn shown(popup: &eu4_harmony_lua::Popup) -> Vec<String> {
    if popup.buttons.is_empty() {
        vec!["OK".to_string()]
    } else {
        popup.buttons.clone()
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
        let opened = Arc::new(Opened { root: dir.clone(), manifest, name: "G".into(), scratch: dir.join("scratch") });
        let lua = harness(&opened).unwrap();
        for method in named {
            let kind: String = lua.load(format!("return type(open().{method})")).eval().unwrap();
            assert_eq!(kind, "function", "the testing guide names mod:{method}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
