//! Parliament: the issues on offer, debating one, and seats.
use eu4_harmony_remote::{
    parliament::{Parliament, ParliamentAnswer, ParliamentRequest},
    Error,
};

layer! {
    fn parliament() -> Parliament = crate::game::parliament::options;
    fn debate(issue: String) -> () = crate::game::parliament::debate;
    fn add_seat(province: u32) -> () = crate::game::parliament::add_seat;
    fn remove_seat(province: u32) -> () = crate::game::parliament::remove_seat;
}

pub fn run(request: ParliamentRequest, game: &mut (impl Layer + ?Sized)) -> Result<ParliamentAnswer, Error> {
    Ok(match request {
        ParliamentRequest::Options {} => ParliamentAnswer::Options(game.parliament()?),
        ParliamentRequest::Debate { issue } => ParliamentAnswer::Debate(game.debate(issue)?),
        ParliamentRequest::AddSeat { province } => ParliamentAnswer::AddSeat(game.add_seat(province.0)?),
        ParliamentRequest::RemoveSeat { province } => ParliamentAnswer::RemoveSeat(game.remove_seat(province.0)?),
    })
}
