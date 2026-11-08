//! Pop-ups: the extensions' pop-ups open, and pressing one of their buttons
//! as the player does.
use crate::game;
use eu4_harmony_remote::popups::{args, Popup, PopupAnswer, PopupRequest};

layer! {
    run PopupRequest => PopupAnswer;
    List => fn popups -> Vec<Popup> = game::popups::list;
    Answer => fn answer_popup -> () = game::popups::answer;
}
