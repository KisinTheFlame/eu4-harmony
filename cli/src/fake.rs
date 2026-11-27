//! The fake game: the Lua host's other host (`eu4_harmony_lua::Host`), a
//! data store and session controller in place of the running game, for
//! `eu4-harmony test`.
//!
//! [`FakeMod`] owns an opened extension, its private host state, its session
//! calendar, its optional [`crate::world::World`], and pending capability and
//! HTTP queues. A caller answers capabilities (a value, a refusal, an answer
//! that comes later, or a Lua function of the request), raises signals, gives
//! `http.fetch`'s requests their responses, interacts with hosted elements,
//! extension windows and pop-ups, reloads code in place, and reads back what
//! the mod did.
//!
//! Requests are checked as the core checks them, by the capability catalogue:
//! an argument of the wrong kind is the same error. A capability no test
//! answered answers `not_ready` and is recorded in [`SessionNotes`]. The mod's
//! windows and pop-ups are kept by the Lua host's own rules
//! (`eu4_harmony_lua::windows::Windows`), as in the game: a window opens and a
//! pop-up shows only in a session, and a session's end closes them all.
use eu4_harmony_loc::{Catalog, Language};
use eu4_harmony_lua::{
    dispatch::{self, Heard},
    fetch, values,
    windows::{HostedElements, Windows},
    Answered, Called, Element, Extension, Host, Mod, Pending, Popup, Property, WindowChange,
};
use eu4_harmony_manifest::Manifest;
use eu4_harmony_remote::{
    signals::{Kind, SessionEnd, Signal},
    ArmyId, Code, Date, Error, Request,
};
use mlua::{Function, Lua, Value as LuaValue, WeakLua};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// Identifies a Harmony mod opened against the fake game.
pub struct ModSource {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub name: String,
}

/// Modifier keys and optional list row for a button press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClickOptions {
    pub right: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub row: Option<u32>,
}

/// Outcome of an extension reload in the fake game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReloadReport {
    pub session: bool,
    pub saved_kept: bool,
    pub lost: Option<String>,
}

/// An open pop-up as a test reads it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakePopup {
    pub number: u32,
    pub title: String,
    pub text: String,
    pub header: Option<String>,
    pub buttons: Vec<String>,
}

/// An HTTP request sent by the extension, as a test reads it back.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timeout: f64,
}

/// Notes collected while driving an extension: capabilities left unanswered,
/// and misshapen answers returned by Lua answer functions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionNotes {
    pub unanswered: Vec<String>,
    pub misshapen: Vec<String>,
}

/// Where the fake game's time and session stand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Calendar {
    /// Active session number; `0` before the first `session()`.
    pub session: u64,
    /// Whether a session is open (`session()` to `session_ended()`).
    pub going_on: bool,
    /// Last day raised in the session, or `None` before the first day.
    pub today: Option<Date>,
}

/// One of the mod's windows as a test reads it back.
#[derive(Clone, Debug, Default, PartialEq)]
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

/// A date in Lua's form, `{ year, month, day }`.
pub fn lua_form(date: Date) -> Value {
    let (year, month, day) = date.ymd();
    serde_json::json!({ "year": year, "month": month, "day": day })
}

/// Whether `answer` (JSON in Lua's form) is the shape of `capability`'s
/// result, or why not. A field left out is allowed.
pub fn check(capability: &str, answer: &Value) -> Result<(), String> {
    match eu4_harmony_remote::result_shape(capability) {
        Some(shape) => shape.check(answer).map_err(|why| format!("{capability}'s answer: {why}")),
        None => Err(format!("there is no capability {capability}")),
    }
}

/// A mod opened against a fake game of its own, driving its handlers and
/// exposing typed readbacks without leaking internal host locks or stores.
pub struct FakeMod {
    source: Arc<ModSource>,
    data: PathBuf,
    extension: Mutex<Arc<Extension>>,
    fake: Arc<Fake>,
}

impl FakeMod {
    /// Opens a mod against a fake game of its own. Pass `tests` when driving
    /// from the Lua test harness so Lua function answers can execute in that
    /// state; pass `None` when driving directly from Rust.
    pub fn open(source: impl Into<Arc<ModSource>>, data: PathBuf, tests: Option<&Lua>) -> Result<Self, String> {
        let source = source.into();
        let harmony = source.root.join("eu4-harmony");
        let layout = eu4_harmony_manifest::layout(&harmony);
        let mut slots = HashMap::new();
        let report = eu4_harmony_manifest::ui::read_fragments(&source.root, &source.manifest.id, &layout.ui, &mut slots);
        if !report.problems.is_empty() {
            return Err(report.problems.join("\n"));
        }
        let (texts, _) = eu4_harmony_loc::load_dir(&harmony.join("localisation"));
        let fake = Arc::new(Fake::new(texts, tests.map(Lua::weak)));
        let m = Mod { id: &source.manifest.id, name: &source.name, harmony, permissions: &source.manifest.permissions, data: Some(data.clone()) };
        let extension = eu4_harmony_lua::open(0, &m, fake.clone())?;
        {
            let mut store = fake.store();
            store.windows.declare(extension.index(), &source.manifest.id, extension.windows());
            if !layout.ui.is_empty() {
                store.hosted.declare(extension.index(), report.elements.into_iter().map(|e| (e.name, e.kind.is_button())));
            }
        }
        Ok(Self { source, data, extension: Mutex::new(extension), fake })
    }

    fn ext(&self) -> Arc<Extension> {
        self.extension.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    // ---------- answers and environment ----------

    /// Sets a static JSON value answer for `capability`, checked against its
    /// result shape in the capability catalogue.
    pub fn answer(&self, capability: &str, value: Value) -> Result<(), String> {
        check(capability, &value)?;
        self.fake.store().answers.insert(capability.to_string(), Answer::Value(value));
        Ok(())
    }

    /// Sets a Lua function answer for `capability`, evaluated on each call in
    /// the test harness's Lua state.
    pub fn answer_function(&self, capability: &str, function: Function) -> Result<(), String> {
        if eu4_harmony_remote::result_shape(capability).is_none() {
            return Err(format!("there is no capability {capability}"));
        }
        self.fake.store().answers.insert(capability.to_string(), Answer::Function(function));
        Ok(())
    }

    /// Makes `capability` return a refusal with the named error code.
    pub fn refuse(&self, capability: &str, code: &str, message: Option<String>) -> Result<(), String> {
        let code = parse_code(code)?;
        self.fake.store().answers.insert(capability.to_string(), Answer::Refusal(code, message.unwrap_or_default()));
        Ok(())
    }

    /// Makes calls to `capability` wait until [`Self::reply`] or
    /// [`Self::reply_refusal`] delivers an answer.
    pub fn later(&self, capability: &str) {
        self.fake.store().answers.insert(capability.to_string(), Answer::Later);
    }

    /// Delivers `value` to the oldest pending [`Self::later`] call for
    /// `capability`, checked against its result shape.
    pub fn reply(&self, capability: &str, value: Value) -> Result<(), String> {
        check(capability, &value)?;
        self.deliver_reply(capability, Ok(value))
    }

    /// Delivers a refusal to the oldest pending [`Self::later`] call for
    /// `capability`.
    pub fn reply_refusal(&self, capability: &str, code: &str, message: Option<String>) -> Result<(), String> {
        let code = parse_code(code)?;
        self.deliver_reply(capability, Err(Error::new(code, message.unwrap_or_default())))
    }

    fn deliver_reply(&self, capability: &str, answer: Answered) -> Result<(), String> {
        let mut store = self.fake.store();
        let Some(at) = store.later.iter().position(|(name, _)| name == capability) else {
            return Err(format!("no answer of {capability} is still to come; mod:later(\"{capability}\") makes it come later"));
        };
        let (_, slot) = store.later.remove(at);
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(answer);
        Ok(())
    }

    /// Delivers an HTTP response to the 1-based `number`th request sent by the
    /// extension.
    pub fn respond(&self, number: usize, response: fetch::Response) -> Result<(), String> {
        self.deliver_request(number, Ok(response))
    }

    /// Fails the 1-based `number`th HTTP request with `"timeout"`, `"network"`,
    /// or `"too_large"`.
    pub fn fail_request(&self, number: usize, kind: &str, message: Option<String>) -> Result<(), String> {
        let failure = match kind {
            "timeout" => fetch::Failure::Timeout,
            "too_large" => fetch::Failure::TooLarge,
            "network" => fetch::Failure::Network(message.unwrap_or_else(|| "the host cannot be reached".into())),
            other => return Err(format!("a request fails with timeout, network or too_large, not {other}")),
        };
        self.deliver_request(number, Err(failure))
    }

    fn deliver_request(&self, number: usize, answer: fetch::Answer) -> Result<(), String> {
        let store = self.fake.store();
        let Some((_, slot)) = number.checked_sub(1).and_then(|i| store.requests.get(i)) else {
            return Err(format!("there is no request {number}; the mod sent {}", store.requests.len()));
        };
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(answer);
        Ok(())
    }

    /// Installs a [`crate::world::World`] that answers capabilities not
    /// explicitly answered by the test.
    pub fn set_world(&self, world: Value) -> Result<(), String> {
        let world = crate::world::World::new(world)?;
        self.fake.store().world = Some(world);
        Ok(())
    }

    /// Sets the player language used to resolve localisation texts.
    pub fn set_language(&self, name: &str) -> Result<(), String> {
        let language = Language::from_id(name).ok_or_else(|| format!("no language {name}"))?;
        *self.fake.language.lock().unwrap_or_else(|e| e.into_inner()) = language;
        Ok(())
    }

    // ---------- signals and game loop ----------

    fn hear(&self, heard: &Heard, saved: Option<Value>) {
        let ext = self.ext();
        dispatch::hear(std::slice::from_ref(&ext), heard, &mut Fed(saved, Vec::new()));
    }

    /// Starts a new session and delivers `on_session` with `saved`.
    pub fn session(&self, saved: Option<Value>) {
        let session = self.fake.start_session();
        self.hear(&Heard::of(&Signal::Session { session }), saved);
    }

    /// Ends the current session (`return`, `load`, `new_game`, or `country`)
    /// and closes all open extension windows and pop-ups afterward.
    pub fn session_ended(&self, reason: Option<&str>) -> Result<(), String> {
        let reason_name = reason.unwrap_or("return");
        let reason: SessionEnd = serde_json::from_value(Value::String(reason_name.to_string()))
            .map_err(|_| format!("a session ends for return, load, new_game or country, not {reason_name}"))?;
        // As in the game: the session is over as the mod hears it end, and its
        // windows and pop-ups close after.
        self.fake.end_session();
        self.hear(&Heard::of(&Signal::SessionEnded { reason }), None);
        self.fake.store().windows.end_session();
        Ok(())
    }

    /// Advances to `date`, polls tasks waiting on the date, and raises `day`
    /// plus `month` and `year` when turning into a new one.
    pub fn day(&self, date: Date) {
        let (month, year) = self.fake.advance_day(date);
        self.ext().poll();
        self.hear(&Heard::of(&Signal::Day { date }), None);
        if month {
            self.hear(&Heard::of(&Signal::Month { date }), None);
        }
        if year {
            self.hear(&Heard::of(&Signal::Year { date }), None);
        }
    }

    /// Raises any signal by name with fields held to its catalogue shape.
    pub fn raise(&self, kind: &str, fields: Value) -> Result<(), String> {
        let named = Kind::named(kind).ok_or_else(|| format!("there is no signal {kind}; eu4-harmony wiki signals lists them"))?;
        if named == Kind::Session {
            return Err("mod:session(saved) starts a session, with the save data it began from".into());
        }
        let heard = Heard::new(named, fields)?;
        match heard.field("date").filter(|_| named == Kind::Day).and_then(lua_date) {
            Some(date) => self.day(date),
            None => self.hear(&heard, None),
        }
        Ok(())
    }

    /// Resumes waiting tasks whose answers have arrived.
    pub fn poll(&self) {
        self.ext().poll();
    }

    /// Runs a safe frame: polls waiting tasks, delivers pending `on_window`
    /// transitions, and calls `on_frame`.
    pub fn frame(&self) {
        let ext = self.ext();
        let seen = self.fake.store().windows.take_seen();
        dispatch::frame(std::slice::from_ref(&ext), &[], &mut Fed(None, seen));
    }

    /// Raises `army_ordered` for `army`.
    pub fn army_ordered(&self, army: u64) {
        self.hear(&Heard::of(&Signal::ArmyOrdered { army: ArmyId(army) }), None);
    }

    /// Raises `army_defeated` for `army`.
    pub fn army_defeated(&self, army: u64) {
        self.hear(&Heard::of(&Signal::ArmyDefeated { army: ArmyId(army) }), None);
    }

    /// Delivers `on_map_lost` to the extension.
    pub fn map_lost(&self) {
        let ext = self.ext();
        dispatch::map_lost(std::slice::from_ref(&ext), ext.index());
    }

    // ---------- interface interactions ----------

    /// Presses a hosted button spliced into a host window.
    pub fn button(&self, name: &str, click: ClickOptions) -> Result<(), String> {
        let ext = self.ext();
        self.fake.store().hosted.check_press(ext.index(), name)?;
        let click = dispatch::Click { extension: ext.index(), button: name.to_string(), right: click.right, shift: click.shift, ctrl: click.ctrl, row: None };
        dispatch::click(std::slice::from_ref(&ext), &click);
        Ok(())
    }

    /// Presses a button game widget inside an open extension window.
    pub fn press(&self, window: &str, widget: &str, click: ClickOptions) -> Result<(), String> {
        let ext = self.ext();
        self.fake.store().windows.check_press(ext.index(), window, widget, click.row)?;
        let click = dispatch::Click {
            extension: ext.index(),
            button: format!("{window}.{widget}"),
            right: click.right,
            shift: click.shift,
            ctrl: click.ctrl,
            row: click.row,
        };
        dispatch::click(std::slice::from_ref(&ext), &click);
        Ok(())
    }

    /// Presses an extension window's entry button, toggling it and delivering
    /// `on_window` immediately.
    pub fn entry(&self, window: &str) -> Result<(), String> {
        let ext = self.ext();
        let seen = self.fake.store().windows.press_entry(ext.index(), window)?;
        for seen in seen {
            dispatch::window(std::slice::from_ref(&ext), &seen);
        }
        Ok(())
    }

    /// Closes an open extension window as the player would, delivering
    /// `on_window` immediately.
    pub fn close(&self, window: &str) -> Result<(), String> {
        let ext = self.ext();
        let seen = {
            let mut store = self.fake.store();
            if !store.windows.is_open(ext.index(), window) {
                return Err(format!("the mod's window {window:?} is not open"));
            }
            store.windows.show(ext.index(), window, false)
        };
        for seen in seen {
            dispatch::window(std::slice::from_ref(&ext), &seen);
        }
        Ok(())
    }

    /// Presses the 1-based `button` of the open pop-up `number`.
    pub fn answer_popup(&self, number: u32, button: u32) -> Result<(), String> {
        let ext = self.ext();
        let pressed = {
            let mut store = self.fake.store();
            store.windows.press(number, button)?;
            store.windows.take_pressed()
        };
        for pressed in pressed {
            dispatch::popup_pressed(std::slice::from_ref(&ext), &pressed);
        }
        Ok(())
    }

    // ---------- persistence and extension reload ----------

    /// Calls the extension's `save_data` handler.
    pub fn save(&self) -> Result<Option<Value>, String> {
        match self.ext().save_data() {
            None | Some(Ok(None)) => Ok(None),
            Some(Ok(Some(data))) => Ok(Some(data)),
            Some(Err(message)) => Err(message),
        }
    }

    /// Reloads the extension's Lua scripts and localisation files in place,
    /// carrying save data across and delivering `on_session` when a session is
    /// going on through the same `eu4_harmony_lua::reload` transaction as the
    /// core.
    pub fn reload(&self) -> Result<ReloadReport, String> {
        let old = self.ext();
        let in_session = self.fake.calendar().going_on;
        let req = eu4_harmony_lua::Reload {
            dir: &self.source.root,
            id: &self.source.manifest.id,
            name: &self.source.name,
            permissions: &self.source.manifest.permissions,
            data: Some(self.data.clone()),
            in_session,
        };
        let reloaded = eu4_harmony_lua::reload(
            &old,
            &req,
            self.fake.clone(),
            |harmony| {
                let (texts, problems) = eu4_harmony_loc::load_dir(&harmony.join("localisation"));
                (problems, self.fake.replace_texts(texts))
            },
            |prev_texts| {
                self.fake.replace_texts(prev_texts);
            },
            |new, _| {
                *self.extension.lock().unwrap_or_else(|e| e.into_inner()) = new.clone();
            },
        )?;
        Ok(ReloadReport { session: in_session, saved_kept: reloaded.saved.is_some(), lost: reloaded.lost })
    }

    // ---------- typed readbacks ----------

    /// Current fake-game session number, active flag, and last raised date.
    #[cfg(test)]
    pub fn calendar(&self) -> Calendar {
        self.fake.calendar()
    }

    /// Every line the extension logged.
    pub fn log(&self) -> Vec<String> {
        self.fake.store().log.clone()
    }

    /// Log lines recording handler errors.
    pub fn errors(&self) -> Vec<String> {
        self.fake.store().log.iter().filter(|l| l.starts_with("error in ")).cloned().collect()
    }

    /// Capabilities called by the extension, optionally filtered by capability
    /// name.
    pub fn calls(&self, capability: Option<&str>) -> Vec<Value> {
        self.fake.store().calls.iter().filter(|call| capability.is_none_or(|c| call.get("capability").and_then(Value::as_str) == Some(c))).cloned().collect()
    }

    /// Hosted interface element `name`, dynamically resolved in the current
    /// player language.
    pub fn element(&self, name: &str) -> Properties {
        let ext = self.ext();
        self.fake.store().element(ext.index(), name, |key| self.fake.text(ext.id(), key))
    }

    /// Extension window `name`, dynamically resolved in the current player
    /// language.
    pub fn window(&self, name: &str) -> Result<FakeWindow, String> {
        let ext = self.ext();
        if !ext.windows().iter().any(|w| w.name == name) {
            return Err(format!("the mod has no window {name:?}"));
        }
        let window = self.fake.store().window(ext.index(), name, |key| self.fake.text(ext.id(), key));
        Ok(window.unwrap_or_default())
    }

    /// Currently open pop-ups in the player language.
    pub fn popups(&self) -> Vec<FakePopup> {
        self.fake
            .store()
            .windows
            .popups()
            .iter()
            .map(|p| FakePopup {
                number: p.number,
                title: p.popup.title.clone(),
                text: p.popup.text.clone(),
                header: p.popup.header.clone(),
                buttons: p.popup.shown_buttons("OK"),
            })
            .collect()
    }

    /// The map colors painted by the extension, or `None` if released/unpainted.
    pub fn map(&self) -> Option<BTreeMap<u32, [u8; 4]>> {
        self.fake.store().map.clone()
    }

    /// HTTP requests sent by the extension, in order.
    pub fn requests(&self) -> Vec<FakeRequest> {
        self.fake
            .store()
            .requests
            .iter()
            .map(|(request, _)| FakeRequest {
                url: request.target.url(),
                method: request.method.as_str().to_string(),
                headers: request.headers.clone(),
                body: request.body.clone(),
                timeout: request.timeout.as_secs_f64(),
            })
            .collect()
    }

    /// Number of waiting tasks currently suspended in the extension.
    pub fn waiting(&self) -> usize {
        self.ext().waiting()
    }

    /// Notices the player was shown (first error per session, stops).
    pub fn told(&self) -> Vec<String> {
        self.fake.store().told.clone()
    }

    /// Why the extension was stopped, or `None` if it is still running.
    pub fn stopped(&self) -> Option<String> {
        self.ext().stopped()
    }

    /// Drains unanswered-capability and misshapen-answer notes collected since
    /// the last call.
    pub fn take_notes(&self) -> SessionNotes {
        let mut store = self.fake.store();
        let mut unanswered = Vec::new();
        for name in store.unanswered.drain(..) {
            if !unanswered.contains(&name) {
                unanswered.push(name);
            }
        }
        let mut misshapen = Vec::new();
        for why in store.misshapen.drain(..) {
            if !misshapen.contains(&why) {
                misshapen.push(why);
            }
        }
        SessionNotes { unanswered, misshapen }
    }
}

// ---------- internal host state and dispatch feed ----------

/// What a capability answers in a test.
enum Answer {
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

/// A `time.wait` for the day `until`, answered once a day the test raises
/// reaches it.
struct Until {
    calendar: Arc<Mutex<Calendar>>,
    until: Date,
}

impl Pending<Answered> for Until {
    fn poll(&mut self) -> Option<Answered> {
        let today = self.calendar.lock().unwrap_or_else(|e| e.into_inner()).today?;
        (today >= self.until).then(|| Ok(lua_form(today)))
    }
}

/// A date from Lua's form.
fn lua_date(value: &Value) -> Option<Date> {
    let part = |name: &str| value.get(name).and_then(Value::as_i64);
    Date::from_ymd(i32::try_from(part("year")?).ok()?, u32::try_from(part("month")?).ok()?, u32::try_from(part("day")?).ok()?)
}

/// The error code named `name`, as `err.code` writes it.
fn code_named(name: &str) -> Option<Code> {
    Code::ALL.iter().copied().find(|c| c.name() == name)
}

fn parse_code(name: &str) -> Result<Code, String> {
    code_named(name).ok_or_else(|| format!("no error code {name}; eu4-harmony wiki errors lists them"))
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

/// What the fake game holds internally behind [`FakeMod`].
#[derive(Default)]
struct Store {
    answers: HashMap<String, Answer>,
    /// Every capability the mod called, as the catalogue's request.
    calls: Vec<Value>,
    /// Capabilities called with no answer given.
    unanswered: Vec<String>,
    /// Answers that come later, by capability, oldest first.
    later: Vec<(String, Slot<Answered>)>,
    /// Hosted interface elements (`game.ui.<element>`), retaining semantic
    /// descriptions so language changes re-resolve dynamically.
    hosted: HostedElements,
    /// The map as the mod painted it, `[r, g, b, a]` by province; none when
    /// it holds no map.
    map: Option<BTreeMap<u32, [u8; 4]>>,
    log: Vec<String>,
    requests: Vec<(fetch::Request, Slot<fetch::Answer>)>,
    /// What the player was told: errors and stops.
    told: Vec<String>,
    /// Answers the test's functions gave that are not their capability's
    /// shape, which fail the test.
    misshapen: Vec<String>,
    /// The world the test described, which answers what no answer of the
    /// test's does.
    world: Option<crate::world::World>,
    /// The mod's windows and pop-ups, kept by the rules the game keeps
    /// them by: what it opened or closed itself it hears at the next frame.
    windows: Windows,
}

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
    fn window(&self, extension: usize, name: &str, words: impl Fn(&str) -> Option<String>) -> Option<FakeWindow> {
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
    fn element(&self, extension: usize, name: &str, words: impl Fn(&str) -> Option<String>) -> Properties {
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

/// The private `eu4_harmony_lua::Host` implementation for one fake game.
struct Fake {
    store: Mutex<Store>,
    texts: Mutex<Catalog>,
    language: Mutex<Language>,
    calendar: Arc<Mutex<Calendar>>,
    /// The test's Lua state, for answers that are functions; weak, as the
    /// state holds the fake game in turn. `None` when driven directly from Rust.
    tests: Option<WeakLua>,
}

impl Fake {
    fn new(texts: Catalog, tests: Option<WeakLua>) -> Self {
        Self { store: Mutex::new(Store::default()), texts: Mutex::new(texts), language: Mutex::new(Language::English), calendar: Arc::default(), tests }
    }

    fn calendar(&self) -> Calendar {
        *self.calendar.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn start_session(&self) -> u64 {
        let mut cal = self.calendar.lock().unwrap_or_else(|e| e.into_inner());
        cal.session += 1;
        cal.going_on = true;
        cal.today = None;
        cal.session
    }

    fn end_session(&self) {
        self.calendar.lock().unwrap_or_else(|e| e.into_inner()).going_on = false;
    }

    fn advance_day(&self, date: Date) -> (bool, bool) {
        let mut cal = self.calendar.lock().unwrap_or_else(|e| e.into_inner());
        date.turned(cal.today.replace(date))
    }

    fn replace_texts(&self, texts: Catalog) -> Catalog {
        std::mem::replace(&mut *self.texts.lock().unwrap_or_else(|e| e.into_inner()), texts)
    }

    /// `time.wait`, as the game answers it: once the days the test raises
    /// reach the date, counted from the last day raised, or else the world's
    /// date, or else 1444.11.11.
    fn wait(&self, days: u32, store: &Store) -> Called {
        let today = self.calendar().today;
        let from = today.or_else(|| store.world.as_ref().and_then(|w| lua_date(&w.date()))).or_else(|| Date::from_ymd(1444, 11, 11));
        match from.map(|from| eu4_harmony_remote::checks::time::days_after(from, days)) {
            Some(Ok(until)) => Called::Later(Box::new(Until { calendar: self.calendar.clone(), until })),
            Some(Err(refusal)) => Called::Done(Err(refusal)),
            None => Called::Done(Err(Error::new(Code::Internal, "the fake game has no date"))),
        }
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The answer `function` gives to `request`.
    fn call(&self, function: &Function, request: &Value) -> Answered {
        let internal = |e: mlua::Error| Error::new(Code::Internal, format!("the test's answer failed: {e}"));
        let tests = self.tests.as_ref().and_then(WeakLua::try_upgrade).ok_or_else(|| Error::new(Code::Internal, "the test has ended"))?;
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

impl Host for Fake {
    fn run(&self, _: usize, request: Request) -> Called {
        let name = request.name();
        let json = eu4_harmony_remote::form::as_lua(|| serde_json::to_value(&request)).unwrap_or(Value::Null);
        let mut store = self.store();
        store.calls.push(json.clone());
        let answered = match store.answers.get(&name) {
            None => {
                if let Request::Time(eu4_harmony_remote::time::TimeRequest::Wait { days }) = request {
                    return self.wait(days, &store);
                }
                let calendar = self.calendar();
                if let Some(result) = store.world.as_mut().and_then(|w| w.answer(&request, calendar)) {
                    result
                } else {
                    store.unanswered.push(name.clone());
                    Err(Error::new(Code::NotReady, format!("the fake game has no answer for {name}; give one with mod:answer(\"{name}\", ...)")))
                }
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
        self.store().hosted.set(extension, element, value)
    }

    fn window(&self, extension: usize, change: WindowChange) -> Result<(), String> {
        let in_game = self.calendar().going_on;
        self.store().windows.change(extension, change, in_game)
    }

    fn tooltip(&self, extension: usize, window: Option<&str>, widget: &str) -> Result<eu4_harmony_lua::Text, String> {
        let store = self.store();
        match window {
            Some(window) => store.windows.tooltip(extension, window, widget).ok_or_else(|| format!("no tooltip target {window}.{widget}")),
            None => store.hosted.tooltip(extension, widget),
        }
    }

    fn popup(&self, extension: usize, popup: Popup) -> Result<u32, String> {
        let in_game = self.calendar().going_on;
        self.store().windows.popup(extension, popup, in_game)
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
        self.texts.lock().unwrap_or_else(|e| e.into_inner()).text(*self.language.lock().unwrap_or_else(|e| e.into_inner()), key)
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
        self.calendar().session
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

    fn reload_reset(&self, extension: usize, new: &Extension) {
        let mut store = self.store();
        store.hosted.forget(extension);
        store.map = None;
        store.windows.declare(extension, new.id(), new.windows());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempMod {
        root: PathBuf,
        id: String,
    }

    impl TempMod {
        fn new(name: &str, id: &str, extra_manifest: &str) -> Self {
            let root = std::env::temp_dir().join(format!("eu4h-fake-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let this = Self { root, id: id.to_string() };
            let manifest = format!("id = \"{id}\"\napi_version = \"{}\"\n{extra_manifest}", eu4_harmony_remote::API_VERSION);
            this.write("manifest.toml", &manifest);
            this
        }

        fn write(&self, relative: &str, text: &str) {
            let path = self.root.join("eu4-harmony").join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        fn open(&self) -> FakeMod {
            let manifest = eu4_harmony_manifest::HarmonyMod::read(&self.root).unwrap().manifest;
            let source = ModSource { root: self.root.clone(), manifest, name: self.id.clone() };
            FakeMod::open(source, self.root.join("data"), None).unwrap()
        }
    }

    impl Drop for TempMod {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn answers_world_refusals_and_unanswered_notes_work_directly() {
        let dir = TempMod::new("answers", "ans", "");
        dir.write(
            "lua/main.lua",
            r#"
            return {
              on_button = function(game, name)
                if name == "read" then
                  local p = game.province.get(1)
                  local _, refused = game.country.get("FRA")
                  local _, missing = game.diplomacy.propose_alliance("POR")
                  log(p.name .. "|" .. refused.code .. ":" .. refused.message .. "|" .. missing.code)
                end
              end,
            }
            "#,
        );
        let m = dir.open();
        assert!(m.answer("no.such", json!({})).unwrap_err().contains("there is no capability no.such"));
        assert!(m.answer("army.get", json!({ "bogus": 1 })).unwrap_err().contains("bogus is no field"));
        assert!(m.refuse("country.get", "not_a_code", None).unwrap_err().contains("no error code not_a_code"));

        m.set_world(json!({ "player": "CAS", "provinces": [{ "id": 1, "name": "Madrid", "owner": "CAS" }] })).unwrap();
        m.answer("province.get", json!({ "name": "Override" })).unwrap();
        m.refuse("country.get", "not_found", Some("gone".into())).unwrap();

        m.button("read", ClickOptions::default()).unwrap();
        assert_eq!(m.log(), vec!["Override|not_found:gone|not_ready"]);
        assert_eq!(m.calls(None).len(), 3);
        assert_eq!(m.calls(Some("province.get")).len(), 1);

        let notes = m.take_notes();
        assert_eq!(notes.unanswered, vec!["diplomacy.propose_alliance"]);
        assert!(notes.misshapen.is_empty());
        assert_eq!(m.take_notes(), SessionNotes::default(), "take_notes drains recorded notes");
    }

    #[test]
    fn pending_replies_and_http_requests_resume_tasks_on_poll() {
        let dir = TempMod::new("pending", "pend", "permissions = { network = [\"api.example.com\"] }\n");
        dir.write(
            "lua/main.lua",
            r#"
            return {
              on_session = function(game)
                spawn(function()
                  local ok, err = game.diplomacy.propose_alliance("POR")
                  log("first:" .. (ok and ok.answer or err.code))
                end)
                spawn(function()
                  local ok, err = game.diplomacy.propose_alliance("ENG")
                  log("second:" .. (ok and ok.answer or err.code))
                end)
                spawn(function()
                  local r, err = http.fetch("https://api.example.com/ok", { method = "POST", body = "hi" })
                  log("http1:" .. (r and r.status or err.code))
                end)
                spawn(function()
                  local r, err = http.fetch("https://api.example.com/fail")
                  log("http2:" .. (r and r.status or err.code))
                end)
              end,
            }
            "#,
        );
        let m = dir.open();
        m.later("diplomacy.propose_alliance");
        assert!(m.reply("diplomacy.propose_alliance", json!({ "answer": "accepted" })).unwrap_err().contains("no answer of"));
        assert!(m.respond(1, fetch::Response { status: 200, headers: vec![], body: vec![] }).unwrap_err().contains("there is no request 1"));

        m.session(None);
        assert_eq!(m.waiting(), 4);
        let reqs = m.requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].url, "https://api.example.com:443/ok");
        assert_eq!(reqs[0].method, "POST");
        assert_eq!(reqs[0].body, b"hi");

        assert!(m.reply("diplomacy.propose_alliance", json!({ "wrong": 1 })).is_err());
        assert!(m.fail_request(2, "bogus", None).unwrap_err().contains("timeout, network or too_large"));

        m.reply("diplomacy.propose_alliance", json!({ "answer": "accepted" })).unwrap();
        m.reply_refusal("diplomacy.propose_alliance", "rejected", Some("no".into())).unwrap();
        m.respond(1, fetch::Response { status: 201, headers: vec![("x-ok".into(), "1".into())], body: b"done".to_vec() }).unwrap();
        m.fail_request(2, "timeout", None).unwrap();

        m.poll();
        assert_eq!(m.waiting(), 0);
        assert_eq!(m.log(), vec!["first:accepted", "second:rejected", "http1:201", "http2:timeout"]);
    }

    #[test]
    fn calendar_signals_wait_map_and_save_follow_session_lifecycle() {
        let dir = TempMod::new("lifecycle", "life", "");
        dir.write(
            "lua/main.lua",
            r#"
            local count = 0
            return {
              on_session = function(game, saved)
                count = saved and saved.n or 0
                log("session:" .. count)
              end,
              on_day = function(game, date)
                log("day:" .. date.year .. "." .. date.month .. "." .. date.day)
                if date.day == 31 then
                  spawn(function()
                    local arrived = game.time.wait(2)
                    log("waited:" .. arrived.month .. "." .. arrived.day)
                  end)
                end
              end,
              on_month = function(_, date) log("month:" .. date.month) end,
              on_year = function(_, date) log("year:" .. date.year) end,
              on_army_ordered = function(game, id)
                count = count + 1
                game.map.paint({ [151] = 0x112233 })
                log("ordered:" .. id)
              end,
              on_army_defeated = function(_, id) log("defeated:" .. id) end,
              on_map_lost = function(game)
                game.map.release()
                log("map_lost")
              end,
              on_session_ended = function(_, reason) log("ended:" .. reason) end,
              save_data = function() return { n = count } end,
            }
            "#,
        );
        let m = dir.open();
        assert_eq!(m.calendar(), Calendar { session: 0, going_on: false, today: None });
        assert!(m.raise("session", json!({})).unwrap_err().contains("mod:session(saved)"));
        assert!(m.session_ended(Some("quit")).unwrap_err().contains("return, load, new_game or country"));

        m.session(Some(json!({ "n": 5 })));
        assert_eq!(m.calendar(), Calendar { session: 1, going_on: true, today: None });
        m.day(Date::from_ymd(1444, 12, 31).unwrap());
        assert_eq!(m.waiting(), 1);
        m.day(Date::from_ymd(1445, 1, 1).unwrap());
        assert_eq!(m.waiting(), 1);
        m.day(Date::from_ymd(1445, 1, 2).unwrap());
        assert_eq!(m.waiting(), 0);

        m.army_ordered(42);
        assert_eq!(m.map().unwrap().get(&151), Some(&[0x11, 0x22, 0x33, 0xff]));
        m.army_defeated(99);
        m.map_lost();
        assert!(m.map().is_none());
        assert_eq!(m.save().unwrap(), Some(json!({ "n": 6 })));

        m.session_ended(Some("load")).unwrap();
        assert!(!m.calendar().going_on);
        assert_eq!(
            m.log(),
            vec![
                "session:5",
                "day:1444.12.31",
                "day:1445.1.1",
                "month:1",
                "year:1445",
                "waited:1.2",
                "day:1445.1.2",
                "ordered:42",
                "defeated:99",
                "map_lost",
                "ended:load",
            ]
        );
    }

    #[test]
    fn windows_hosted_elements_popups_and_language_resolve_and_enforce_rules() {
        let dir = TempMod::new("ui", "ui", "");
        dir.write("localisation/ui_l_english.yml", "\u{feff}l_english:\n T_LBL:0 \"Hello\"\n T_ASK:0 \"Sure?\"\n");
        dir.write("localisation/ui_l_simp_chinese.yml", "\u{feff}l_simp_chinese:\n T_LBL:0 \"你好\"\n T_ASK:0 \"确定？\"\n");
        dir.write("ui/single_unitpanel/buttons/go.gui", "guiButtonType = { name = \"go\" }\n");
        dir.write(
            "lua/main.lua",
            r#"
            return {
              windows = {
                panel = {
                  size = { 300, 200 },
                  entry = { sprite = "GFX_panel" },
                  items = {
                    { type = "text", name = "status", size = { 200, 20 }, text = { key = "T_LBL" } },
                    { type = "button", name = "act", text = { key = "T_LBL" } },
                  },
                },
              },
              on_session = function(game)
                game.ui.go.text = { key = "T_LBL" }
                game.window.open("panel")
                game.popup { title = "T_ASK", text = "T_LBL", buttons = { { text = "T_LBL", action = function() log("confirmed") end } } }
              end,
              on_window = function(_, name, open) log("win:" .. name .. ":" .. tostring(open)) end,
              on_button = function(_, name) log("btn:" .. name) end,
            }
            "#,
        );
        let m = dir.open();
        assert!(m.window("missing").unwrap_err().contains("has no window"));
        assert!(m.close("panel").unwrap_err().contains("is not open"));
        assert!(m.set_language("klingon").unwrap_err().contains("no language klingon"));

        m.session(None);
        assert_eq!(m.element("go").get("text"), Some(&json!("Hello")));
        assert_eq!(m.window("panel").unwrap().widgets["status"].get("text"), Some(&json!("Hello")));
        assert_eq!(m.window("panel").unwrap().widgets["act"].get("enabled"), Some(&json!(true)));
        assert!(m.log().is_empty(), "mod-initiated open waits for frame");
        m.frame();
        assert_eq!(m.log(), vec!["win:panel:true"]);

        m.set_language("simp_chinese").unwrap();
        assert_eq!(m.element("go").get("text"), Some(&json!("你好")));
        assert_eq!(m.window("panel").unwrap().widgets["status"].get("text"), Some(&json!("你好")));

        m.press("panel", "act", ClickOptions::default()).unwrap();
        let popups = m.popups();
        assert_eq!(popups.len(), 1);
        assert_eq!(popups[0].title, "Sure?");
        m.answer_popup(popups[0].number, 1).unwrap();
        assert!(m.popups().is_empty());

        m.close("panel").unwrap();
        assert!(!m.window("panel").unwrap().open);
        m.entry("panel").unwrap();
        assert!(m.window("panel").unwrap().open);
        m.session_ended(None).unwrap();
        assert!(!m.window("panel").unwrap().open);
        assert_eq!(m.log(), vec!["win:panel:true", "btn:panel.act", "confirmed", "win:panel:false", "win:panel:true"]);
    }

    #[test]
    fn reload_preserves_session_state_handles_out_of_session_and_rolls_back_on_error() {
        let dir = TempMod::new("reload", "rel", "");
        dir.write("localisation/rel_l_english.yml", "l_english:\n REL_V:0 \"v1\"\n");
        dir.write(
            "lua/main.lua",
            r#"
            local state = 0
            return {
              on_session = function(game, saved)
                state = saved and saved.n or 1
                log("session:" .. text.get("REL_V") .. ":" .. state)
              end,
              on_button = function(_, name)
                if name == "bump" then state = state + 1 end
              end,
              save_data = function() return { n = state } end,
            }
            "#,
        );
        let m = dir.open();

        // Reload outside a session does not call on_session.
        let before = m.reload().unwrap();
        assert_eq!(before, ReloadReport { session: false, saved_kept: false, lost: None });
        assert!(m.log().is_empty());

        // Start a session, mutate state, update localisation, reload in place.
        m.session(None);
        m.button("bump", ClickOptions::default()).unwrap();
        dir.write("localisation/rel_l_english.yml", "l_english:\n REL_V:0 \"v2\"\n");
        let after = m.reload().unwrap();
        assert_eq!(after, ReloadReport { session: true, saved_kept: true, lost: None });
        assert_eq!(m.calendar().session, 1, "reload keeps the same session number");
        assert_eq!(m.log(), vec!["session:v1:1", "session:v2:2"]);

        // Broken code fails reload and restores previous localisation and extension.
        dir.write("localisation/rel_l_english.yml", "l_english:\n REL_V:0 \"v3\"\n");
        dir.write("lua/main.lua", "return { broken = ");
        assert!(m.reload().unwrap_err().contains("the new code did not load"));
        assert_eq!(m.save().unwrap(), Some(json!({ "n": 2 })));

        // A failing save_data loses state cleanly without aborting reload.
        dir.write(
            "lua/main.lua",
            r#"
            return {
              on_session = function(_, saved) log("reloaded:" .. tostring(saved)) end,
              save_data = function() error("corrupt") end,
            }
            "#,
        );
        let swapped = m.reload().unwrap();
        assert_eq!(swapped, ReloadReport { session: true, saved_kept: true, lost: None });
        let lost_reload = m.reload().unwrap();
        assert!(lost_reload.session && !lost_reload.saved_kept);
        assert!(lost_reload.lost.as_deref().is_some_and(|l| l.contains("save_data failed")));
        assert_eq!(m.log().last().map(String::as_str), Some("reloaded:nil"));

        // Manifest permission changes are rejected.
        dir.write("lua/main.lua", "return {}");
        dir.write("manifest.toml", &format!("id = \"rel\"\napi_version = \"{}\"\npermissions = {{ files = true }}\n", eu4_harmony_remote::API_VERSION));
        assert!(m.reload().unwrap_err().contains("permissions changed"));
    }
}
