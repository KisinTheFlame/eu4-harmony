//! POPS without sleeping (docs/plans/game-speed.md).
//!
//! Paradox's client (`pops_api.dll`) is served twice a frame on the main
//! thread, and its event loop waits 1 ms for its sockets, sleeping that long
//! when it has none: 5 to 6.5% of the main thread at the fastest speed and a
//! third of a paused frame (docs/research/main-thread.md). With a timeout of 0
//! it polls.
use super::SpeedUp;
use crate::points::{Module, Place, Point};

pub const SPEED_UP: SpeedUp = SpeedUp { name: "pops-polling", group: "frames", points, checkable: false };

fn points() -> Vec<Point> {
    vec![
        // mov rcx, [rdi+0x30]; mov edx, 1; call the event loop (+0x1bf9f0):
        // the client's tick serves its loop with a timeout of 1 ms.
        Point::write(
            Place::In(Module::Pops, 0x6bdd1),
            "48 8b 4f 30 ba 01 00 00 00 e8 11 3c 15 00",
            "POPS client's event loop timeout",
            "48 8b 4f 30 ba 00 00 00 00 e8 11 3c 15 00",
        ),
    ]
}
