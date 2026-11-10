//! Countries: one read of a country, as the player sees it at a glance, and
//! its budget, as the economy view shows it.
use crate::{CountryTag, Date, Words};
use serde::{Deserialize, Serialize};

group! {
    /// Countries.
    "country": CountryRequest, CountryAnswer;

    /// Whether two countries are at war with each other, or either is a
    /// rebel faction.
    world Hostile "hostile" {
        /// One country.
        country: CountryTag,
        /// The other.
        other: CountryTag,
    } -> bool, "whether they are hostile";
    /// One read of a country: its treasury, manpower, stability, prestige,
    /// monarch points, the wars it takes part in, its allies, subjects (each
    /// with its subject type), overlord and rivals, its technology, the idea
    /// groups it has taken, its governing capacity, inflation, mercantilism,
    /// loans and the rebel factions against it; its ruler, heir and
    /// consort; its legitimacy, corruption, absolutism, traditions and army
    /// professionalism; and the named modifiers its scripts put on it. In
    /// the player's view what the game's panels do not show of another
    /// country is null: its treasury, monarch points, governing capacity,
    /// inflation, mercantilism, loans, rebels, corruption, absolutism, army
    /// professionalism, what its legitimacy adds to its modifiers, and its
    /// named modifiers; and a named modifier its scripts hide is left out of
    /// the player's own.
    world Get "get" {
        /// The country.
        country: CountryTag,
    } -> Box<Country>, "the country: tag, treasury, manpower and max_manpower, stability, prestige, power (adm, dip, mil), wars (each its name, attackers and defenders, the leaders first), relations (each a kind, ally, subject, overlord or rival, and a country; a subject or an overlord also its subject_type, the key the game or a mod gives the kind of subject, such as vassal, march, personal_union or tributary_state), technology (adm, dip, mil), ideas (each an idea group's key, how many of its ideas are taken, and whether it is the national ideas), governing_capacity (used, max), inflation and mercantilism (percent), loans (count, ducats), rebels (each its kind, the rebel type's key, its name and progress in percent), ruler, heir and consort (each a name, dynasty, adm, dip, mil and age; null when there is none), legitimacy (kind, one of legitimacy, republican_tradition, devotion, horde_unity and meritocracy, value, max and effects, each a modifier's key and what it adds; null for a government with none), corruption, absolutism, army_tradition, navy_tradition, army_professionalism (percent) and named_modifiers (each its key and the day it ends, null for one that does not); in the player's view the fields another country's panels do not show are null";
    /// What a country has of each modifier asked for, by its key, as the
    /// game totals it from everything that gives it: its ideas, policies,
    /// government, named modifiers and the rest. Refused (not_found) for a
    /// key that names no modifier. In the player's view another country's
    /// are null, as no panel totals them.
    world Modifiers "modifiers" {
        /// The country.
        country: CountryTag,
        /// The modifiers' keys, such as `discipline` or
        /// `global_tax_modifier`.
        keys: Words,
    } -> Modifiers, "the country and its values, one for each key asked, in their order: modifier (its key) and value, as the game's scripts write it (0.05 for 5%); null in the player's view for another country";
    /// A country's budget as the economy view shows it: this month's income
    /// and expenses by category, and its loans. In the player's view another
    /// country's are null.
    world Budget "budget" {
        /// The country.
        country: CountryTag,
    } -> Budget, "the budget: country, income and expenses (each a category, the game's key for it such as INCOMETAX, its name as the view shows it, and ducats; null in the player's view for another country), and loans (each its lender, null for none, ducats, interest in percent and the date it falls due; null likewise)";
    /// What one country thinks of another, as the diplomacy view shows it:
    /// the opinion and its parts as its tooltip lists them, the aggressive
    /// expansion it holds against the other, its trust in the other and the
    /// favors it owes the other. In the player's view the trust and the
    /// favors are null unless one of the two is the country acted as.
    /// Refused for a country's opinion of itself, which the game does not
    /// show.
    world Opinion "opinion" {
        /// The country whose opinion it is.
        of: CountryTag,
        /// The country it is about.
        toward: CountryTag,
    } -> Opinion, "of, toward, opinion (from -200 to 200), parts (each a text, as the tooltip writes it in the player language, a value, and yearly, how much it changes a year, null for a part that does not), aggressive_expansion, trust (from 0 to 100) and favors; trust and favors null in the player's view unless of or toward is the country acted as";
    /// What every other country that owns a province thinks of one, as
    /// `country.opinion` answers each, without the parts.
    world Opinions "opinions" {
        /// The country they think of.
        toward: CountryTag,
    } -> Vec<OpinionOf>, "one entry for each other country that owns a province: country, opinion, aggressive_expansion, trust and favors, trust and favors null in the player's view unless toward is the country acted as";
}

/// What one country thinks of another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Opinion {
    /// The country whose opinion it is.
    pub of: CountryTag,
    /// The country it is about.
    pub toward: CountryTag,
    /// From -200 to 200, as the diplomacy view shows it.
    pub opinion: i32,
    /// What makes it up, as the opinion's tooltip lists it, in its order.
    /// Their sum may differ from `opinion`: the game counts the opinion it
    /// shows again only from time to time, and holds it between -200 and
    /// 200.
    pub parts: Vec<OpinionPart>,
    /// The aggressive expansion `of` holds against `toward`: what the
    /// opinion loses for it, as a positive number; 0 for none.
    pub aggressive_expansion: f64,
    /// `of`'s trust in `toward`, from 0 to 100; null in the player's view
    /// unless one of the two is the country acted as.
    pub trust: Option<i32>,
    /// The favors `of` owes `toward`; null as the trust.
    pub favors: Option<f64>,
}

/// One part of an opinion, as its tooltip lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct OpinionPart {
    /// What it is, as the tooltip writes it in the player language, such as
    /// "Opinion of same Religion"; a line the tooltip writes under it, such
    /// as the provinces a country covets, follows on a line of its own.
    pub text: String,
    /// What it adds to the opinion, negative for what it takes away.
    pub value: i32,
    /// How much it changes a year, as the tooltip shows it in brackets;
    /// null for a part that does not change with time.
    pub yearly: Option<f64>,
}

/// What one country thinks of the one asked about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct OpinionOf {
    /// The country whose opinion it is.
    pub country: CountryTag,
    /// As in [`Opinion`].
    pub opinion: i32,
    pub aggressive_expansion: f64,
    pub trust: Option<i32>,
    pub favors: Option<f64>,
}

/// A country at one moment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Country {
    pub tag: CountryTag,
    /// Ducats, to the thousandth; null in the player's view for another
    /// country.
    pub treasury: Option<f64>,
    /// Men.
    pub manpower: i32,
    pub max_manpower: i32,
    /// From -3 to 3.
    pub stability: f64,
    /// From -100 to 100.
    pub prestige: f64,
    /// Monarch points; null in the player's view for another country.
    pub power: Option<Power>,
    /// The wars it takes part in, on either side.
    pub wars: Vec<War>,
    /// Its allies, its subjects, its overlord and the countries it rivals,
    /// in that order.
    pub relations: Vec<Relation>,
    /// Its technology levels.
    pub technology: Technology,
    /// The idea groups it has taken, its national ideas first.
    pub ideas: Vec<IdeaGroup>,
    /// Null in the player's view for another country, as the next four.
    pub governing_capacity: Option<GoverningCapacity>,
    /// Percent.
    pub inflation: Option<f64>,
    /// Percent.
    pub mercantilism: Option<f64>,
    pub loans: Option<Loans>,
    /// The rebel factions against it, as the stability page lists them.
    pub rebels: Option<Vec<Rebels>>,
    /// Its ruler, heir and consort, as the court view shows them; null when
    /// it has none.
    pub ruler: Option<Person>,
    pub heir: Option<Person>,
    pub consort: Option<Person>,
    /// Its legitimacy, of the kind its government has; null for one with
    /// none.
    pub legitimacy: Option<Legitimacy>,
    /// From 0 to 100; null in the player's view for another country, as
    /// absolutism and army professionalism.
    pub corruption: Option<f64>,
    /// From 0 to its maximum, 100 unless modified.
    pub absolutism: Option<f64>,
    /// Percent.
    pub army_professionalism: Option<f64>,
    /// From 0 to 100.
    pub army_tradition: f64,
    pub navy_tradition: f64,
    /// The named modifiers its scripts put on it, such as an event's
    /// `country_modifier`; null in the player's view for another country,
    /// and one its scripts hide is left out of the player's own.
    pub named_modifiers: Option<Vec<NamedModifier>>,
}

/// A ruler, an heir or a consort.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Person {
    /// The given name, with a ruler's regnal number in it (`Louis XI`).
    pub name: String,
    /// The dynasty's name; null when it has none.
    pub dynasty: Option<String>,
    /// Administrative, diplomatic and military skill, 0 to 6 unless
    /// modified.
    pub adm: i32,
    pub dip: i32,
    pub mil: i32,
    /// In whole years.
    pub age: i32,
}

/// Technology levels of each kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Technology {
    pub adm: i32,
    pub dip: i32,
    pub mil: i32,
}

/// An idea group a country has taken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct IdeaGroup {
    /// Its content key, such as `quality_ideas`, or a national ideas' key
    /// such as `FRA_ideas`.
    pub group: String,
    /// How many of its ideas are taken.
    pub ideas: i32,
    /// It is the country's national ideas, which take no slot.
    pub national: bool,
}

/// Governing capacity, as the government view shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct GoverningCapacity {
    pub used: f64,
    pub max: f64,
}

/// What a country owes, summed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Loans {
    /// How many loans.
    pub count: i32,
    /// Ducats owed, all loans together.
    pub ducats: f64,
}

/// A rebel faction against a country.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Rebels {
    /// The key of its rebel type, such as `nationalist_rebels`.
    pub kind: String,
    /// Its name, as the game shows it.
    pub name: String,
    /// Its progress towards rising, in percent.
    pub progress: f64,
}

/// A country's budget.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Budget {
    pub country: CountryTag,
    /// This month's income by category, in the view's order; null in the
    /// player's view for another country, as the expenses and the loans.
    pub income: Option<Vec<Entry>>,
    pub expenses: Option<Vec<Entry>>,
    pub loans: Option<Vec<Loan>>,
}

/// One line of the budget.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Entry {
    /// The game's key for it, the key of its name in the localisation, such
    /// as `INCOMETAX` or `EXPENSEARMY`.
    pub category: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// Ducats this month.
    pub ducats: f64,
}

/// A loan a country has taken.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Loan {
    /// Who lent it; null for the bank.
    pub lender: Option<CountryTag>,
    /// Ducats.
    pub ducats: f64,
    /// Percent a year.
    pub interest: f64,
    /// When it falls due.
    pub due: Date,
}

/// Monarch points of each kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Power {
    pub adm: i32,
    pub dip: i32,
    pub mil: i32,
}

/// A war going on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct War {
    /// Its name, as the game shows it.
    pub name: String,
    /// The attackers, their leader first.
    pub attackers: Vec<CountryTag>,
    /// The defenders, their leader first.
    pub defenders: Vec<CountryTag>,
}

/// What another country is to this one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
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

/// A country's legitimacy, of the kind its government has.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Legitimacy {
    pub kind: LegitimacyKind,
    /// From 0 to `max`.
    pub value: f64,
    pub max: f64,
    /// What it adds to the country's modifiers now, as the court view's
    /// tooltip of it totals it; null in the player's view for another
    /// country.
    pub effects: Option<Vec<ModifierValue>>,
}

/// The kind of legitimacy a government has, by its key in the game's
/// scripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum LegitimacyKind {
    /// A monarchy's.
    Legitimacy,
    /// A republic's.
    RepublicanTradition,
    /// A theocracy's.
    Devotion,
    /// A horde's.
    HordeUnity,
    /// The Celestial Empire's government's, in place of legitimacy.
    Meritocracy,
}

/// What a country or a province has of a modifier, or what something adds
/// to it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ModifierValue {
    /// The modifier's key, such as `discipline`.
    pub modifier: String,
    /// As the game's scripts write it: 0.05 for 5%.
    pub value: f64,
}

/// What a country has of the modifiers asked for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Modifiers {
    pub country: CountryTag,
    /// One for each key asked, in their order; null in the player's view
    /// for another country.
    pub values: Option<Vec<ModifierValue>>,
}

/// A named modifier on a country or a province.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct NamedModifier {
    /// Its key, as the scripts that put it there name it.
    pub key: String,
    /// The day it ends; null for one that does not.
    pub expires: Option<Date>,
}
