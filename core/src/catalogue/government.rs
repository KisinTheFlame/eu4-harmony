//! Government: the player's reforms and rank, adopting reforms and raising
//! the rank.
use eu4_harmony_remote::government::{Government, GovernmentAnswer, GovernmentRequest, ReformTier};

layer! {
    run GovernmentRequest => GovernmentAnswer;
    Options => fn government() -> Government = caller crate::game::government::options;
    Reforms => fn reforms() -> Vec<ReformTier> = caller crate::game::government::reforms;
    AdoptReform => fn adopt_reform(reform: String) -> () = caller crate::game::government::adopt_reform;
    RaiseRank => fn raise_rank() -> () = caller crate::game::government::raise_rank;
}
