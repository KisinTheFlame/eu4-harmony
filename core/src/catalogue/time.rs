//! Time: the clock, pausing, resuming and the speed, as the player would, and
//! advancing time, which only remote control does. An advance answers at a
//! later safe point: once the date is reached and the game paused there, or
//! earlier, saying why, among it once the main menu is up after an
//! in-process return; or, when the game starts `eu4.exe` again, just before
//! this process ends. Whatever it stops for before or at its date, it leaves
//! the game paused by the player's own pause, the one `time.pause` sets, also
//! when a window holds the game paused (an event window, when the game pauses
//! on events, or a message set to pause): a client that got the answer finds
//! the game paused until it resumes it, not running on once the window is
//! closed. Not when another game was loaded or the game returns to the main
//! menu, which leave the advance no game of its own to pause. While the
//! player observes, the country annexed or the game console's observe run,
//! time and the advance go on as in a game the player plays.
use super::{events, process, Live, Outcome, Place, Poll, Wait};
use crate::game::{self, time::Pause, GResult, Status};
use eu4_harmony_remote::{
    events::Event,
    time::{Advanced, Stop, Time, TimeAnswer, TimeRequest},
    Answer, Code, Date, Error, Response,
};
use std::time::{Duration, Instant};

pub use eu4_harmony_remote::time::MOST_DAYS;
/// A day in the game's dates.
const DAY: i32 = 24;
/// How long an advance that reached its date waits for that day's event,
/// which comes once visibility reflects the day, before it answers without.
const DAY_EVENT_WAIT: Duration = Duration::from_secs(2);

/// The group's slice of the capability layer.
pub trait Layer {
    fn time(&mut self) -> GResult<game::time::Time>;
    /// The player's own pause, whatever else holds the game paused.
    fn pause(&mut self) -> GResult<()>;
    fn resume(&mut self) -> GResult<()>;
    fn set_speed(&mut self, speed: u32) -> GResult<()>;
    /// How many questions wait for the player's answer.
    fn questions_waiting(&mut self) -> GResult<usize>;
    /// Pauses the game as the daily update reaches `date`.
    fn stop_at(&mut self, date: i32) -> GResult<Box<dyn Held>>;
}

/// A date time stops at, held for an advance: released when the advance
/// answers; dropped unreleased, when its client gave the advance up, it calls
/// the advance off, and time stops at the end of the day under way.
pub trait Held: Send {
    fn release(self: Box<Self>);
}

impl Held for game::time::Stop {
    fn release(self: Box<Self>) {
        game::time::Stop::release(*self);
    }
}

impl Layer for Live {
    fn time(&mut self) -> GResult<game::time::Time> {
        game::time::time(game::WorldSession::open(self.0)?)
    }

    fn pause(&mut self) -> GResult<()> {
        game::time::pause(game::WorldSession::open(self.0)?)
    }

    fn resume(&mut self) -> GResult<()> {
        game::time::resume(game::WorldSession::open(self.0)?)
    }

    fn set_speed(&mut self, speed: u32) -> GResult<()> {
        game::time::set_speed(self.0, speed)
    }

    fn questions_waiting(&mut self) -> GResult<usize> {
        game::time::questions_waiting(game::WorldSession::open(self.0)?)
    }

    fn stop_at(&mut self, date: i32) -> GResult<Box<dyn Held>> {
        Ok(Box::new(game::time::stop_at(self.0, date)?))
    }
}

/// The group's slice is written above by hand, without `layer!`'s lines.
#[cfg(test)]
pub fn bindings() -> super::Bindings {
    super::Bindings::default()
}

pub fn run(request: TimeRequest, game: &mut (impl Layer + ?Sized), events: Option<&events::Buffer>) -> Outcome {
    let answer = |result: GResult<TimeAnswer>| Outcome::Done(result.map(Answer::Time).map_err(Error::from));
    match request {
        TimeRequest::Get {} => answer(game.time().map(|t| TimeAnswer::Get(Time { date: Date(t.date), paused: t.paused(), speed: t.speed }))),
        TimeRequest::Pause {} => answer(game.pause().map(TimeAnswer::Pause)),
        TimeRequest::Resume {} => answer(game.resume().map(TimeAnswer::Resume)),
        TimeRequest::SetSpeed { speed } => answer(game.set_speed(speed).map(TimeAnswer::SetSpeed)),
        // An advance answers with the events heard while it ran.
        TimeRequest::Advance { days, speed } => match events.map(|events| start(game, events, days, speed)) {
            None => Outcome::Done(Err(events::unkept())),
            Some(Ok(Started::Running(advance))) => Outcome::Later(Box::new(advance)),
            Some(Ok(Started::Stopped(advanced))) => Outcome::Done(Ok(Answer::Time(TimeAnswer::Advance(advanced)))),
            Some(Err(error)) => Outcome::Done(Err(error)),
        },
    }
}

/// What starting an advance comes to.
enum Started {
    Running(Advance),
    /// It did not run, and says why.
    Stopped(Advanced),
}

fn start(game: &mut (impl Layer + ?Sized), events: &events::Buffer, days: u32, speed: Option<u32>) -> Result<Started, Error> {
    if !(1..=MOST_DAYS).contains(&days) {
        return Err(Error::new(Code::InvalidArgument, format!("days is from 1 to {MOST_DAYS}, not {days}")));
    }
    if let Some(speed) = speed {
        eu4_harmony_checks::time::speed(speed)?;
    }
    let time = game.time()?;
    if let Some(end) = time.over {
        return Err(eu4_harmony_checks::time::over(Date(end)).into());
    }
    let since = events.last();
    if game.questions_waiting()? > 0 {
        game.pause()?;
        let advanced =
            Advanced { stopped: Stop::Question, reason: QUESTION.into(), date: Date(time.date), events: events.read(since, process(), &Default::default()) };
        return Ok(Started::Stopped(advanced));
    }
    if let Some(speed) = speed {
        game.set_speed(speed)?;
    }
    let until = time.date.checked_add(days as i32 * DAY).ok_or_else(|| Error::new(Code::InvalidArgument, "the date would run past the game's last"))?;
    // Before time runs, so that no day slips past the date.
    let stop = game.stop_at(until)?;
    if let Err(fail) = game.resume() {
        stop.release();
        return Err(fail.into());
    }
    Ok(Started::Running(Advance { session: time.session, since, until, date: time.date, reached: None, stop: Some(stop) }))
}

const QUESTION: &str = "a question waits for the player's answer";

/// An advance under way.
struct Advance {
    session: u64,
    /// The last event before time ran.
    since: u64,
    /// The date it stops at.
    until: i32,
    /// The date at the last safe point.
    date: i32,
    /// When the date was reached, while the advance waits for its day's
    /// event.
    reached: Option<Instant>,
    /// The stop at the date, until the advance answers.
    stop: Option<Box<dyn Held>>,
}

impl Advance {
    /// Whether, and why, it stops at this safe point; `observing` when the
    /// player only observes, which hears no day's event to wait for.
    fn check(&mut self, game: &mut (impl Layer + ?Sized), events: &events::Buffer, now: Instant, observing: bool) -> GResult<Option<(Stop, &'static str)>> {
        let time = game.time()?;
        self.date = time.date;
        if time.session != self.session {
            return Ok(Some((Stop::Session, "another game was loaded or started")));
        }
        if time.date >= self.until {
            game.pause()?;
            let reached = *self.reached.get_or_insert(now);
            let until = self.until;
            let heard =
                events.read(self.since, process(), &Default::default()).events.iter().any(|n| matches!(n.event, Event::Day { date } if date.0 >= until));
            return Ok((heard || observing || now.duration_since(reached) >= DAY_EVENT_WAIT).then_some((Stop::Date, "the date was reached")));
        }
        if game.questions_waiting()? > 0 {
            game.pause()?;
            return Ok(Some((Stop::Question, QUESTION)));
        }
        let why = match time.pause {
            Pause::Running => return Ok(None),
            Pause::Player => "the game was paused before the date, by the player, an extension or another remote client",
            Pause::Window => "a window paused the game before the date, such as a message set to pause it",
        };
        // The player's pause, which a window's closing does not lift.
        game.pause()?;
        Ok(Some((Stop::Paused, why)))
    }

    /// Where it stopped, and why.
    fn stopped(&self, stopped: Stop, reason: &str, events: &events::Buffer) -> Response {
        Ok(Answer::Time(TimeAnswer::Advance(Advanced {
            stopped,
            reason: reason.into(),
            date: Date(self.date),
            events: events.read(self.since, process(), &Default::default()),
        })))
    }

    /// Answers with `response`, whatever it is: the stop goes, so that it
    /// never pauses the game after the advance is over.
    fn answer(&mut self, response: Response) -> Option<Response> {
        self.let_go();
        Some(response)
    }

    fn let_go(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.release();
        }
    }
}

impl Wait for Advance {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response> {
        let Some(events) = at.events else { return self.answer(Err(events::unkept())) };
        // The main menu is open: the game returned to it in this process,
        // and no game frame comes until another game starts.
        if let Place::Outside(Some(_)) = at.place {
            let response = self.stopped(Stop::MainMenu, "the game returned to the main menu", events);
            return self.answer(response);
        }
        // While the player observes, as when the country was annexed or the
        // game console's observe ran, time runs on for the advance as well,
        // but no day's event is heard.
        let observing = at.unplayable().is_some();
        let now = at.now;
        let game = at.world()?;
        match self.check(game, events, now, observing) {
            Ok(None) => None,
            // A frame where the game cannot be read comes and goes.
            Err(fail) if fail.status == Status::NotReady => None,
            Ok(Some((stopped, reason))) => {
                let response = self.stopped(stopped, reason, events);
                self.answer(response)
            }
            Err(fail) => self.answer(Err(fail.into())),
        }
    }

    /// Given up at a safe point where it would answer, as when the game
    /// paused within the frame before (a question came up, the player
    /// paused), it does what answering does but answer: it leaves the
    /// player's pause, and its stop goes rather than being called off, which
    /// would pause the game a day after it is next resumed. So too once it
    /// paused the game at its date, waiting for the day's event. Otherwise
    /// its stop is called off as it goes.
    fn given_up(&mut self, at: &mut Poll<'_>) {
        if self.poll(at).is_none() && self.reached.is_some() {
            self.let_go();
        }
    }

    fn ending(&mut self, events: &events::Buffer) -> Option<Response> {
        let response = self.stopped(Stop::MainMenu, "the game is returning to the main menu: eu4.exe starts again", events);
        self.answer(response)
    }
}

#[cfg(test)]
mod tests;
