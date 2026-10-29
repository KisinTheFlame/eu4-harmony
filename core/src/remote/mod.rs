//! Remote control's engine (ADR 0016). Transports hand it typed requests from
//! any thread ([`submit`]); it runs them at the safe points
//! (`crate::safe_point`) on the game's main thread, through the capability
//! layer as the player (ADR 0015), within a time budget per frame, and hands
//! the responses back. A request that waits, such as one for the next event,
//! is answered at a later safe point. The engine keeps the latest events
//! extensions hear, numbered, and writes every request to the framework log
//! (ADR 0017). It knows nothing of connections or JSON.
//!
//! A request that needs a game belongs to the session it was sent in: one
//! still queued when that session ends (a save loads, another game starts)
//! is answered `session_changed` instead of acting on a campaign its client
//! never saw. Every answer that numbers sessions or events says which
//! process gave it ([`process`]): returning to the main menu starts the game
//! again, and the numbers start again with it. The game's restart ends this
//! process's engine ([`Engine::end`]), whoever asked for the return: from
//! then on it answers at once.
//!
//! One file per group of capabilities, as in the capability layer: each has
//! the group's slice of the layer as a trait ([`Game`] is all of them), the
//! layer itself behind it ([`Live`]), and turns the group's requests into
//! calls and the layer's types into the answers. Tests put a fake game behind
//! the traits.
pub mod armies;
pub mod camera;
pub mod console;
pub mod countries;
pub mod describe;
pub mod diplomacy;
pub mod events;
#[cfg(test)]
pub(crate) mod fake;
pub mod menu;
pub mod offers;
pub mod provinces;
pub mod questions;
pub mod recruiting;
pub mod screenshot;
pub mod session;
#[cfg(test)]
mod tests;
pub mod texts;
pub mod time;

use crate::game;
use eu4_harmony_remote::{Answer, Code, CountryTag, Error, Request, Response};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// How long the requests run at one safe point may take together; at least
/// one runs.
pub const BUDGET: Duration = Duration::from_millis(5);

/// The capability layer, as remote control sees it: every group's slice.
pub trait Game: session::Layer + countries::Layer + provinces::Layer + armies::Layer + texts::Layer + camera::Layer + screenshot::Layer + console::Layer + time::Layer + recruiting::Layer + questions::Layer + offers::Layer + menu::Layer + diplomacy::Layer {}
impl<T: session::Layer + countries::Layer + provinces::Layer + armies::Layer + texts::Layer + camera::Layer + screenshot::Layer + console::Layer + time::Layer + recruiting::Layer + questions::Layer + offers::Layer + menu::Layer + diplomacy::Layer + ?Sized> Game for T {}

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

/// The game process remote control answers for: when it started, in
/// milliseconds since 1970. Returning to the main menu starts the game
/// again, as a new process, in which sessions and events are numbered from
/// 1 again; a client tells the two apart by this.
pub fn process() -> u64 {
    static PROCESS: OnceLock<u64> = OnceLock::new();
    *PROCESS.get_or_init(|| started().unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)))
}

/// When this process started, in milliseconds since 1970.
fn started() -> Option<u64> {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading::{GetCurrentProcess, GetProcessTimes}};
    let mut times = [FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }; 4];
    let [created, exited, kernel, user] = &mut times;
    if unsafe { GetProcessTimes(GetCurrentProcess(), created, exited, kernel, user) } == 0 {
        return None;
    }
    // 100-nanosecond intervals since 1601.
    let since_1601 = (u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime)) / 10_000;
    since_1601.checked_sub(11_644_473_600_000)
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

    /// Its client gave it up, as seen at this safe point: what it does before
    /// it goes, unanswered. Nothing by default.
    fn given_up(&mut self, _at: &mut Poll<'_>) {}

    /// The process is about to end, as the game starts `eu4.exe` again: the
    /// answer to write while it can, if it has one of its own. Without one,
    /// it is answered as every other request then ([`Engine::end`]).
    fn ending(&mut self, _events: &events::Buffer) -> Option<Response> {
        None
    }
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

    /// The response given is no longer here: its submitter took it.
    fn taken(&self) -> bool {
        self.response.lock().unwrap_or_else(|e| e.into_inner()).is_none()
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
}

/// The engine: a queue any thread adds to, and what runs at safe points.
pub struct Engine {
    queue: Mutex<Queue>,
    running: Mutex<Running>,
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
            running: Mutex::new(Running { waiting: Vec::new(), events: events::Buffer::new() }),
            next: AtomicU64::new(1),
            session: AtomicU64::new(0),
            log,
        }
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
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
        let mut queued = std::mem::take(&mut self.queue().requests);
        let mut kept = VecDeque::new();
        let mut ran = false;
        let session = self.session.load(Ordering::Relaxed);
        while let Some(q) = queued.pop_front() {
            if q.slot.abandoned() {
                (self.log)(&format!("remote #{} {}: given up by its client before it ran", q.number, json(&q.request)));
                continue;
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
        let mut queue = self.queue();
        while let Some(q) = kept.pop_back() {
            queue.requests.push_front(q);
        }
    }

    /// Runs a request.
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
                w.wait.given_up(&mut at);
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

    /// The process is about to end, as the game starts `eu4.exe` again to
    /// return to the main menu, whoever asked for it: every request is
    /// answered while the pipe can still carry the answers. A waiting one
    /// gives its own answer when it has one ([`Wait::ending`]: an advance
    /// answers `main_menu`, a diplomatic action `pending`), and is answered
    /// `why` otherwise, as are those queued and every one sent from now on.
    /// Their submitters get up to `within` to take the answers, so that a
    /// transport writes them before the process goes.
    pub fn end(&self, why: Error, within: Duration) {
        let started = Instant::now();
        let mut answered = Vec::new();
        let queued: Vec<Queued> = {
            let mut queue = self.queue();
            queue.ending = Some(why.clone());
            queue.requests.drain(..).collect()
        };
        (self.log)(&format!("remote: this process ends; every request is answered {why}"));
        for q in queued {
            if !q.slot.abandoned() {
                self.answer(&q, Err(why.clone()), Duration::ZERO);
                answered.push(q.slot);
            }
        }
        {
            let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
            let running = &mut *running;
            for mut w in running.waiting.drain(..) {
                if w.slot.abandoned() {
                    continue;
                }
                let response = w.wait.ending(&running.events).unwrap_or_else(|| Err(why.clone()));
                (self.log)(&format!("remote #{} {}: {} as the process ends, after {} ms", w.number, w.name, outcome(&response), started.duration_since(w.started).as_millis()));
                w.slot.answer(response);
                answered.push(w.slot);
            }
        }
        while answered.iter().any(|slot| !slot.taken() && !slot.abandoned()) && started.elapsed() < within {
            std::thread::sleep(Duration::from_millis(5));
        }
        if !answered.is_empty() {
            // Taken, an answer is written at once; a moment more for that.
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Runs one request, group by group.
fn dispatch(request: Request, game: &mut dyn Game, events: &events::Buffer) -> Outcome {
    let done = |response: Response| Outcome::Done(response);
    match request {
        Request::Session(r) => done(session::run(r, game).map(Answer::Session)),
        Request::Time(r) => time::run(r, game, events),
        Request::Country(r) => done(countries::run(r, game).map(Answer::Country)),
        Request::Province(r) => done(provinces::run(r, game).map(Answer::Province)),
        Request::Army(r) => done(armies::run(r, game).map(Answer::Army)),
        Request::Text(r) => done(texts::run(r, game).map(Answer::Text)),
        Request::Screenshot(r) => screenshot::run(r, game),
        Request::Console(r) => done(console::run(r, game).map(Answer::Console)),
        Request::Question(r) => done(questions::run(r, game).map(Answer::Question)),
        Request::Offer(r) => done(offers::run(r, game).map(Answer::Offer)),
        Request::Events(r) => events::run(r, events),
        Request::Describe(r) => done(Ok(Answer::Describe(describe::run(r)))),
        Request::Camera(r) => done(camera::run(r, game).map(Answer::Camera)),
        Request::Recruit(r) => done(recruiting::run(r, game).map(Answer::Recruit)),
        Request::Menu(r) => menu::run(r, game),
        Request::Diplomacy(r) => diplomacy::run(r, game),
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
    ENGINE.enter(frame.session);
    ENGINE.run(&mut Live, &Place::Game, BUDGET);
}

/// The campaign changes (a save begins to load): requests that need a game
/// sent before it never act on the next one.
pub(crate) fn campaign_changes() {
    ENGINE.leave();
}

/// At a safe point with no game to read: `why` answers the requests that
/// need one, or, when None, they wait.
pub(crate) fn run_outside(why: Option<game::Fail>) {
    ENGINE.run(&mut Live, &Place::Outside(why.map(Error::from)), BUDGET);
}

/// Before the game starts `eu4.exe` again, which ends this process: every
/// request is answered while the pipe can still carry the answers, those sent
/// later at once.
pub(crate) fn restarting() {
    ENGINE.end(menu::ending(), Duration::from_secs(2));
}
