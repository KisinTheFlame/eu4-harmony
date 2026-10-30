//! The events remote control keeps (ADR 0016): what extensions hear at a
//! game's safe frame, numbered from 1 as they come, the latest [`KEPT`] of
//! them. A remote client reads those after the last number it saw, and may
//! wait for the next.
use super::{process, Outcome, Poll, Wait};
use eu4_harmony_remote::{
    events::{Event, Events, EventsAnswer, EventsRequest, Numbered},
    Answer, Code, Error, Response,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub use eu4_harmony_remote::events::{KEPT, LONGEST_WAIT};

/// The latest events.
pub struct Buffer {
    kept: VecDeque<Numbered>,
    last: u64,
}

impl Buffer {
    pub const fn new() -> Self {
        Self { kept: VecDeque::new(), last: 0 }
    }

    pub fn push(&mut self, event: Event) {
        self.last += 1;
        if self.kept.len() == KEPT {
            self.kept.pop_front();
        }
        self.kept.push_back(Numbered { number: self.last, event });
    }

    /// The events after `since`, which came from the process `from`. A
    /// `since` from another process, or beyond the last event, comes from
    /// before the game started again: every event kept is new to its reader,
    /// and the events that followed `since` then are gone, so the read is
    /// `missed`.
    pub fn read(&self, since: u64, from: u64) -> Events {
        let before_restart = from != process() || since > self.last;
        let since = if before_restart { 0 } else { since };
        let first = self.kept.front().map_or(self.last + 1, |n| n.number);
        Events {
            process: process(),
            events: self.kept.iter().filter(|n| n.number > since).copied().collect(),
            last: self.last,
            missed: before_restart || since + 1 < first,
        }
    }

    /// The number of the latest event; 0 before the first.
    pub fn last(&self) -> u64 {
        self.last
    }

    /// Whether there is something to read after `since`.
    fn news(&self, since: u64) -> bool {
        self.last != since
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn run(request: EventsRequest, events: &Buffer) -> Outcome {
    let EventsRequest::Read { since, wait_ms, process: from } = request;
    let (since, from) = (since.unwrap_or(0), from.unwrap_or_else(process));
    match wait_ms {
        Some(ms) if ms > LONGEST_WAIT => Outcome::Done(Err(Error::new(Code::InvalidArgument, format!("wait_ms is at most {LONGEST_WAIT}")))),
        // A `since` from another process has the events kept to read at once.
        Some(ms) if from == process() && !events.news(since) => {
            Outcome::Later(Box::new(Next { since, until: Instant::now() + Duration::from_millis(ms.into()) }))
        }
        _ => Outcome::Done(read(events, since, from)),
    }
}

fn read(events: &Buffer, since: u64, from: u64) -> Response {
    Ok(Answer::Events(EventsAnswer::Read(events.read(since, from))))
}

/// A read waiting for the next event, or for its time to run out.
struct Next {
    since: u64,
    until: Instant,
}

impl Wait for Next {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response> {
        (at.events.news(self.since) || at.now >= self.until).then(|| read(at.events, self.since, process()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_remote::ArmyId;

    fn ordered(army: u64) -> Event {
        Event::ArmyOrdered { army: ArmyId(army) }
    }

    #[test]
    fn events_are_numbered_and_read_after_a_number() {
        let me = process();
        let mut buffer = Buffer::new();
        assert_eq!(buffer.read(0, me), Events { process: me, events: vec![], last: 0, missed: false });
        for army in 1..=3 {
            buffer.push(ordered(army));
        }
        let all = buffer.read(0, me);
        assert_eq!((all.events.len(), all.last, all.missed), (3, 3, false));
        assert_eq!(all.events[0], Numbered { number: 1, event: ordered(1) });
        assert_eq!(buffer.read(2, me).events, vec![Numbered { number: 3, event: ordered(3) }]);
        assert_eq!(buffer.read(3, me), Events { process: me, events: vec![], last: 3, missed: false });
        // A number from before the game started again reads everything, with
        // `missed`: the events that followed it then are gone. A number
        // beyond the last one says so, and so does one from another process,
        // whatever it is.
        let after_restart = buffer.read(99, me);
        assert_eq!((after_restart.events.len(), after_restart.last, after_restart.missed), (3, 3, true));
        assert!(Buffer::new().read(99, me).missed);
        let other_process = buffer.read(2, me - 1);
        assert_eq!((other_process.events.len(), other_process.missed), (3, true));
    }

    #[test]
    fn only_the_latest_are_kept_and_a_reader_that_fell_behind_is_told() {
        let mut buffer = Buffer::new();
        for army in 1..=(KEPT as u64 + 5) {
            buffer.push(ordered(army));
        }
        let me = process();
        let all = buffer.read(0, me);
        assert_eq!((all.events.len(), all.last, all.missed), (KEPT, KEPT as u64 + 5, true));
        assert_eq!(all.events[0].number, 6);
        assert!(buffer.read(4, me).missed);
        assert!(!buffer.read(5, me).missed);
        assert_eq!(buffer.read(5, me).events.len(), KEPT);
    }
}
