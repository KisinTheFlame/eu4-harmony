//! The fake game: the Lua host's other host (`eu4_harmony_lua::Host`), a
//! data store in place of the running game, for `eu4-harmony test`.
//!
//! A test says what each capability answers (a value, a refusal, an answer
//! that comes later, or a Lua function of the request), raises signals, gives
//! `http.fetch`'s requests their responses, and reads back what the mod did:
//! the capabilities it called, its interface elements, its windows and
//! pop-ups, the map, its log and its save data. Requests are checked as the core checks them, by the
//! catalogue: an argument of the wrong kind is the same error. A capability
//! no test answered answers `not_ready` and is reported. The mod's windows
//! and pop-ups are kept by the Lua host's own rules
//! (`eu4_harmony_lua::windows::Windows`), as in the game: a window opens and
//! a pop-up shows only in a session, and a session's end closes them all.
use eu4_harmony_loc::{Catalog, Language};
use eu4_harmony_lua::{
    fetch, values,
    windows::{HostedElements, Windows},
    Answered, Called, Element, Extension, Host, Pending, Popup, Property, WindowChange,
};
use eu4_harmony_remote::{Code, Date, Error, Request};
use mlua::{Function, Lua, Value as LuaValue, WeakLua};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

/// What a capability answers in a test.
pub enum Answer {
    Value(Value),
    Refusal(Code, String),
    /// The answer comes when the test replies.
    Later,
    /// A function of the request, in the test's Lua state, returning the
    /// answer, or `nil, code, message` for a refusal.
    Function(Function),
}

/// A slot an answer is put in, and the pending answer that takes it.
type Slot<T> = Arc<Mutex<Option<T>>>;

struct Taken<T>(Slot<T>);

impl<T: Send> Pending<T> for Taken<T> {
    fn poll(&mut self) -> Option<T> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// The fake game's day: the last a test raised in the session, none before
/// the first.
pub type Today = Arc<Mutex<Option<Date>>>;

/// A `time.wait` for the day `until`, answered once a day the test raises
/// reaches it.
struct Until {
    today: Today,
    until: Date,
}

impl Pending<Answered> for Until {
    fn poll(&mut self) -> Option<Answered> {
        let today = (*self.today.lock().unwrap_or_else(|e| e.into_inner()))?;
        (today >= self.until).then(|| Ok(lua_form(today)))
    }
}

/// A date in Lua's form, `{ year, month, day }`.
pub fn lua_form(date: Date) -> Value {
    let (year, month, day) = date.ymd();
    serde_json::json!({ "year": year, "month": month, "day": day })
}

/// A date from Lua's form.
pub fn lua_date(value: &Value) -> Option<Date> {
    let part = |name: &str| value.get(name).and_then(Value::as_i64);
    Date::from_ymd(i32::try_from(part("year")?).ok()?, u32::try_from(part("month")?).ok()?, u32::try_from(part("day")?).ok()?)
}

/// What the fake game holds.
#[derive(Default)]
pub struct Store {
    pub answers: HashMap<String, Answer>,
    /// Every capability the mod called, as the catalogue's request.
    pub calls: Vec<Value>,
    /// Capabilities called with no answer given.
    pub unanswered: Vec<String>,
    /// Answers that come later, by capability, oldest first.
    pub later: Vec<(String, Slot<Answered>)>,
    /// Hosted interface elements (`game.ui.<element>`), retaining semantic
    /// descriptions so language changes re-resolve dynamically.
    pub hosted: HostedElements,
    /// The map as the mod painted it, `[r, g, b, a]` by province; none when
    /// it holds no map.
    pub map: Option<BTreeMap<u32, [u8; 4]>>,
    pub log: Vec<String>,
    pub requests: Vec<(fetch::Request, Slot<fetch::Answer>)>,
    /// What the player was told: errors and stops.
    pub told: Vec<String>,
    /// Answers the test's functions gave that are not their capability's
    /// shape, which fail the test.
    pub misshapen: Vec<String>,
    /// The world the test described, which answers what no answer of the
    /// test's does.
    pub world: Option<crate::world::World>,
    /// The mod's windows and pop-ups, kept by the rules the game keeps
    /// them by: what it opened or closed itself it hears at the next frame.
    pub windows: Windows,
    /// A session goes on: from `mod:session()` to `mod:session_ended()`.
    pub going_on: bool,
}

/// One of the mod's windows as a test reads it back.
#[derive(Default)]
pub struct FakeWindow {
    pub open: bool,
    /// Each game widget's properties as the mod set them, by name; its
    /// entry button's as `entry`.
    pub widgets: BTreeMap<String, Properties>,
    /// Each list's rows, each its game widgets' properties by name.
    pub rows: BTreeMap<String, Vec<Row>>,
}

/// A game widget's properties as the mod set them, by property.
pub type Properties = BTreeMap<&'static str, Value>;
/// A list's row: its game widgets' properties by name.
pub type Row = BTreeMap<String, Properties>;

/// A property as a test reads it back.
fn property_json(value: &Property, words: &impl Fn(&str) -> Option<String>) -> (&'static str, Value) {
    let json = match value {
        Property::Visible(b) | Property::Enabled(b) => Value::Bool(*b),
        Property::Country(t) => Value::String(t.clone()),
        Property::Text(t) | Property::Tooltip(t) => Value::String(t.resolve(words)),
        Property::Frame(n) => Value::from(*n),
        Property::Value(v) => serde_json::Number::from_f64(*v).map_or(Value::Null, Value::Number),
        Property::Number(v) => v.and_then(serde_json::Number::from_f64).map_or(Value::Null, Value::Number),
    };
    (value.name(), json)
}

impl Store {
    /// `extension`'s window `name` resolved in the current player language,
    /// merging layout defaults, dynamic overrides, list rows and entry formatting.
    pub fn window(&self, extension: usize, name: &str, words: impl Fn(&str) -> Option<String>) -> Option<FakeWindow> {
        let resolved = self.windows.resolved_window(extension, name, &words)?;
        let mut widgets: BTreeMap<String, Properties> =
            resolved.widgets.iter().map(|(widget, set)| (widget.clone(), set.values().map(|p| property_json(p, &words)).collect())).collect();
        if let Some(entry) = resolved.entry {
            let mut props = Properties::new();
            props.insert("visible", Value::Bool(entry.visible));
            props.insert("enabled", Value::Bool(entry.enabled));
            props.insert("frame", Value::from(entry.frame));
            props.insert("value", entry.value.and_then(serde_json::Number::from_f64).map_or(Value::Null, Value::Number));
            props.insert("text", Value::String(entry.text));
            props.insert("tooltip", Value::String(entry.tooltip));
            widgets.insert(eu4_harmony_lua::layout::ENTRY.into(), props);
        }
        let rows = resolved
            .rows
            .iter()
            .map(|(list, rows)| {
                let rows = rows
                    .iter()
                    .map(|row| row.iter().map(|(cell, set)| (cell.clone(), set.values().map(|p| property_json(p, &words)).collect())).collect())
                    .collect();
                (list.clone(), rows)
            })
            .collect();
        Some(FakeWindow { open: resolved.open, widgets, rows })
    }

    /// `extension`'s hosted element `name`, dynamically resolved in the current
    /// player language.
    pub fn element(&self, extension: usize, name: &str, words: impl Fn(&str) -> Option<String>) -> Properties {
        let mut out = Properties::new();
        let Some(state) = self.hosted.get(extension, name) else { return out };
        if let Some(visible) = state.visible {
            out.insert("visible", Value::Bool(visible));
        }
        if let Some(enabled) = state.enabled {
            out.insert("enabled", Value::Bool(enabled));
        }
        if let Some(text) = &state.text {
            out.insert("text", Value::String(text.resolve(&words)));
        }
        if state.tooltip_set {
            out.insert("tooltip", Value::String(state.tooltip.resolve(&words)));
        }
        out
    }
}

/// Whether `answer` (JSON in Lua's form) is the shape of `capability`'s
/// result, or why not. A field left out is allowed.
pub fn check(capability: &str, answer: &Value) -> Result<(), String> {
    match eu4_harmony_remote::result_shape(capability) {
        Some(shape) => shape.check(answer).map_err(|why| format!("{capability}'s answer: {why}")),
        None => Err(format!("there is no capability {capability}")),
    }
}

/// The fake game, for one mod.
pub struct Fake {
    pub store: Mutex<Store>,
    texts: Catalog,
    pub language: Mutex<Language>,
    pub today: Today,
    /// The test's Lua state, for answers that are functions; weak, as the
    /// state holds the fake game in turn.
    tests: WeakLua,
}

impl Fake {
    pub fn new(texts: Catalog, tests: &Lua) -> Self {
        Self { store: Mutex::new(Store::default()), texts, language: Mutex::new(Language::English), today: Today::default(), tests: tests.weak() }
    }

    /// `time.wait`, as the game answers it: once the days the test raises
    /// reach the date, counted from the last day raised, or else the world's
    /// date, or else 1444.11.11.
    fn wait(&self, request: &Value, store: &Store) -> Called {
        let days = request.get("days").and_then(Value::as_u64).and_then(|d| u32::try_from(d).ok()).unwrap_or(0);
        let today = *self.today.lock().unwrap_or_else(|e| e.into_inner());
        let from = today.or_else(|| store.world.as_ref().and_then(|w| lua_date(&w.date()))).or_else(|| Date::from_ymd(1444, 11, 11));
        match from.map(|from| eu4_harmony_checks::time::days_after(from, days)) {
            Some(Ok(until)) => Called::Later(Box::new(Until { today: self.today.clone(), until })),
            Some(Err(refusal)) => Called::Done(Err(refusal.into())),
            None => Called::Done(Err(Error::new(Code::Internal, "the fake game has no date"))),
        }
    }

    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The answer `function` gives to `request`.
    fn call(&self, function: &Function, request: &Value) -> Answered {
        let internal = |e: mlua::Error| Error::new(Code::Internal, format!("the test's answer failed: {e}"));
        let tests = self.tests.try_upgrade().ok_or_else(|| Error::new(Code::Internal, "the test has ended"))?;
        let argument = values::to_lua(&tests, request).map_err(internal)?;
        let returned: mlua::MultiValue = function.call(argument).map_err(internal)?;
        let mut returned = returned.into_iter();
        match (returned.next(), returned.next(), returned.next()) {
            (Some(LuaValue::Nil) | None, Some(LuaValue::String(code)), message) => {
                let code = code.to_str().map_err(&internal)?.to_string();
                let message = match message {
                    Some(LuaValue::String(m)) => m.to_str().map_err(&internal)?.to_string(),
                    _ => String::new(),
                };
                Err(Error::new(code_named(&code).ok_or_else(|| Error::new(Code::Internal, format!("no error code {code}")))?, message))
            }
            (Some(value), ..) => values::to_json(&value).map_err(|e| Error::new(Code::Internal, format!("the test's answer cannot be an answer: {e}"))),
            (None, ..) => Ok(Value::Null),
        }
    }
}

/// The error code named `name`, as `err.code` writes it.
pub fn code_named(name: &str) -> Option<Code> {
    Code::ALL.iter().copied().find(|c| c.name() == name)
}

impl Host for Fake {
    fn run(&self, _: usize, request: Request) -> Called {
        let json = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(&request)).unwrap_or(Value::Null);
        let name = json.get("capability").and_then(Value::as_str).unwrap_or_default().to_string();
        let mut store = self.store();
        store.calls.push(json.clone());
        let answered = match store.answers.get(&name) {
            None if name == "time.wait" => return self.wait(&json, &store),
            None if store.world.is_some() && crate::world::ANSWERS.contains(&name.as_str()) => {
                store.world.as_mut().and_then(|w| w.answer(&name, &json)).unwrap_or(Ok(Value::Null))
            }
            None => {
                store.unanswered.push(name.clone());
                Err(Error::new(Code::NotReady, format!("the fake game has no answer for {name}; give one with mod:answer(\"{name}\", ...)")))
            }
            Some(Answer::Value(value)) => Ok(value.clone()),
            Some(Answer::Refusal(code, message)) => Err(Error::new(*code, message.clone())),
            Some(Answer::Later) => {
                let slot = Arc::new(Mutex::new(None));
                store.later.push((name, slot.clone()));
                return Called::Later(Box::new(Taken(slot)));
            }
            Some(Answer::Function(function)) => {
                let function = function.clone();
                drop(store);
                let answered = self.call(&function, &json);
                if let Err(why) = answered.as_ref().map_or(Ok(()), |answer| check(&name, answer)) {
                    self.store().misshapen.push(why.clone());
                    return Called::Done(Err(Error::new(Code::Internal, why)));
                }
                return Called::Done(answered);
            }
        };
        Called::Done(answered)
    }

    fn set_element(&self, extension: usize, element: &str, value: Element) -> Result<(), String> {
        self.store().hosted.set(extension, element, value);
        Ok(())
    }

    fn window(&self, extension: usize, change: WindowChange) -> Result<(), String> {
        let mut store = self.store();
        let in_game = store.going_on;
        store.windows.change(extension, change, in_game)
    }

    fn tooltip(&self, extension: usize, window: Option<&str>, widget: &str) -> Result<eu4_harmony_lua::Text, String> {
        let store = self.store();
        match window {
            Some(window) => store.windows.tooltip(extension, window, widget).ok_or_else(|| format!("no tooltip target {window}.{widget}")),
            None => Ok(store.hosted.tooltip(extension, widget)),
        }
    }

    fn popup(&self, extension: usize, popup: Popup) -> Result<u32, String> {
        let mut store = self.store();
        let in_game = store.going_on;
        store.windows.popup(extension, popup, in_game)
    }

    fn close_popup(&self, extension: usize, popup: u32) -> Result<(), String> {
        self.store().windows.close_popup(extension, popup);
        Ok(())
    }

    fn paint(&self, _: usize, colors: &[(u32, [u8; 4])]) -> Result<(), Error> {
        self.store().map = Some(colors.iter().copied().collect());
        Ok(())
    }

    fn release_map(&self, _: usize) -> Result<(), Error> {
        self.store().map = None;
        Ok(())
    }

    fn text(&self, _: &str, key: &str) -> Option<String> {
        // As the core gives it: escapes such as \n written out.
        self.texts.text(*self.language.lock().unwrap_or_else(|e| e.into_inner()), key)
    }

    fn language(&self) -> String {
        self.language.lock().unwrap_or_else(|e| e.into_inner()).id().to_string()
    }

    fn fetch(&self, request: fetch::Request) -> Box<dyn Pending<fetch::Answer>> {
        let slot = Arc::new(Mutex::new(None));
        self.store().requests.push((request, slot.clone()));
        Box::new(Taken(slot))
    }

    fn log(&self, _: &str, line: &str) {
        self.store().log.push(line.to_string());
    }

    fn session(&self) -> u64 {
        1
    }

    fn first_error(&self, _: &Extension, from: &str, message: &str) {
        self.store().told.push(format!("error in {from}: {message}"));
    }

    fn stopped(&self, extension: &Extension, reason: &str) {
        let mut store = self.store();
        store.hosted.hide(extension.index());
        store.windows.forget(extension.index());
        store.map = None;
        store.told.push(format!("stopped: {reason}"));
    }
}
