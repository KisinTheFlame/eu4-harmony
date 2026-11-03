//! The fake game: the Lua host's other host (`eu4_harmony_lua::Host`), a
//! data store in place of the running game, for `eu4-harmony test`.
//!
//! A test says what each capability answers (a value, a refusal, an answer
//! that comes later, or a Lua function of the request), raises events, gives
//! `http.fetch`'s requests their responses, and reads back what the mod did:
//! the capabilities it called, its interface elements, the map, its log and
//! its save data. Requests are checked as the core checks them, by the
//! catalogue: an argument of the wrong kind is the same error. A capability
//! no test answered answers `not_ready` and is reported.
use eu4_harmony_loc::{Catalog, Language};
use eu4_harmony_lua::{fetch, values, Answered, Called, Element, Extension, Host, Pending};
use eu4_harmony_remote::{Code, Error, Request};
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
    /// Each element's properties as the mod set them.
    pub elements: BTreeMap<String, BTreeMap<&'static str, Value>>,
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
    /// The test's Lua state, for answers that are functions; weak, as the
    /// state holds the fake game in turn.
    tests: WeakLua,
}

impl Fake {
    pub fn new(texts: Catalog, tests: &Lua) -> Self {
        Self { store: Mutex::new(Store::default()), texts, language: Mutex::new(Language::English), tests: tests.weak() }
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

    fn set_element(&self, _: usize, element: &str, value: Element) -> Result<(), String> {
        let (property, value) = match value {
            Element::Visible(on) => ("visible", Value::Bool(on)),
            Element::Enabled(on) => ("enabled", Value::Bool(on)),
            Element::Text(text) => ("text", Value::String(text)),
            Element::Tooltip(text) => ("tooltip", Value::String(text)),
        };
        self.store().elements.entry(element.to_string()).or_default().insert(property, value);
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

    fn stopped(&self, _: &Extension, reason: &str) {
        self.store().told.push(format!("stopped: {reason}"));
    }
}
