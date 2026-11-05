//! Parliament: the issues on offer, debating one, and seats.
use eu4_harmony_remote::parliament::{Parliament, ParliamentAnswer, ParliamentRequest};

layer! {
    run ParliamentRequest => ParliamentAnswer;
    Options => fn parliament() -> Parliament = crate::game::parliament::options;
    Debate => fn debate(issue: String) -> () = crate::game::parliament::debate;
    AddSeat => fn add_seat(province: u32) -> () = crate::game::parliament::add_seat;
    RemoveSeat => fn remove_seat(province: u32) -> () = crate::game::parliament::remove_seat;
}
