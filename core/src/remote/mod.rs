//! Remote control's engine (ADR remote-control-engine-and-transports).
//! Transports hand it typed requests from any thread ([`submit`]); it runs them
//! at the safe points (`crate::safe_point`) on the game's main thread, through
//! the capability catalogue (`crate::catalogue`) as the player
//! (ADR remote-control-uses-capabilities), within a time budget per frame,
//! and hands the responses back. A request that waits, such as one for the
//! next event, is answered at a later safe point. The engine keeps the latest
//! events extensions hear, numbered, and writes every request to the
//! framework log (ADR remote-control-is-always-on). It knows nothing of
//! connections or JSON.
//!
//! A request that needs a game belongs to the session it was sent in: one
//! still queued when that session ends (a save loads, another game starts)
//! is answered `session_changed` instead of acting on a campaign its client
//! never saw. Every answer that numbers sessions or events says which
//! process gave it (`catalogue::process`): a restart (GLOSSARY), a return to
//! the main menu with the in-process return turned off, starts the game
//! again, and the numbers start again with it; an in-process return keeps
//! them. The restart ends this process's engine ([`Engine::end`]), whoever
//! asked for the return: from then on it answers at once.
#[cfg(test)]
mod tests;

use crate::catalogue::{self, events, input, menu, Game, Live, Place, Poll, Wait};
use crate::game;
use eu4_harmony_remote::{Answer, Code, Error, Request, Response};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

/// How long the requests run at one safe point may take together; at least
/// one runs.
pub const BUDGET: Duration = Duration::from_millis(5);

/// Where a response goes: the submitter waits on it, and lets it go by
/// dropping its [`Reply`]: before the answer, that gives the request up;
/// after it, the submitter is done with the answer (a transport has written
/// it, and the client read it).
#[derive(Default)]
struct Slot {
    response: Mutex<Option<Response>>,
    ready: Condvar,
    released: AtomicBool,
}

impl Slot {
    fn answer(&self, response: Response) {
        *self.response.lock().unwrap_or_else(|e| e.into_inner()) = Some(response);
        self.ready.notify_all();
    }

    /// The submitter let its [`Reply`] go.
    fn released(&self) -> bool {
        self.released.load(Ordering::Relaxed)
    }
}

/// The response to a submitted request, once it comes. Dropping it before
/// the response gives the request up: one not yet run never runs. A
/// transport keeps it until it has written the response and the client read
/// it: the process ending waits for that ([`Engine::end`]).
pub struct Reply(Arc<Slot>);

impl Reply {
    /// Waits up to `timeout` for the response; None when it has not come.
    pub fn wait(&self, timeout: Duration) -> Option<Response> {
        let deadline = Instant::now() + timeout;
        let mut response = self.0.response.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(r) = response.take() {
                return Some(r);
            }
            let left = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())?;
            response = self.0.ready.wait_timeout(response, left).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        self.0.released.store(true, Ordering::Relaxed);
    }
}

/// A request in the queue.
struct Queued {
    number: u64,
    request: Request,
    slot: Arc<Slot>,
    /// The session it was sent in, 0 for none.
    session: u64,
}

/// The requests waiting to run, and, once the process ends, what every
/// request is answered.
struct Queue {
    requests: VecDeque<Queued>,
    ending: Option<Error>,
}

/// A request that ran and waits to answer.
struct Waiting {
    number: u64,
    name: String,
    started: Instant,
    slot: Arc<Slot>,
    wait: Box<dyn Wait>,
}

/// What only the safe points touch.
struct Running {
    waiting: Vec<Waiting>,
    events: events::Buffer,
    /// The last input taken, until the game has had it: the requests after
    /// it wait till then.
    input: Option<game::input::Taken>,
}

/// The engine: a queue any thread adds to, and what runs at safe points.
pub struct Engine {
    queue: Mutex<Queue>,
    running: Mutex<Running>,
    /// What happened since the events were last taken into [`Running`]:
    /// kept apart, as it is kept also from inside a request the engine runs,
    /// holding `running`, such as a console command that loads a save and so
    /// ends the session.
    recorded: Mutex<Vec<eu4_harmony_remote::events::Event>>,
    /// The responses given whose submitters still hold their replies, as a
    /// transport does until the response is written and read.
    delivering: Mutex<Vec<Arc<Slot>>>,
    next: AtomicU64,
    /// The session of the last game frame; 0 when the campaign changed
    /// since, or before the first.
    session: AtomicU64,
    log: fn(&str),
}

impl Engine {
    /// An engine writing its log lines to `log`.
    pub const fn new(log: fn(&str)) -> Self {
        Self {
            queue: Mutex::new(Queue { requests: VecDeque::new(), ending: None }),
            running: Mutex::new(Running { waiting: Vec::new(), events: events::Buffer::new(), input: None }),
            recorded: Mutex::new(Vec::new()),
            delivering: Mutex::new(Vec::new()),
            next: AtomicU64::new(1),
            session: AtomicU64::new(0),
            log,
        }
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn delivering(&self) -> std::sync::MutexGuard<'_, Vec<Arc<Slot>>> {
        self.delivering.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queues a request, from any thread; once the process ends, answers it
    /// at once.
    pub fn submit(&self, request: Request) -> Reply {
        let slot = Arc::new(Slot::default());
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        let q = Queued { number, request, slot: slot.clone(), session: self.session.load(Ordering::Relaxed) };
        let mut queue = self.queue();
        match queue.ending.clone() {
            Some(error) => {
                drop(queue);
                self.answer(&q, Err(error), Duration::ZERO);
            }
            None => queue.requests.push_back(q),
        }
        Reply(slot)
    }

    /// A game's safe frame of `session` begins: the requests that need a
    /// game sent from now on belong to it.
    pub fn enter(&self, session: u64) {
        self.session.store(session, Ordering::Relaxed);
    }

    /// The campaign changes, such as when a save begins to load: the
    /// requests sent from now on belong to the session that comes next.
    pub fn leave(&self) {
        self.session.store(0, Ordering::Relaxed);
    }

    /// The session going on ends, for `reason`: kept as an event, and, as at
    /// a load, the requests sent from now on belong to the session that
    /// comes next, also when no save is read for it (an in-process return,
    /// then a new game).
    pub fn session_ended(&self, reason: eu4_harmony_remote::events::SessionEnd) {
        self.record([eu4_harmony_remote::events::Event::SessionEnded { reason }]);
        self.leave();
    }

    /// Keeps what happened, for remote clients to read: numbered in order
    /// as the engine next runs, so also from inside a request it runs.
    pub fn record(&self, happened: impl IntoIterator<Item = eu4_harmony_remote::events::Event>) {
        self.recorded.lock().unwrap_or_else(|e| e.into_inner()).extend(happened);
    }

    /// Numbers what was recorded since the last time, into the events kept.
    fn take_recorded(&self, running: &mut Running) {
        for event in std::mem::take(&mut *self.recorded.lock().unwrap_or_else(|e| e.into_inner())) {
            running.events.push(event);
        }
    }

    /// Runs at a safe point: answers the waiting requests that are due, then
    /// runs queued requests in order until `budget` is spent. What does not
    /// run stays queued, in order.
    pub fn run(&self, game: &mut dyn Game, place: &Place, budget: Duration) {
        self.run_at(game, place, budget, Instant::now());
    }

    /// [`run`](Self::run) with the time the requests' waits are measured
    /// against: now, but for a test that lets time pass without waiting.
    pub(crate) fn run_at(&self, game: &mut dyn Game, place: &Place, budget: Duration, now: Instant) {
        let started = Instant::now();
        self.delivering().retain(|slot| !slot.released());
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        let running = &mut *running;
        self.take_recorded(running);
        self.poll(running, game, place, now);
        let mut queued = std::mem::take(&mut self.queue().requests);
        let mut kept = VecDeque::new();
        let mut ran = false;
        let session = self.session.load(Ordering::Relaxed);
        while let Some(q) = queued.pop_front() {
            if q.slot.released() {
                (self.log)(&format!("remote #{} {}: given up by its client before it ran", q.number, json(&q.request)));
                continue;
            }
            // A request after an input runs once the game has had the input;
            // it and those after it wait, in order.
            if let Some(taken) = running.input.filter(|_| !q.request.gives_input()) {
                if !game.handled(taken) {
                    kept.push_back(q);
                    kept.append(&mut queued);
                    break;
                }
                running.input = None;
            }
            if q.request.in_game() {
                match place {
                    Place::Game if q.session != 0 && q.session != session => {
                        let why = format!("it was sent in session {}, which ended before it ran; this is session {session}", q.session);
                        self.answer(&q, Err(Error::new(Code::SessionChanged, why)), Duration::ZERO);
                        continue;
                    }
                    Place::Game => {}
                    Place::Outside(None) => {
                        kept.push_back(q);
                        continue;
                    }
                    Place::Outside(Some(error)) | Place::Unplayable(error) => {
                        self.answer(&q, Err(error.clone()), Duration::ZERO);
                        continue;
                    }
                }
            } else if q.request.needs_place() && matches!(place, Place::Outside(None)) {
                kept.push_back(q);
                continue;
            }
            if ran && started.elapsed() >= budget {
                kept.push_back(q);
                kept.append(&mut queued);
                break;
            }
            ran = true;
            self.start(running, game, q, now);
            // What the request made happen, for the requests after it.
            self.take_recorded(running);
        }
        let mut queue = self.queue();
        while let Some(q) = kept.pop_back() {
            queue.requests.push_front(q);
        }
    }

    /// Runs a request at `now`.
    fn start(&self, running: &mut Running, game: &mut dyn Game, q: Queued, now: Instant) {
        let started = Instant::now();
        if let Request::Input(r) = &q.request {
            let (response, taken) = input::run(r.clone(), game);
            running.input = taken.or(running.input);
            self.answer(&q, response.map(Answer::Input), started.elapsed());
            return;
        }
        match catalogue::dispatch(q.request.clone(), game, &running.events, now) {
            catalogue::Outcome::Done(response) => self.answer(&q, response, started.elapsed()),
            catalogue::Outcome::Later(wait) => {
                (self.log)(&format!("remote #{} {}: answers later", q.number, json(&q.request)));
                running.waiting.push(Waiting { number: q.number, name: q.request.name(), started, slot: q.slot, wait });
            }
        }
    }

    fn answer(&self, q: &Queued, response: Response, took: Duration) {
        (self.log)(&format!("remote #{} {}: {} in {:.1} ms", q.number, json(&q.request), outcome(&response), took.as_secs_f64() * 1000.0));
        self.deliver(&q.slot, response);
    }

    /// Gives `slot` its response, which its submitter has until it lets its
    /// reply go.
    fn deliver(&self, slot: &Arc<Slot>, response: Response) {
        slot.answer(response);
        self.delivering().push(slot.clone());
    }

    fn poll(&self, running: &mut Running, game: &mut dyn Game, place: &Place, now: Instant) {
        let mut at = Poll::new(&running.events, now, place, game);
        for mut w in std::mem::take(&mut running.waiting) {
            if w.slot.released() {
                (self.log)(&format!("remote #{} {}: given up by its client while it waited", w.number, w.name));
                w.wait.given_up(&mut at);
                continue;
            }
            match w.wait.poll(&mut at) {
                Some(response) => {
                    (self.log)(&format!("remote #{} {}: {} after {} ms", w.number, w.name, outcome(&response), now.duration_since(w.started).as_millis()));
                    self.deliver(&w.slot, response);
                }
                None => running.waiting.push(w),
            }
        }
    }

    /// The process is about to end, as the game starts `eu4.exe` again to
    /// return to the main menu, whoever asked for it: every request is
    /// answered while the pipe can still carry the answers. A waiting one
    /// gives its own answer when it has one ([`Wait::ending`]: an advance
    /// answers `main_menu`, a diplomatic action `pending`), and is answered
    /// `why` otherwise, as are those queued and every one sent from now on.
    /// The submitters of these answers, and of those given earlier that they
    /// still hold, get up to `within` to be done with them, each letting its
    /// [`Reply`] go once its transport has written the answer and the client
    /// has read it: an answer larger than the pipe's buffer, which the client
    /// reads as it is written, so gets through whole before the process goes.
    pub fn end(&self, why: Error, within: Duration) {
        let started = Instant::now();
        let queued: Vec<Queued> = {
            let mut queue = self.queue();
            queue.ending = Some(why.clone());
            queue.requests.drain(..).collect()
        };
        (self.log)(&format!("remote: this process ends; every request is answered {why}"));
        for q in queued {
            if !q.slot.released() {
                self.answer(&q, Err(why.clone()), Duration::ZERO);
            }
        }
        {
            let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
            let running = &mut *running;
            self.take_recorded(running);
            for mut w in running.waiting.drain(..) {
                if w.slot.released() {
                    continue;
                }
                let response = w.wait.ending(&running.events).unwrap_or_else(|| Err(why.clone()));
                (self.log)(&format!(
                    "remote #{} {}: {} as the process ends, after {} ms",
                    w.number,
                    w.name,
                    outcome(&response),
                    started.duration_since(w.started).as_millis()
                ));
                self.deliver(&w.slot, response);
            }
        }
        let delivering = std::mem::take(&mut *self.delivering());
        while delivering.iter().any(|slot| !slot.released()) && started.elapsed() < within {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn json(request: &Request) -> String {
    serde_json::to_string(request).unwrap_or_else(|_| request.name())
}

fn outcome(response: &Response) -> String {
    match response {
        Ok(_) => "ok".into(),
        Err(e) => e.to_string(),
    }
}

// ---------- the engine in the game ----------

fn framework_log(line: &str) {
    log!("{line}");
}

static ENGINE: Engine = Engine::new(framework_log);

/// Queues a request for the game, from any thread.
pub fn submit(request: Request) -> Reply {
    ENGINE.submit(request)
}

/// What extensions hear at a game's safe frame, in the order they hear it, as
/// events.
fn happened(frame: &game::Frame) -> Vec<eu4_harmony_remote::events::Event> {
    use eu4_harmony_remote::{events::Event, ArmyId, Date, ProvinceId};
    use game::{events::Change, wars::Happened, Heard};
    let tag = catalogue::tag;
    let tags = |ts: Vec<game::Tag>| ts.into_iter().map(tag).collect();
    // Remote control reads the player's view unless the setting gives it the
    // full view: a battle the player neither sees nor takes part in is left
    // out.
    let full = game::view(game::Caller::Remote) == game::View::Full;
    let event = |heard| {
        Some(match heard {
            Heard::Session(session) => Event::Session { session },
            Heard::Change(Change::Ordered(army)) => Event::ArmyOrdered { army: ArmyId(army) },
            Heard::Change(Change::Defeated(army)) => Event::ArmyDefeated { army: ArmyId(army) },
            Heard::Change(Change::BuildingFinished { province, building, country }) => match game::buildings::key_of(building) {
                Ok(building) => Event::BuildingFinished { province: ProvinceId(province), building, country: catalogue::tag(country) },
                Err(e) => {
                    log!("building finished: {}", e.message);
                    return None;
                }
            },
            Heard::Change(Change::Battle(battle)) if full || battle.seen => battle_event(&battle),
            Heard::Change(Change::Battle(_)) => return None,
            Heard::War(Happened::WarDeclared { war, attacker, defender }) => Event::WarDeclared { war, attacker: tag(attacker), defender: tag(defender) },
            Heard::War(Happened::PeaceMade { war, attackers, defenders, ended }) => {
                Event::PeaceMade { war, attackers: tags(attackers), defenders: tags(defenders), ended }
            }
            Heard::War(Happened::OwnerChanged { province, from, to }) => {
                Event::ProvinceOwnerChanged { province: ProvinceId(province), from: from.map(tag), to: to.map(tag) }
            }
            Heard::Day(date) => Event::Day { date: Date(date) },
            Heard::Month(date) => Event::Month { date: Date(date) },
            Heard::Year(date) => Event::Year { date: Date(date) },
        })
    };
    frame.heard().into_iter().filter_map(event).collect()
}

/// A battle, as the `battle` event tells it.
pub fn battle_event(battle: &game::events::Battle) -> eu4_harmony_remote::events::Event {
    use eu4_harmony_remote::events::{Event, Winner};
    Event::Battle {
        province: eu4_harmony_remote::ProvinceId(battle.province),
        naval: battle.naval,
        attackers: battle.attackers.iter().copied().map(catalogue::tag).collect(),
        defenders: battle.defenders.iter().copied().map(catalogue::tag).collect(),
        winner: if battle.attacker_won { Winner::Attackers } else { Winner::Defenders },
        attacker_losses: battle.attacker_losses.into(),
        defender_losses: battle.defender_losses.into(),
    }
}

/// The session is being left, for `end`, as extensions hear it.
pub(crate) fn session_ended(end: crate::sessions::End) {
    use crate::sessions::End;
    use eu4_harmony_remote::events::SessionEnd;
    let reason = match end {
        End::Return => SessionEnd::Return,
        End::Load => SessionEnd::Load,
        End::NewGame => SessionEnd::NewGame,
        End::Country => SessionEnd::Country,
    };
    ENGINE.session_ended(reason);
}

/// At a game's safe frame: keeps what extensions just heard, then runs.
pub(crate) fn run_in_game(frame: &game::Frame) {
    ENGINE.record(happened(frame));
    ENGINE.enter(frame.session);
    ENGINE.run(&mut Live(game::Caller::Remote), &Place::Game, BUDGET);
}

/// The campaign changes (a save begins to load): requests that need a game
/// sent before it never act on the next one.
pub(crate) fn campaign_changes() {
    ENGINE.leave();
}

/// At a safe point with no game to read: `why` answers the requests that
/// need one, or, when None, they wait.
pub(crate) fn run_outside(why: Option<game::Fail>) {
    ENGINE.run(&mut Live(game::Caller::Remote), &Place::Outside(why.map(Error::from)), BUDGET);
}

/// At a game's frame with no game the player can play: `why` answers the
/// requests that need one.
pub(crate) fn run_unplayable(why: game::Fail) {
    ENGINE.run(&mut Live(game::Caller::Remote), &Place::Unplayable(Error::from(why)), BUDGET);
}

/// Before the game starts `eu4.exe` again, which ends this process: every
/// request is answered while the pipe can still carry the answers, those sent
/// later at once.
pub(crate) fn restarting() {
    ENGINE.end(menu::ending(), Duration::from_secs(2));
}
