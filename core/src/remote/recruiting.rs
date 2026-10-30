//! Recruiting regiments and ships.
use super::provinces::layer_kind;
use crate::game::{self, provinces::UnitKind};
use eu4_harmony_remote::{
    recruiting::{RecruitAnswer, RecruitRequest},
    Error,
};

layer! {
    fn recruit(province: u32, kind: UnitKind) -> () = game::recruiting::recruit;
}

pub fn run(request: RecruitRequest, game: &mut (impl Layer + ?Sized)) -> Result<RecruitAnswer, Error> {
    match request {
        RecruitRequest::Recruit { province, kind } => Ok(RecruitAnswer::Recruit(game.recruit(province.0, layer_kind(kind))?)),
    }
}
