//! Technology: what the technology view offers, and researching.
use eu4_harmony_remote::{
    choices::PowerKind,
    technology::{Technology, TechnologyAnswer, TechnologyRequest},
};

layer! {
    run TechnologyRequest => TechnologyAnswer;
    Options => fn technologies() -> Vec<Technology> = crate::game::technology::options;
    Research => fn research(technology: PowerKind) -> () = crate::game::technology::research;
}
