//! Countries.
use crate::CountryTag;

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
}
