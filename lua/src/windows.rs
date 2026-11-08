//! `game.window` and `game.popup` (docs/plans/extension-windows.md): what an
//! extension sets on its windows, checked against their layouts, and the
//! pop-ups it shows, put into words in the player language, before the host
//! has either. The running game and the fake game take them the same.
use crate::layout::{self, Kind, Placed, Window};
use mlua::{Lua, Table, Value as LuaValue};

/// The most buttons a pop-up has.
pub const BUTTONS: usize = 4;

/// A change an extension makes to its windows, already checked against
/// their layouts.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowChange {
    Open(String),
    Close(String),
    /// A property of one of a window's game widgets, or of its entry button
    /// (`widget` is [`layout::ENTRY`]).
    Set {
        window: String,
        widget: String,
        value: Property,
    },
    /// Every row of a list, each the properties of its row's game widgets
    /// by name.
    Rows {
        window: String,
        widget: String,
        rows: Vec<Vec<(String, Property)>>,
    },
}

/// A property of a game widget.
#[derive(Clone, Debug, PartialEq)]
pub enum Property {
    Visible(bool),
    Enabled(bool),
    /// Words, as the player reads them.
    Text(String),
    Tooltip(String),
    Frame(u32),
    /// A progress bar's, from 0 to 1.
    Value(f64),
    /// A flag's country, by its tag.
    Country(String),
}

impl Property {
    pub fn name(&self) -> &'static str {
        match self {
            Property::Visible(_) => "visible",
            Property::Enabled(_) => "enabled",
            Property::Text(_) => "text",
            Property::Tooltip(_) => "tooltip",
            Property::Frame(_) => "frame",
            Property::Value(_) => "value",
            Property::Country(_) => "country",
        }
    }
}

/// A pop-up to show, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Popup {
    /// The mod's name for it, by which it may close it.
    pub id: Option<String>,
    pub title: String,
    pub text: String,
    pub buttons: Vec<String>,
}

/// What a property is called with on a game widget, `what` naming it for
/// an error.
fn property(placed: Option<&Placed>, property: &str, value: &LuaValue, what: &str) -> Result<Property, String> {
    let words = || match value {
        LuaValue::String(s) => s.to_str().map(|s| s.to_string()).map_err(|_| format!("{what}.{property} is not UTF-8")),
        LuaValue::Nil => Ok(String::new()),
        LuaValue::Integer(i) => Ok(i.to_string()),
        LuaValue::Number(n) => Ok(n.to_string()),
        other => Err(format!("{what}.{property} takes a string, not a {}", other.type_name())),
    };
    let flag = || match value {
        LuaValue::Boolean(b) => Ok(*b),
        other => Err(format!("{what}.{property} takes true or false, not a {}", other.type_name())),
    };
    let number = || match value {
        LuaValue::Integer(i) => Ok(*i as f64),
        LuaValue::Number(n) => Ok(*n),
        other => Err(format!("{what}.{property} takes a number, not a {}", other.type_name())),
    };
    // An entry button has no layout of a game widget.
    let kind = placed.map(|p| &p.kind);
    let has = |names: &[&str]| names.contains(&property);
    let allowed: &[&str] = match kind {
        None => &["visible", "tooltip", "frame"],
        Some(Kind::Text { .. }) => &["visible", "text"],
        Some(Kind::Icon { .. }) => &["visible", "tooltip", "frame"],
        Some(Kind::Button { sprite: Some(_), .. }) => &["visible", "tooltip", "enabled", "frame"],
        Some(Kind::Button { .. }) => &["visible", "tooltip", "enabled"],
        Some(Kind::Progress { .. }) => &["visible", "tooltip", "value"],
        Some(Kind::Flag { .. }) => &["visible", "tooltip", "country"],
        Some(Kind::List { .. }) => &["visible", "rows"],
    };
    if !has(allowed) {
        let thing = kind.map(Kind::word).unwrap_or("entry button");
        return Err(format!("{what}.{property}: a {thing} has {}", allowed.join(", ")));
    }
    Ok(match property {
        "visible" => Property::Visible(flag()?),
        "enabled" => Property::Enabled(flag()?),
        "text" => Property::Text(words()?),
        "tooltip" => Property::Tooltip(words()?),
        "frame" => {
            let n = number()?;
            if n < 1.0 || n.fract() != 0.0 || n > 1000.0 {
                return Err(format!("{what}.frame is a frame's number, from 1"));
            }
            Property::Frame(n as u32)
        }
        "value" => Property::Value(number()?.clamp(0.0, 1.0)),
        "country" => {
            let tag = words()?;
            if !(tag.len() == 3 && tag.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())) && !tag.is_empty() {
                return Err(format!("{what}.country is a country's tag, such as FRA, not {tag:?}"));
            }
            Property::Country(tag)
        }
        _ => unreachable!("rows is set as rows"),
    })
}

/// The window `name` of `windows`, or why there is none.
fn window<'a>(windows: &'a [Window], name: &str) -> Result<&'a Window, String> {
    windows.iter().find(|w| w.name == name).ok_or_else(|| format!("the mod has no window {name:?} in the windows its main.lua returns"))
}

/// `game.window.open(name)` and `game.window.close(name)`.
pub fn open_or_close(windows: &[Window], open: bool, name: &str) -> Result<WindowChange, String> {
    window(windows, name)?;
    Ok(if open { WindowChange::Open(name.to_string()) } else { WindowChange::Close(name.to_string()) })
}

/// `game.window.<window>.<widget>.<property> = value`.
pub fn set(windows: &[Window], name: &str, widget: &str, property_name: &str, value: &LuaValue) -> Result<WindowChange, String> {
    let w = window(windows, name)?;
    let what = format!("game.window.{name}.{widget}");
    let placed = if widget == layout::ENTRY {
        if w.entry.is_none() {
            return Err(format!("{what}: the window has no entry button in its layout"));
        }
        None
    } else {
        Some(w.widget(widget).ok_or_else(|| format!("{what}: the window has no game widget named {widget}"))?)
    };
    if property_name == "rows" {
        let Some(Placed { kind: Kind::List { row, .. }, .. }) = placed else {
            return Err(format!("{what}.rows: only a list has rows"));
        };
        return rows(row, value, &what).map(|rows| WindowChange::Rows { window: name.to_string(), widget: widget.to_string(), rows });
    }
    let value = property(placed, property_name, value, &what)?;
    Ok(WindowChange::Set { window: name.to_string(), widget: widget.to_string(), value })
}

/// A list's rows: a list of tables, each from its row's game widgets' names
/// to what they show: words for a text, true or false for whether a text
/// button is enabled, a number for a progress bar or an icon's frame, a tag
/// for a flag, or a table of properties.
fn rows(row: &layout::Row, value: &LuaValue, what: &str) -> Result<Vec<Vec<(String, Property)>>, String> {
    let LuaValue::Table(list) = value else { return Err(format!("{what}.rows takes a list of rows, not a {}", value.type_name())) };
    let mut out = Vec::new();
    for (i, item) in list.sequence_values::<LuaValue>().enumerate() {
        let item = item.map_err(|e| e.to_string())?;
        let at = format!("{what}.rows[{}]", i + 1);
        let LuaValue::Table(cells) = item else { return Err(format!("{at} is a table of game widgets by name, not a {}", item.type_name())) };
        let mut row_out = Vec::new();
        for pair in cells.pairs::<String, LuaValue>() {
            let (name, cell) = pair.map_err(|e| format!("{at}: {e}"))?;
            let placed = row.widget(&name).ok_or_else(|| format!("{at}: its row has no game widget named {name}"))?;
            let here = format!("{at}.{name}");
            match &cell {
                LuaValue::Table(properties) => {
                    for pair in properties.pairs::<String, LuaValue>() {
                        let (p, v) = pair.map_err(|e| format!("{here}: {e}"))?;
                        row_out.push((name.clone(), property(Some(placed), &p, &v, &here)?));
                    }
                }
                plain => {
                    let main = match placed.kind {
                        Kind::Text { .. } => "text",
                        Kind::Button { sprite: None, .. } => "enabled",
                        Kind::Icon { .. } | Kind::Button { .. } => "frame",
                        Kind::Progress { .. } => "value",
                        Kind::Flag { .. } => "country",
                        Kind::List { .. } => "rows",
                    };
                    row_out.push((name.clone(), property(Some(placed), main, plain, &here)?));
                }
            }
        }
        out.push(row_out);
    }
    Ok(out)
}

/// What puts a key of the mod's into the player language with its values,
/// or says it has none.
pub type Words<'a> = &'a dyn Fn(&str, &[(String, String)]) -> Option<String>;

/// `game.popup { title = key, text = key, values = { ... }, buttons = { key, ... }, id = name }`
/// in words: `words` puts a key of the mod's into the player language with
/// its values, or says it has none.
pub fn popup(spec: &Table, words: Words, lua: &Lua) -> Result<Popup, String> {
    let fail = |e: mlua::Error| format!("game.popup: {e}");
    for pair in spec.pairs::<String, LuaValue>() {
        let (key, _) = pair.map_err(fail)?;
        if !["title", "text", "values", "buttons", "id"].contains(&key.as_str()) {
            return Err(format!("game.popup: {key} is not one of title, text, values, buttons, id"));
        }
    }
    let mut values = Vec::new();
    if let Some(table) = spec.get::<Option<Table>>("values").map_err(fail)? {
        let tostring: mlua::Function = lua.globals().get("tostring").map_err(fail)?;
        for pair in table.pairs::<String, LuaValue>() {
            let (name, value) = pair.map_err(fail)?;
            values.push((name, tostring.call::<String>(value).map_err(fail)?));
        }
    }
    let text_of = |field: &str, key: Option<String>| -> Result<String, String> {
        let key = key.ok_or_else(|| format!("game.popup has a {field}, a localisation key of the mod's"))?;
        words(&key, &values).ok_or_else(|| format!("game.popup: {field}: the mod has no text {key:?} in its localisation"))
    };
    let title = text_of("title", spec.get::<Option<String>>("title").map_err(fail)?)?;
    let text = text_of("text", spec.get::<Option<String>>("text").map_err(fail)?)?;
    let keys: Vec<String> = match spec.get::<Option<Table>>("buttons").map_err(fail)? {
        None => return Err("game.popup has buttons, a list of one to four localisation keys".into()),
        Some(t) => t.sequence_values::<String>().collect::<mlua::Result<_>>().map_err(fail)?,
    };
    if keys.is_empty() || keys.len() > BUTTONS {
        return Err(format!("game.popup has one to {BUTTONS} buttons, not {}", keys.len()));
    }
    let buttons = keys.into_iter().map(|k| text_of("buttons", Some(k))).collect::<Result<_, _>>()?;
    let id = spec.get::<Option<String>>("id").map_err(fail)?;
    if id.as_deref().is_some_and(|i| !layout::is_name(i)) {
        return Err("game.popup: id is a name of lowercase letters, digits and _".into());
    }
    Ok(Popup { id, title, text, buttons })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn windows() -> Vec<Window> {
        let windows = json!({ "reforms": {
            "size": [400, 400], "entry": { "sprite": "GFX_x" },
            "items": [
                { "type": "text", "name": "status", "size": [100, 20] },
                { "type": "button", "name": "pick", "sprite": "GFX_pick", "size": [30, 30] },
                { "type": "list", "name": "countries", "size": [300, 280], "row": { "size": [300, 140], "items": [
                    { "type": "flag", "name": "flag" }, { "type": "progress", "name": "share" }, { "type": "text", "name": "name", "size": [100, 20] },
                ]}},
            ],
        }});
        layout::read(&windows, &|_| true).unwrap()
    }

    #[test]
    fn a_property_is_one_its_game_widget_has() {
        let lua = Lua::new();
        let w = windows();
        let words = LuaValue::String(lua.create_string("Reformed").unwrap());
        assert_eq!(
            set(&w, "reforms", "status", "text", &words),
            Ok(WindowChange::Set { window: "reforms".into(), widget: "status".into(), value: Property::Text("Reformed".into()) })
        );
        assert!(set(&w, "reforms", "status", "frame", &LuaValue::Integer(2)).unwrap_err().contains("a text has visible, text"));
        assert!(set(&w, "reforms", "pick", "frame", &LuaValue::Integer(2)).is_ok(), "an icon button's frame");
        assert!(set(&w, "reforms", "entry", "frame", &LuaValue::Integer(3)).is_ok());
        assert!(set(&w, "reforms", "gone", "visible", &LuaValue::Boolean(true)).unwrap_err().contains("no game widget named gone"));
        assert!(set(&w, "other", "status", "visible", &LuaValue::Boolean(true)).unwrap_err().contains("no window \"other\""));
    }

    #[test]
    fn rows_take_plain_values_by_their_game_widget_s_kind() {
        let lua = Lua::new();
        let w = windows();
        let rows: Table = lua.load(r#"return { { flag = "FRA", share = 0.25, name = { text = "France", visible = true } }, {} }"#).eval().unwrap();
        let WindowChange::Rows { rows, .. } = set(&w, "reforms", "countries", "rows", &LuaValue::Table(rows)).unwrap() else { panic!() };
        assert_eq!(rows.len(), 2);
        let mut first = rows[0].clone();
        first.sort_by(|a, b| (a.0.as_str(), a.1.name()).cmp(&(b.0.as_str(), b.1.name())));
        assert_eq!(
            first,
            [
                ("flag".into(), Property::Country("FRA".into())),
                ("name".into(), Property::Text("France".into())),
                ("name".into(), Property::Visible(true)),
                ("share".into(), Property::Value(0.25)),
            ]
        );
        let wrong: Table = lua.load(r#"return { { flag = "france" } }"#).eval().unwrap();
        assert!(set(&w, "reforms", "countries", "rows", &LuaValue::Table(wrong)).unwrap_err().contains("a country's tag"));
        assert!(set(&w, "reforms", "status", "rows", &LuaValue::Nil).unwrap_err().contains("only a list has rows"));
    }

    #[test]
    fn a_popup_is_put_into_words() {
        let lua = Lua::new();
        let words = |key: &str, values: &[(String, String)]| match key {
            "T" => Some("Title".to_string()),
            "B" => Some(format!("Body {}", values.iter().map(|(n, v)| format!("{n}={v}")).collect::<String>())),
            "OK" => Some("Fine".into()),
            _ => None,
        };
        let spec: Table = lua.load(r#"return { title = "T", text = "B", values = { N = 3 }, buttons = { "OK" }, id = "first" }"#).eval().unwrap();
        assert_eq!(
            popup(&spec, &words, &lua),
            Ok(Popup { id: Some("first".into()), title: "Title".into(), text: "Body N=3".into(), buttons: vec!["Fine".into()] })
        );
        let none: Table = lua.load(r#"return { title = "T", text = "B", buttons = {} }"#).eval().unwrap();
        assert!(popup(&none, &words, &lua).unwrap_err().contains("one to 4 buttons"));
        let missing: Table = lua.load(r#"return { title = "T", text = "X", buttons = { "OK" } }"#).eval().unwrap();
        assert!(popup(&missing, &words, &lua).unwrap_err().contains("no text \"X\""));
    }
}
