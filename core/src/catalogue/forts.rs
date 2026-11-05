//! Forts: the player's forts, mothballing and reopening them.
use eu4_harmony_remote::{
    forts::{Fort, FortAnswer, FortRequest},
    Error,
};

layer! {
    fn forts() -> Vec<Fort> = crate::game::forts::list;
    fn mothball_fort(province: u32) -> () = crate::game::forts::mothball;
    fn reopen_fort(province: u32) -> () = crate::game::forts::reopen;
}

pub fn run(request: FortRequest, game: &mut (impl Layer + ?Sized)) -> Result<FortAnswer, Error> {
    Ok(match request {
        FortRequest::List {} => FortAnswer::List(game.forts()?),
        FortRequest::Mothball { province } => FortAnswer::Mothball(game.mothball_fort(province.0)?),
        FortRequest::Reopen { province } => FortAnswer::Reopen(game.reopen_fort(province.0)?),
    })
}
