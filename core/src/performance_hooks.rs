//! Game boundaries for on-demand performance collection. Existing research in
//! docs/research/drawing-beside-the-day.md, headless.md and time.md establishes
//! these boundaries; no sampling reads live world state on the drawing thread.
use crate::{game, hooks::Call, performance, points::Point};
use eu4_harmony_remote::performance::{PerformanceAnswer, PerformanceRequest};

pub fn answer(request: PerformanceRequest) -> game::GResult<PerformanceAnswer> {
    match request {
        PerformanceRequest::Begin { seconds } => {
            refresh_context()?;
            performance::begin(seconds)?;
            Ok(PerformanceAnswer::Begin(()))
        }
        PerformanceRequest::End {} => performance::end().map(PerformanceAnswer::End),
    }
}

/// Only the main thread reads the world, including at frames inside a day.
fn refresh_context() -> game::GResult<()> {
    let m = crate::memory().ok_or_else(|| game::Fail { status: game::Status::NotReady, message: "game memory is unavailable".into() })?;
    let world = game::world_context_any(m)?;
    let index = m.i32(world.world + crate::objects::campaign::world::SPEED)?;
    if !(0..=4).contains(&index) {
        return Err("invalid game speed during performance collection".to_string().into());
    }
    performance::context(crate::sessions::number(), index as u32 + 1, game::paused(m, &world)?, crate::drawing::performance_interval());
    Ok(())
}

pub fn frame() {
    if performance::active() && refresh_context().is_err() {
        performance::interruption("game context unavailable");
    }
}

pub fn session_ended() {
    if performance::active() {
        performance::interruption("session ended");
        performance::context(0, 0, true, None);
    }
}

pub fn day_begins(call: &Call) {
    if performance::active() {
        match call.memory().i32(call.arg(0) + crate::objects::campaign::world::DATE) {
            Ok(date) => performance::day_begins(date),
            Err(_) => performance::interruption("day date unavailable"),
        }
    }
}

pub fn day_ends(call: &Call) {
    if performance::active() {
        match call.memory().i32(call.arg(0) + crate::objects::campaign::world::DATE) {
            Ok(date) => performance::day_ends(date, call.faulted()),
            Err(_) => performance::interruption("day date unavailable"),
        }
    }
}

pub fn main_draws(_: &mut Call) {
    if crate::thread_id() == crate::main_thread() {
        // The native path has no separately published frame: its interface
        // is ready when the in-game drawing is entered. This includes the
        // main-thread fallback and the drawing-thread speed-up left off.
        performance::prepared();
    }
}

pub fn save_begins(_: &mut Call) {
    performance::interruption("local save begins");
}

pub fn save_ends(call: &mut Call) {
    performance::interruption(if call.faulted() { "local save faulted" } else { "local save ends" });
}

pub fn points() -> Vec<Point> {
    let mut points = crate::drawing::performance_points();
    points.extend(crate::saving::background::performance_points());
    points
}
