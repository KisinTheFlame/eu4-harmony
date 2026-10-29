//! Countries.
use crate::CountryTag;
use serde::{Deserialize, Serialize};

group! {
    /// Countries.
    "country": CountryRequest, CountryAnswer;

    /// Whether two countries are at war with each other, or either is a
    /// rebel faction.
    game Hostile "hostile" {
        /// One country.
        country: CountryTag,
        /// The other.
        other: CountryTag,
    } -> bool, "whether they are hostile";
    /// One read of a country: its treasury, manpower, stability, prestige,
    /// monarch points, the wars it takes part in, and its allies, subjects
    /// (each with its subject type), overlord and rivals.
    game Get "get" {
        /// The country.
        country: CountryTag,
    } -> Country, "the country: tag, treasury, manpower and max_manpower, stability, prestige, power (adm, dip, mil), wars (each its name, attackers and defenders, the leaders first) and relations (each a kind, ally, subject, overlord or rival, and a country; a subject or an overlord also its subject_type, the key the game or a mod gives the kind of subject, such as vassal, march, personal_union or tributary_state)";
}

/// A country at one moment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Country {
    pub tag: CountryTag,
    /// Ducats, to the thousandth.
    pub treasury: f64,
    /// Men.
    pub manpower: i32,
    pub max_manpower: i32,
    /// From -3 to 3.
    pub stability: f64,
    /// From -100 to 100.
    pub prestige: f64,
    /// Monarch points.
    pub power: Power,
    /// The wars it takes part in, on either side.
    pub wars: Vec<War>,
    /// Its allies, its subjects, its overlord and the countries it rivals,
    /// in that order.
    pub relations: Vec<Relation>,
}

/// Monarch points of each kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Power {
    pub adm: i32,
    pub dip: i32,
    pub mil: i32,
}

/// A war going on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct War {
    /// Its name, as the game shows it.
    pub name: String,
    /// The attackers, their leader first.
    pub attackers: Vec<CountryTag>,
    /// The defenders, their leader first.
    pub defenders: Vec<CountryTag>,
}

/// What another country is to this one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub kind: RelationKind,
    pub country: CountryTag,
    /// For a subject or an overlord, the kind of subject the subject is: the
    /// key `common/subject_types` gives it (`vassal`, `march`,
    /// `personal_union`, `tributary_state`, or one a mod adds). Left out for
    /// the other kinds.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subject_type: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// They are allied.
    Ally,
    /// The other is its subject, of the relation's `subject_type`.
    Subject,
    /// The other is its overlord, and it is the relation's `subject_type` to
    /// it.
    Overlord,
    /// It has chosen the other as a rival.
    Rival,
}
