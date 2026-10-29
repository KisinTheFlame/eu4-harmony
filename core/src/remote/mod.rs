//! Remote control's engine (ADR 0016). Transports hand it typed requests from
//! any thread ([`submit`]); it runs them at the safe points
//! (`crate::safe_point`) on the game's main thread, through the capability
//! layer as the player (ADR 0015), within a time budget per frame, and hands
//! the responses back. A request that waits, such as one for the next event,
//! is answered at a later safe point. The engine keeps the latest events
//! extensions hear, numbered, and writes every request to the framework log
//! (ADR 0017). It knows nothing of connections or JSON.
//!
//! One file per group of capabilities, as in the capability layer: each has
//! the group's slice of the layer as a trait ([`Game`] is all of them), the
//! layer itself behind it ([`Live`]), and turns the group's requests into
//! calls and the layer's types into the answers. Tests put a fake game behind
//! the traits.
pub mod armies;
pub mod camera;
pub mod countries;
pub mod describe;
pub mod events;
#[cfg(test)]
pub(crate) mod fake;
pub mod provinces;
pub mod screenshot;
pub mod session;
#[cfg(test)]
mod tests;
pub mod texts;

use crate::game;
use eu4_harmony_remote::{Answer, Code, CountryTag, Error, Request, Response};
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

/// The capability layer, as remote control sees it: every group's slice.
pub trait Game: session::Layer + countries::Layer + provinces::Layer + armies::Layer + texts::Layer + camera::Layer + screenshot::Layer {}
impl<T: session::Layer + countries::Layer + provinces::Layer + armies::Layer + texts::Layer + camera::Layer + screenshot::Layer + ?Sized> Game for T {}

/// The capability layer itself, called as remote control.
pub struct Live;

impl From<game::Fail> for Error {
    fn from(fail: game::Fail) -> Self {
        let code = match fail.status {
            game::Status::NotInGame => Code::NotInGame,
            game::Status::NotReady => Code::NotReady,
            game::Status::NotFound => Code::NotFound,
            game::Status::Rejected => Code::Rejected,
            game::Status::InvalidArgument => Code::InvalidArgument,
            game::Status::Unavailable => Code::Unavailable,
            game::Status::WrongContext | game::Status::Internal => Code::Internal,
        };
        Error::new(code, fail.message)
    }
}

/// A country's tag as remote clients write it.
pub fn tag(tag: game::Tag) -> CountryTag {
    CountryTag(tag.letters())
}

/// Where the engine runs.
#[derive(Clone, Debug)]
pub enum Place {
    /// A game's safe frame: every request runs.
    Game,
    /// A safe point with no game to read. Requests that need one wait for a
    /// game frame, or, given an error, are answered with it.
    Outside(Option<Error>),
}

/// What running a request comes to.
pub enum Outcome {
    Done(Response),
    /// The answer comes at a later safe point.
    Later(Box<dyn Wait>),
}

/// A request that waits: asked at every safe point until it answers.
pub trait Wait: Send {
    fn poll(&mut self, at: &mut Poll<'_>) -> Option<Response>;
}

/// What a waiting request may look at.
pub struct Poll<'a> {
    pub events: &'a events::Buffer,
    pub now: Instant,
    /// The game, at a game's safe frame only.
    pub game: Option<&'a mut dyn Game>,
}

/// Where a response goes: the submitter waits on it, and gives it up by
/// dropping its [`Reply`].
#[derive(Default)]
struct Slot {
    response: Mutex<Option<Response>>,
    ready: Condvar,
    abandoned: AtomicBool,
}

impl Slot {
    fn answer(&self, response: Response) {
        *self.response.lock().unwrap_or_else(|e| e.into_inner()) = Some(response);
        self.ready.notify_all();
    }

    fn abandoned(&self) -> bool {
        self.abandoned.load(Ordering::Relaxed)
    }
}

/// The response to a submitted request, once it comes. Dropping it gives the
/// request up: one not yet run never runs.
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
        self.0.abandoned.store(true, Ordering::Relaxed);
    }
}

/// A request in the queue.
struct Queued {
    number: u64,
    request: Request,
    slot: Arc<Slot>,
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
}

/// The engine: a queue any thread adds to, and what runs at safe points.
pub struct Engine {
    queue: Mutex<VecDeque<Queued>>,
    running: Mutex<Running>,
    next: AtomicU64,
    log: fn(&str),
}

impl Engine {
    /// An engine writing its log lines to `log`.
    pub const fn new(log: fn(&str)) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            running: Mutex::new(Running { waiting: Vec::new(), events: events::Buffer::new() }),
            next: AtomicU64::new(1),
            log,
        }
    }

    /// Queues a request, from any thread.
    pub fn submit(&self, request: Request) -> Reply {
        let slot = Arc::new(Slot::default());
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).push_back(Queued { number, request, slot: slot.clone() });
        Reply(slot)
    }

    /// Keeps what happened, for remote clients to read.
    pub fn record(&self, happened: impl IntoIterator<Item = eu4_harmony_remote::events::Event>) {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        for event in happened {
            running.events.push(event);
        }
    }

    /// Runs at a safe point: answers the waiting requests that are due, then
    /// runs queued requests in order until `budget` is spent. What does not
    /// run stays queued, in order.
    pub fn run(&self, game: &mut dyn Game, place: &Place, budget: Duration) {
        let started = Instant::now();
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        let running = &mut *running;
        self.poll(running, game, place);
        let mut queued = std::mem::take(&mut *self.queue.lock().unwrap_or_else(|e| e.into_inner()));
        let mut kept = VecDeque::new();
        let mut ran = false;
        while let Some(q) = queued.pop_front() {
            if q.slot.abandoned() {
                (self.log)(&format!("remote #{} {}: given up by its client before it ran", q.number, json(&q.request)));
                continue;
            }
            if q.request.in_game() {
                match place {
                    Place::Game => {}
                    Place::Outside(None) => {
                        kept.push_back(q);
                        continue;
                    }
                    Place::Outside(Some(error)) => {
                        self.answer(&q, Err(error.clone()), Duration::ZERO);
                        continue;
                    }
                }
            }
            if ran && started.elapsed() >= budget {
                kept.push_back(q);
                kept.append(&mut queued);
                break;
            }
            ran = true;
            self.start(running, game, q);
        }
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        while let Some(q) = kept.pop_back() {
            queue.push_front(q);
        }
    }

    fn start(&self, running: &mut Running, game: &mut dyn Game, q: Queued) {
        let started = Instant::now();
        match dispatch(q.request.clone(), game, &running.events) {
            Outcome::Done(response) => self.answer(&q, response, started.elapsed()),
            Outcome::Later(wait) => {
                (self.log)(&format!("remote #{} {}: answers later", q.number, json(&q.request)));
                running.waiting.push(Waiting { number: q.number, name: q.request.name(), started, slot: q.slot, wait });
            }
        }
    }

    fn answer(&self, q: &Queued, response: Response, took: Duration) {
        (self.log)(&format!("remote #{} {}: {} in {:.1} ms", q.number, json(&q.request), outcome(&response), took.as_secs_f64() * 1000.0));
        q.slot.answer(response);
    }

    fn poll(&self, running: &mut Running, game: &mut dyn Game, place: &Place) {
        let now = Instant::now();
        let mut at = Poll { events: &running.events, now, game: matches!(place, Place::Game).then_some(game) };
        for mut w in std::mem::take(&mut running.waiting) {
            if w.slot.abandoned() {
                (self.log)(&format!("remote #{} {}: given up by its client while it waited", w.number, w.name));
                continue;
            }
            match w.wait.poll(&mut at) {
                Some(response) => {
                    (self.log)(&format!("remote #{} {}: {} after {} ms", w.number, w.name, outcome(&response), now.duration_since(w.started).as_millis()));
                    w.slot.answer(response);
                }
                None => running.waiting.push(w),
            }
        }
    }
}

/// Runs one request, group by group.
fn dispatch(request: Request, game: &mut dyn Game, events: &events::Buffer) -> Outcome {
    let done = |response: Response| Outcome::Done(response);
    match request {
        Request::Session(r) => done(session::run(r, game).map(Answer::Session)),
        Request::Country(r) => done(countries::run(r, game).map(Answer::Country)),
        Request::Province(r) => done(provinces::run(r, game).map(Answer::Province)),
        Request::Army(r) => done(armies::run(r, game).map(Answer::Army)),
        Request::Text(r) => done(texts::run(r, game).map(Answer::Text)),
        Request::Screenshot(r) => screenshot::run(r, game),
        Request::Events(r) => events::run(r, events),
        Request::Describe(r) => done(Ok(Answer::Describe(describe::run(r)))),
        Request::Camera(r) => done(camera::run(r, game).map(Answer::Camera)),
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
    use eu4_harmony_remote::{events::Event, ArmyId, Date};
    let mut happened = Vec::new();
    if frame.new_session {
        happened.push(Event::Session { session: frame.session });
    }
    for change in &frame.changes {
        happened.push(match *change {
            game::events::Change::Ordered(army) => Event::ArmyOrdered { army: ArmyId(army) },
            game::events::Change::Defeated(army) => Event::ArmyDefeated { army: ArmyId(army) },
        });
    }
    if frame.new_day {
        happened.push(Event::Day { date: Date(frame.ctx.date) });
    }
    happened
}

/// At a game's safe frame: keeps what extensions just heard, then runs.
pub(crate) fn run_in_game(frame: &game::Frame) {
    ENGINE.record(happened(frame));
    ENGINE.run(&mut Live, &Place::Game, BUDGET);
}

/// At a safe point with no game to read: `why` answers the requests that
/// need one, or, when None, they wait.
pub(crate) fn run_outside(why: Option<game::Fail>) {
    ENGINE.run(&mut Live, &Place::Outside(why.map(Error::from)), BUDGET);
}
