//! Countries. A remote client names a country by its tag; the capability
//! layer finds the country it stands for.
use eu4_harmony_remote::countries::args;
use eu4_harmony_remote::countries::{Budget, Country, CountryAnswer, CountryRequest, Modifiers, Opinion, OpinionOf};

layer! {
    run CountryRequest => CountryAnswer;
    Hostile => fn hostile -> bool = crate::game::countries::hostile;
    Get => fn country -> Box<Country> = crate::game::countries::get;
    GoverningCapacity => fn governing_capacity -> eu4_harmony_remote::countries::CurrentGoverningCapacity = crate::game::areas::governing_capacity;
    Modifiers => fn country_modifiers -> Modifiers = crate::game::countries::modifiers;
    Budget => fn budget -> Budget = crate::game::countries::budget;
    Opinion => fn opinion -> Opinion = crate::game::opinions::opinion;
    Opinions => fn opinions -> Vec<OpinionOf> = crate::game::opinions::opinions;
}
