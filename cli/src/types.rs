//! `eu4-harmony types`: the editor's type definitions for a mod, made from
//! the framework's own descriptions (the capability catalogue and
//! `eu4_harmony_remote::lua`), never written by hand. They are LuaLS
//! annotations of every capability, handler and global, so an editor
//! completes them and marks a misspelled capability or a wrong argument.
//!
//! They go in the mod's `.eu4-harmony/types/`, with a `.luarc.json` beside
//! the mod's folders that points the editor at them; `package` leaves both
//! out, as it does every name that starts with a dot.
use eu4_harmony_remote::{capabilities, lua, prose, result_shape, shape::Shape, Capability, Kind, API_VERSION};
use std::{fmt::Write as _, path::Path};

/// How `err` is returned: not as `eu4.Err|nil`, which would have the editor
/// ask for a nil check after testing the result instead of `err`, though
/// `err` is there exactly when the result is not. The result that comes with
/// it is not written `|nil` either, for the same reason (LuaLS, 2026-11-03):
/// `if err then return end` leaves it unchecked.
const ERR: &str = "eu4.Err err why it failed: nil when it did not";

/// Where the definitions go in a mod, and the editor's settings.
pub const FILE: &str = ".eu4-harmony/types/eu4-harmony.lua";
pub const LUARC: &str = ".luarc.json";

/// The editor's settings: Lua 5.4, the sandbox's libraries only, and the
/// definitions.
const LUARC_TEXT: &str = r#"{
  "runtime.version": "Lua 5.4",
  "runtime.builtin": { "io": "disable", "debug": "disable", "package": "disable" },
  "workspace.library": [".eu4-harmony/types"],
  "workspace.checkThirdParty": false
}
"#;

/// The classes the definitions use besides the capability groups.
const CLASSES: &str = r#"---@class eu4.Date
---@field year integer
---@field month integer
---@field day integer

---@class eu4.Err
---@field code string not_found, not_ready, rejected, invalid_argument, ... (eu4-harmony wiki errors)
---@field message string why, in the player language where the framework has the words

---@class eu4.Click
---@field right boolean
---@field shift boolean
---@field ctrl boolean

---@class eu4.Color
---@field r integer 0 to 255
---@field g integer 0 to 255
---@field b integer 0 to 255
---@field a? integer 0 to 255, 255 when left out

---@class eu4.FetchOptions
---@field method? string GET when left out
---@field headers? table<string, string>
---@field body? string
---@field timeout? number seconds, 30 when left out, at most 300

---@class eu4.Response
---@field status integer
---@field headers table<string, string> names in lowercase
---@field body string

---@class eu4.Element
---@field visible boolean set, never read
---@field enabled boolean set, never read
---@field text string|nil set, never read; nil is no text
---@field tooltip string|nil set, never read; nil is no tooltip
"#;

/// Writes the definitions, and the editor's settings where the mod has none,
/// into the mod at `root`. Returns the files written.
pub fn write(root: &Path) -> Result<Vec<String>, String> {
    let mut written = vec![FILE.to_string()];
    crate::write(&root.join(FILE), &definitions())?;
    if !root.join(LUARC).exists() {
        crate::write(&root.join(LUARC), LUARC_TEXT)?;
        written.push(LUARC.to_string());
    }
    Ok(written)
}

/// The type a value of `kind` has in Lua.
fn lua_type(kind: Kind) -> &'static str {
    match kind {
        Kind::Country | Kind::Text | Kind::UnitKind => "string",
        Kind::Province | Kind::Integer | Kind::Army => "integer",
        Kind::Date => "eu4.Date|string",
        Kind::Boolean => "boolean",
        Kind::Allies => "\"all\"|\"none\"|string[]",
        Kind::Provinces => "integer[]",
    }
}

/// The capabilities `game` has, as the Lua host builds it.
fn game_capabilities() -> Vec<Capability> {
    capabilities().into_iter().filter(|c| c.extensions && !c.action.is_empty() && c.group != "text").collect()
}

/// The definitions file.
pub fn definitions() -> String {
    let mut out = format!(
        "---@meta\n-- EU4 Harmony, capability catalogue {API_VERSION}: what a mod's Lua has.\n-- Written by eu4-harmony types; it writes them again for its version. Do not edit.\n\n{CLASSES}\n"
    );
    handlers(&mut out);
    results(&mut out);
    game(&mut out);
    globals(&mut out);
    out
}

fn handlers(out: &mut String) {
    let _ = writeln!(out, "--- The table main.lua returns: a handler for each event the mod hears.\n---@class eu4.Handlers");
    for h in lua::HANDLERS {
        let args: Vec<String> = std::iter::once("game: eu4.Game".to_string()).chain(h.args.iter().map(|a| format!("{}: {}", a.name, a.ty))).collect();
        let returns = h.returns.map_or(String::new(), |r| format!(": {}", r.ty));
        let _ = writeln!(out, "---@field {}? fun({}){returns} {}", h.name, args.join(", "), h.about);
    }
    out.push('\n');
}

fn game(out: &mut String) {
    let mut groups: Vec<&str> = game_capabilities().iter().map(|c| c.group).collect();
    groups.dedup();
    let _ = writeln!(out, "--- What handlers get: the game's state and commands, by capability group.\n---@class eu4.Game");
    let _ = writeln!(out, "---@field ui table<string, eu4.Element> {}", lua::ELEMENTS);
    let _ = writeln!(out, "---@field map eu4.game.map");
    for group in &groups {
        let _ = writeln!(out, "---@field {group} eu4.game.{group}");
    }
    out.push('\n');
    for group in groups {
        let table = format!("eu4_game_{group}");
        let _ = writeln!(out, "---@class eu4.game.{group}\nlocal {table} = {{}}\n");
        for c in game_capabilities().iter().filter(|c| c.group == group) {
            capability(out, &table, c);
        }
    }
    let _ = writeln!(out, "---@class eu4.game.map\nlocal eu4_game_map = {{}}\n");
    for f in lua::FUNCTIONS.iter().filter(|f| f.name.starts_with("game.map.")) {
        function(out, &format!("eu4_game_map.{}", &f.name["game.map.".len()..]), f);
    }
}

/// A capability: the required arguments in order, the optional ones in a
/// table last; its result, or `nil, err`.
fn capability(out: &mut String, table: &str, c: &Capability) {
    let _ = writeln!(out, "--- {}", prose(c.about));
    let mut names = Vec::new();
    for arg in c.args.iter().filter(|a| !a.optional) {
        let _ = writeln!(out, "---@param {} {} {}", arg.name, lua_type(arg.kind), prose(arg.about));
        names.push(arg.name.to_string());
    }
    let optional: Vec<String> = c.args.iter().filter(|a| a.optional).map(|a| format!("{}?: {}", a.name, lua_type(a.kind))).collect();
    if !optional.is_empty() {
        let _ = writeln!(out, "---@param options? {{ {} }}", optional.join(", "));
        names.push("options".into());
    }
    let result = result_shape(&c.name()).map_or("any".to_string(), |shape| shape_type(&shape));
    let _ = writeln!(out, "---@return {result} result {}; nil when it failed\n---@return {ERR}", prose(c.result));
    let _ = writeln!(out, "function {table}.{}({}) end\n", c.action, names.join(", "));
}

/// The type a value of `shape` has in Lua, an object's by its class.
fn shape_type(shape: &Shape) -> String {
    let grouped = |inner: &Shape| {
        let t = shape_type(inner);
        if t.contains('|') {
            format!("({t})")
        } else {
            t
        }
    };
    match shape {
        Shape::Integer => "integer".into(),
        Shape::Number => "number".into(),
        Shape::Boolean => "boolean".into(),
        Shape::Text => "string".into(),
        Shape::Words(words) => words.iter().map(|w| format!("\"{w}\"")).collect::<Vec<_>>().join("|"),
        Shape::Optional(inner) => format!("{}|nil", shape_type(inner)),
        Shape::List(inner) => format!("{}[]", grouped(inner)),
        Shape::Map(inner) => format!("table<string, {}>", shape_type(inner)),
        Shape::Object { class, .. } => format!("eu4.{class}"),
        Shape::Either(shapes) => shapes.iter().map(shape_type).collect::<Vec<_>>().join("|"),
        Shape::Date => "eu4.Date".into(),
        Shape::Nothing => "true".into(),
        Shape::Any => "any".into(),
    }
}

/// Every class `shape` names, with the classes inside it, once each, in
/// the order met.
fn classes(shape: &Shape, found: &mut Vec<Shape>) {
    match shape {
        Shape::Object { class, fields, .. } => {
            if !found.iter().any(|f| matches!(f, Shape::Object { class: c, .. } if c == class)) {
                found.push(shape.clone());
                for field in fields {
                    classes(&field.shape, found);
                }
            }
        }
        Shape::Optional(inner) | Shape::List(inner) | Shape::Map(inner) => classes(inner, found),
        Shape::Either(shapes) => shapes.iter().for_each(|s| classes(s, found)),
        _ => {}
    }
}

/// The classes of the capabilities' results.
fn results(out: &mut String) {
    let mut found = Vec::new();
    for c in game_capabilities() {
        if let Some(shape) = result_shape(&c.name()) {
            classes(&shape, &mut found);
        }
    }
    for class in found {
        let Shape::Object { class, about, fields } = class else { continue };
        if !about.is_empty() {
            let _ = writeln!(out, "--- {about}");
        }
        let _ = writeln!(out, "---@class eu4.{class}");
        for field in fields {
            let left_out = if field.may_be_left_out || matches!(field.shape, Shape::Optional(_)) { "?" } else { "" };
            let shape = match &field.shape {
                Shape::Optional(inner) => inner.as_ref(),
                other => other,
            };
            let _ = writeln!(out, "---@field {}{left_out} {} {}", field.name, shape_type(shape), field.about);
        }
        out.push('\n');
    }
}

/// A function of the sandbox, as `lua` describes it.
fn function(out: &mut String, name: &str, f: &lua::Function) {
    let _ = writeln!(out, "--- {}", f.about);
    if f.waits {
        let _ = writeln!(out, "--- It waits: call it in a handler or a task, not at the top of main.lua or in on_save.");
    }
    if let Some(permission) = f.permission.and_then(|p| lua::PERMISSIONS.iter().find(|q| q.name == p)) {
        let _ = writeln!(out, "--- Needs permissions = {{ {} }} in manifest.toml.", permission.declared);
    }
    let mut names = Vec::new();
    for arg in f.args {
        let optional = if arg.optional && arg.name != "..." { "?" } else { "" };
        let _ = writeln!(out, "---@param {}{optional} {} {}", arg.name, arg.ty, arg.about);
        names.push(arg.name);
    }
    for r in f.returns {
        if r.ty == "eu4.Err" {
            let _ = writeln!(out, "---@return {ERR}");
            continue;
        }
        // A result that comes with err is nil exactly when err is not.
        let with_err = f.returns.iter().any(|r| r.ty == "eu4.Err");
        let (ty, nil) = match (r.optional, with_err) {
            (true, true) => (r.ty.to_string(), "; nil when it failed"),
            (true, false) => (format!("{}|nil", r.ty), ""),
            (false, _) => (r.ty.to_string(), ""),
        };
        let _ = writeln!(out, "---@return {ty} {} {}{nil}", r.name, r.about);
    }
    let _ = writeln!(out, "function {name}({}) end\n", names.join(", "));
}

/// The globals, but those Lua's own definitions have (require and os).
fn globals(out: &mut String) {
    let mut tables = Vec::new();
    for f in lua::FUNCTIONS {
        let Some((table, _)) = f.name.split_once('.') else {
            if f.name != "require" {
                function(out, f.name, f);
            }
            continue;
        };
        if table == "game" || table == "os" {
            continue;
        }
        if !tables.contains(&table) {
            tables.push(table);
            let _ = writeln!(out, "{table} = {{}}\n");
        }
        function(out, f.name, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_definitions_are_lua_and_name_every_capability_handler_and_global() {
        let text = definitions();
        mlua::Lua::new().load(&text).set_name("eu4-harmony.lua").into_function().expect("the definitions compile");
        for c in game_capabilities() {
            assert!(text.contains(&format!("function eu4_game_{}.{}(", c.group, c.action)), "{}", c.name());
        }
        for h in lua::HANDLERS {
            assert!(text.contains(&format!("---@field {}? fun(game: eu4.Game", h.name)), "{}", h.name);
        }
        for g in lua::GLOBALS.iter().filter(|g| **g != "require") {
            assert!(text.contains(&format!("function {g}")) || text.contains(&format!("{g} = {{}}")), "{g}");
        }
        assert!(text.contains("function eu4_game_map.paint(colors) end"));
    }

    #[test]
    fn a_capability_takes_its_required_arguments_and_then_its_options() {
        let text = definitions();
        let declare = text.split("\n\n").find(|block| block.contains("function eu4_game_diplomacy.declare_war(")).unwrap();
        assert!(declare.contains("---@param country string"), "{declare}");
        assert!(
            declare.contains("---@param options? { war_goal?: string, province?: integer, target?: string, call_allies?: \"all\"|\"none\"|string[] }"),
            "{declare}"
        );
        assert!(declare.contains("function eu4_game_diplomacy.declare_war(country, casus_belli, options) end"), "{declare}");
        let fetch = text.split("\n\n").find(|block| block.contains("function http.fetch(")).unwrap();
        assert!(fetch.contains("It waits") && fetch.contains("Needs permissions = { network = "), "{fetch}");
        // err is not nil-checked once the result has been (LuaLS, 2026-11-03).
        assert!(declare.contains(&format!("---@return {ERR}")) && fetch.contains(&format!("---@return {ERR}")), "{declare}\n{fetch}");
        assert!(!text.contains("eu4.Err|nil"));
        // An element takes nil, as text.get may answer.
        assert!(text.contains("---@field tooltip string|nil"));
    }

    #[test]
    fn a_result_has_its_class_and_its_fields() {
        let text = definitions();
        let army = text
            .split(
                "

",
            )
            .find(|block| {
                block.contains(
                    "---@class eu4.Army
",
                )
            })
            .unwrap();
        for field in ["---@field id integer", "---@field kind \"army\"|\"navy\"", "---@field province? integer", "---@field route integer[]"] {
            assert!(army.contains(field), "{field} in {army}");
        }
        let get = text
            .split(
                "

",
            )
            .find(|block| block.contains("function eu4_game_army.get("))
            .unwrap();
        assert!(get.contains("---@return eu4.Army result"), "{get}");
        // Every class a result names is written.
        for class in text.match_indices("eu4.").map(|(i, _)| text[i + 4..].split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap()) {
            assert!(text.contains(&format!("---@class eu4.{class}")) || class == "game", "eu4.{class} is named but not written");
        }
        // A result with err is nil only when err is not.
        assert!(!text.contains("|nil result") && !text.contains("string|nil text"));
    }

    #[test]
    fn the_editor_s_settings_are_written_once_and_the_definitions_every_time() {
        let root = std::env::temp_dir().join(format!("eu4h-types-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(write(&root).unwrap(), [FILE, LUARC]);
        std::fs::write(root.join(LUARC), "{}").unwrap();
        assert_eq!(write(&root).unwrap(), [FILE]);
        assert_eq!(std::fs::read_to_string(root.join(LUARC)).unwrap(), "{}");
        serde_json::from_str::<serde_json::Value>(LUARC_TEXT).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
}
