//! Sieges: the siege view's buttons.
use eu4_harmony_remote::sieges::{SiegeAction, SiegeAnswer, SiegeRequest};
use eu4_harmony_remote::sieges::args;

layer! {
    run SiegeRequest => SiegeAnswer;
    Options => fn siege_options -> Vec<SiegeAction> = crate::game::sieges::options;
    Bombard => fn bombard -> () = crate::game::sieges::bombard;
    NavalBombard => fn naval_bombard -> () = crate::game::sieges::naval_bombard;
    Assault => fn assault -> () = crate::game::sieges::assault;
}
