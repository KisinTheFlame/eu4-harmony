//! TechnologyOption: what the technology view offers, and researching.
use eu4_harmony_remote::{
    choices::PowerKind,
    technology::{TechnologyAnswer, TechnologyOption, TechnologyRequest},
    Error,
};

layer! {
    fn technologies() -> Vec<TechnologyOption> = crate::game::technology::options;
    fn research(technology: PowerKind) -> () = crate::game::technology::research;
}

pub fn run(request: TechnologyRequest, game: &mut (impl Layer + ?Sized)) -> Result<TechnologyAnswer, Error> {
    Ok(match request {
        TechnologyRequest::Options {} => TechnologyAnswer::Options(game.technologies()?),
        TechnologyRequest::Research { technology } => TechnologyAnswer::Research(game.research(technology)?),
    })
}
