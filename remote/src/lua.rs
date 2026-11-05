//! What a Lua extension has besides the capabilities
//! (ADR extensions-are-lua-in-a-sandbox): its handlers, the sandbox's globals,
//! its manifest and the sensitive capabilities that manifest may declare
//! (ADR sensitive-capabilities-are-declared). Described once here, for
//! `eu4-harmony wiki`, the editor's type definitions and the core's test that
//! the sandbox holds every global described and no other.
//!
//! A type is written as the editor's type definitions write it (LuaLS):
//! `string`, `integer`, `eu4.Date`, `eu4.Err`, `string[]`.

/// A handler: a function `main.lua`'s table may hold, called for an event.
#[derive(Clone, Copy, Debug)]
pub struct Handler {
    pub name: &'static str,
    /// What follows `game`.
    pub args: &'static [Value],
    /// What it may return, where that counts.
    pub returns: Option<Value>,
    pub about: &'static str,
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

const fn optional(name: &'static str, ty: &'static str, about: &'static str) -> Value {
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

pub const HANDLERS: &[Handler] = &[
    Handler {
        name: "on_session",
        args: &[optional("saved", "any", "what this mod's save_data returned when the save it began from was written, or, after an extension reload, what the code before it returned; nil for a new game or a save without it")],
        returns: None,
        about: "A session began: a save was loaded or a game started.",
    },
    Handler {
        name: "on_session_ended",
        args: &[value("reason", "\"return\"|\"load\"|\"new_game\"", "why: for the main menu, for a save loaded, or for a new game")],
        returns: None,
        about: "The session is being left, while its state is still there to read. Its waiting tasks are dropped after this.",
    },
    Handler {
        name: "on_day",
        args: &[value("date", "eu4.Date", "the new day")],
        returns: None,
        about: "A new day, once visibility reflects it. With the game running fast, days in between may be left out.",
    },
    Handler { name: "on_month", args: &[value("date", "eu4.Date", "the first day heard in the month")], returns: None, about: "A new month: the first day heard in it, after on_day. With the game running fast it may be past the 1st." },
    Handler { name: "on_year", args: &[value("date", "eu4.Date", "the first day heard in the year")], returns: None, about: "A new year: the first day heard in it, after on_month." },
    Handler {
        name: "on_army_ordered",
        args: &[value("army", "integer", "the army's ID")],
        returns: None,
        about: "The player gave one of their armies an order, by hand or through remote control; not for this mod's own orders.",
    },
    Handler { name: "on_army_defeated", args: &[value("army", "integer", "the army's ID")], returns: None, about: "One of the player's armies lost a battle." },
    Handler {
        name: "on_battle",
        args: &[value(
            "battle",
            "table",
            "{ province, naval, attackers, defenders, winner, attacker_losses, defender_losses }: the countries on each side, the leading one first; winner \"attackers\" or \"defenders\"; the losses in men in a land battle, in ships in a naval one",
        )],
        returns: None,
        about: "A land or naval battle was decided, anywhere in the world.",
    },
    Handler {
        name: "on_war_declared",
        args: &[
            value("war", "string", "the war's name, as the game shows it"),
            value("attacker", "string", "the attackers' leader, a tag"),
            value("defender", "string", "the defenders' leader, a tag"),
        ],
        returns: None,
        about: "A war began, between any two countries.",
    },
    Handler {
        name: "on_peace_made",
        args: &[
            value("war", "string", "the war's name, as the game shows it"),
            value("attackers", "string[]", "the attackers that left it, tags"),
            value("defenders", "string[]", "the defenders that left it, tags"),
            value("ended", "boolean", "whether the war is over"),
        ],
        returns: None,
        about: "Countries left a war on a day: by a peace, the whole war's or a separate one, or by ceasing to be.",
    },
    Handler {
        name: "on_province_owner_changed",
        args: &[
            value("province", "integer", "the province's number"),
            optional("from", "string", "the old owner, a tag; nil for none"),
            optional("to", "string", "the new owner, a tag; nil for none"),
        ],
        returns: None,
        about: "A province changed owner.",
    },
    Handler {
        name: "on_building_finished",
        args: &[
            value("province", "integer", "the province's number"),
            value("building", "string", "the building's content key"),
            value("country", "string", "the province's owner, by its tag"),
        ],
        returns: None,
        about: "A building was finished in a province, any country's. Worked out only while some mod has this handler or a remote client reads it.",
    },
    Handler {
        name: "on_button",
        args: &[value("name", "string", "the button's name in its interface fragment"), value("click", "eu4.Click", "{ right, shift, ctrl }")],
        returns: None,
        about: "The player clicked one of this mod's buttons.",
    },
    Handler { name: "on_map_lost", args: &[], returns: None, about: "Another mod, or the player changing the map mode, took the map this mod painted." },
    Handler {
        name: "on_frame",
        args: &[],
        returns: None,
        about: "Every frame of a game, after the events above. Keep it short: it runs as often as the game draws.",
    },
    Handler {
        name: "save_data",
        args: &[],
        returns: Some(optional("data", "any", "this mod's save data, any value JSON can hold; nil keeps nothing")),
        about: "The framework asks for this mod's data: the game writes its world, the ironman exit save included, or the mod's code is about to be reloaded. It cannot wait.",
    },
];

/// The handler `event` calls, named for its kind (`on_day` for `day`), and
/// its arguments in Lua's form (`eu4_harmony_checks::form`): the event's
/// field each of the handler's arguments names, in their order; for an
/// argument that names none, the whole event as a table (`on_battle`'s).
/// `on_session`'s argument is the mod's saved data, which the Lua host gives
/// itself. None for an event without a handler, which a test rules out.
pub fn handler_call(event: &crate::events::Event) -> Option<(&'static str, Vec<serde_json::Value>)> {
    let name = format!("on_{}", event.kind());
    let handler = HANDLERS.iter().find(|h| h.name == name)?;
    let mut table = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(event)).ok()?;
    let fields = table.as_object_mut()?;
    fields.remove("event");
    let args = handler.args.iter().map(|arg| fields.get(arg.name).cloned().unwrap_or_else(|| serde_json::Value::Object(fields.clone()))).collect();
    Some((handler.name, args))
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
];

/// The globals the framework puts in the sandbox, besides Lua's own and the
/// `game` its handlers get: each a function or a table of them.
pub const GLOBALS: &[&str] = &["log", "print", "json", "mod_files", "text", "require", "spawn", "files", "http"];

/// What Lua has in the sandbox of its own.
pub const SANDBOX: &str = "Lua 5.4 with its string, table, math, utf8 and coroutine libraries and the base library, \
except dofile, loadfile and loading bytecode (load takes text only). There is no io, debug or package; os has os.clock, \
and os.time and os.date for a mod that declares clock. Nothing reaches the process, the system, other mods or native code.";

/// What `game.ui.<element>` is.
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
