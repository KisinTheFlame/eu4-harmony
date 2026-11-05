//! Recruiting regiments and ships: what can be recruited in a province, and
//! ordering it.
use super::Convert;
use crate::game::{self, provinces::UnitKind};
use eu4_harmony_remote::{
    recruiting::{Recruit, RecruitAnswer, RecruitRequest},
    Error,
};

layer! {
    run RecruitRequest => RecruitAnswer;
    fn province_snapshot(province: u32) -> game::provinces::Snapshot = game::provinces::province;
    Options { province } => options(game, province.0);
    Order => fn recruit(province: u32, kind: UnitKind) -> () = game::recruiting::recruit;
}

/// What can be recruited in the province, and whether it can now.
fn options(game: &mut (impl Layer + ?Sized), province: u32) -> Result<Vec<Recruit>, Error> {
    Ok(game
        .province_snapshot(province)?
        .recruit
        .into_iter()
        .map(|r| Recruit { kind: r.kind.convert(), unit_type: r.unit_type, allowed: r.allowed, reasons: r.reasons })
        .collect())
}
