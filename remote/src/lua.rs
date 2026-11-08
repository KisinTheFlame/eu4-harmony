//! What a Lua extension has besides the capabilities
//! (ADR extensions-are-lua-in-a-sandbox): its handlers, the sandbox's globals,
//! its manifest and the sensitive capabilities that manifest may declare
//! (ADR sensitive-capabilities-are-declared). Described once here, for
//! `eu4-harmony wiki`, the editor's type definitions and the core's test that
//! the sandbox holds every global described and no other.
//!
//! A type is written as the editor's type definitions write it (LuaLS):
//! `string`, `integer`, `eu4.Date`, `eu4.Err`, `string[]`.

use crate::events::{Event, Kind, EVENTS};
use crate::shape::Shape;

/// A handler: a function `main.lua`'s table may hold, called for an event.
#[derive(Clone, Debug)]
pub struct Handler {
    pub name: &'static str,
    /// What follows `game`.
    pub args: Vec<Arg>,
    /// What it may return, where that counts.
    pub returns: Option<Arg>,
    pub about: String,
}

/// A handler's argument or what it returns.
#[derive(Clone, Debug)]
pub struct Arg {
    pub name: &'static str,
    pub ty: Ty,
    pub optional: bool,
    pub about: String,
}

/// The type of a handler's argument.
#[derive(Clone, Debug)]
pub enum Ty {
    /// As the editor's type definitions write it.
    Written(&'static str),
    /// The shape of the event's field it is, or of the event as a whole.
    Shaped(Shape),
}

impl From<&Value> for Arg {
    fn from(v: &Value) -> Self {
        Self { name: v.name, ty: Ty::Written(v.ty), optional: v.optional, about: v.about.to_string() }
    }
}

/// What an event's handler is called with after `game`
/// (`crate::events::Kind::called`).
#[derive(Clone, Copy, Debug)]
pub enum Called {
    /// The event's fields, one argument each, by its name.
    Fields,
    /// The whole event as one table, named as the event.
    Table,
    /// Arguments the Lua host gives itself.
    Given(&'static [Value]),
}

/// A function the sandbox has, by its full name (`files.read`).
#[derive(Clone, Copy, Debug)]
pub struct Function {
    pub name: &'static str,
    pub args: &'static [Value],
    pub returns: &'static [Value],
    pub about: &'static str,
    /// It waits (ADR handlers-run-as-tasks-that-can-wait): it is called only in
    /// a task.
    pub waits: bool,
    /// The sensitive capability it needs declared, if any.
    pub permission: Option<&'static str>,
}

/// An argument or a returned value.
#[derive(Clone, Copy, Debug)]
pub struct Value {
    pub name: &'static str,
    pub ty: &'static str,
    pub optional: bool,
    pub about: &'static str,
}

const fn value(name: &'static str, ty: &'static str, about: &'static str) -> Value {
    Value { name, ty, optional: false, about }
}

pub(crate) const fn optional(name: &'static str, ty: &'static str, about: &'static str) -> Value {
    Value { name, ty, optional: true, about }
}

/// What a function that may fail as the game or the system may make it fail
/// returns besides its result.
const ERR: Value = optional("err", "eu4.Err", "why it failed, when it did: { code, message }");

/// A field of `manifest.toml`.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub required: bool,
    pub example: &'static str,
    pub about: &'static str,
}

/// A sensitive capability (ADR sensitive-capabilities-are-declared).
#[derive(Clone, Copy, Debug)]
pub struct Permission {
    pub name: &'static str,
    /// How the manifest declares it.
    pub declared: &'static str,
    /// What it gives.
    pub gives: &'static str,
    pub about: &'static str,
}

/// The handlers besides the events': name, arguments, what it may return
/// and what it is for.
const OTHER_HANDLERS: &[(&str, &[Value], Option<Value>, &str)] = &[
    (
        "on_button",
        &[
            value("name", "string", "the button's name in its interface fragment, or window.widget for a button of one of this mod's windows"),
            value("click", "eu4.Click", "{ right, shift, ctrl, row }, row the number of the list's row the button is in"),
        ],
        None,
        "The player clicked one of this mod's buttons.",
    ),
    (
        "on_window",
        &[value("name", "string", "the window's name in the windows main.lua returns"), value("open", "boolean", "whether it opened or closed")],
        None,
        "One of this mod's windows opened or closed: by the mod, its entry button, its close button, Esc, or another of the game's big panels opening.",
    ),
    ("on_map_lost", &[], None, "Another mod, or the player changing the map mode, took the map this mod painted."),
    ("on_frame", &[], None, "Every frame of a game, after the events above. Keep it short: it runs as often as the game draws."),
    (
        "save_data",
        &[],
        Some(optional("data", "any", "this mod's save data, any value JSON can hold; nil keeps nothing")),
        "The framework asks for this mod's data: the game writes its world, the ironman exit save included, or the mod's code is about to be reloaded. It cannot wait.",
    ),
];

/// Every handler: one for each event ([`EVENTS`]), as its declaration says
/// it is called (`crate::events::Kind::called`) and with what it says of the
/// event, then the others.
pub fn handlers() -> Vec<Handler> {
    let events = EVENTS.iter().map(|(kind, about)| {
        let about = match kind.note() {
            "" => crate::prose(about),
            note => format!("{} {note}", crate::prose(about)),
        };
        Handler { name: kind.handler(), args: event_args(*kind), returns: None, about }
    });
    let others = OTHER_HANDLERS.iter().map(|(name, args, returns, about)| Handler {
        name,
        args: args.iter().map(Arg::from).collect(),
        returns: returns.as_ref().map(Arg::from),
        about: about.to_string(),
    });
    events.chain(others).collect()
}

/// The arguments the handler of `kind` is called with after `game`.
fn event_args(kind: Kind) -> Vec<Arg> {
    match kind.called() {
        Called::Fields => kind
            .fields()
            .into_iter()
            .map(|field| {
                let (ty, optional) = match field.shape {
                    Shape::Optional(inner) => (*inner, true),
                    shape => (shape, false),
                };
                Arg { name: field.name, ty: Ty::Shaped(ty), optional, about: field.about.to_string() }
            })
            .collect(),
        Called::Table => {
            let fields = kind.fields();
            let names: Vec<&str> = fields.iter().map(|f| f.name).collect();
            let about = format!("{{ {} }}, the {kind} event's fields (eu4-harmony wiki events)", names.join(", "));
            let shape = Shape::Object { class: kind.class(), about: "", fields };
            vec![Arg { name: kind.name(), ty: Ty::Shaped(shape), optional: false, about }]
        }
        Called::Given(given) => given.iter().map(Arg::from).collect(),
    }
}

/// The handler `event` calls, `on_<kind>`, and its arguments in Lua's form
/// (`eu4_harmony_checks::form`), as its kind says it is called. None for an
/// event whose handler's arguments the Lua host gives itself (`on_session`'s
/// saved data).
pub fn handler_call(event: &Event) -> Option<(&'static str, Vec<serde_json::Value>)> {
    let kind = event.kind();
    let mut table = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(event)).ok()?;
    let fields = table.as_object_mut()?;
    fields.remove("event");
    Some((kind.handler(), handler_args(kind, fields)?))
}

/// The arguments the handler of `kind` is called with after `game`, from
/// the event's fields in Lua's form, as its kind says it is called; a field
/// left out is nil. None for an event whose handler's arguments the Lua host
/// gives itself.
pub fn handler_args(kind: Kind, fields: &serde_json::Map<String, serde_json::Value>) -> Option<Vec<serde_json::Value>> {
    match kind.called() {
        Called::Fields => Some(kind.fields().iter().map(|field| fields.get(field.name).cloned().unwrap_or(serde_json::Value::Null)).collect()),
        Called::Table => Some(vec![serde_json::Value::Object(fields.clone())]),
        Called::Given(_) => None,
    }
}

pub const FUNCTIONS: &[Function] = &[
    Function {
        name: "log",
        args: &[value("...", "any", "values, written with tostring and a space between them")],
        returns: &[],
        about: "Writes a line to the mod's log, eu4-harmony/logs/<id>.log in the game's user folder. print does the same.",
        waits: false,
        permission: None,
    },
    Function {
        name: "print",
        args: &[value("...", "any", "values, written with tostring and a space between them")],
        returns: &[],
        about: "The same as log.",
        waits: false,
        permission: None,
    },
    Function {
        name: "json.encode",
        args: &[value("value", "any", "a value JSON can hold: a table whose keys are all strings is an object, a sequence an array")],
        returns: &[value("text", "string", "the JSON")],
        about: "Writes a value as JSON; integers stay 64-bit. A function, a table with mixed keys or a cycle is an error.",
        waits: false,
        permission: None,
    },
    Function {
        name: "json.decode",
        args: &[value("text", "string", "JSON")],
        returns: &[value("value", "any", "the value; an object of year, month and day alone is an eu4.Date")],
        about: "Reads JSON. Text that is not JSON is an error.",
        waits: false,
        permission: None,
    },
    Function {
        name: "mod_files.read",
        args: &[value("path", "string", "relative to the mod's eu4-harmony/ folder, such as data/units.json")],
        returns: &[optional("text", "string", "the file's bytes"), ERR],
        about: "Reads one of the mod's own files. A path that leaves the folder is an error.",
        waits: false,
        permission: None,
    },
    Function {
        name: "text.get",
        args: &[value("key", "string", "a key in the mod's localisation"), optional("values", "table<string, any>", "values for its $NAME$ places")],
        returns: &[optional("text", "string", "the text in the player language"), ERR],
        about: "One of the mod's texts in the player language.",
        waits: false,
        permission: None,
    },
    Function {
        name: "text.language",
        args: &[],
        returns: &[value("language", "string", "its Paradox name, such as english or simp_chinese")],
        about: "The player language.",
        waits: false,
        permission: None,
    },
    Function {
        name: "require",
        args: &[value("name", "string", "a module's dotted name: range.bfs is eu4-harmony/lua/range/bfs.lua")],
        returns: &[value("module", "any", "what the module returned, true for nothing; loaded once")],
        about: "Loads one of the mod's own modules.",
        waits: false,
        permission: None,
    },
    Function {
        name: "spawn",
        args: &[value("fn", "fun()", "the task's function")],
        returns: &[],
        about: "Starts fn as a task of its own, from a handler or a task, and returns as soon as fn first waits or ends.",
        waits: false,
        permission: None,
    },
    Function {
        name: "files.read",
        args: &[value("path", "string", "relative to the mod's data folder")],
        returns: &[optional("text", "string", "the file's bytes"), ERR],
        about: "Reads a file from the mod's data folder; not_found when it is not there.",
        waits: false,
        permission: Some("files"),
    },
    Function {
        name: "files.write",
        args: &[value("path", "string", "relative to the mod's data folder"), value("text", "string", "the file's bytes")],
        returns: &[optional("ok", "true", "written"), ERR],
        about: "Writes a file in the mod's data folder, whole or not at all, making the folders its path needs.",
        waits: false,
        permission: Some("files"),
    },
    Function {
        name: "files.list",
        args: &[optional("path", "string", "a folder relative to the mod's data folder; the data folder itself when left out")],
        returns: &[optional("names", "string[]", "sorted, a folder's ending in /; none for a folder not made yet"), ERR],
        about: "Lists a folder in the mod's data folder.",
        waits: false,
        permission: Some("files"),
    },
    Function {
        name: "files.remove",
        args: &[value("path", "string", "relative to the mod's data folder")],
        returns: &[optional("ok", "true", "removed"), ERR],
        about: "Removes a file, or a folder once it is empty, from the mod's data folder.",
        waits: false,
        permission: Some("files"),
    },
    Function {
        name: "http.fetch",
        args: &[
            value("url", "string", "http:// or https://, to a host the manifest lists"),
            optional("options", "eu4.FetchOptions", "{ method, headers, body, timeout }"),
        ],
        returns: &[optional("response", "eu4.Response", "{ status, headers, body }, whatever the status"), ERR],
        about: "Sends an HTTP or HTTPS request and waits for its response. err is timeout, network or too_large; redirects are not followed.",
        waits: true,
        permission: Some("network"),
    },
    Function {
        name: "os.clock",
        args: &[],
        returns: &[value("seconds", "number", "processor time the game has spent")],
        about: "Lua's os.clock, for measuring.",
        waits: false,
        permission: None,
    },
    Function {
        name: "os.time",
        args: &[optional("date", "table", "as Lua's os.time takes it")],
        returns: &[value("time", "integer", "seconds since 1970")],
        about: "Lua's os.time: the real time.",
        waits: false,
        permission: Some("clock"),
    },
    Function {
        name: "os.date",
        args: &[optional("format", "string", "as Lua's os.date takes it"), optional("time", "integer", "seconds since 1970")],
        returns: &[value("date", "string|table", "the real date, formatted")],
        about: "Lua's os.date: the real date.",
        waits: false,
        permission: Some("clock"),
    },
    Function {
        name: "game.map.paint",
        args: &[value("colors", "table<integer, integer|eu4.Color>", "{ [province] = 0xRRGGBB or { r, g, b, a } }")],
        returns: &[optional("ok", "true", "painted"), ERR],
        about: "Paints provinces on the map in colors of the mod's own, until game.map.release or another mod or the player takes the map (on_map_lost).",
        waits: false,
        permission: None,
    },
    Function {
        name: "game.map.release",
        args: &[],
        returns: &[optional("ok", "true", "released"), ERR],
        about: "Gives the map back.",
        waits: false,
        permission: None,
    },
    Function {
        name: "game.window.open",
        args: &[value("name", "string", "a window of the windows main.lua returns")],
        returns: &[],
        about: "Opens one of the mod's windows, in a game; one of the game's big panels closes the others. on_window hears it.",
        waits: false,
        permission: None,
    },
    Function {
        name: "game.window.close",
        args: &[value("name", "string", "a window of the windows main.lua returns")],
        returns: &[],
        about: "Closes one of the mod's windows. on_window hears it.",
        waits: false,
        permission: None,
    },
    Function {
        name: "game.popup",
        args: &[value(
            "popup",
            "eu4.Popup",
            "{ title = key, text = key, values = { NAME = value }, buttons = { key, ... }, id = name }: the mod's localisation keys, one to four buttons, values for the keys' $NAME$s",
        )],
        returns: &[value("answer", "integer|string", "the number of the button pressed, from 1, or \"closed\" when it closed without one")],
        about: "Shows the player a pop-up in the look of the game's dialogs, and waits until a button is pressed. Several may be open at once; it does not pause the game.",
        waits: true,
        permission: None,
    },
    Function {
        name: "game.popup.close",
        args: &[value("id", "string", "the id the pop-ups were shown with")],
        returns: &[],
        about: "Closes the mod's open pop-ups of that id; each answers \"closed\".",
        waits: false,
        permission: None,
    },
];

/// The globals the framework puts in the sandbox, besides Lua's own and the
/// `game` its handlers get: each a function or a table of them.
pub const GLOBALS: &[&str] = &["log", "print", "json", "mod_files", "text", "require", "spawn", "files", "http"];

/// What Lua has in the sandbox of its own.
pub const SANDBOX: &str = "Lua 5.4 with its string, table, math, utf8 and coroutine libraries and the base library, \
except dofile, loadfile and loading bytecode (load takes text only). There is no io, debug or package; os has os.clock, \
and os.time and os.date for a mod that declares clock. Nothing reaches the process, the system, other mods or native code. \
A Lua state holds at most 256 MiB; past that, an allocation raises Lua's not enough memory error.";

/// What `game.ui.<element>` is.
pub const WINDOWS: &str = "game.window.<window>.<widget>, a game widget of one of the windows main.lua returns by its name, or entry for its entry button, has visible, enabled, text, tooltip, frame, value (a progress bar's, 0 to 1), country (a flag's tag), number (an entry button's) and rows (a list's, a list of tables by its row's game widgets' names), as its kind has them; they are set and never read.";

pub const ELEMENTS: &str = "game.ui.<element>, an element of the mod's interface fragments by its name, has visible, enabled \
(true or false), text and tooltip (strings), which are set and never read.";

pub const MANIFEST: &[Field] = &[
    Field {
        name: "id",
        required: true,
        example: "id = \"my-mod\"",
        about: "The mod's id: lowercase letters, digits and -, starting with a letter. It names its log, its data folder and its save data.",
    },
    Field {
        name: "api_version",
        required: true,
        example: "api_version = \"0.3\"",
        about: "The capability catalogue the mod was written for; in 0.x the framework loads only a mod written for its own.",
    },
    Field {
        name: "description",
        required: false,
        example: "description = \"MY_MOD_DESCRIPTION\"",
        about: "A key in the mod's localisation, shown in the mods window in the player language.",
    },
    Field {
        name: "permissions",
        required: false,
        example: "permissions = { clock = true, files = true, network = [\"api.example.com\"] }",
        about: "The sensitive capabilities the mod uses; each is shown to the player in the mods window.",
    },
];

pub const PERMISSIONS: &[Permission] = &[
    Permission { name: "clock", declared: "clock = true", gives: "os.time, os.date", about: "The real date and time." },
    Permission {
        name: "files",
        declared: "files = true",
        gives: "files.read, files.write, files.list, files.remove",
        about: "Files in the mod's data folder, eu4-harmony/data/<id>/ in the game's user folder, kept across mod updates.",
    },
    Permission {
        name: "network",
        declared: "network = [\"api.example.com\", \"192.168.1.10:8080\"]",
        gives: "http.fetch",
        about: "HTTP and HTTPS requests to the hosts listed, by name or address, with a port where it is not the default.",
    },
];
