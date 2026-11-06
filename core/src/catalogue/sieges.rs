//! Sieges: the siege view's buttons.
use eu4_harmony_remote::sieges::{SiegeAction, SiegeAnswer, SiegeRequest};

layer! {
    run SiegeRequest => SiegeAnswer;
    Options => fn siege_options(province: u32) -> Vec<SiegeAction> = caller crate::game::sieges::options;
    Bombard => fn bombard(province: u32) -> () = caller crate::game::sieges::bombard;
    NavalBombard => fn naval_bombard(province: u32) -> () = caller crate::game::sieges::naval_bombard;
    Assault => fn assault(province: u32) -> () = caller crate::game::sieges::assault;
}
