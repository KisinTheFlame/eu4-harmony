//! Text shared by declarations and runtime properties. A localisation key
//! remains a key until presentation, so replacement and language changes do
//! not turn it into an implicit default or a stale translated string.
use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Text {
    Localized(Localized),
    Literal(Literal),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Localized {
    pub key: String,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Literal {
    pub text: String,
}

impl Default for Text {
    fn default() -> Self {
        String::new().into()
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Self::Literal(Literal { text })
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self {
        text.to_string().into()
    }
}

impl Text {
    pub fn key(key: String) -> Self {
        Self::Localized(Localized { key, values: BTreeMap::new() })
    }

    pub fn localisation_key(&self) -> Option<&str> {
        match self {
            Self::Localized(t) => Some(&t.key),
            Self::Literal(_) => None,
        }
    }

    pub fn resolve(&self, words: impl Fn(&str) -> Option<String>) -> String {
        match self {
            Self::Literal(t) => t.text.clone(),
            Self::Localized(t) => {
                let text = words(&t.key).unwrap_or_else(|| t.key.clone());
                let values: Vec<_> = t.values.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect();
                eu4_harmony_loc::fill(&text, &values)
            }
        }
    }

    /// A declaration's old bare key is normalized here only. New declarations
    /// and runtime assignments use the same explicit text description.
    pub fn declaration(value: &serde_json::Value, has_text: &dyn Fn(&str) -> bool) -> Result<Self, String> {
        let text = match value {
            // Internal migration: remove after the bundled mods use descriptions.
            serde_json::Value::String(key) => Self::key(key.clone()),
            _ => {
                let mut value = value.clone();
                if let Some(values) = value.get_mut("values").and_then(serde_json::Value::as_object_mut) {
                    for value in values.values_mut() {
                        if let serde_json::Value::Number(number) = value {
                            *value = serde_json::Value::String(number.to_string());
                        }
                    }
                }
                serde_json::from_value(value).map_err(|e| format!("text is {{ text = ... }} or {{ key = ..., values = {{ ... }} }}: {e}"))?
            }
        };
        text.check(has_text)?;
        Ok(text)
    }

    /// Internal declaration adapter: bare strings are keys here, while the
    /// same string assigned at runtime remains finished literal text.
    pub fn declaration_lua(value: LuaValue) -> mlua::Result<Self> {
        match value {
            LuaValue::String(key) => Ok(Self::key(key.to_str()?.to_string())),
            value => Self::from_lua(value),
        }
    }

    pub fn check(&self, has_text: &dyn Fn(&str) -> bool) -> Result<(), String> {
        if let Some(key) = self.localisation_key() {
            if key.is_empty() || !has_text(key) {
                return Err(format!("the mod has no text {key:?} in its localisation"));
            }
        }
        Ok(())
    }

    pub fn from_lua(value: LuaValue) -> mlua::Result<Self> {
        let scalar = |v: LuaValue| -> mlua::Result<String> {
            match v {
                LuaValue::String(s) => Ok(s.to_str()?.to_string()),
                LuaValue::Integer(n) => Ok(n.to_string()),
                LuaValue::Number(n) if n.is_finite() => Ok(n.to_string()),
                _ => Err(mlua::Error::runtime("a text value is a string or a finite number")),
            }
        };
        match value {
            LuaValue::Nil => Ok(Self::default()),
            // Internal migration: old hosted controls supply finished strings.
            LuaValue::String(s) => Ok(s.to_str()?.to_string().into()),
            LuaValue::Integer(n) => Ok(n.to_string().into()),
            LuaValue::Number(n) if n.is_finite() => Ok(n.to_string().into()),
            LuaValue::Table(table) => {
                for pair in table.clone().pairs::<String, LuaValue>() {
                    let (key, _) = pair?;
                    if !["text", "key", "values"].contains(&key.as_str()) {
                        return Err(mlua::Error::runtime(format!("text has no property {key:?}")));
                    }
                }
                let (text, key): (Option<String>, Option<String>) = (table.get("text")?, table.get("key")?);
                match (text, key) {
                    (Some(text), None) if matches!(table.get::<LuaValue>("values")?, LuaValue::Nil) => Ok(text.into()),
                    (None, Some(key)) if !key.is_empty() => {
                        let mut values = BTreeMap::new();
                        if let Some(named) = table.get::<Option<mlua::Table>>("values")? {
                            for pair in named.pairs::<String, LuaValue>() {
                                let (name, value) = pair?;
                                values.insert(name, scalar(value)?);
                            }
                        }
                        Ok(Self::Localized(Localized { key, values }))
                    }
                    _ => Err(mlua::Error::runtime("text has either text or key, with optional values for a key")),
                }
            }
            other => Err(mlua::Error::runtime(format!("text is {{ text = ... }} or {{ key = ..., values = {{ ... }} }}, not a {}", other.type_name()))),
        }
    }

    pub fn to_lua(&self, lua: &Lua) -> mlua::Result<LuaValue> {
        lua.to_value(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_description_resolves_after_replacement_and_language_changes() {
        let t = Text::declaration(&serde_json::json!({"key":"COUNT", "values":{"N":"0"}}), &|k| k == "COUNT").unwrap();
        assert_eq!(t.resolve(|_| Some("Count: §Y$N$§!\nReady".into())), "Count: §Y0§!\nReady");
        assert_eq!(t.resolve(|_| Some("数量：$N$".into())), "数量：0");
        assert_eq!(Text::default().resolve(|_| Some("Never restored".into())), "");
        assert!(Text::declaration(&serde_json::json!({"key":"MISSING"}), &|_| false).is_err());
        assert!(Text::declaration(&serde_json::json!({"text":"x", "key":"COUNT"}), &|_| true).is_err());
    }
}
