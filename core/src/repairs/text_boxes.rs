//! What a text box leaves as it goes, wherever the game lets one go: its
//! animation, which its element deletes only while it is on the element's
//! animations, and the text box takes off them while it does not play. It
//! is put back on them, owned, as the text box goes.
use crate::{
    hooks::Call,
    objects::gui::{classes as gui_classes, element, text_box},
    points::{GameFunction, Point},
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// `(text box)`: a text box's destructor, which its deleting destructor
/// (`0x152aab0`) and its subclass's (`0x15f2d20`) call first; it ends in the
/// element's (`0x1552680`), which deletes the animations the element owns
/// by their first slots. `(element, animation)`: puts an animation on an
/// element's animations, owned by it, or marks it owned there.
const TEXT_BOX_DESTRUCTOR: usize = 0x152aaf0;
const OWN_ANIMATION: GameFunction<2> = GameFunction::at(0x15529e0);

/// The text boxes' animations handed to their elements since the last world
/// reset, for the log.
static TEXT_BOX_ANIMATIONS: AtomicUsize = AtomicUsize::new(0);

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset), ..super::Repair::NONE };

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    format!("{} text boxes' animations were deleted with what owned them", TEXT_BOX_ANIMATIONS.swap(0, Ordering::Relaxed))
}

/// A text box's destructor begins: its animation, which its element deletes
/// only from its animations, is put on them owned, as the text box puts it
/// there when it plays; the element's destructor then deletes it with the
/// others, the text box still whole, as it deletes a playing one. The
/// destructor lets it go, so a developer build's quarantine keeps what the
/// whole destructor frees, until [`text_box_gone`].
fn text_box_going(call: &mut Call) {
    #[cfg(feature = "dev-heap-census")]
    crate::census::quarantine::begin("text box's animation");
    let m = call.memory();
    let boxed = call.arg(0);
    let owned = (|| {
        let animation = m.ptr(boxed + text_box::ANIMATION)?;
        if animation != 0 {
            OWN_ANIMATION.call(&m, [boxed, animation])?;
            TEXT_BOX_ANIMATIONS.fetch_add(1, Ordering::Relaxed);
        }
        crate::Result::Ok(())
    })();
    if let Err(e) = owned {
        log!("repair: a text box's animation was not handed to its element: {e}");
    }
}

/// A text box's destructor is done, its animation deleted with it.
#[cfg(feature = "dev-heap-census")]
fn text_box_gone(_: &mut Call) {
    crate::census::quarantine::end();
}

pub(super) fn points() -> Vec<Point> {
    vec![
        // A text box's destructor, which leaves its animation, and ends in
        // the element's; that deletes the animations the element owns, and
        // the text box makes its own with it.
        Point::observe(
            TEXT_BOX_DESTRUCTOR,
            "40 53 48 83 ec 20 48 8b d9 48 8d 05 10 52 86 00 48 89 01",
            "text box destructor",
            1,
            Some(text_box_going),
            #[cfg(feature = "dev-heap-census")]
            Some(text_box_gone),
            #[cfg(not(feature = "dev-heap-census"))]
            None,
        )
        .uses(&[gui_classes::TEXT_BOX]),
        Point::keep(0x152ab58usize, "48 8b cb 48 83 c4 20 5b e9 1b 7b 02 00", "text box destructor ends in the element's"),
        Point::keep(OWN_ANIMATION, "48 85 d2 0f 84 a9 00 00 00 53 48 83 ec 30 4c 8b 81 90 00 00 00", "own an animation").uses(&[element::ANIMATIONS]),
        Point::keep(0x1552a22usize, "49 8b 92 98 00 00 00 48 89 5c 24 20 c6 44 24 28 01", "animation put on owned"),
        Point::keep(0x155269ausize, "48 8d b9 90 00 00 00", "element destructor's animations").uses(&[element::ANIMATIONS]),
        Point::keep(
            0x15526c0usize,
            "48 8b 17 80 7c 13 08 00 74 16 48 8b 0c 13 48 85 c9 74 0d 48 8b 01 ba 01 00 00 00 ff 10",
            "element deletes its owned animations",
        )
        .uses(&[element::ANIMATION_OWNED]),
        Point::keep(0x152a036usize, "48 89 9e 10 02 00 00", "text box's animation made").uses(&[text_box::ANIMATION]),
    ]
}
