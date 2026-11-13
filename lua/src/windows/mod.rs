//! `game.window` and `game.popup` (docs/plans/extension-windows.md): what an
//! extension sets on its windows, checked against their layouts, and the
//! pop-ups it shows, put into words in the player language, before the host
//! has either. The running game and the fake game take them the same, and
//! keep what extensions want of them by the same rules ([`Windows`]).
mod hosted;
mod wanted;

pub use hosted::{HostedElement, HostedElements};
pub use wanted::{placed_defaults, Declared, OpenPopup, ResolvedEntry, ResolvedWindow, Wanted, Windows, POPUPS};

use crate::layout::{self, Kind, Placed, Window};
use mlua::{Lua, Table, Value as LuaValue};

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
    /// The same semantic text description used by declarations and tooltips.
    Text(crate::Text),
    Tooltip(crate::Text),
    Frame(u32),
    /// A progress bar's, from 0 to 1.
    Value(f64),
    /// An entry's independent number; none hides it.
    Number(Option<f64>),
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
            Property::Value(_) | Property::Number(_) => "value",
            Property::Country(_) => "country",
        }
    }
}

/// A pop-up to show, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Popup {
    /// The small line above the title.
    pub header: Option<String>,
    pub title: String,
    pub text: String,
    /// Its buttons' words, left to right, at most [`POPUP_BUTTONS`]; none
    /// for the game's own "OK". What each does is its action's, which the
    /// extension keeps: a button is no kind of button.
    pub buttons: Vec<String>,
    /// The countries of its left and right shields, by tag; the player's
    /// where none is given.
    pub shields: [Option<String>; 2],
}

impl Popup {
    /// How many buttons it shows: its own, or the game's own "OK".
    pub fn buttons_shown(&self) -> usize {
        self.buttons.len().max(1)
    }

    /// Its buttons' words left to right, or `[default_ok]` when the mod gave none.
    pub fn shown_buttons(&self, default_ok: &str) -> Vec<String> {
        if self.buttons.is_empty() {
            vec![default_ok.to_string()]
        } else {
            self.buttons.clone()
        }
    }
}

/// The most buttons a pop-up has, as the game's message pop-up has places
/// for.
pub const POPUP_BUTTONS: usize = 2;

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
        None => &["visible", "enabled", "tooltip", "frame", "value"],
        Some(Kind::Text { .. }) => &["visible", "text", "tooltip"],
        Some(Kind::Icon { .. }) => &["visible", "tooltip", "frame"],
        Some(Kind::Button { sprite: Some(_), .. }) => &["visible", "tooltip", "enabled", "frame"],
        Some(Kind::Button { .. }) => &["visible", "tooltip", "enabled"],
        Some(Kind::Progress { .. }) => &["visible", "tooltip", "value"],
        Some(Kind::Flag { .. }) => &["visible", "tooltip", "country"],
        Some(Kind::List { .. }) => &["visible", "tooltip", "rows"],
    };
    if !has(allowed) {
        let thing = kind.map(Kind::word).unwrap_or("entry button");
        return Err(format!("{what}.{property}: a {thing} has {}", allowed.join(", ")));
    }
    Ok(match property {
        "visible" => Property::Visible(flag()?),
        "enabled" => Property::Enabled(flag()?),
        "text" => Property::Text(crate::Text::from_lua(value.clone()).map_err(|e| format!("{what}.text: {e}"))?),
        "tooltip" => Property::Tooltip(crate::Text::from_lua(value.clone()).map_err(|e| format!("{what}.tooltip: {e}"))?),
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
    if widget == layout::ENTRY && property_name == "value" {
        let entry = w.entry.as_ref().unwrap();
        let value = match value {
            LuaValue::Nil => None,
            LuaValue::Integer(n) => Some(*n as f64),
            LuaValue::Number(n) => Some(*n),
            _ => return Err(format!("{what}.value is a finite number or nil")),
        };
        if value.is_some_and(|value| !entry.format.accepts(value)) {
            return Err(format!("{what}.value is a finite number in its format"));
        }
        return Ok(WindowChange::Set { window: name.to_string(), widget: widget.to_string(), value: Property::Number(value) });
    }
    if property_name == "rows" {
        let Some(Placed { kind: Kind::List { row, .. }, .. }) = placed else {
            return Err(format!("{what}.rows: only a list has rows"));
        };
        return rows(row, value, &what).map(|rows| WindowChange::Rows { window: name.to_string(), widget: widget.to_string(), rows });
    }
    let value = property(placed, property_name, value, &what)?;
    if widget == layout::ENTRY && w.entry.as_ref().is_some_and(|entry| entry.image.is_some()) && matches!(value, Property::Frame(frame) if frame != 1) {
        return Err(format!("{what}.frame: an entry image has one frame"));
    }
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
                LuaValue::Table(properties)
                    if matches!(placed.kind, Kind::Text { .. }) && properties.contains_key("key").map_err(|error| format!("{here}: {error}"))? =>
                {
                    row_out.push((name.clone(), property(Some(placed), "text", &cell, &here)?));
                }
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
pub type Words<'a> = &'a dyn Fn(&str) -> Option<String>;

/// `game.popup { header = key, title = key, text = key, values = { ... },
/// buttons = { { text = key, action = function }, ... }, shields = { tag, tag } }`
/// in words: `words` puts a key of the mod's into the player language with
/// its values, or says it has none. The buttons' actions stay the
/// extension's; only that each is a function, or none, is checked here.
pub fn popup(spec: &Table, words: Words, lua: &Lua) -> Result<Popup, String> {
    let fail = |e: mlua::Error| format!("game.popup: {e}");
    const FIELDS: [&str; 6] = ["header", "title", "text", "values", "buttons", "shields"];
    for pair in spec.pairs::<String, LuaValue>() {
        let (key, _) = pair.map_err(fail)?;
        if !FIELDS.contains(&key.as_str()) {
            return Err(format!("game.popup: {key} is not one of {}", FIELDS.join(", ")));
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
    let text_of = |field: &str, value: LuaValue| -> Result<String, String> {
        if value.is_nil() {
            return Err(format!("game.popup has a {field}, a text description"));
        }
        let legacy = matches!(value, LuaValue::String(_));
        let mut text = crate::Text::declaration_lua(value).map_err(fail)?;
        if legacy {
            if let crate::Text::Localized(localized) = &mut text {
                localized.values.extend(values.iter().cloned());
            }
        }
        text.check(&|key| words(key).is_some()).map_err(|error| format!("game.popup: {field}: {error}"))?;
        Ok(text.resolve(words))
    };
    let optional = |field: &str| -> Result<Option<String>, String> {
        let value = spec.get::<LuaValue>(field).map_err(fail)?;
        if value.is_nil() {
            Ok(None)
        } else {
            text_of(field, value).map(Some)
        }
    };
    let header = optional("header")?;
    let title = text_of("title", spec.get::<LuaValue>("title").map_err(fail)?)?;
    let text = text_of("text", spec.get::<LuaValue>("text").map_err(fail)?)?;
    let mut buttons = Vec::new();
    if let Some(list) = spec.get::<Option<Table>>("buttons").map_err(fail)? {
        let list: Vec<Table> = list
            .sequence_values::<Table>()
            .collect::<mlua::Result<_>>()
            .map_err(|_| "game.popup: buttons is a list of { text = key, action = function }".to_string())?;
        if list.len() > POPUP_BUTTONS {
            return Err(format!("game.popup: a pop-up has {POPUP_BUTTONS} buttons at most, as the game's do"));
        }
        for (i, button) in list.iter().enumerate() {
            let here = format!("buttons[{}]", i + 1);
            for pair in button.pairs::<String, LuaValue>() {
                let (key, _) = pair.map_err(fail)?;
                if key != "text" && key != "action" {
                    return Err(format!("game.popup: {here}: {key} is not one of text, action"));
                }
            }
            if !matches!(button.get::<LuaValue>("action").map_err(fail)?, LuaValue::Nil | LuaValue::Function(_)) {
                return Err(format!("game.popup: {here}.action is a function, called when the button is pressed"));
            }
            buttons.push(text_of(&format!("{here}.text"), button.get::<LuaValue>("text").map_err(fail)?)?);
        }
    }
    let mut shields = [None, None];
    if let Some(t) = spec.get::<Option<Table>>("shields").map_err(fail)? {
        let tags: Vec<String> = t.sequence_values::<String>().collect::<mlua::Result<_>>().map_err(fail)?;
        if tags.len() > 2 {
            return Err("game.popup: shields is { left, right }, two tags at most".into());
        }
        for (i, tag) in tags.into_iter().enumerate() {
            if !(tag.len() == 3 && tag.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())) {
                return Err(format!("game.popup: shields: {tag:?} is not a country's tag, such as FRA"));
            }
            shields[i] = Some(tag);
        }
    }
    Ok(Popup { header, title, text, buttons, shields })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn windows() -> Vec<Window> {
        let windows = json!({ "reforms": {
            "size": [400, 480], "entry": { "sprite": "GFX_x" },
            "items": [
                { "type": "text", "name": "status", "size": [100, 20], "tooltip": "STATUS_DETAILS" },
                { "type": "button", "name": "pick", "sprite": "GFX_pick", "size": [30, 30] },
                { "type": "list", "name": "countries", "size": [300, 280], "row": { "size": [300, 150], "items": [
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
        assert!(set(&w, "reforms", "status", "frame", &LuaValue::Integer(2)).unwrap_err().contains("a text has visible, text, tooltip"));
        assert_eq!(
            set(&w, "reforms", "status", "tooltip", &words),
            Ok(WindowChange::Set { window: "reforms".into(), widget: "status".into(), value: Property::Tooltip("Reformed".into()) })
        );
        assert_eq!(
            set(&w, "reforms", "status", "tooltip", &LuaValue::Nil),
            Ok(WindowChange::Set { window: "reforms".into(), widget: "status".into(), value: Property::Tooltip(String::new().into()) })
        );
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
        let words = |key: &str| match key {
            "T" => Some("Title".to_string()),
            "B" => Some("Body N=$N$".into()),
            "OK" => Some("Fine".into()),
            _ => None,
        };
        let spec: Table = lua
            .load(r#"return { title = "T", text = "B", values = { N = 3 }, buttons = { { text = "OK", action = function() end }, { text = "T" } }, shields = { "FRA" } }"#)
            .eval()
            .unwrap();
        assert_eq!(
            popup(&spec, &words, &lua),
            Ok(Popup {
                header: None,
                title: "Title".into(),
                text: "Body N=3".into(),
                buttons: vec!["Fine".into(), "Title".into()],
                shields: [Some("FRA".into()), None],
            })
        );
        let plain: Table = lua.load(r#"return { title = "T", text = "B" }"#).eval().unwrap();
        assert_eq!(popup(&plain, &words, &lua).map(|p| p.buttons), Ok(Vec::new()), "the game's own OK");
        let crowded: Table = lua.load(r#"return { title = "T", text = "B", buttons = { { text = "OK" }, { text = "OK" }, { text = "OK" } } }"#).eval().unwrap();
        assert!(popup(&crowded, &words, &lua).unwrap_err().contains("2 buttons at most"));
        let typed: Table = lua.load(r#"return { title = "T", text = "B", buttons = { { text = "OK", answer = "yes" } } }"#).eval().unwrap();
        assert!(popup(&typed, &words, &lua).unwrap_err().contains("answer is not one of text, action"), "a button has no kind");
        let missing: Table = lua.load(r#"return { title = "T", text = "B", buttons = { { text = "X" } } }"#).eval().unwrap();
        assert!(popup(&missing, &words, &lua).unwrap_err().contains("no text \"X\""));
    }
}
