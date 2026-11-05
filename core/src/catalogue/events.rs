//! The events remote control keeps (ADR remote-control-engine-and-transports):
//! what extensions hear at a game's safe frame, numbered from 1 as they come,
//! the latest [`KEPT`] of them. A remote client reads those after the last
//! number it saw, and may wait for the next.
use super::{process, Outcome, Poll, Wait};
use eu4_harmony_remote::{
    events::{Event, Events, EventsAnswer, EventsRequest, Filter, Numbered},
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
    /// `missed`. Only the events `filter` takes are read; `last` is the
    /// latest of any kind, to read from next.
    pub fn read(&self, since: u64, from: u64, filter: &Filter) -> Events {
        let before_restart = from != process() || since > self.last;
        let since = if before_restart { 0 } else { since };
        let first = self.kept.front().map_or(self.last + 1, |n| n.number);
        Events {
            process: process(),
            events: self.kept.iter().filter(|n| n.number > since && filter.takes(&n.event)).cloned().collect(),
            last: self.last,
            missed: before_restart || since + 1 < first,
        }
    }

    /// The number of the latest event; 0 before the first.
    pub fn last(&self) -> u64 {
        self.last
    }

    /// Whether there is something `filter` takes to read after `since`.
    fn news(&self, since: u64, filter: &Filter) -> bool {
        self.last != since && self.kept.iter().rev().take_while(|n| n.number > since).any(|n| filter.takes(&n.event))
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

/// The answer to a request that reads remote control's events, run by a
/// caller that keeps none: an extension hears them through its handlers.
pub fn unkept() -> Error {
    Error::new(Code::Unavailable, "only remote control keeps events to read; an extension hears them through its handlers")
}

pub fn run(request: EventsRequest, events: &Buffer, now: Instant) -> Outcome {
    let EventsRequest::Read { since, wait_ms, process: from, kinds, countries } = request;
    let (since, from) = (since.unwrap_or(0), from.unwrap_or_else(process));
    let filter = Filter::new(kinds, countries);
    if let Some(kind) = filter.unknown_kind() {
        return Outcome::Done(Err(Error::new(Code::InvalidArgument, format!("no event is called {kind}"))));
    }
    crate::game::events::read_remotely(filter.kinds.as_deref());
    match wait_ms {
        Some(ms) if ms > LONGEST_WAIT => Outcome::Done(Err(Error::new(Code::InvalidArgument, format!("wait_ms is at most {LONGEST_WAIT}")))),
        // A `since` from another process has the events kept to read at once.
        Some(ms) if from == process() && !events.news(since, &filter) => {
            Outcome::Later(Box::new(Next { since, until: now + Duration::from_millis(ms.into()), filter }))
        }
        _ => Outcome::Done(read(events, since, from, &filter)),
    }
}

fn read(events: &Buffer, since: u64, from: u64, filter: &Filter) -> Response {
    Ok(Answer::Events(EventsAnswer::Read(events.read(since, from, filter))))
}

/// A read waiting for the next event it takes, or for its time to run out.
struct Next {
    since: u64,
    until: Instant,
    filter: Filter,
}

impl Wait for Next {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response> {
        let Some(events) = at.events else { return Some(Err(unkept())) };
        (events.news(self.since, &self.filter) || at.now >= self.until).then(|| read(events, self.since, process(), &self.filter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_remote::ArmyId;

    fn ordered(army: u64) -> Event {
        Event::ArmyOrdered { army: ArmyId(army) }
    }

    impl Buffer {
        fn read_all(&self, since: u64, from: u64) -> Events {
            self.read(since, from, &Filter::default())
        }
    }

    #[test]
    fn a_read_takes_the_kinds_and_countries_asked_for_and_news_waits_for_them() {
        use eu4_harmony_remote::{CountryTag, Date};
        let me = process();
        let mut buffer = Buffer::new();
        buffer.push(Event::Day { date: Date(1) });
        buffer.push(ordered(7));
        buffer.push(Event::Month { date: Date(2) });
        let months = Filter { kinds: Some(vec!["month".into()]), countries: None };
        let read = buffer.read(0, me, &months);
        assert_eq!((read.events.len(), read.events[0].number, read.last), (1, 3, 3));
        // An event that names no country passes any countries.
        let french = Filter { kinds: None, countries: Some(vec![CountryTag(*b"FRA")]) };
        assert_eq!(buffer.read(0, me, &french).events.len(), 3);
        assert!(buffer.news(2, &months));
        assert!(!buffer.news(3, &months));
        let years = Filter { kinds: Some(vec!["year".into()]), countries: None };
        assert!(!buffer.news(0, &years));
        assert_eq!(Filter { kinds: Some(vec!["week".into()]), countries: None }.unknown_kind(), Some("week"));
    }

    #[test]
    fn events_are_numbered_and_read_after_a_number() {
        let me = process();
        let mut buffer = Buffer::new();
        assert_eq!(buffer.read_all(0, me), Events { process: me, events: vec![], last: 0, missed: false });
        for army in 1..=3 {
            buffer.push(ordered(army));
        }
        let all = buffer.read_all(0, me);
        assert_eq!((all.events.len(), all.last, all.missed), (3, 3, false));
        assert_eq!(all.events[0], Numbered { number: 1, event: ordered(1) });
        assert_eq!(buffer.read_all(2, me).events, vec![Numbered { number: 3, event: ordered(3) }]);
        assert_eq!(buffer.read_all(3, me), Events { process: me, events: vec![], last: 3, missed: false });
        // A number from before the game started again reads everything, with
        // `missed`: the events that followed it then are gone. A number
        // beyond the last one says so, and so does one from another process,
        // whatever it is.
        let after_restart = buffer.read_all(99, me);
        assert_eq!((after_restart.events.len(), after_restart.last, after_restart.missed), (3, 3, true));
        assert!(Buffer::new().read_all(99, me).missed);
        let other_process = buffer.read_all(2, me - 1);
        assert_eq!((other_process.events.len(), other_process.missed), (3, true));
    }

    #[test]
    fn only_the_latest_are_kept_and_a_reader_that_fell_behind_is_told() {
        let mut buffer = Buffer::new();
        for army in 1..=(KEPT as u64 + 5) {
            buffer.push(ordered(army));
        }
        let me = process();
        let all = buffer.read_all(0, me);
        assert_eq!((all.events.len(), all.last, all.missed), (KEPT, KEPT as u64 + 5, true));
        assert_eq!(all.events[0].number, 6);
        assert!(buffer.read_all(4, me).missed);
        assert!(!buffer.read_all(5, me).missed);
        assert_eq!(buffer.read_all(5, me).events.len(), KEPT);
    }
}
