//! What the game's and the mods' scripts share with extensions: their flags
//! and variables, and firing their events. Each of these does or reads what
//! no player could, so a remote client asks for it only as the game (ADR
//! requests-act-as-the-player-or-as-the-game); extensions always may.
//!
//! A flag is a name set on a scope, with the day it was set, as
//! `set_country_flag` and its kin set it; a variable a number kept under a
//! name in a country or a province, as `set_variable` sets it; an event one
//! of the scripts' `events/`, fired by its id as `country_event` or
//! `province_event` fires it.
use crate::{CountryTag, Date, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The scripts' flags, variables and events.
    "script": ScriptRequest, ScriptAnswer;

    /// Whether a flag is set on a scope, as the scripts' `has_country_flag`
    /// and its kin ask, and the day it was set, as `had_country_flag` reads
    /// it. A country, a province, a ruler, an heir, a consort or the whole
    /// game has flags; an heir or a consort a country has not has none.
    as_game Flag "flag" {
        /// Whose flag: `{country = "FRA"}`, `{province = 183}`, `{ruler =
        /// "FRA"}`, `{heir = "FRA"}`, `{consort = "FRA"}` or `{global =
        /// true}`.
        scope: Scope,
        /// The flag's name, as the scripts write it.
        name: String,
    } -> FlagState, "set, whether it is set, and day, the day it was set (null when it is not)";

    /// Sets a flag on a scope, as the scripts' `set_country_flag` and its
    /// kin do, on today's date; a flag set before is set again today.
    /// Refused when the ruler, heir or consort named is not there.
    as_game SetFlag "set_flag" {
        /// Whose flag, as `script.flag` takes it.
        scope: Scope,
        /// The flag's name, as the scripts write it.
        name: String,
    } -> (), "null once the flag is set";

    /// Clears a flag of a scope, as the scripts' `clr_country_flag` and its
    /// kin do; nothing happens when it is not set.
    as_game ClearFlag "clear_flag" {
        /// Whose flag, as `script.flag` takes it.
        scope: Scope,
        /// The flag's name, as the scripts write it.
        name: String,
    } -> (), "null once the flag is not set";

    /// A variable's value, as the scripts' `check_variable` reads it, 0 when
    /// the scope has none of that name, and whether it has one. A country or
    /// a province has variables.
    as_game Variable "variable" {
        /// Whose variable: `{country = "FRA"}` or `{province = 183}`.
        scope: Scope,
        /// The variable's name, as the scripts write it.
        name: String,
    } -> VariableState, "set, whether the scope has the variable, and value, its value to three decimal places (0 when it has none)";

    /// Sets a variable, as the scripts' `set_variable` does, making it when
    /// the scope has none of that name. The game keeps three decimal places.
    as_game SetVariable "set_variable" {
        /// Whose variable: `{country = "FRA"}` or `{province = 183}`.
        scope: Scope,
        /// The variable's name, as the scripts write it.
        name: String,
        /// Its new value, from -2000000 to 2000000.
        value: f64,
    } -> (), "null once the variable is set";

    /// Fires an event at once, as the scripts' `country_event` and
    /// `province_event` do: for a country, or for a province and its owner.
    /// The game asks the event's trigger first and fires it only when it
    /// holds; the AI picks an option of its own countries' events, and the
    /// player's waits as a question. Refused when the event's trigger does
    /// not hold, `not_found` for an id no event has, and for a province no
    /// one owns.
    as_game FireEvent "fire_event" {
        /// Whose event: `{country = "FRA"}` or `{province = 183}`.
        scope: Scope,
        /// The event's id, as its file writes it, such as `flavor_fra.1`.
        id: String,
    } -> (), "null once the event is fired";
}

/// A flag of a scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct FlagState {
    /// Whether it is set.
    pub set: bool,
    /// The day it was set; null when it is not set.
    pub day: Option<Date>,
}

/// A variable of a scope.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct VariableState {
    /// Whether the scope has it.
    pub set: bool,
    /// Its value, to three decimal places; 0 when the scope has none.
    pub value: f64,
}

/// Whose flag or variable, or whose event: a table of one key, which names
/// the kind of scope and holds what names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// A country, by its tag.
    Country(CountryTag),
    /// A province, by its number.
    Province(ProvinceId),
    /// A country's ruler.
    Ruler(CountryTag),
    /// A country's heir.
    Heir(CountryTag),
    /// A country's consort.
    Consort(CountryTag),
    /// The whole game: `true`.
    Global(bool),
}

impl Scope {
    /// A scope as a command line writes it: `global`, or a kind and what
    /// names it joined with a colon, such as `country:FRA` or
    /// `province:183`; or as JSON, `{"country":"FRA"}`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.starts_with('{') {
            return serde_json::from_str(text).map_err(|e| format!("{text:?} is no scope: {e}"));
        }
        if text == "global" {
            return Ok(Self::Global(true));
        }
        let (kind, name) = text.split_once(':').ok_or_else(|| format!("{text:?} is no scope: write global, or a kind and a tag or number, such as country:FRA"))?;
        let tag = || CountryTag::parse(name).ok_or_else(|| format!("{name:?} is no country tag"));
        Ok(match kind {
            "country" => Self::Country(tag()?),
            "ruler" => Self::Ruler(tag()?),
            "heir" => Self::Heir(tag()?),
            "consort" => Self::Consort(tag()?),
            "province" => Self::Province(ProvinceId(name.parse().map_err(|_| format!("{name:?} is no province number"))?)),
            _ => return Err(format!("{kind:?} is no kind of scope: country, province, ruler, heir, consort or global")),
        })
    }

    /// Its kind, as its key names it.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Country(_) => "country",
            Self::Province(_) => "province",
            Self::Ruler(_) => "ruler",
            Self::Heir(_) => "heir",
            Self::Consort(_) => "consort",
            Self::Global(_) => "global",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_is_a_table_of_one_key() {
        let scope: Scope = serde_json::from_str(r#"{"country":"FRA"}"#).unwrap();
        assert_eq!(scope, Scope::Country(CountryTag(*b"FRA")));
        assert_eq!(serde_json::to_string(&Scope::Province(ProvinceId(183))).unwrap(), r#"{"province":183}"#);
        assert_eq!(serde_json::to_string(&Scope::Global(true)).unwrap(), r#"{"global":true}"#);
        assert!(serde_json::from_str::<Scope>(r#"{"country":"FRA","province":1}"#).is_err());
    }

    #[test]
    fn a_command_line_writes_a_scope_with_a_colon() {
        assert_eq!(Scope::parse("country:FRA"), Ok(Scope::Country(CountryTag(*b"FRA"))));
        assert_eq!(Scope::parse("province:183"), Ok(Scope::Province(ProvinceId(183))));
        assert_eq!(Scope::parse("heir:ENG"), Ok(Scope::Heir(CountryTag(*b"ENG"))));
        assert_eq!(Scope::parse("global"), Ok(Scope::Global(true)));
        assert_eq!(Scope::parse(r#"{"consort":"CAS"}"#), Ok(Scope::Consort(CountryTag(*b"CAS"))));
        assert!(Scope::parse("country").is_err());
        assert!(Scope::parse("army:1").is_err());
        assert!(Scope::parse("province:x").is_err());
    }
}
