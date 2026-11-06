//! Parliament: the issues on offer, debating one, and seats.
use eu4_harmony_remote::parliament::{Parliament, ParliamentAnswer, ParliamentRequest};

layer! {
    run ParliamentRequest => ParliamentAnswer;
    Options => fn parliament() -> Parliament = session crate::game::parliament::options;
    Debate => fn debate(issue: String) -> () = session crate::game::parliament::debate;
    AddSeat => fn add_seat(province: u32) -> () = session crate::game::parliament::add_seat;
    RemoveSeat => fn remove_seat(province: u32) -> () = session crate::game::parliament::remove_seat;
}
