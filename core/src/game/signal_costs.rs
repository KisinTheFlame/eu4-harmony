//! What working out the signals of the game costs while someone listens to
//! them (docs/plans/player-actions.md, "Signals"; docs/performance.md): the wars and
//! the provinces' owners watched at each new day, the buildings finished and
//! the battles heard, and turning what was heard into the signals extensions
//! and remote control hear. Logged when a session ends, then counted afresh.
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
/// A frame's signals made, at every safe frame.
pub static SIGNALS: Cost = Cost::new();

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
        line("frames' signals", &SIGNALS),
    ]
    .join("; ")
}

/// Logs the session's costs and starts counting afresh.
pub fn log_session() {
    log!("signal costs: {}", summary());
}
