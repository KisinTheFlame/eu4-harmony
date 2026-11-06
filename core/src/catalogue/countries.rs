//! Countries. A remote client names a country by its tag; the capability
//! layer finds the country it stands for.
use super::tag;
use crate::game::{
    self,
    countries::{self as layer, RelationKind as Kind},
    thousandths, Caller, GResult, Tag,
};
use eu4_harmony_remote::{
    countries::{Budget, Country, CountryAnswer, CountryRequest, Power, Relation, RelationKind, War},
    CountryTag, Error,
};

/// The country with this tag.
fn find(caller: Caller, tag: CountryTag) -> GResult<Tag> {
    game::countries::find(caller, tag.0)
}

layer! {
    run CountryRequest => CountryAnswer;
    /// The country with this tag.
    fn country(tag: CountryTag) -> Tag = caller find;
    fn hostile(a: Tag, b: Tag) -> bool = caller game::countries::hostile;
    fn snapshot(country: Tag) -> layer::Country = caller game::countries::snapshot;
    fn budget(country: Tag) -> Budget = caller game::countries::budget;
    Hostile { country, other } => hostile(game, country, other);
    Get { country } => get(game, country);
    Budget { country } => budget(game, country);
}

/// Whether the two countries are at war with each other, or either is a
/// rebel faction.
fn hostile(game: &mut (impl Layer + ?Sized), country: CountryTag, other: CountryTag) -> Result<bool, Error> {
    let (a, b) = (game.country(country)?, game.country(other)?);
    Ok(game.hostile(a, b)?)
}

fn get(game: &mut (impl Layer + ?Sized), country: CountryTag) -> Result<Box<Country>, Error> {
    let found = game.country(country)?;
    Ok(Box::new(snapshot(&game.snapshot(found)?)))
}

fn budget(game: &mut (impl Layer + ?Sized), country: CountryTag) -> Result<Budget, Error> {
    let found = game.country(country)?;
    Ok(game.budget(found)?)
}

fn snapshot(c: &layer::Country) -> Country {
    let tags = |tags: &[Tag]| tags.iter().copied().map(tag).collect();
    Country {
        tag: tag(c.tag),
        treasury: c.treasury.map(thousandths),
        manpower: c.manpower,
        max_manpower: c.max_manpower,
        stability: thousandths(c.stability),
        prestige: thousandths(c.prestige),
        power: c.power.map(|p| Power { adm: p[0], dip: p[1], mil: p[2] }),
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
        technology: c.technology,
        ideas: c.ideas.clone(),
        governing_capacity: c.own.as_ref().map(|o| o.governing_capacity),
        inflation: c.own.as_ref().map(|o| thousandths(o.inflation)),
        mercantilism: c.own.as_ref().map(|o| thousandths(o.mercantilism)),
        loans: c.own.as_ref().map(|o| o.loans),
        rebels: c.own.as_ref().map(|o| o.rebels.clone()),
    }
}
