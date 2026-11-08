//! The shape of a capability's answer as Lua has it (`checks::form`): an
//! army's ID a number, a date an object of year, month and day. Read from
//! the answer's own type by `#[derive(Shaped)]`, so it is the answer's, for
//! the editor's type definitions and for holding a fake game's answers to it.
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub use eu4_harmony_remote_derive::Shaped;

/// A value's shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Integer,
    Number,
    Boolean,
    Text,
    /// One of these words.
    Words(Vec<&'static str>),
    /// The value, or nil.
    Optional(Box<Shape>),
    List(Box<Shape>),
    /// Texts to values.
    Map(Box<Shape>),
    /// An object with these fields, named after its type.
    Object {
        class: &'static str,
        about: &'static str,
        fields: Vec<Field>,
    },
    /// One of these.
    Either(Vec<Shape>),
    /// `{ year, month, day }`, which Lua compares and prints.
    Date,
    /// No result: `true` in Lua.
    Nothing,
    Any,
}

/// A field of an object.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: &'static str,
    pub about: &'static str,
    pub shape: Shape,
    /// It is left out when it has nothing to say.
    pub may_be_left_out: bool,
}

/// What has a shape.
pub trait Shaped {
    fn shape() -> Shape;
}

/// Puts a flattened object's fields among `fields`. A flattened enum, an
/// object of one of several shapes, is kept in its place as a field with no
/// name, which [`object`] makes into one object for each.
pub fn flatten(fields: &mut Vec<Field>, inner: Shape) {
    match inner {
        Shape::Object { fields: more, .. } => fields.extend(more),
        either @ Shape::Either(_) => fields.push(Field { name: "", about: "", shape: either, may_be_left_out: false }),
        _ => {}
    }
}

/// The object of a struct of `fields`: one object, or where it flattens an
/// enum (a field with no name, from [`flatten`]), one for each of the
/// enum's objects, its fields in the enum's place.
pub fn object(class: &'static str, about: &'static str, fields: Vec<Field>) -> Shape {
    let Some(at) = fields.iter().position(|f| f.name.is_empty()) else {
        return Shape::Object { class, about, fields };
    };
    let Shape::Either(shapes) = fields[at].shape.clone() else { unreachable!("flatten keeps only an Either unnamed") };
    let objects = shapes
        .into_iter()
        .map(|shape| {
            let mut these = fields[..at].to_vec();
            flatten(&mut these, shape);
            these.extend(fields[at + 1..].iter().cloned());
            object(class, about, these)
        })
        .collect();
    Shape::Either(objects)
}

/// A variant of an internally tagged enum: its object, with the tag first.
pub fn tagged(tag: &'static str, name: &'static str, class: &'static str, inner: Shape) -> Shape {
    let mut fields = vec![Field { name: tag, about: "", shape: Shape::Words(vec![name]), may_be_left_out: false }];
    flatten(&mut fields, inner);
    Shape::Object { class, about: "", fields }
}

macro_rules! shaped {
    ($($ty:ty => $shape:expr),* $(,)?) => {
        $( impl Shaped for $ty { fn shape() -> Shape { $shape } } )*
    };
}
shaped! {
    bool => Shape::Boolean,
    u8 => Shape::Integer, u16 => Shape::Integer, u32 => Shape::Integer, u64 => Shape::Integer,
    i8 => Shape::Integer, i16 => Shape::Integer, i32 => Shape::Integer, i64 => Shape::Integer, usize => Shape::Integer,
    f32 => Shape::Number, f64 => Shape::Number,
    String => Shape::Text, &str => Shape::Text,
    () => Shape::Nothing,
    Value => Shape::Any,
    // As Lua has them.
    crate::ArmyId => Shape::Integer,
    crate::ProvinceId => Shape::Integer,
    crate::CountryTag => Shape::Text,
    crate::Date => Shape::Date,
}

impl<T: Shaped> Shaped for Option<T> {
    fn shape() -> Shape {
        Shape::Optional(Box::new(T::shape()))
    }
}

impl<T: Shaped> Shaped for Vec<T> {
    fn shape() -> Shape {
        Shape::List(Box::new(T::shape()))
    }
}

impl<T: Shaped> Shaped for Box<T> {
    fn shape() -> Shape {
        T::shape()
    }
}

impl<K, T: Shaped> Shaped for BTreeMap<K, T> {
    fn shape() -> Shape {
        Shape::Map(Box::new(T::shape()))
    }
}

impl<K, T: Shaped, S> Shaped for HashMap<K, T, S> {
    fn shape() -> Shape {
        Shape::Map(Box::new(T::shape()))
    }
}

impl Shape {
    /// Whether `value` (JSON in Lua's form) has this shape, or why not; a
    /// field left out is allowed, a field the shape does not have is not.
    pub fn check(&self, value: &Value) -> Result<(), String> {
        self.check_at(value, "", Form::Lua)
    }

    /// Whether `value`, JSON as remote control answers it, has this shape,
    /// or why not: a date is its text, `"1444.11.11"`, and an integer may be
    /// a decimal text, as an army's ID is.
    pub fn check_json(&self, value: &Value) -> Result<(), String> {
        self.check_at(value, "", Form::Json)
    }

    fn check_at(&self, value: &Value, at: &str, form: Form) -> Result<(), String> {
        let wrong = |what: &str| Err(format!("{}{what}, not {}", if at.is_empty() { String::new() } else { format!("{at}: ") }, kind(value)));
        match (self, value) {
            (Shape::Any, _) => Ok(()),
            (Shape::Optional(_), Value::Null) | (Shape::Nothing, Value::Null | Value::Bool(true)) => Ok(()),
            (Shape::Optional(inner), _) => inner.check_at(value, at, form),
            (Shape::Integer, Value::Number(n)) if n.is_i64() || n.is_u64() => Ok(()),
            (Shape::Integer, Value::String(s)) if form == Form::Json && !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => Ok(()),
            (Shape::Integer, _) => wrong("an integer"),
            (Shape::Number, Value::Number(_)) => Ok(()),
            (Shape::Number, _) => wrong("a number"),
            (Shape::Boolean, Value::Bool(_)) => Ok(()),
            (Shape::Boolean, _) => wrong("true or false"),
            (Shape::Text, Value::String(_)) => Ok(()),
            (Shape::Text, _) => wrong("a string"),
            (Shape::Words(words), Value::String(s)) if words.contains(&s.as_str()) => Ok(()),
            (Shape::Words(words), _) => wrong(&format!("one of {}", words.join(", "))),
            (Shape::List(inner), Value::Array(items)) => {
                items.iter().enumerate().try_for_each(|(i, item)| inner.check_at(item, &format!("{at}[{}]", i + 1), form))
            }
            (Shape::List(_), _) => wrong("a list"),
            (Shape::Map(inner), Value::Object(fields)) => fields.iter().try_for_each(|(k, v)| inner.check_at(v, &join(at, k), form)),
            (Shape::Map(_), _) => wrong("a table"),
            (Shape::Date, Value::Object(fields)) if ["year", "month", "day"].iter().all(|k| fields.get(*k).is_some_and(Value::is_i64)) && fields.len() == 3 => {
                Ok(())
            }
            (Shape::Date, Value::String(s)) if form == Form::Json && crate::Date::parse(s).is_some() => Ok(()),
            (Shape::Date, _) => wrong("a date { year, month, day }"),
            (Shape::Object { class, fields, .. }, Value::Object(given)) => {
                for (name, value) in given {
                    match fields.iter().find(|f| f.name == name) {
                        Some(field) => field.shape.check_at(value, &join(at, name), form)?,
                        None => {
                            let names: Vec<&str> = fields.iter().map(|f| f.name).collect();
                            return Err(format!("{} is no field of eu4.{class}, which has {}", join(at, name), names.join(", ")));
                        }
                    }
                }
                Ok(())
            }
            (Shape::Object { class, .. }, _) => wrong(&format!("an eu4.{class}")),
            (Shape::Either(shapes), _) => {
                let mut why = Vec::new();
                for shape in shapes {
                    match shape.check_at(value, at, form) {
                        Ok(()) => return Ok(()),
                        Err(e) => why.push(e),
                    }
                }
                Err(why.join("; or "))
            }
            (Shape::Nothing, _) => wrong("nothing (true)"),
        }
    }
}

/// The form of the JSON a shape is checked against: Lua's, or remote
/// control's.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Lua,
    Json,
}

fn join(at: &str, name: &str) -> String {
    if at.is_empty() {
        name.to_string()
    } else {
        format!("{at}.{name}")
    }
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "nil",
        Value::Bool(_) => "a boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "an integer",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a table",
    }
}

/// Written by hand: its kind and its bytes in base64.
impl Shaped for crate::Binary {
    fn shape() -> Shape {
        let field = |name, about| Field { name, about, shape: Shape::Text, may_be_left_out: false };
        Shape::Object {
            class: "Binary",
            about: "A file's bytes.",
            fields: vec![field("media_type", "Its kind, such as image/png."), field("base64", "Its bytes in base64.")],
        }
    }
}

/// Written by hand, as `describe` answers it.
impl Shaped for crate::Capability {
    fn shape() -> Shape {
        let field = |name, shape| Field { name, about: "", shape, may_be_left_out: false };
        let kinds = crate::Kind::ALL.iter().map(|kind| kind.name()).collect();
        let parameter = Shape::Object {
            class: "Parameter",
            about: "",
            fields: vec![field("name", Shape::Text), field("kind", Shape::Words(kinds)), field("optional", Shape::Boolean), field("about", Shape::Text)],
        };
        Shape::Object {
            class: "Capability",
            about: "",
            fields: vec![
                field("capability", Shape::Text),
                field("about", Shape::Text),
                field("in_game", Shape::Boolean),
                field("country", Shape::Boolean),
                field("extensions", Shape::Boolean),
                field("args", Shape::List(Box::new(parameter))),
                field("result", Shape::Text),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn an_answer_may_leave_fields_out_but_not_add_or_mistype_them() {
        let army = crate::result_shape("army.get").unwrap();
        assert_eq!(army.check(&json!({"id": 545460846646i64, "kind": "army", "province": 151})), Ok(()));
        assert_eq!(army.check(&json!({"province": null, "route": [151, 152]})), Ok(()));
        let unknown = army.check(&json!({"regiment": 3})).unwrap_err();
        assert!(unknown.contains("regiment is no field of eu4.Army, which has id, owner, kind"), "{unknown}");
        assert_eq!(army.check(&json!({"province": "x"})), Err("province: an integer, not a string".into()));
        assert!(army.check(&json!({"kind": "fleet"})).unwrap_err().contains("kind: one of army, navy"));
        assert_eq!(army.check(&json!({"route": [151, "x"]})), Err("route[2]: an integer, not a string".into()));
        let session = crate::result_shape("session.get").unwrap();
        assert_eq!(session.check(&json!({"date": {"year": 1444, "month": 11, "day": 11}})), Ok(()));
        assert!(session.check(&json!({"date": "1444.11.11"})).unwrap_err().contains("a date { year, month, day }"));
        // No result: true, or nothing.
        let moved = crate::result_shape("army.move").unwrap();
        assert_eq!((moved.check(&json!(true)), moved.check(&json!(null))), (Ok(()), Ok(())));
    }

    /// Remote control writes a date as its text and an army's ID as a
    /// decimal text, which Lua has as a table and an integer.
    #[test]
    fn an_answer_in_remote_control_s_form_is_checked_as_remote_control_writes_it() {
        let session = crate::result_shape("session.get").unwrap();
        assert_eq!(session.check_json(&json!({"number": 1, "player": "FRA", "observing": false, "date": "1444.11.11"})), Ok(()));
        assert!(session.check_json(&json!({"date": "the eleventh"})).unwrap_err().contains("a date"));
        let army = crate::result_shape("army.get").unwrap();
        assert_eq!(army.check_json(&json!({"id": "545460846646", "province": 151})), Ok(()));
        assert_eq!(army.check_json(&json!({"id": "x"})), Err("id: an integer, not a string".into()));
        assert!(army.check(&json!({"id": "545460846646"})).is_err(), "Lua's form has the ID as an integer");
    }

    /// An event flattened into its number is one of the events' objects,
    /// each with the number beside its own fields (the sweep found the
    /// events' fields missing from the shape).
    #[test]
    fn a_flattened_enum_gives_an_object_for_each_of_its_variants() {
        let events = crate::result_shape("events").unwrap();
        let day = json!({"process": 1, "events": [{"number": 3, "event": "day", "date": "1444.11.12"}], "last": 3, "missed": false});
        assert_eq!(events.check_json(&day), Ok(()));
        let wrong = json!({"events": [{"number": 3, "event": "day", "no_such_field": 1}]});
        assert!(events.check_json(&wrong).is_err());
    }

    #[test]
    fn every_capability_has_a_result_shape() {
        for c in crate::capabilities() {
            assert!(crate::result_shape(&c.name()).is_some(), "{}", c.name());
        }
    }
}
