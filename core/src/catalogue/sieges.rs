//! Sieges: the siege view's buttons.
use eu4_harmony_remote::{
    sieges::{SiegeAction, SiegeAnswer, SiegeRequest},
    Error,
};

layer! {
    fn siege_options(province: u32) -> Vec<SiegeAction> = crate::game::sieges::options;
    fn bombard(province: u32) -> () = crate::game::sieges::bombard;
    fn naval_bombard(province: u32) -> () = crate::game::sieges::naval_bombard;
    fn assault(province: u32) -> () = crate::game::sieges::assault;
}

pub fn run(request: SiegeRequest, game: &mut (impl Layer + ?Sized)) -> Result<SiegeAnswer, Error> {
    Ok(match request {
        SiegeRequest::Options { province } => SiegeAnswer::Options(game.siege_options(province.0)?),
        SiegeRequest::Bombard { province } => SiegeAnswer::Bombard(game.bombard(province.0)?),
        SiegeRequest::NavalBombard { province } => SiegeAnswer::NavalBombard(game.naval_bombard(province.0)?),
        SiegeRequest::Assault { province } => SiegeAnswer::Assault(game.assault(province.0)?),
    })
}
