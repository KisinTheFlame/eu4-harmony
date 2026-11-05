//! The national focus: what the focus buttons offer, choosing and cancelling.
use eu4_harmony_remote::{
    choices::PowerKind,
    focus::{Focus, FocusAnswer, FocusRequest},
    Error,
};

layer! {
    fn focus() -> Focus = crate::game::focus::options;
    fn choose_focus(focus: PowerKind) -> () = crate::game::focus::choose;
    fn cancel_focus() -> () = crate::game::focus::cancel;
}

pub fn run(request: FocusRequest, game: &mut (impl Layer + ?Sized)) -> Result<FocusAnswer, Error> {
    Ok(match request {
        FocusRequest::Options {} => FocusAnswer::Options(game.focus()?),
        FocusRequest::Choose { focus } => FocusAnswer::Choose(game.choose_focus(focus)?),
        FocusRequest::Cancel {} => FocusAnswer::Cancel(game.cancel_focus()?),
    })
}
