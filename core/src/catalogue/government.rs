//! Government: the player's reforms and rank, adopting reforms and raising
//! the rank.
use eu4_harmony_remote::{
    government::{Government, GovernmentAnswer, GovernmentRequest, ReformTier},
    Error,
};

layer! {
    fn government() -> Government = crate::game::government::options;
    fn reforms() -> Vec<ReformTier> = crate::game::government::reforms;
    fn adopt_reform(reform: String) -> () = crate::game::government::adopt_reform;
    fn raise_rank() -> () = crate::game::government::raise_rank;
}

pub fn run(request: GovernmentRequest, game: &mut (impl Layer + ?Sized)) -> Result<GovernmentAnswer, Error> {
    Ok(match request {
        GovernmentRequest::Options {} => GovernmentAnswer::Options(game.government()?),
        GovernmentRequest::Reforms {} => GovernmentAnswer::Reforms(game.reforms()?),
        GovernmentRequest::AdoptReform { reform } => GovernmentAnswer::AdoptReform(game.adopt_reform(reform)?),
        GovernmentRequest::RaiseRank {} => GovernmentAnswer::RaiseRank(game.raise_rank()?),
    })
}
