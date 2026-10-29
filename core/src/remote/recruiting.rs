//! Recruiting regiments and ships.
use super::{provinces::layer_kind, Live};
use crate::game::{self, provinces::UnitKind, Caller, GResult};
use eu4_harmony_remote::{
    recruiting::{RecruitAnswer, RecruitRequest},
    Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn recruit(&mut self, province: u32, kind: UnitKind) -> GResult<()>;
}

impl Layer for Live {
    fn recruit(&mut self, province: u32, kind: UnitKind) -> GResult<()> {
        game::recruiting::recruit(Caller::Remote, province, kind)
    }
}

pub fn run(request: RecruitRequest, game: &mut (impl Layer + ?Sized)) -> Result<RecruitAnswer, Error> {
    match request {
        RecruitRequest::Recruit { province, kind } => Ok(RecruitAnswer::Recruit(game.recruit(province.0, layer_kind(kind))?)),
    }
}
