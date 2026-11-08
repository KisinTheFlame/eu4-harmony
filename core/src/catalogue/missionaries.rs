//! Missionaries: the province view's missionary button, and the religion
//! view's automatic missionaries.
use eu4_harmony_remote::missionaries::{AutomaticMissionaries, MissionaryAnswer, MissionaryOptions, MissionaryRequest};
use eu4_harmony_remote::missionaries::args;

layer! {
    run MissionaryRequest => MissionaryAnswer;
    Options => fn missionary_options -> MissionaryOptions = crate::game::missionaries::options;
    Send => fn send_missionary -> () = crate::game::missionaries::send;
    Recall => fn recall_missionary -> () = crate::game::missionaries::recall;
    Automatic => fn automatic_missionaries -> AutomaticMissionaries = crate::game::missionaries::automatic;
    fn set_automatic_missionaries(raise: bool) -> () = session crate::game::missionaries::set_automatic;
    RaiseAutomatic {} => game.set_automatic_missionaries(true);
    LowerAutomatic {} => game.set_automatic_missionaries(false);
}
