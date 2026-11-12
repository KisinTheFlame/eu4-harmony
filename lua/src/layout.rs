//! Extension windows' layouts (docs/plans/extension-windows.md): the `windows` an
//! extension's `main.lua` returns, read, checked and laid out, the same for
//! the running game and the fake game.
//!
//! A window is a column of game widgets under its title; columns and rows
//! inside it lay their items out with spacing and alignment, and any game
//! widget may instead stand at `at = {x, y}` from the window's top left
//! corner. What comes out is every game widget at its place in the window,
//! and each list's row laid out the same way, from the row's top left.
//! Texts are the mod's localisation keys, checked when the extension loads.
use serde_json::{Map, Value};
use std::collections::HashSet;

/// Room between a window's edges and what it holds.
pub const MARGIN: i32 = 24;
/// Room the title takes at a window's top, on its ribbon, the margin below
/// it included.
pub const TITLE: i32 = 66;
/// Room the game's long Close button takes at a window's bottom, as its
/// panels have it, the margin above it included.
pub const FOOTER_CLOSE: i32 = 60;
/// A window's Close button, the game's text button, centered along its
/// bottom, this far above it.
pub const CLOSE_ABOVE: i32 = 14;
/// The widths the game's text buttons come in, all as high as
/// [`BUTTON_HEIGHT`]: a text button takes the widest that its size has room
/// for, or the narrowest.
pub const BUTTON_WIDTHS: [i32; 5] = [79, 109, 149, 189, 224];
pub const BUTTON_HEIGHT: i32 = 31;
/// The progress bar a `progress` game widget is unless it names its sprite,
/// and that sprite's size.
pub const PROGRESS_SPRITE: &str = "GFX_wargoal_progressbar";
pub const PROGRESS_SIZE: (i32, i32) = (288, 18);
/// The flag a `flag` game widget is unless it names its sprite, and its
/// size.
pub const FLAG_SPRITE: &str = "GFX_shield_thin";
pub const FLAG_SIZE: (i32, i32) = (92, 92);
/// The room between the items of a column or a row, unless it says.
const SPACING: i32 = 6;

/// Names an extension's window may not have: `game.window`'s own.
const RESERVED_WINDOWS: [&str; 2] = ["open", "close"];
/// The name an entry button is set by, which no game widget may have.
pub const ENTRY: &str = "entry";

/// One extension window, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub name: String,
    /// Width and height.
    pub size: (i32, i32),
    /// The title's localisation key.
    pub title: Option<String>,
    pub close: bool,
    pub anchor: Anchor,
    /// From the anchor, in pixels, right and down.
    pub offset: (i32, i32),
    pub moveable: bool,
    /// One of the game's big panels: opening it closes the others.
    pub panel: bool,
    /// The color of its title's ribbon.
    pub ribbon: Ribbon,
    pub entry: Option<Entry>,
    pub widgets: Vec<Placed>,
}

/// The color of a window's title ribbon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ribbon {
    Red,
    Blue,
}

/// Where a window stands on the screen, before its offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Center,
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    const NAMES: [(&'static str, Anchor); 9] = [
        ("center", Anchor::Center),
        ("top_left", Anchor::TopLeft),
        ("top", Anchor::Top),
        ("top_right", Anchor::TopRight),
        ("left", Anchor::Left),
        ("right", Anchor::Right),
        ("bottom_left", Anchor::BottomLeft),
        ("bottom", Anchor::Bottom),
        ("bottom_right", Anchor::BottomRight),
    ];

    /// Its fraction of the room left beside the window, across and down:
    /// 0 at the left or top, 1 at the right or bottom.
    pub fn fraction(self) -> (f64, f64) {
        match self {
            Anchor::Center => (0.5, 0.5),
            Anchor::TopLeft => (0.0, 0.0),
            Anchor::Top => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::Bottom => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }
}

/// A window's button on the panel button strip.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub visible: bool,
    pub enabled: bool,
    pub format: crate::numbers::Format,
    pub value: Option<f64>,
    pub image: Option<String>,
    pub sprite: String,
    pub frame: u32,
    pub tooltip: Option<crate::Text>,
}

/// A game widget at its place: `x` and `y` from the top left corner of its
/// window, or of its row for a game widget in a list's row.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub name: Option<String>,
    pub kind: Kind,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// Its tooltip's localisation key.
    pub tooltip: Option<crate::Text>,
    pub visible: bool,
}

/// What a game widget is, with what it shows at first.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Text {
        /// A localisation key.
        text: Option<String>,
        font: Font,
        align: Align,
    },
    Icon {
        sprite: String,
        frame: u32,
    },
    /// A text button when it has a text, an icon button when it has a
    /// sprite.
    Button {
        text: Option<String>,
        sprite: Option<String>,
        frame: u32,
    },
    /// A progress bar: an icon of one of the game's `progressbartype`
    /// sprites, which has its own size.
    Progress {
        sprite: String,
        value: f64,
    },
    /// A flag: a button of one of the game's `maskedShieldType` sprites.
    Flag {
        sprite: String,
        country: Option<String>,
    },
    List {
        row: Row,
        spacing: i32,
    },
}

impl Kind {
    pub fn word(&self) -> &'static str {
        match self {
            Kind::Text { .. } => "text",
            Kind::Icon { .. } => "icon",
            Kind::Button { .. } => "button",
            Kind::Progress { .. } => "progress",
            Kind::Flag { .. } => "flag",
            Kind::List { .. } => "list",
        }
    }
}

/// A list's row: its size and its game widgets, from its top left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub size: (i32, i32),
    pub widgets: Vec<Placed>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    /// The game's body text.
    Body,
    /// The game's headings.
    Heading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

impl Align {
    fn named(word: &str) -> Option<Self> {
        match word {
            "left" | "top" | "start" => Some(Align::Start),
            "center" => Some(Align::Center),
            "right" | "bottom" | "end" => Some(Align::End),
            _ => None,
        }
    }

    /// Where something `size` long stands in `room`.
    fn at(self, room: i32, size: i32) -> i32 {
        match self {
            Align::Start => 0,
            Align::Center => (room - size) / 2,
            Align::End => room - size,
        }
    }
}

impl Window {
    /// Its game widget of `name`, in the window itself, not in a list's
    /// row.
    pub fn widget(&self, name: &str) -> Option<&Placed> {
        self.widgets.iter().find(|w| w.name.as_deref() == Some(name))
    }
}

impl Row {
    pub fn widget(&self, name: &str) -> Option<&Placed> {
        self.widgets.iter().find(|w| w.name.as_deref() == Some(name))
    }
}

/// Reads the `windows` of an extension's `main.lua`, written as JSON: every
/// window laid out, in name order, or what is wrong with the first that is
/// not right. `has_text` says whether the mod has a localisation key.
pub fn read(windows: &Value, has_text: &dyn Fn(&str) -> bool) -> Result<Vec<Window>, String> {
    let Value::Object(windows) = windows else { return Err(format!("windows is a table of windows by name, not {}", kind_of(windows))) };
    let mut out = Vec::new();
    for (name, window) in windows {
        let at = format!("windows.{name}");
        out.push(read_window(name, window, has_text).map_err(|e| format!("{at}: {e}"))?);
    }
    Ok(out)
}

fn read_window(name: &str, value: &Value, has_text: &dyn Fn(&str) -> bool) -> Result<Window, String> {
    if !is_name(name) || RESERVED_WINDOWS.contains(&name) {
        return Err(format!("a window is named by lowercase letters, digits and _, other than {}", RESERVED_WINDOWS.join(" and ")));
    }
    let table = object(value, "a window")?;
    known(table, &["size", "title", "close", "anchor", "offset", "moveable", "panel", "ribbon", "entry", "items", "spacing", "align"])?;
    let size = pair(table.get("size"), "size")?.ok_or("a window has a size = { width, height }")?;
    if size.0 <= 0 || size.1 <= 0 {
        return Err("size is a width and a height above 0".into());
    }
    let title = key(table.get("title"), "title", has_text)?;
    let close = flag(table.get("close"), "close")?.unwrap_or(true);
    let anchor = match table.get("anchor") {
        None => Anchor::Center,
        Some(Value::String(word)) => Anchor::NAMES
            .iter()
            .find(|(n, _)| n == word)
            .map(|(_, a)| *a)
            .ok_or_else(|| format!("anchor is one of {}", Anchor::NAMES.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")))?,
        Some(other) => return Err(format!("anchor is a word, not {}", kind_of(other))),
    };
    let offset = pair(table.get("offset"), "offset")?.unwrap_or((0, 0));
    let moveable = flag(table.get("moveable"), "moveable")?.unwrap_or(true);
    let panel = flag(table.get("panel"), "panel")?.unwrap_or(true);
    let ribbon = match text(table.get("ribbon"), "ribbon")?.as_deref() {
        None | Some("red") => Ribbon::Red,
        Some("blue") => Ribbon::Blue,
        Some(_) => return Err("ribbon is red or blue".into()),
    };
    let entry = match table.get("entry") {
        None => None,
        Some(value) => {
            let entry = object(value, "entry")?;
            known(entry, &["image", "sprite", "frame", "tooltip", "visible", "enabled", "format", "decimals", "value"]).map_err(|e| format!("entry: {e}"))?;
            let image = text(entry.get("image"), "entry.image")?;
            let sprite = text(entry.get("sprite"), "entry.sprite")?;
            if image.is_some() == sprite.is_some() {
                return Err("entry has either image or sprite".into());
            }
            if image.is_some() && frame(entry.get("frame"), "entry.frame")? != 1 {
                return Err("entry image has one frame".into());
            }
            let format = crate::numbers::Format::read(entry.get("format"), entry.get("decimals"))?;
            let value =
                entry.get("value").map(|v| v.as_f64().filter(|v| format.accepts(*v)).ok_or("entry.value is a finite number in its format")).transpose()?;
            Some(Entry {
                visible: flag(entry.get("visible"), "entry.visible")?.unwrap_or(true),
                enabled: flag(entry.get("enabled"), "entry.enabled")?.unwrap_or(true),
                format,
                value,
                image,
                sprite: sprite.unwrap_or_default(),
                frame: frame(entry.get("frame"), "entry.frame")?,
                tooltip: entry.get("tooltip").map(|v| crate::Text::declaration(v, has_text)).transpose().map_err(|e| format!("entry.tooltip: {e}"))?,
            })
        }
    };
    let top = if title.is_some() { TITLE } else { MARGIN };
    let footer = if close { FOOTER_CLOSE } else { MARGIN };
    let room = (size.0 - 2 * MARGIN, size.1 - top - footer);
    let mut widgets = Vec::new();
    let content = Container::read(table, true)?;
    content.arrange((MARGIN, top), Some(room.0), has_text, &mut widgets, "")?;
    let mut names = HashSet::new();
    for w in &widgets {
        fits(w, (size.0, size.1 - footer), (0, 0), "the window, above its Close button")?;
        if let Some(name) = &w.name {
            if !names.insert(name.clone()) {
                return Err(format!("two game widgets are named {name}"));
            }
        }
    }
    Ok(Window { name: name.to_string(), size, title, close, anchor, offset, moveable, panel, ribbon, entry, widgets })
}

/// A column or a row of game widgets.
struct Container<'a> {
    across: bool,
    items: &'a [Value],
    spacing: i32,
    align: Align,
    size: Option<(i32, i32)>,
}

impl<'a> Container<'a> {
    /// The column a window or a row is, or a container game widget.
    fn read(table: &'a Map<String, Value>, column: bool) -> Result<Self, String> {
        let items = match table.get("items") {
            None => &[][..],
            Some(Value::Array(items)) => items.as_slice(),
            Some(Value::Object(o)) if o.is_empty() => &[][..],
            Some(other) => return Err(format!("items is a list of game widgets, not {}", kind_of(other))),
        };
        let spacing = number(table.get("spacing"), "spacing")?.unwrap_or(SPACING);
        let align = match table.get("align") {
            None => Align::Start,
            Some(Value::String(word)) => Align::named(word).ok_or("align is left, center or right (top, center or bottom in a row)")?,
            Some(other) => return Err(format!("align is a word, not {}", kind_of(other))),
        };
        Ok(Self { across: !column, items, spacing, align, size: None })
    }

    /// Lays its items out from `origin`, across a room `room` wide (high
    /// for a row) when it has one, into `out`; its size.
    fn arrange(&self, origin: (i32, i32), room: Option<i32>, has_text: &dyn Fn(&str) -> bool, out: &mut Vec<Placed>, at: &str) -> Result<(i32, i32), String> {
        let mut flowing = Vec::new();
        for (i, item) in self.items.iter().enumerate() {
            let here = format!("{at}items[{}]", i + 1);
            let table = object(item, "a game widget").map_err(|e| format!("{here}: {e}"))?;
            let placed_at = pair(table.get("at"), "at").map_err(|e| format!("{here}: {e}"))?;
            flowing.push((here, table, placed_at));
        }
        // Each item's size first, then its place.
        let mut sizes = Vec::new();
        for (here, table, _) in &flowing {
            let mut scratch = Vec::new();
            let size = item(table, (0, 0), has_text, &mut scratch, here)?;
            sizes.push(size);
        }
        let in_flow: Vec<usize> = (0..flowing.len()).filter(|&i| flowing[i].2.is_none()).collect();
        let along = |s: (i32, i32)| if self.across { s.0 } else { s.1 };
        let cross = |s: (i32, i32)| if self.across { s.1 } else { s.0 };
        let length: i32 = in_flow.iter().map(|&i| along(sizes[i])).sum::<i32>() + self.spacing * (in_flow.len() as i32 - 1).max(0);
        let width = in_flow.iter().map(|&i| cross(sizes[i])).max().unwrap_or(0);
        let room = self.size.map(cross).or(room).unwrap_or(width);
        let mut next = 0;
        for (i, (here, table, placed_at)) in flowing.iter().enumerate() {
            let place = match placed_at {
                Some(p) => *p,
                None => {
                    let offset = self.align.at(room, cross(sizes[i]));
                    let place = if self.across { (origin.0 + next, origin.1 + offset) } else { (origin.0 + offset, origin.1 + next) };
                    next += along(sizes[i]) + self.spacing;
                    place
                }
            };
            item(table, place, has_text, out, here)?;
        }
        let size = if self.across { (length, room) } else { (room, length) };
        Ok(self.size.unwrap_or(size))
    }
}

/// Lays out one game widget, or a container's items, at `place` into `out`;
/// its size.
fn item(table: &Map<String, Value>, place: (i32, i32), has_text: &dyn Fn(&str) -> bool, out: &mut Vec<Placed>, at: &str) -> Result<(i32, i32), String> {
    let fail = |e: String| format!("{at}: {e}");
    let kind = text(table.get("type"), "type")
        .map_err(fail)?
        .ok_or_else(|| fail("a game widget has a type: text, icon, button, progress, flag, list, column or row".into()))?;
    if kind == "column" || kind == "row" {
        known(table, &["type", "items", "spacing", "align", "at", "size"]).map_err(fail)?;
        let mut container = Container::read(table, kind == "column").map_err(fail)?;
        container.size = pair(table.get("size"), "size").map_err(fail)?;
        return container.arrange(place, None, has_text, out, &format!("{at}."));
    }
    let common = ["type", "name", "size", "at", "tooltip", "visible"];
    let only: &[&str] = match kind.as_str() {
        "text" => &["text", "font", "align"],
        "icon" => &["sprite", "frame"],
        "button" => &["text", "sprite", "frame"],
        "progress" => &["sprite", "value"],
        "flag" => &["sprite", "country"],
        "list" => &["row", "spacing"],
        other => return Err(fail(format!("{other:?} is no game widget's type: text, icon, button, progress, flag, list, column or row"))),
    };
    known(table, &[&common[..], only].concat()).map_err(fail)?;
    let name = match table.get("name") {
        None => None,
        Some(Value::String(n)) if is_name(n) && n != ENTRY => Some(n.clone()),
        Some(_) => return Err(fail(format!("a game widget is named by lowercase letters, digits and _, other than {ENTRY}"))),
    };
    // A progress bar or a flag of the default sprite has its size.
    let own_sprite = table.contains_key("sprite");
    let defaults = match kind.as_str() {
        "button" => Some((149, BUTTON_HEIGHT)),
        "progress" if !own_sprite => Some(PROGRESS_SIZE),
        "flag" if !own_sprite => Some(FLAG_SIZE),
        _ => None,
    };
    let mut size = pair(table.get("size"), "size").map_err(fail)?.or(defaults).ok_or_else(|| fail(format!("a {kind} has a size = {{ width, height }}")))?;
    if kind == "button" && table.get("text").is_some() {
        size = (button_width(size.0), BUTTON_HEIGHT);
    }
    if size.0 <= 0 || size.1 <= 0 {
        return Err(fail("size is a width and a height above 0".into()));
    }
    let tooltip = table.get("tooltip").map(|v| crate::Text::declaration(v, has_text)).transpose().map_err(fail)?;
    let visible = flag(table.get("visible"), "visible").map_err(fail)?.unwrap_or(true);
    let kind = match kind.as_str() {
        "text" => {
            let font = match text(table.get("font"), "font").map_err(fail)?.as_deref() {
                None | Some("body") => Font::Body,
                Some("heading") => Font::Heading,
                Some(_) => return Err(fail("font is body or heading".into())),
            };
            let align = match text(table.get("align"), "align").map_err(fail)? {
                None => Align::Start,
                Some(word) => Align::named(&word).ok_or_else(|| fail("align is left, center or right".into()))?,
            };
            Kind::Text { text: key(table.get("text"), "text", has_text).map_err(fail)?, font, align }
        }
        "icon" => Kind::Icon {
            sprite: text(table.get("sprite"), "sprite").map_err(fail)?.ok_or_else(|| fail("an icon has a sprite".into()))?,
            frame: frame(table.get("frame"), "frame").map_err(fail)?,
        },
        "button" => {
            let text_key = key(table.get("text"), "text", has_text).map_err(fail)?;
            let sprite = text(table.get("sprite"), "sprite").map_err(fail)?;
            if text_key.is_some() == sprite.is_some() {
                return Err(fail("a button has a text or a sprite, not both".into()));
            }
            Kind::Button { text: text_key, sprite, frame: frame(table.get("frame"), "frame").map_err(fail)? }
        }
        "progress" => {
            let value = match table.get("value") {
                None => 0.0,
                Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
                Some(other) => return Err(fail(format!("value is a number, not {}", kind_of(other)))),
            };
            let sprite = text(table.get("sprite"), "sprite").map_err(fail)?.unwrap_or_else(|| PROGRESS_SPRITE.to_string());
            Kind::Progress { sprite, value: value.clamp(0.0, 1.0) }
        }
        "flag" => Kind::Flag {
            sprite: text(table.get("sprite"), "sprite").map_err(fail)?.unwrap_or_else(|| FLAG_SPRITE.to_string()),
            country: text(table.get("country"), "country").map_err(fail)?,
        },
        _ => {
            let row = object(table.get("row").ok_or_else(|| fail("a list has a row = { size = { width, height }, items = { ... } }".into()))?, "row")
                .map_err(fail)?;
            known(row, &["size", "items", "spacing", "align"]).map_err(|e| fail(format!("row: {e}")))?;
            let row_size = pair(row.get("size"), "row.size").map_err(fail)?.ok_or_else(|| fail("a list's row has a size".into()))?;
            let mut widgets = Vec::new();
            Container::read(row, true).map_err(fail)?.arrange((0, 0), Some(row_size.0), has_text, &mut widgets, &format!("{at}.row."))?;
            let mut names = HashSet::new();
            for w in &widgets {
                fits(w, row_size, (0, 0), "its row").map_err(fail)?;
                if matches!(w.kind, Kind::List { .. }) {
                    return Err(fail("a list's row holds no list".into()));
                }
                if let Some(name) = &w.name {
                    if !names.insert(name.clone()) {
                        return Err(fail(format!("two game widgets of its row are named {name}")));
                    }
                }
            }
            if name.is_none() {
                return Err(fail("a list has a name, by which its rows are set".into()));
            }
            Kind::List { row: Row { size: row_size, widgets }, spacing: number(table.get("spacing"), "spacing").map_err(fail)?.unwrap_or(0) }
        }
    };
    out.push(Placed { name, kind, x: place.0, y: place.1, width: size.0, height: size.1, tooltip, visible });
    Ok(size)
}

/// The width of the game's text button for a button `width` wide.
pub fn button_width(width: i32) -> i32 {
    BUTTON_WIDTHS.iter().rev().copied().find(|&w| w <= width).unwrap_or(BUTTON_WIDTHS[0])
}

/// Whether `w` lies inside `size` from `origin`, or why not.
fn fits(w: &Placed, size: (i32, i32), origin: (i32, i32), inside: &str) -> Result<(), String> {
    if w.x < origin.0 || w.y < origin.1 || w.x + w.width > size.0 || w.y + w.height > size.1 {
        let name = w.name.clone().unwrap_or_else(|| format!("a {}", w.kind.word()));
        return Err(format!("{name} at ({}, {}), {} by {}, does not fit inside {inside}, {} by {}", w.x, w.y, w.width, w.height, size.0, size.1));
    }
    Ok(())
}

/// Whether `name` is a name a window or a game widget may have.
pub fn is_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "nil",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a table",
    }
}

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>, String> {
    match value {
        Value::Object(o) => Ok(o),
        // An empty table is written as an empty list.
        Value::Array(a) if a.is_empty() => {
            static EMPTY: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
            Ok(EMPTY.get_or_init(Map::new))
        }
        other => Err(format!("{what} is a table, not {}", kind_of(other))),
    }
}

fn known(table: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    match table.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(format!("{k} is not one of {}", allowed.join(", "))),
        None => Ok(()),
    }
}

fn pair(value: Option<&Value>, what: &str) -> Result<Option<(i32, i32)>, String> {
    match value {
        None => Ok(None),
        Some(Value::Array(a)) if a.len() == 2 => {
            let n = |v: &Value| v.as_i64().filter(|n| n.abs() <= 10_000).map(|n| n as i32);
            match (n(&a[0]), n(&a[1])) {
                (Some(x), Some(y)) => Ok(Some((x, y))),
                _ => Err(format!("{what} is two whole numbers")),
            }
        }
        Some(_) => Err(format!("{what} is {{ x, y }}, two whole numbers")),
    }
}

fn number(value: Option<&Value>, what: &str) -> Result<Option<i32>, String> {
    match value {
        None => Ok(None),
        Some(v) => v.as_i64().filter(|n| (0..=10_000).contains(n)).map(|n| Some(n as i32)).ok_or_else(|| format!("{what} is a whole number from 0")),
    }
}

fn frame(value: Option<&Value>, what: &str) -> Result<u32, String> {
    match value {
        None => Ok(1),
        Some(v) => v.as_u64().filter(|n| (1..=1000).contains(n)).map(|n| n as u32).ok_or_else(|| format!("{what} is a frame's number, from 1")),
    }
}

fn flag(value: Option<&Value>, what: &str) -> Result<Option<bool>, String> {
    match value {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(other) => Err(format!("{what} is true or false, not {}", kind_of(other))),
    }
}

fn text(value: Option<&Value>, what: &str) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
        Some(other) => Err(format!("{what} is a word, not {}", kind_of(other))),
    }
}

/// A localisation key the mod has.
fn key(value: Option<&Value>, what: &str, has_text: &dyn Fn(&str) -> bool) -> Result<Option<String>, String> {
    let key = text(value, what)?;
    match key {
        Some(k) if !has_text(&k) => Err(format!("{what}: the mod has no text {k:?} in its localisation")),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn every(_: &str) -> bool {
        true
    }

    #[test]
    fn a_column_lays_its_items_out_down_under_the_title() {
        let windows = json!({ "reforms": {
            "size": [400, 340], "title": "TITLE",
            "items": [
                { "type": "text", "name": "status", "size": [200, 20] },
                { "type": "row", "spacing": 10, "items": [
                    { "type": "button", "name": "yes", "text": "YES" },
                    { "type": "button", "name": "no", "text": "NO", "size": [100, 34] },
                ]},
                { "type": "icon", "name": "seal", "sprite": "GFX_seal", "size": [32, 32], "at": [350, 240] },
            ],
        }});
        let window = &read(&windows, &every).unwrap()[0];
        let at = |name: &str| window.widget(name).map(|w| (w.x, w.y, w.width, w.height)).unwrap();
        assert_eq!(at("status"), (MARGIN, TITLE, 200, 20));
        assert_eq!(at("yes"), (MARGIN, TITLE + 20 + SPACING, 149, 31), "the game's text button");
        assert_eq!(at("no"), (MARGIN + 159, TITLE + 20 + SPACING, 79, 31), "the widest that 100 has room for");
        assert_eq!(at("seal"), (350, 240, 32, 32), "at, from the window's corner");
        assert!(window.panel && window.moveable && window.close);
        assert_eq!((window.anchor, window.offset), (Anchor::Center, (0, 0)));
    }

    #[test]
    fn alignment_centers_in_the_room_there_is() {
        let windows = json!({ "w": { "size": [300, 200], "close": false, "align": "center", "items": [
            { "type": "text", "name": "a", "size": [100, 20] },
            { "type": "row", "align": "bottom", "items": [
                { "type": "icon", "name": "tall", "sprite": "S", "size": [20, 40] },
                { "type": "icon", "name": "short", "sprite": "S", "size": [20, 10] },
            ]},
        ]}});
        let window = &read(&windows, &every).unwrap()[0];
        assert_eq!(window.widget("a").map(|w| (w.x, w.y)), Some((MARGIN + (300 - 2 * MARGIN - 100) / 2, MARGIN)), "no title, no close: the margin");
        let row_x = MARGIN + (300 - 2 * MARGIN - (20 + SPACING + 20)) / 2;
        assert_eq!(window.widget("tall").map(|w| (w.x, w.y)), Some((row_x, MARGIN + 20 + SPACING)));
        assert_eq!(window.widget("short").map(|w| (w.x, w.y)), Some((row_x + 20 + SPACING, MARGIN + 20 + SPACING + 30)), "at the row's bottom");
    }

    #[test]
    fn a_list_lays_its_row_out_from_the_row_s_corner() {
        let windows = json!({ "electors": { "size": [400, 420], "items": [
            { "type": "list", "name": "countries", "size": [352, 300], "row": { "size": [340, 96], "items": [
                { "type": "row", "items": [
                    { "type": "flag", "name": "flag" },
                    { "type": "text", "name": "name", "size": [200, 20] },
                ]},
            ]}},
        ]}});
        let window = &read(&windows, &every).unwrap()[0];
        let Kind::List { row, .. } = &window.widget("countries").unwrap().kind else { panic!() };
        assert_eq!(row.widget("flag").map(|w| (w.x, w.y, w.width)), Some((0, 0, 92)));
        assert_eq!(row.widget("name").map(|w| (w.x, w.y)), Some((92 + SPACING, 0)));
    }

    #[test]
    fn what_is_wrong_is_said_with_where() {
        let wrong = |window: Value| read(&json!({ "w": window }), &|k| k != "MISSING").unwrap_err();
        assert!(wrong(json!({})).contains("has a size"));
        assert!(wrong(json!({ "size": [100, 100], "title": "MISSING" })).contains("no text \"MISSING\""));
        let too_wide = wrong(json!({ "size": [100, 100], "items": [{ "type": "text", "name": "t", "size": [90, 10] }] }));
        assert!(too_wide.starts_with("windows.w: t at (24, 24), 90 by 10, does not fit"), "{too_wide}");
        let twice = wrong(json!({ "size": [300, 300], "items": [
            { "type": "text", "name": "t", "size": [9, 9] }, { "type": "text", "name": "t", "size": [9, 9] },
        ]}));
        assert!(twice.contains("two game widgets are named t"), "{twice}");
        let unknown = wrong(json!({ "size": [300, 300], "items": [{ "type": "text", "name": "t", "size": [9, 9], "colour": 1 }] }));
        assert!(unknown.starts_with("windows.w: items[1]: colour is not one of"), "{unknown}");
        assert!(wrong(json!({ "size": [300, 300], "items": [{ "type": "button", "name": "b" }] })).contains("a text or a sprite"));
        assert!(wrong(json!({ "size": [300, 300], "items": [{ "type": "text", "name": "entry", "size": [9, 9] }] })).contains("other than entry"));
        assert!(read(&json!({ "open": { "size": [10, 10] } }), &every).unwrap_err().contains("other than open and close"));
        assert!(wrong(json!({ "size": [300, 300], "items": [{ "type": "list", "size": [9, 9], "row": { "size": [9, 9] } }] })).contains("a list has a name"));
    }
}
