//! Countries. A remote client names a country by its tag; the capability
//! layer finds the country it stands for.
use super::Live;
use crate::game::{self, Caller, GResult, Tag};
use eu4_harmony_remote::{
    countries::{CountryAnswer, CountryRequest},
    CountryTag, Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    /// The country with this tag.
    fn country(&mut self, tag: CountryTag) -> GResult<Tag>;
    fn hostile(&mut self, a: Tag, b: Tag) -> GResult<bool>;
}

impl Layer for Live {
    fn country(&mut self, tag: CountryTag) -> GResult<Tag> {
        game::countries::find(Caller::Remote, tag.0)
    }

    fn hostile(&mut self, a: Tag, b: Tag) -> GResult<bool> {
        game::countries::hostile(Caller::Remote, a, b)
    }
}

pub fn run(request: CountryRequest, game: &mut (impl Layer + ?Sized)) -> Result<CountryAnswer, Error> {
    match request {
        CountryRequest::Hostile { country, other } => {
            let (a, b) = (game.country(country)?, game.country(other)?);
            Ok(CountryAnswer::Hostile(game.hostile(a, b)?))
        }
    }
}
