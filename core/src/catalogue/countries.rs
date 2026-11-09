//! Countries. A remote client names a country by its tag; the capability
//! layer finds the country it stands for.
use eu4_harmony_remote::countries::args;
use eu4_harmony_remote::countries::{Budget, Country, CountryAnswer, CountryRequest};

layer! {
    run CountryRequest => CountryAnswer;
    Hostile => fn hostile -> bool = crate::game::countries::hostile;
    Get => fn country -> Box<Country> = crate::game::countries::get;
    Budget => fn budget -> Budget = crate::game::countries::budget;
}
