//! TechnologyOption: what the technology view offers, and researching.
use eu4_harmony_remote::technology::{TechnologyAnswer, TechnologyOption, TechnologyRequest};
use eu4_harmony_remote::technology::args;

layer! {
    run TechnologyRequest => TechnologyAnswer;
    Options => fn technologies -> Vec<TechnologyOption> = crate::game::technology::options;
    Research => fn research -> () = crate::game::technology::research;
}
