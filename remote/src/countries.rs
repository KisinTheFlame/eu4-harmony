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
    /// monarch points, the wars it takes part in, and its allies, subjects,
    /// overlord and rivals.
    game Get "get" {
        /// The country.
        country: CountryTag,
    } -> Country, "the country: tag, treasury, manpower and max_manpower, stability, prestige, power (adm, dip, mil), wars (each its name, attackers and defenders, the leaders first) and relations (each a kind, ally, subject, overlord or rival, and a country)";
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub kind: RelationKind,
    pub country: CountryTag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// They are allied.
    Ally,
    /// The other is its subject: a vassal, a march, a personal union, a
    /// tributary and the like.
    Subject,
    /// The other is its overlord.
    Overlord,
    /// It has chosen the other as a rival.
    Rival,
}
