//! Stability: what the stability button offers, and raising it.
use eu4_harmony_remote::{
    stability::{StabilityAnswer, StabilityOption, StabilityRequest},
    Error,
};

layer! {
    fn stability_options() -> Vec<StabilityOption> = crate::game::stability::options;
    fn raise_stability() -> () = crate::game::stability::raise;
}

pub fn run(request: StabilityRequest, game: &mut (impl Layer + ?Sized)) -> Result<StabilityAnswer, Error> {
    Ok(match request {
        StabilityRequest::Options {} => StabilityAnswer::Options(game.stability_options()?),
        StabilityRequest::Raise {} => StabilityAnswer::Raise(game.raise_stability()?),
    })
}
