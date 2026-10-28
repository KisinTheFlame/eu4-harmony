//! Time: the date, whether the game is paused and by what, and its speed;
//! pausing, resuming and setting the speed as the player would. The evidence
//! is in docs/research/time.md.
use super::{context, events, fail, game, map_manager, paused, Caller, Ctx, GResult, Status};
use crate::{
    memory::Memory,
    objects::campaign::{globals::WORLD, map_manager as manager, pause as pause_state},
    points::Point,
};

/// `(world, index)`: sets the speed, clamped to 0..=4, as both speed commands
/// do.
const SET_SPEED: usize = 0x7579f0;
/// The speed-up command's execution, which reads the speed.
const SPEED_UP: usize = 0xb79970;
/// `(pause state)`: the toggle the game console's `gamespeed` uses. Running,
/// it pauses as the player does; paused for any reason, it resumes and lets go
/// of every window holding the game paused, as the player's resuming does.
const TOGGLE_PAUSE: usize = 0x7e0250;

/// The world's speed, i32: 0 for speed 1 to 4 for speed 5.
const SPEED: usize = 0x1e08;
/// The pause state's own flag (u8): nonzero once paused by the player (or by
/// remote control or an extension, through [`pause`]); when it is zero and
/// the game is paused, windows hold it paused.
const PLAYER_PAUSED: usize = 0x48;

/// The game speeds, slowest first.
pub const SPEEDS: std::ops::RangeInclusive<u32> = 1..=5;

/// What holds the game paused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pause {
    /// Nothing: time runs.
    #[default]
    Running,
    /// The player paused it, or remote control or an extension did.
    Player,
    /// A window that pauses the game while it is open, such as a game event's
    /// when the game pauses on events, or a message set to pause.
    Window,
}

/// The game's clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Time {
    /// The session, as [`super::session::Session`] has it.
    pub session: u64,
    /// The date (docs/research/dates.md), 24 a day.
    pub date: i32,
    /// 1 (slowest) to 5 (fastest).
    pub speed: u32,
    pub pause: Pause,
}

impl Time {
    pub fn paused(&self) -> bool {
        self.pause != Pause::Running
    }
}

/// The date, the speed, and what holds the game paused.
pub fn time(_: Caller) -> GResult<Time> {
    let m = game()?;
    let ctx = context(m)?;
    let index = m.i32(ctx.world + SPEED)?;
    if !(0..=4).contains(&index) {
        return Err(format!("unexpected game speed {index}").into());
    }
    let pause = if !paused(m, &ctx)? {
        Pause::Running
    } else if m.u8(pause_state_of(m, &ctx)? + PLAYER_PAUSED)? != 0 {
        Pause::Player
    } else {
        Pause::Window
    };
    Ok(Time { session: events::session(), date: ctx.date, speed: index as u32 + 1, pause })
}

/// Pauses the game, as the player would; nothing when it is paused already.
pub fn pause(_: Caller) -> GResult<()> {
    let m = game()?;
    pause_game(m, &context(m)?)
}

/// Resumes the game, as the player would: also when windows hold it paused,
/// which stay open. Nothing when it runs.
pub fn resume(_: Caller) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    if paused(m, &ctx)? {
        toggle(m, &ctx)?;
        if paused(m, &ctx)? {
            return fail(Status::Rejected, "the game stays paused");
        }
    }
    Ok(())
}

/// Sets the game speed, 1 (slowest) to 5 (fastest), as the player would.
pub fn set_speed(_: Caller, speed: u32) -> GResult<()> {
    if !SPEEDS.contains(&speed) {
        return fail(Status::InvalidArgument, format!("the speed is from 1 to 5, not {speed}"));
    }
    let m = game()?;
    let ctx = context(m)?;
    m.call(m.at(SET_SPEED), &[ctx.world, (speed - 1) as usize])?;
    Ok(())
}

/// The map manager's pause state, checked by its owner.
fn pause_state_of(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    let manager = map_manager(m, ctx)?;
    let state = m.ptr(manager + manager::PAUSE)?;
    if m.ptr(state + pause_state::MANAGER)? != manager {
        return Err("unexpected pause owner".to_string().into());
    }
    Ok(state)
}

fn toggle(m: &Memory, ctx: &Ctx) -> GResult<()> {
    m.call(m.at(TOGGLE_PAUSE), &[pause_state_of(m, ctx)?])?;
    Ok(())
}

fn pause_game(m: &Memory, ctx: &Ctx) -> GResult<()> {
    if !paused(m, ctx)? {
        toggle(m, ctx)?;
        if !paused(m, ctx)? {
            return fail(Status::Rejected, "the game does not pause");
        }
    }
    Ok(())
}

/// The speed and the pause toggle.
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(
            SET_SPEED,
            "48 81 ec 88 00 00 00 41 b8 04 00 00 00 41 3b d0 44 0f 4e c2 33 c0 45 85 c0 41 0f 49 c0 89 81 08 1e 00 00",
            "set speed",
        )
        .uses(&[SPEED, 4]),
        Point::keep(SPEED_UP, "48 8b 0d 01 65 7c 01 8b 91 08 1e 00 00 ff c2 e9 6c e0 bd ff", "speed up command").uses(&[WORLD, SPEED]),
        Point::keep(TOGGLE_PAUSE, "40 53 48 83 ec 20 80 79 48 00 48 8b d9 75 5c", "pause toggle").uses(&[PLAYER_PAUSED]),
    ]
}

