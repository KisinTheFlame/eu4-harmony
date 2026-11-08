//! Government: the player's reforms and rank, adopting reforms and raising
//! the rank.
use eu4_harmony_remote::government::{Government, GovernmentAnswer, GovernmentRequest, ReformTier};
use eu4_harmony_remote::government::args;

layer! {
    run GovernmentRequest => GovernmentAnswer;
    Options => fn government -> Government = crate::game::government::options;
    Reforms => fn reforms -> Vec<ReformTier> = crate::game::government::reforms;
    AdoptReform => fn adopt_reform -> () = crate::game::government::adopt_reform;
    RaiseRank => fn raise_rank -> () = crate::game::government::raise_rank;
}
