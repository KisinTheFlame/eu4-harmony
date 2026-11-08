//! Parliament: the issues on offer, debating one, and seats.
use eu4_harmony_remote::parliament::{Parliament, ParliamentAnswer, ParliamentRequest};
use eu4_harmony_remote::parliament::args;

layer! {
    run ParliamentRequest => ParliamentAnswer;
    Options => fn parliament -> Parliament = crate::game::parliament::options;
    Debate => fn debate -> () = crate::game::parliament::debate;
    AddSeat => fn add_seat -> () = crate::game::parliament::add_seat;
    RemoveSeat => fn remove_seat -> () = crate::game::parliament::remove_seat;
}
