//! Buildings: what the building panel offers in a province, and building,
//! cancelling and destroying one there.
use super::tag;
use crate::game::{self, buildings as layer};
use eu4_harmony_remote::{
    buildings::{BuildingAnswer, BuildingConstruction, BuildingOption, BuildingOptions, BuildingRequest},
    Date, Error,
};

layer! {
    fn building_options(province: u32) -> (Option<layer::Constructing>, Vec<layer::Offer>) = layer::options;
    fn build(province: u32, building: String) -> () = layer::build;
    fn cancel_building(province: u32) -> () = layer::cancel;
    fn destroy_building(province: u32, building: String) -> () = layer::destroy;
}

pub fn run(request: BuildingRequest, game: &mut (impl Layer + ?Sized)) -> Result<BuildingAnswer, Error> {
    Ok(match request {
        BuildingRequest::Options { province } => {
            let (constructing, offers) = game.building_options(province.0)?;
            BuildingAnswer::Options(BuildingOptions {
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
        BuildingRequest::Build { province, building } => BuildingAnswer::Build(game.build(province.0, building)?),
        BuildingRequest::Cancel { province } => BuildingAnswer::Cancel(game.cancel_building(province.0)?),
        BuildingRequest::Destroy { province, building } => BuildingAnswer::Destroy(game.destroy_building(province.0, building)?),
    })
}
