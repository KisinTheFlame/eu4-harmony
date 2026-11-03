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

/// Puts a flattened object's fields among `fields`.
pub fn flatten(fields: &mut Vec<Field>, inner: Shape) {
    if let Shape::Object { fields: more, .. } = inner {
        fields.extend(more);
    }
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
        self.check_at(value, "")
    }

    fn check_at(&self, value: &Value, at: &str) -> Result<(), String> {
        let wrong = |what: &str| Err(format!("{}{what}, not {}", if at.is_empty() { String::new() } else { format!("{at}: ") }, kind(value)));
        match (self, value) {
            (Shape::Any, _) => Ok(()),
            (Shape::Optional(_), Value::Null) | (Shape::Nothing, Value::Null | Value::Bool(true)) => Ok(()),
            (Shape::Optional(inner), _) => inner.check_at(value, at),
            (Shape::Integer, Value::Number(n)) if n.is_i64() || n.is_u64() => Ok(()),
            (Shape::Integer, _) => wrong("an integer"),
            (Shape::Number, Value::Number(_)) => Ok(()),
            (Shape::Number, _) => wrong("a number"),
            (Shape::Boolean, Value::Bool(_)) => Ok(()),
            (Shape::Boolean, _) => wrong("true or false"),
            (Shape::Text, Value::String(_)) => Ok(()),
            (Shape::Text, _) => wrong("a string"),
            (Shape::Words(words), Value::String(s)) if words.contains(&s.as_str()) => Ok(()),
            (Shape::Words(words), _) => wrong(&format!("one of {}", words.join(", "))),
            (Shape::List(inner), Value::Array(items)) => items.iter().enumerate().try_for_each(|(i, item)| inner.check_at(item, &format!("{at}[{}]", i + 1))),
            (Shape::List(_), _) => wrong("a list"),
            (Shape::Map(inner), Value::Object(fields)) => fields.iter().try_for_each(|(k, v)| inner.check_at(v, &join(at, k))),
            (Shape::Map(_), _) => wrong("a table"),
            (Shape::Date, Value::Object(fields)) if ["year", "month", "day"].iter().all(|k| fields.get(*k).is_some_and(Value::is_i64)) && fields.len() == 3 => {
                Ok(())
            }
            (Shape::Date, _) => wrong("a date { year, month, day }"),
            (Shape::Object { class, fields, .. }, Value::Object(given)) => {
                for (name, value) in given {
                    match fields.iter().find(|f| f.name == name) {
                        Some(field) => field.shape.check_at(value, &join(at, name))?,
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
                    match shape.check_at(value, at) {
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
        let kinds = vec!["country", "province", "army", "date", "integer", "boolean", "text", "unit_kind", "allies", "provinces"];
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

    #[test]
    fn every_capability_has_a_result_shape() {
        for c in crate::capabilities() {
            assert!(crate::result_shape(&c.name()).is_some(), "{}", c.name());
        }
    }
}
