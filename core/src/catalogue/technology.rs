//! TechnologyOption: what the technology view offers, and researching.
use eu4_harmony_remote::{
    choices::PowerKind,
    technology::{TechnologyAnswer, TechnologyOption, TechnologyRequest},
};

layer! {
    run TechnologyRequest => TechnologyAnswer;
    Options => fn technologies() -> Vec<TechnologyOption> = crate::game::technology::options;
    Research => fn research(technology: PowerKind) -> () = crate::game::technology::research;
}
