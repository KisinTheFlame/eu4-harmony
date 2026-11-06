//! The national focus: what the focus buttons offer, choosing and cancelling.
use eu4_harmony_remote::{
    choices::PowerKind,
    focus::{Focus, FocusAnswer, FocusRequest},
};

layer! {
    run FocusRequest => FocusAnswer;
    Options => fn focus() -> Focus = session crate::game::focus::options;
    Choose => fn choose_focus(focus: PowerKind) -> () = session crate::game::focus::choose;
    Cancel => fn cancel_focus() -> () = session crate::game::focus::cancel;
}
