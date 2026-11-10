//! The signals remote control keeps (ADR remote-control-engine-and-transports):
//! what extensions hear at a game's safe frame, numbered from 1 as they come,
//! the latest [`KEPT`] of them. A remote client reads those after the last
//! number it saw, and may wait for the next: a request as the game reads
//! every one, a request as the player only those the player's view shows
//! (ADR requests-act-as-the-player-or-as-the-game).
use super::{process, Outcome, Poll, Wait};
use crate::game::{Heard, View};
use eu4_harmony_remote::{
    signals::{Filter, Numbered, Signal, Signals, SignalsAnswer, SignalsRequest},
    Answer, Code, Error, Response,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub use eu4_harmony_remote::signals::{KEPT, LONGEST_WAIT};

/// The latest signals.
pub struct Buffer {
    kept: VecDeque<Kept>,
    last: u64,
}

/// A signal kept, and whether the player's view shows it.
struct Kept {
    numbered: Numbered,
    seen: bool,
}

impl Kept {
    /// Whether a read of `view` that `filter` makes takes it.
    fn read(&self, view: View, filter: &Filter) -> bool {
        (self.seen || view == View::Full) && filter.takes(&self.numbered.signal)
    }
}

impl Buffer {
    pub const fn new() -> Self {
        Self { kept: VecDeque::new(), last: 0 }
    }

    /// Keeps a signal the player's view shows.
    pub fn push(&mut self, signal: Signal) {
        self.keep(Heard { signal, seen: true });
    }

    /// Keeps a signal, numbered after the last.
    pub fn keep(&mut self, heard: Heard) {
        self.last += 1;
        if self.kept.len() == KEPT {
            self.kept.pop_front();
        }
        self.kept.push_back(Kept { numbered: Numbered { number: self.last, signal: heard.signal }, seen: heard.seen });
    }

    /// The signals after `since`, which came from the process `from`. A
    /// `since` from another process, or beyond the last signal, comes from
    /// before the game started again: every signal kept is new to its reader,
    /// and the signals that followed `since` then are gone, so the read is
    /// `missed`. Only the signals `view` shows and `filter` takes are read;
    /// `last` is the latest of any, to read from next.
    pub fn read(&self, since: u64, from: u64, view: View, filter: &Filter) -> Signals {
        let before_restart = from != process() || since > self.last;
        let since = if before_restart { 0 } else { since };
        let first = self.kept.front().map_or(self.last + 1, |k| k.numbered.number);
        Signals {
            process: process(),
            signals: self.kept.iter().filter(|k| k.numbered.number > since && k.read(view, filter)).map(|k| k.numbered.clone()).collect(),
            last: self.last,
            missed: before_restart || since + 1 < first,
        }
    }

    /// The number of the latest signal; 0 before the first.
    pub fn last(&self) -> u64 {
        self.last
    }

    /// Whether there is something `view` shows and `filter` takes to read
    /// after `since`.
    fn news(&self, since: u64, view: View, filter: &Filter) -> bool {
        self.last != since && self.kept.iter().rev().take_while(|k| k.numbered.number > since).any(|k| k.read(view, filter))
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

/// The answer to a request that reads remote control's signals, run by a
/// caller that keeps none: an extension hears them through its handlers.
pub fn unkept() -> Error {
    Error::new(Code::Unavailable, "only remote control keeps signals to read; an extension hears them through its handlers")
}

/// Reads the signals `view` shows, as `request` asks.
pub fn run(request: SignalsRequest, signals: &Buffer, view: View, now: Instant) -> Outcome {
    let SignalsRequest::Read { since, wait_ms, process: from, kinds, countries } = request;
    let (since, from) = (since.unwrap_or(0), from.unwrap_or_else(process));
    let filter = Filter::new(kinds, countries);
    if let Some(kind) = filter.unknown_kind() {
        return Outcome::Done(Err(Error::new(Code::InvalidArgument, format!("no signal is called {kind}"))));
    }
    crate::game::signals::read_remotely(filter.kinds.as_deref());
    match wait_ms {
        Some(ms) if ms > LONGEST_WAIT => Outcome::Done(Err(Error::new(Code::InvalidArgument, format!("wait_ms is at most {LONGEST_WAIT}")))),
        // A `since` from another process has the signals kept to read at once.
        Some(ms) if from == process() && !signals.news(since, view, &filter) => {
            Outcome::Later(Box::new(Next { since, until: now + Duration::from_millis(ms.into()), view, filter }))
        }
        _ => Outcome::Done(read(signals, since, from, view, &filter)),
    }
}

fn read(signals: &Buffer, since: u64, from: u64, view: View, filter: &Filter) -> Response {
    Ok(Answer::Signals(SignalsAnswer::Read(signals.read(since, from, view, filter))))
}

/// A read waiting for the next signal it takes, or for its time to run out.
struct Next {
    since: u64,
    until: Instant,
    view: View,
    filter: Filter,
}

impl Wait for Next {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response> {
        let Some(signals) = at.signals else { return Some(Err(unkept())) };
        (signals.news(self.since, self.view, &self.filter) || at.now >= self.until).then(|| read(signals, self.since, process(), self.view, &self.filter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_remote::ArmyId;

    fn ordered(army: u64) -> Signal {
        Signal::ArmyOrdered { army: ArmyId(army) }
    }

    impl Buffer {
        fn read_all(&self, since: u64, from: u64) -> Signals {
            self.read(since, from, View::Full, &Filter::default())
        }
    }

    #[test]
    fn a_read_takes_the_kinds_and_countries_asked_for_and_news_waits_for_them() {
        use eu4_harmony_remote::{CountryTag, Date};
        let me = process();
        let mut buffer = Buffer::new();
        buffer.push(Signal::Day { date: Date(1) });
        buffer.push(ordered(7));
        buffer.push(Signal::Month { date: Date(2) });
        let months = Filter { kinds: Some(vec!["month".into()]), countries: None };
        let read = buffer.read(0, me, View::Player, &months);
        assert_eq!((read.signals.len(), read.signals[0].number, read.last), (1, 3, 3));
        // A signal that names no country passes any countries.
        let french = Filter { kinds: None, countries: Some(vec![CountryTag(*b"FRA")]) };
        assert_eq!(buffer.read(0, me, View::Player, &french).signals.len(), 3);
        assert!(buffer.news(2, View::Player, &months));
        assert!(!buffer.news(3, View::Player, &months));
        let years = Filter { kinds: Some(vec!["year".into()]), countries: None };
        assert!(!buffer.news(0, View::Player, &years));
        assert_eq!(Filter { kinds: Some(vec!["week".into()]), countries: None }.unknown_kind(), Some("week"));
    }

    #[test]
    fn the_player_s_view_reads_only_what_it_shows_and_the_full_view_everything() {
        let me = process();
        let mut buffer = Buffer::new();
        buffer.push(ordered(1));
        buffer.keep(Heard { signal: ordered(2), seen: false });
        buffer.push(ordered(3));
        let numbers = |view| buffer.read(0, me, view, &Filter::default()).signals.iter().map(|n| n.number).collect::<Vec<_>>();
        assert_eq!(numbers(View::Player), [1, 3]);
        assert_eq!(numbers(View::Full), [1, 2, 3]);
        let mut unseen = Buffer::new();
        unseen.keep(Heard { signal: ordered(4), seen: false });
        assert!(!unseen.news(0, View::Player, &Filter::default()));
        assert!(unseen.news(0, View::Full, &Filter::default()));
        assert_eq!(unseen.read(0, me, View::Player, &Filter::default()).last, 1);
    }

    #[test]
    fn signals_are_numbered_and_read_after_a_number() {
        let me = process();
        let mut buffer = Buffer::new();
        assert_eq!(buffer.read_all(0, me), Signals { process: me, signals: vec![], last: 0, missed: false });
        for army in 1..=3 {
            buffer.push(ordered(army));
        }
        let all = buffer.read_all(0, me);
        assert_eq!((all.signals.len(), all.last, all.missed), (3, 3, false));
        assert_eq!(all.signals[0], Numbered { number: 1, signal: ordered(1) });
        assert_eq!(buffer.read_all(2, me).signals, vec![Numbered { number: 3, signal: ordered(3) }]);
        assert_eq!(buffer.read_all(3, me), Signals { process: me, signals: vec![], last: 3, missed: false });
        // A number from before the game started again reads everything, with
        // `missed`: the signals that followed it then are gone. A number
        // beyond the last one says so, and so does one from another process,
        // whatever it is.
        let after_restart = buffer.read_all(99, me);
        assert_eq!((after_restart.signals.len(), after_restart.last, after_restart.missed), (3, 3, true));
        assert!(Buffer::new().read_all(99, me).missed);
        let other_process = buffer.read_all(2, me - 1);
        assert_eq!((other_process.signals.len(), other_process.missed), (3, true));
    }

    #[test]
    fn only_the_latest_are_kept_and_a_reader_that_fell_behind_is_told() {
        let mut buffer = Buffer::new();
        for army in 1..=(KEPT as u64 + 5) {
            buffer.push(ordered(army));
        }
        let me = process();
        let all = buffer.read_all(0, me);
        assert_eq!((all.signals.len(), all.last, all.missed), (KEPT, KEPT as u64 + 5, true));
        assert_eq!(all.signals[0].number, 6);
        assert!(buffer.read_all(4, me).missed);
        assert!(!buffer.read_all(5, me).missed);
        assert_eq!(buffer.read_all(5, me).signals.len(), KEPT);
    }
}
