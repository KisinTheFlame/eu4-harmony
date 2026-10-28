//! Time: the clock, pausing, resuming and the speed, as the player would.
use super::Live;
use crate::game::{self, Caller, GResult};
use eu4_harmony_remote::{
    time::{Time, TimeAnswer, TimeRequest},
    Date, Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn time(&mut self) -> GResult<game::time::Time>;
    fn pause(&mut self) -> GResult<()>;
    fn resume(&mut self) -> GResult<()>;
    fn set_speed(&mut self, speed: u32) -> GResult<()>;
}

impl Layer for Live {
    fn time(&mut self) -> GResult<game::time::Time> {
        game::time::time(Caller::Remote)
    }

    fn pause(&mut self) -> GResult<()> {
        game::time::pause(Caller::Remote)
    }

    fn resume(&mut self) -> GResult<()> {
        game::time::resume(Caller::Remote)
    }

    fn set_speed(&mut self, speed: u32) -> GResult<()> {
        game::time::set_speed(Caller::Remote, speed)
    }
}

pub fn run(request: TimeRequest, game: &mut (impl Layer + ?Sized)) -> Result<TimeAnswer, Error> {
    Ok(match request {
        TimeRequest::Get {} => {
            let t = game.time()?;
            TimeAnswer::Get(Time { date: Date(t.date), paused: t.paused(), speed: t.speed })
        }
        TimeRequest::Pause {} => TimeAnswer::Pause(game.pause()?),
        TimeRequest::Resume {} => TimeAnswer::Resume(game.resume()?),
        TimeRequest::Speed { speed } => TimeAnswer::Speed(game.set_speed(speed)?),
    })
}

#[cfg(test)]
mod tests;
