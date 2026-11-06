//! Buildings: what the building panel offers in a province, and building,
//! cancelling and destroying one there.
use super::tag;
use crate::game::{self, buildings as layer};
use eu4_harmony_remote::{
    buildings::{BuildingAnswer, BuildingConstruction, BuildingOption, BuildingOptions, BuildingRequest},
    Date, Error, ProvinceId,
};

layer! {
    run BuildingRequest => BuildingAnswer;
    fn building_options(province: u32) -> (Option<layer::Constructing>, Vec<layer::Offer>) = caller layer::options;
    Options { province } => options(game, province);
    Build => fn build(province: u32, building: String) -> () = caller layer::build;
    Cancel => fn cancel_building(province: u32) -> () = caller layer::cancel;
    Destroy => fn destroy_building(province: u32, building: String) -> () = caller layer::destroy;
}

/// What the building panel offers in the province, and what it builds now.
fn options(game: &mut (impl Layer + ?Sized), province: ProvinceId) -> Result<BuildingOptions, Error> {
    let (constructing, offers) = game.building_options(province.0)?;
    Ok(BuildingOptions {
        province,
        constructing: constructing.map(|c| BuildingConstruction {
            building: c.building,
            progress: game::thousandths(c.progress),
            finished: Date(c.ends),
            builder: tag(c.builder),
        }),
        options: offers.into_iter().map(|o| BuildingOption { action: o.action.into(), building: o.building, choice: o.choice }).collect(),
    })
}
