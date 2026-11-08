//! The national focus: what the focus buttons offer, choosing and cancelling.
use eu4_harmony_remote::focus::{Focus, FocusAnswer, FocusRequest};
use eu4_harmony_remote::focus::args;

layer! {
    run FocusRequest => FocusAnswer;
    Options => fn focus -> Focus = crate::game::focus::options;
    Choose => fn choose_focus -> () = crate::game::focus::choose;
    Cancel => fn cancel_focus -> () = crate::game::focus::cancel;
}
