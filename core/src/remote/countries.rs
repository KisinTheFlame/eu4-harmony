//! Countries. A remote client names a country by its tag; the capability
//! layer finds the country it stands for.
use super::{tag, Live};
use crate::game::{
    self,
    countries::{self as layer, RelationKind as Kind},
    Caller, GResult, Tag,
};
use eu4_harmony_remote::{
    countries::{Country, CountryAnswer, CountryRequest, Power, Relation, RelationKind, War},
    CountryTag, Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    /// The country with this tag.
    fn country(&mut self, tag: CountryTag) -> GResult<Tag>;
    fn hostile(&mut self, a: Tag, b: Tag) -> GResult<bool>;
    fn snapshot(&mut self, country: Tag) -> GResult<layer::Country>;
}

impl Layer for Live {
    fn country(&mut self, tag: CountryTag) -> GResult<Tag> {
        game::countries::find(Caller::Remote, tag.0)
    }

    fn hostile(&mut self, a: Tag, b: Tag) -> GResult<bool> {
        game::countries::hostile(Caller::Remote, a, b)
    }

    fn snapshot(&mut self, country: Tag) -> GResult<layer::Country> {
        game::countries::snapshot(Caller::Remote, country)
    }
}

pub fn run(request: CountryRequest, game: &mut (impl Layer + ?Sized)) -> Result<CountryAnswer, Error> {
    match request {
        CountryRequest::Hostile { country, other } => {
            let (a, b) = (game.country(country)?, game.country(other)?);
            Ok(CountryAnswer::Hostile(game.hostile(a, b)?))
        }
        CountryRequest::Get { country } => {
            let found = game.country(country)?;
            Ok(CountryAnswer::Get(snapshot(&game.snapshot(found)?)))
        }
    }
}

/// A fixed-point number of the game's, thousandths, as a number.
fn fixed(thousandths: i32) -> f64 {
    thousandths as f64 / 1000.0
}

fn snapshot(c: &layer::Country) -> Country {
    let tags = |tags: &[Tag]| tags.iter().copied().map(tag).collect();
    Country {
        tag: tag(c.tag),
        treasury: fixed(c.treasury),
        manpower: c.manpower,
        max_manpower: c.max_manpower,
        stability: fixed(c.stability),
        prestige: fixed(c.prestige),
        power: Power { adm: c.power[0], dip: c.power[1], mil: c.power[2] },
        wars: c.wars.iter().map(|w| War { name: w.name.clone(), attackers: tags(&w.attackers), defenders: tags(&w.defenders) }).collect(),
        relations: c
            .relations
            .iter()
            .map(|r| Relation {
                kind: match r.kind {
                    Kind::Ally => RelationKind::Ally,
                    Kind::Subject => RelationKind::Subject,
                    Kind::Overlord => RelationKind::Overlord,
                    Kind::Rival => RelationKind::Rival,
                },
                country: tag(r.country),
            })
            .collect(),
    }
}
