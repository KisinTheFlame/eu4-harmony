//! What extensions hear, and in what order: a frame's dispatch to every
//! extension, the same for the running game and the fake game.
//!
//! At each of the game's safe frames, every extension, in its order:
//! 1. has its waiting tasks taken up whose answer came;
//! 2. hears what happened, in the order it happened ([`Heard`]), a session's
//!    start with the save data it began from;
//! 3. hears that its map coloring was taken over (`on_map_lost`);
//! 4. hears its buttons clicked (`on_button`);
//! 5. has its `on_frame` called.
//!
//! The host feeds a frame ([`Feed`]) with what only it knows: a session's
//! save data, the maps lost and the clicks, each asked for when its turn
//! comes, since a handler before may change it.
use crate::{Arg, Extension};
use eu4_harmony_remote::events::{Event, Kind, EVENTS};
use eu4_harmony_remote::shape::Shape;
use serde_json::{Map, Value};
use std::sync::Arc;

/// An event as extensions hear it: its kind and its fields in Lua's form
/// (`eu4_harmony_checks::form`).
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    kind: Kind,
    fields: Map<String, Value>,
}

impl Heard {
    /// `event`, as extensions hear it.
    pub fn of(event: &Event) -> Self {
        let written = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(event));
        let mut fields = match written {
            Ok(Value::Object(fields)) => fields,
            _ => Map::new(),
        };
        fields.remove("event");
        Self { kind: event.kind(), fields }
    }

    /// An event of `kind` with `fields`, a table in Lua's form, as a test
    /// raises it: held to the event's shape, every field it does not leave
    /// optional given.
    pub fn new(kind: Kind, fields: Value) -> Result<Self, String> {
        let shape = Shape::Object { class: kind.class(), about: "", fields: kind.fields() };
        let Value::Object(fields) = fields else { return Err(format!("the {kind} event's fields are a table, not {}", type_name(&fields))) };
        shape.check(&Value::Object(fields.clone())).map_err(|why| format!("the {kind} event: {why}"))?;
        let missing: Vec<&str> =
            kind.fields().into_iter().filter(|f| !matches!(f.shape, Shape::Optional(_)) && !fields.contains_key(f.name)).map(|f| f.name).collect();
        if !missing.is_empty() {
            return Err(format!("the {kind} event has no {}", missing.join(", ")));
        }
        Ok(Self { kind, fields })
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// One of its fields, in Lua's form.
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.fields.get(name)
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "nil",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a table",
    }
}

/// A click on one of an extension's buttons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Click {
    /// The index of the extension whose button it is.
    pub extension: usize,
    /// The button's name in its interface fragment.
    pub button: String,
    pub right: bool,
    pub shift: bool,
    pub ctrl: bool,
}

/// What the host gives a frame besides what happened.
pub trait Feed {
    /// The session `session` begins, before any extension hears it.
    fn session_begins(&mut self, session: u64);
    /// The save data of the mod `id`'s extension in the save the session
    /// began from; none for a new game or a save without it.
    fn saved(&mut self, id: &str) -> Option<Value>;
    /// The extensions, by index, whose map coloring was taken over since
    /// the last frame.
    fn map_lost(&mut self) -> Vec<usize>;
    /// The buttons clicked since the last frame, in order.
    fn clicks(&mut self) -> Vec<Click>;
}

/// Dispatches one of the game's safe frames to `extensions`, in the order
/// the module says.
pub fn frame(extensions: &[Arc<Extension>], heard: &[Heard], feed: &mut dyn Feed) {
    for e in extensions {
        e.poll();
    }
    for heard in heard {
        hear(extensions, heard, feed);
    }
    for lost in feed.map_lost() {
        map_lost(extensions, lost);
    }
    for click in feed.clicks() {
        self::click(extensions, &click);
    }
    for e in extensions {
        e.handle("on_frame", &[]);
    }
}

/// Every extension hears `heard`. A session's start gives each the save
/// data the session began from, and begins its session's times; a session's
/// end drops its waiting tasks after its handler.
pub fn hear(extensions: &[Arc<Extension>], heard: &Heard, feed: &mut dyn Feed) {
    match heard.kind {
        Kind::Session => {
            let session = heard.field("session").and_then(Value::as_u64).unwrap_or_default();
            feed.session_begins(session);
            for e in extensions {
                e.take_session_times();
            }
            for e in extensions {
                let saved = feed.saved(e.id()).unwrap_or(Value::Null);
                e.handle("on_session", &[Arg::Json(saved)]);
            }
        }
        kind => {
            let Some(args) = eu4_harmony_remote::lua::handler_args(kind, &heard.fields) else { return };
            let args: Vec<Arg> = args.into_iter().map(Arg::Json).collect();
            for e in extensions {
                e.handle(kind.handler(), &args);
                if kind == Kind::SessionEnded {
                    e.session_over();
                }
            }
        }
    }
}

/// The extension with index `extension` hears its map coloring was taken
/// over.
pub fn map_lost(extensions: &[Arc<Extension>], extension: usize) {
    if let Some(e) = extensions.iter().find(|e| e.index() == extension) {
        e.handle("on_map_lost", &[]);
    }
}

/// The extension whose button was clicked hears it.
pub fn click(extensions: &[Arc<Extension>], click: &Click) {
    if let Some(e) = extensions.iter().find(|e| e.index() == click.extension) {
        let how = Arg::Click { right: click.right, shift: click.shift, ctrl: click.ctrl };
        e.handle("on_button", &[Arg::Text(click.button.clone()), how]);
    }
}

/// The kinds of event some extension has a handler for.
pub fn listened(extensions: &[Arc<Extension>]) -> Vec<Kind> {
    EVENTS.iter().map(|(kind, _)| *kind).filter(|kind| extensions.iter().any(|e| e.has_handler(kind.handler()))).collect()
}
