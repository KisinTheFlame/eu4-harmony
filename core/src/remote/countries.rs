//! Countries. A remote client names a country by its tag; the capability
//! layer finds the country it stands for.
use super::tag;
use crate::game::{
    self,
    countries::{self as layer, RelationKind as Kind},
    thousandths, Caller, GResult, Tag,
};
use eu4_harmony_remote::{
    countries::{Country, CountryAnswer, CountryRequest, Power, Relation, RelationKind, War},
    CountryTag, Error,
};

/// The country with this tag.
fn find(caller: Caller, tag: CountryTag) -> GResult<Tag> {
    game::countries::find(caller, tag.0)
}

layer! {
    /// The country with this tag.
    fn country(tag: CountryTag) -> Tag = find;
    fn hostile(a: Tag, b: Tag) -> bool = game::countries::hostile;
    fn snapshot(country: Tag) -> layer::Country = game::countries::snapshot;
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

fn snapshot(c: &layer::Country) -> Country {
    let tags = |tags: &[Tag]| tags.iter().copied().map(tag).collect();
    Country {
        tag: tag(c.tag),
        treasury: thousandths(c.treasury),
        manpower: c.manpower,
        max_manpower: c.max_manpower,
        stability: thousandths(c.stability),
        prestige: thousandths(c.prestige),
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
                subject_type: r.subject_type.clone(),
            })
            .collect(),
    }
}
