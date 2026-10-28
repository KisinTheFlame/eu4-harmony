//! Provinces and their neighbors.
use super::{tag, Live};
use crate::game::{self, Caller, GResult};
use eu4_harmony_remote::{
    provinces::{Neighbor, Province, ProvinceAnswer, ProvinceRequest},
    Error, ProvinceId,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn provinces(&mut self) -> GResult<Vec<game::provinces::Province>>;
    fn neighbors(&mut self, province: u32) -> GResult<Vec<game::provinces::Neighbor>>;
}

impl Layer for Live {
    fn provinces(&mut self) -> GResult<Vec<game::provinces::Province>> {
        game::provinces::provinces(Caller::Remote)
    }

    fn neighbors(&mut self, province: u32) -> GResult<Vec<game::provinces::Neighbor>> {
        game::provinces::neighbors(Caller::Remote, province)
    }
}

pub fn run(request: ProvinceRequest, game: &mut (impl Layer + ?Sized)) -> Result<ProvinceAnswer, Error> {
    match request {
        // The layer's list starts with a placeholder for number 0.
        ProvinceRequest::List {} => Ok(ProvinceAnswer::List(
            game.provinces()?
                .iter()
                .filter(|p| p.id != 0)
                .map(|p| Province {
                    id: ProvinceId(p.id),
                    land: p.land,
                    lake: p.lake,
                    impassable: p.impassable,
                    visible: p.visible,
                    owner: p.owner.map(tag),
                    controller: p.controller.map(tag),
                })
                .collect(),
        )),
        ProvinceRequest::Neighbors { province } => {
            Ok(ProvinceAnswer::Neighbors(game.neighbors(province.0)?.iter().map(|n| Neighbor { province: ProvinceId(n.to), strait: n.strait }).collect()))
        }
    }
}
