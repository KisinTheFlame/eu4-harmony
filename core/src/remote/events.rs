//! The events remote control keeps (ADR 0016): what extensions hear at a
//! game's safe frame, numbered from 1 as they come, the latest [`KEPT`] of
//! them. A remote client reads those after the last number it saw, and may
//! wait for the next.
use super::{Outcome, Poll, Wait};
use eu4_harmony_remote::{
    events::{Event, Events, EventsAnswer, EventsRequest, Numbered},
    Answer, Code, Error, Response,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How many events are kept.
pub const KEPT: usize = 10_000;
/// The longest a read may wait, in milliseconds.
pub const LONGEST_WAIT: u32 = 60_000;

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

    /// The events after `since`. A `since` beyond the last event comes from
    /// before the game started again: every event kept is new to its reader.
    pub fn read(&self, since: u64) -> Events {
        let since = if since > self.last { 0 } else { since };
        let first = self.kept.front().map_or(self.last + 1, |n| n.number);
        Events { events: self.kept.iter().filter(|n| n.number > since).copied().collect(), last: self.last, missed: since + 1 < first }
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
    let EventsRequest::Read { since, wait_ms } = request;
    let since = since.unwrap_or(0);
    match wait_ms {
        Some(ms) if ms > LONGEST_WAIT => Outcome::Done(Err(Error::new(Code::InvalidArgument, format!("wait_ms is at most {LONGEST_WAIT}")))),
        Some(ms) if !events.news(since) => Outcome::Later(Box::new(Next { since, until: Instant::now() + Duration::from_millis(ms.into()) })),
        _ => Outcome::Done(read(events, since)),
    }
}

fn read(events: &Buffer, since: u64) -> Response {
    Ok(Answer::Events(EventsAnswer::Read(events.read(since))))
}

/// A read waiting for the next event, or for its time to run out.
struct Next {
    since: u64,
    until: Instant,
}

impl Wait for Next {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response> {
        (at.events.news(self.since) || at.now >= self.until).then(|| read(at.events, self.since))
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
        let mut buffer = Buffer::new();
        assert_eq!(buffer.read(0), Events { events: vec![], last: 0, missed: false });
        for army in 1..=3 {
            buffer.push(ordered(army));
        }
        let all = buffer.read(0);
        assert_eq!((all.events.len(), all.last, all.missed), (3, 3, false));
        assert_eq!(all.events[0], Numbered { number: 1, event: ordered(1) });
        assert_eq!(buffer.read(2).events, vec![Numbered { number: 3, event: ordered(3) }]);
        assert_eq!(buffer.read(3), Events { events: vec![], last: 3, missed: false });
        // A number from before the game started again reads everything.
        assert_eq!(buffer.read(99).events.len(), 3);
    }

    #[test]
    fn only_the_latest_are_kept_and_a_reader_that_fell_behind_is_told() {
        let mut buffer = Buffer::new();
        for army in 1..=(KEPT as u64 + 5) {
            buffer.push(ordered(army));
        }
        let all = buffer.read(0);
        assert_eq!((all.events.len(), all.last, all.missed), (KEPT, KEPT as u64 + 5, true));
        assert_eq!(all.events[0].number, 6);
        assert!(buffer.read(4).missed);
        assert!(!buffer.read(5).missed);
        assert_eq!(buffer.read(5).events.len(), KEPT);
    }
}
