//! Occupation: handing a province the player occupies over to an ally.
use eu4_harmony_remote::occupation::args;
use eu4_harmony_remote::occupation::{OccupationAnswer, OccupationRequest, Transferred, Transfers};

layer! {
    run OccupationRequest => OccupationAnswer;
    Options => fn transfers -> Transfers = crate::game::occupation::options;
    Transfer => fn transfer_occupation -> Transferred = crate::game::occupation::transfer;
}
