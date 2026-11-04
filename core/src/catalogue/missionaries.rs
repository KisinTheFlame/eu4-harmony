//! Missionaries: the province view's missionary button, and the religion
//! view's automatic missionaries.
use eu4_harmony_remote::{
    missionaries::{AutomaticMissionaries, MissionaryAnswer, MissionaryOptions, MissionaryRequest},
    Error,
};

layer! {
    fn missionary_options(province: u32) -> MissionaryOptions = crate::game::missionaries::options;
    fn send_missionary(province: u32) -> () = crate::game::missionaries::send;
    fn recall_missionary(province: u32) -> () = crate::game::missionaries::recall;
    fn automatic_missionaries() -> AutomaticMissionaries = crate::game::missionaries::automatic;
    fn set_automatic_missionaries(raise: bool) -> () = crate::game::missionaries::set_automatic;
}

pub fn run(request: MissionaryRequest, game: &mut (impl Layer + ?Sized)) -> Result<MissionaryAnswer, Error> {
    Ok(match request {
        MissionaryRequest::Options { province } => MissionaryAnswer::Options(game.missionary_options(province.0)?),
        MissionaryRequest::Send { province } => MissionaryAnswer::Send(game.send_missionary(province.0)?),
        MissionaryRequest::Recall { province } => MissionaryAnswer::Recall(game.recall_missionary(province.0)?),
        MissionaryRequest::Automatic {} => MissionaryAnswer::Automatic(game.automatic_missionaries()?),
        MissionaryRequest::RaiseAutomatic {} => MissionaryAnswer::RaiseAutomatic(game.set_automatic_missionaries(true)?),
        MissionaryRequest::LowerAutomatic {} => MissionaryAnswer::LowerAutomatic(game.set_automatic_missionaries(false)?),
    })
}
