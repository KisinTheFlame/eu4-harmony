//! TechnologyOption: what the technology view offers, and researching.
use eu4_harmony_remote::{
    choices::PowerKind,
    technology::{TechnologyAnswer, TechnologyOption, TechnologyRequest},
};

layer! {
    run TechnologyRequest => TechnologyAnswer;
    Options => fn technologies() -> Vec<TechnologyOption> = caller crate::game::technology::options;
    Research => fn research(technology: PowerKind) -> () = caller crate::game::technology::research;
}
