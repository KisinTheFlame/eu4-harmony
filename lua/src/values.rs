//! Values between Lua and JSON: an extension's save data, `json.encode` and
//! `json.decode`, a capability's arguments and its answer.
//!
//! JSON holds a table whose keys are all strings as an object and a sequence
//! as an array; integers stay 64-bit. A function, a userdata, a table with
//! keys of both kinds or with a cycle cannot be written. A date is a table
//! of `year`, `month` and `day` with the date metatable, which compares with
//! `<` and `==` and writes `1444.11.11` by `tostring`.
use mlua::{Lua, Table, Value as LuaValue};
use serde_json::{Map, Number, Value};

/// The deepest a table may nest to be written: deeper is taken as a cycle.
const DEEPEST: usize = 200;

/// The registry's name for the date metatable.
const DATE: &str = "eu4harmony.date";

/// A Lua value as JSON, or why it cannot be one.
pub fn to_json(value: &LuaValue) -> Result<Value, String> {
    json_at(value, 0)
}

fn json_at(value: &LuaValue, depth: usize) -> Result<Value, String> {
    if depth > DEEPEST {
        return Err(format!("tables nested more than {DEEPEST} deep, or a table that holds itself"));
    }
    Ok(match value {
        LuaValue::Nil => Value::Null,
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(i) => Value::Number((*i).into()),
        LuaValue::Number(n) => Number::from_f64(*n).map(Value::Number).ok_or_else(|| format!("{n} is not a number JSON can hold"))?,
        LuaValue::String(s) => Value::String(s.to_str().map_err(|_| "a string that is not UTF-8".to_string())?.to_string()),
        LuaValue::Table(t) => table_json(t, depth)?,
        other => return Err(format!("a {} cannot be written", other.type_name())),
    })
}

fn table_json(table: &Table, depth: usize) -> Result<Value, String> {
    let length = table.raw_len();
    let mut count = 0;
    let mut strings = Map::new();
    let mut integers = false;
    for pair in table.pairs::<LuaValue, LuaValue>() {
        let (key, value) = pair.map_err(|e| e.to_string())?;
        count += 1;
        match key {
            LuaValue::String(s) => {
                let key = s.to_str().map_err(|_| "a key that is not UTF-8".to_string())?.to_string();
                strings.insert(key, json_at(&value, depth + 1)?);
            }
            LuaValue::Integer(_) => integers = true,
            other => return Err(format!("a table with a {} key", other.type_name())),
        }
    }
    match (integers, strings.is_empty()) {
        (false, _) => Ok(Value::Object(strings)),
        (true, true) if count == length => {
            (1..=length).map(|i| json_at(&table.raw_get(i).map_err(|e| e.to_string())?, depth + 1)).collect::<Result<_, _>>().map(Value::Array)
        }
        (true, true) => Err("a table whose integer keys are not a sequence from 1".to_string()),
        (true, false) => Err("a table with both string and integer keys".to_string()),
    }
}

/// JSON as a Lua value; an object of `year`, `month` and `day` alone is a
/// date.
pub fn to_lua(lua: &Lua, value: &Value) -> mlua::Result<LuaValue> {
    Ok(match value {
        Value::Null => LuaValue::Nil,
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => LuaValue::Integer(i),
            None => LuaValue::Number(n.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(s) => LuaValue::String(lua.create_string(s)?),
        Value::Array(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for (i, item) in items.iter().enumerate() {
                table.raw_set(i + 1, to_lua(lua, item)?)?;
            }
            LuaValue::Table(table)
        }
        Value::Object(fields) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            for (key, field) in fields {
                table.raw_set(key.as_str(), to_lua(lua, field)?)?;
            }
            if fields.len() == 3 && ["year", "month", "day"].iter().all(|k| fields.get(*k).is_some_and(Value::is_i64)) {
                table.set_metatable(Some(date_metatable(lua)?))?;
            }
            LuaValue::Table(table)
        }
    })
}

/// The metatable every date shares: `<`, `<=` and `==` by year, month and
/// day; `tostring` as the game writes a date.
pub fn date_metatable(lua: &Lua) -> mlua::Result<Table> {
    if let Ok(table) = lua.named_registry_value::<Table>(DATE) {
        return Ok(table);
    }
    let key = |t: &Table| -> mlua::Result<(i64, i64, i64)> { Ok((t.raw_get("year")?, t.raw_get("month")?, t.raw_get("day")?)) };
    let meta = lua.create_table()?;
    meta.set("__lt", lua.create_function(move |_, (a, b): (Table, Table)| Ok(key(&a)? < key(&b)?))?)?;
    meta.set("__le", lua.create_function(move |_, (a, b): (Table, Table)| Ok(key(&a)? <= key(&b)?))?)?;
    meta.set("__eq", lua.create_function(move |_, (a, b): (Table, Table)| Ok(key(&a)? == key(&b)?))?)?;
    meta.set(
        "__tostring",
        lua.create_function(move |_, t: Table| {
            let (y, m, d) = key(&t)?;
            Ok(format!("{y}.{m}.{d}"))
        })?,
    )?;
    lua.set_named_registry_value(DATE, &meta)?;
    Ok(meta)
}

/// A date as JSON writes it, from a date table or the text itself.
pub fn date_text(value: &LuaValue) -> Option<String> {
    match value {
        LuaValue::String(s) => s.to_str().ok().map(|s| s.to_string()),
        LuaValue::Table(t) => {
            let (y, m, d): (i64, i64, i64) = (t.raw_get("year").ok()?, t.raw_get("month").ok()?, t.raw_get("day").ok()?);
            Some(format!("{y}.{m}.{d}"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn eval(lua: &Lua, code: &str) -> LuaValue {
        lua.load(code).eval().unwrap()
    }

    #[test]
    fn tables_are_objects_or_arrays_and_integers_stay_whole() {
        let lua = Lua::new();
        let value = eval(&lua, "return { name = 'x', ids = { 545460846646, 4611686018427387905 }, half = 0.5, empty = {} }");
        assert_eq!(to_json(&value).unwrap(), json!({"name": "x", "ids": [545460846646i64, 4611686018427387905i64], "half": 0.5, "empty": {}}));
        let back = to_lua(&lua, &to_json(&value).unwrap()).unwrap();
        assert_eq!(to_json(&back).unwrap(), to_json(&value).unwrap());
    }

    #[test]
    fn what_json_cannot_hold_is_refused_with_why() {
        let lua = Lua::new();
        for (code, says) in [
            ("return { f = print }", "function"),
            ("return { 1, 2, x = 3 }", "both string and integer"),
            ("return { [1] = 1, [3] = 3 }", "not a sequence"),
            ("local t = {} t.t = t return t", "holds itself"),
            ("return { [true] = 1 }", "boolean key"),
        ] {
            let error = to_json(&eval(&lua, code)).unwrap_err();
            assert!(error.contains(says), "{code}: {error}");
        }
    }

    #[test]
    fn a_date_compares_and_prints_as_the_game_writes_it() {
        let lua = Lua::new();
        lua.globals().set("a", to_lua(&lua, &json!({"year": 1444, "month": 11, "day": 11})).unwrap()).unwrap();
        lua.globals().set("b", to_lua(&lua, &json!({"year": 1444, "month": 2, "day": 1})).unwrap()).unwrap();
        lua.globals().set("c", to_lua(&lua, &json!({"year": 1444, "month": 11, "day": 11})).unwrap()).unwrap();
        let checks: (bool, bool, bool, String) = lua.load("return b < a, a == c, a <= c, tostring(a)").eval().unwrap();
        assert_eq!(checks, (true, true, true, "1444.11.11".to_string()));
        assert_eq!(date_text(&eval(&lua, "return a")).as_deref(), Some("1444.11.11"));
    }
}
