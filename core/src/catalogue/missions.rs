//! Missions: the player's missions, and completing one.
use eu4_harmony_remote::missions::{Mission, MissionAnswer, MissionRequest};

layer! {
    run MissionRequest => MissionAnswer;
    List => fn missions() -> Vec<Mission> = crate::game::missions::list;
    Complete => fn complete_mission(mission: String) -> () = crate::game::missions::complete;
}
