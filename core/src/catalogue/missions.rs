//! Missions: the player's missions, and completing one.
use eu4_harmony_remote::{
    missions::{Mission, MissionAnswer, MissionRequest},
    Error,
};

layer! {
    fn missions() -> Vec<Mission> = crate::game::missions::list;
    fn complete_mission(mission: String) -> () = crate::game::missions::complete;
}

pub fn run(request: MissionRequest, game: &mut (impl Layer + ?Sized)) -> Result<MissionAnswer, Error> {
    Ok(match request {
        MissionRequest::List {} => MissionAnswer::List(game.missions()?),
        MissionRequest::Complete { mission } => MissionAnswer::Complete(game.complete_mission(mission)?),
    })
}
