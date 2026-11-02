//! Recruiting regiments and ships: what can be recruited in a province, and
//! ordering it.
use super::provinces::{kind, layer_kind};
use crate::game::{self, provinces::UnitKind};
use eu4_harmony_remote::{
    recruiting::{Recruit, RecruitAnswer, RecruitRequest},
    Error,
};

layer! {
    fn province_snapshot(province: u32) -> game::provinces::Snapshot = game::provinces::province;
    fn recruit(province: u32, kind: UnitKind) -> () = game::recruiting::recruit;
}

pub fn run(request: RecruitRequest, game: &mut (impl Layer + ?Sized)) -> Result<RecruitAnswer, Error> {
    match request {
        RecruitRequest::Options { province } => Ok(RecruitAnswer::Options(
            game.province_snapshot(province.0)?
                .recruit
                .into_iter()
                .map(|r| Recruit { kind: kind(r.kind), unit_type: r.unit_type, allowed: r.allowed, reasons: r.reasons })
                .collect(),
        )),
        RecruitRequest::Order { province, kind } => Ok(RecruitAnswer::Order(game.recruit(province.0, layer_kind(kind))?)),
    }
}
