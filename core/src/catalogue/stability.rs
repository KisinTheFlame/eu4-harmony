//! Stability: what the stability button offers, and raising it.
use eu4_harmony_remote::stability::{StabilityAnswer, StabilityOption, StabilityRequest};

layer! {
    run StabilityRequest => StabilityAnswer;
    Options => fn stability_options() -> Vec<StabilityOption> = session crate::game::stability::options;
    Raise => fn raise_stability() -> () = session crate::game::stability::raise;
}
