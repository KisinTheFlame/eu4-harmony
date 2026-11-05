//! What working out the events of the game costs while someone listens to
//! them (docs/player-actions.md, "Events"; docs/performance.md): the wars and
//! the provinces' owners watched at each new day, the buildings finished and
//! the battles heard, and turning what was heard into remote control's
//! events. Logged when a session ends, then counted afresh.
use crate::unicode::costs::Cost;
use std::time::Duration;

/// The wars read and compared with the day before, at a new day.
pub static WARS: Cost = Cost::new();
/// The provinces' owners read and compared with the day before, at a new
/// day.
pub static OWNERS: Cost = Cost::new();
/// A building finished, read as the game finishes it.
pub static BUILDINGS: Cost = Cost::new();
/// A combat's results read as the game decides it, a siege's too.
pub static BATTLES: Cost = Cost::new();
/// A frame's events made for remote control, at every safe frame.
pub static REMOTE: Cost = Cost::new();

fn summary() -> String {
    let each = |n: u64, t: Duration| if n == 0 { Duration::ZERO } else { t / n as u32 };
    let line = |what: &str, cost: &Cost| {
        let (n, t) = cost.take();
        format!("{what} {n} in {t:.1?} ({:.1?} each)", each(n, t))
    };
    [
        line("wars watched on days", &WARS),
        line("owners watched on days", &OWNERS),
        line("buildings finished", &BUILDINGS),
        line("combat results", &BATTLES),
        line("frames' events for remote control", &REMOTE),
    ]
    .join("; ")
}

/// Logs the session's costs and starts counting afresh.
pub fn log_session() {
    log!("event costs: {}", summary());
}
