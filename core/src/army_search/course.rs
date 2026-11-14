//! The army searches' course: a country's searches worked out ahead once its
//! King has thought, taken by each army's think and search when they match,
//! called off and waited for at the armies' day's end (`super`).
//!
//! The hooks tell the course their moments — the King's walk of its children,
//! an army's think, the search, a unit handed between agents, the armies' day's
//! end — and the course decides which searches are worked out ahead, which
//! outcome is taken, and what the pool runs. The game's search and the
//! agent/unit memory are reached through [`Game`]: the real ones in [`Real`],
//! and a recorder in the tests whose searches return what the test tells them.
//! The course is the King's thread's own: each country's day runs on a thread
//! of its own, so the batch in flight is kept per thread.
use super::{base, read, search_as_the_game, today, Call, Outcome, ARMIES, ARMY_THINK, COUNTDOWN, NEXT, SPEED_UP, THINK, THINKS_TODAY};
use crate::speed;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};
use std::time::Instant;

/// The game's search a job runs: `(agent, call, access cache) -> outcome`.
type SearchFn = Arc<dyn Fn(usize, &Call, usize) -> Outcome + Send + Sync>;

/// The game's search and the agent/unit memory, as the course reaches them.
pub(super) trait Game {
    /// The King's army agents that think today.
    fn thinking_agents(&mut self, king: usize) -> Vec<usize>;
    /// The search's arguments as the think gives them, or None when it would
    /// not search today, or would search in a battle.
    fn prepare(&mut self, agent: usize, argument: usize) -> Option<Call>;
    /// A hash of the state the search reads of the agent and its army, with
    /// its country's war plan and the offensive its King follows.
    fn state(&mut self, agent: usize, unit: usize) -> u64;
    /// The world's date.
    fn today(&mut self) -> u32;
    /// The game's search: a job worked out ahead runs it on the pool's threads
    /// with no access cache, and the checked comparison runs it again with the
    /// country's.
    fn search_fn(&self) -> SearchFn;
    /// Whether there is a pool to hand the searches to.
    fn ready(&self) -> bool;
    /// Hands the searches to the pool, which works them out while the King
    /// goes on.
    fn hand_out(&mut self, jobs: &[Arc<Job>]);
    /// Whether the searches are left to the game.
    fn game_path(&self) -> bool;
    /// Whether each search taken is checked against the game's.
    fn checked(&self) -> bool;
    /// A search taken was checked against the game's: whether they agreed,
    /// and what differed when they did not.
    fn compared(&mut self, same: bool, detail: String);
}

const PENDING: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;
const CALLED_OFF: u8 = 3;

/// One army's search, worked out ahead.
pub(super) struct Job {
    agent: usize,
    date: u32,
    call: Call,
    /// The agent's and its army's state when the search was prepared.
    state: u64,
    /// Held to read while the search runs; held to write while a unit is
    /// handed between the country's agents.
    gate: Arc<RwLock<()>>,
    /// The game's search this job runs.
    search: SearchFn,
    progress: AtomicU8,
    /// The agent's think has begun.
    thought: AtomicBool,
    outcome: Mutex<Option<Outcome>>,
    finished: Condvar,
}

impl Job {
    /// Works the search out unless the King's thread has called it off.
    fn work(&self) {
        if self.progress.compare_exchange(PENDING, RUNNING, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return;
        }
        let outcome = {
            let _reading = self.gate.read().unwrap_or_else(|e| e.into_inner());
            (self.search)(self.agent, &self.call, 0)
        };
        *self.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
        self.progress.store(DONE, Ordering::Release);
        self.finished.notify_all();
    }

    /// Calls it off if not begun, else waits until it is worked out; its
    /// outcome, if it was.
    fn settle(&self) -> Option<Outcome> {
        if self.progress.compare_exchange(PENDING, CALLED_OFF, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            return None;
        }
        let mut outcome = self.outcome.lock().unwrap_or_else(|e| e.into_inner());
        while outcome.is_none() && self.progress.load(Ordering::Acquire) == RUNNING {
            outcome = self.finished.wait(outcome).unwrap_or_else(|e| e.into_inner());
        }
        *outcome
    }
}

/// The searches of the country whose day this thread is in.
struct Batch {
    jobs: Vec<Arc<Job>>,
    gate: Arc<RwLock<()>>,
}

/// The course of the country whose day this thread is in.
pub(super) struct Course {
    batch: Option<Batch>,
    /// This thread holds its batch's gate to write.
    handing: bool,
}

impl Course {
    pub(super) const fn new() -> Self {
        Self { batch: None, handing: false }
    }

    /// Once the King has thought: its army agents' searches of today, handed
    /// to the pool.
    fn begin(&mut self, g: &mut impl Game, king: usize, argument: usize) {
        if let Some(old) = self.batch.take() {
            settle(old);
        }
        // The `agents-order` walks call this whether the searches are left to
        // the game or not.
        if g.game_path() || !g.ready() {
            return;
        }
        let began = Instant::now();
        let date = g.today();
        let gate = Arc::new(RwLock::new(()));
        let search = g.search_fn();
        let mut jobs = Vec::new();
        for agent in g.thinking_agents(king) {
            let Some(call) = g.prepare(agent, argument) else { continue };
            let state = g.state(agent, call.unit);
            jobs.push(Arc::new(Job {
                agent,
                date,
                call,
                state,
                gate: gate.clone(),
                search: search.clone(),
                progress: AtomicU8::new(PENDING),
                thought: AtomicBool::new(false),
                outcome: Mutex::new(None),
                finished: Condvar::new(),
            }));
        }
        add(Count::PreparingMicros, began.elapsed().as_micros() as u64);
        if jobs.is_empty() {
            return;
        }
        note_prepared(jobs.len() as u64);
        g.hand_out(&jobs);
        self.batch = Some(Batch { jobs, gate });
    }

    /// The army agent's think: its own search settled first, so that its think
    /// and its search never use its region scores at once.
    fn think(&mut self, agent: usize) {
        let Some(job) = self.job_of(agent) else { return };
        job.thought.store(true, Ordering::Relaxed);
        match job.progress.load(Ordering::Acquire) {
            DONE => {
                add(Count::Ready, 1);
            }
            PENDING => {
                add(Count::NotBegun, 1);
            }
            _ => {
                let began = Instant::now();
                job.settle();
                add(Count::WaitedMicros, began.elapsed().as_micros() as u64);
                add(Count::UnderWay, 1);
            }
        }
        job.settle();
    }

    /// The framework's search: the one worked out ahead when the think gives
    /// the same arguments and the agent and its army are as they were; None to
    /// search as the game does.
    fn search(&mut self, g: &mut impl Game, agent: usize, given: Call, argument: usize) -> Option<Outcome> {
        let job = self.batch.as_mut().and_then(|b| {
            let at = b.jobs.iter().position(|j| j.agent == agent)?;
            Some(b.jobs.swap_remove(at))
        });
        let Some(job) = job else {
            if self.batch.is_some() {
                add(Count::Unprepared, 1);
            }
            return None;
        };
        let outcome = job.settle();
        if outcome.is_some() && (job.date != g.today() || given != job.call) {
            add(Count::OtherArguments, 1);
            return None;
        }
        if outcome.is_some() && g.state(agent, given.unit) != job.state {
            add(Count::OtherState, 1);
            return None;
        }
        // Called off at the think, not begun, and counted there.
        let outcome = outcome?;
        if g.checked() {
            let game = (g.search_fn())(agent, &given, argument);
            g.compared(
                game.same(&outcome),
                format!(
                    "an army's search worked out ahead differs in {}: province {} at {}, the game's {} at {}",
                    outcome.differences(&game).join(", "),
                    outcome.province,
                    outcome.score,
                    game.province,
                    game.score
                ),
            );
        }
        add(Count::Taken, 1);
        Some(outcome)
    }

    /// The batch's gate to hold to write while a unit is handed, unless this
    /// thread is already handing one (the game hands units within a hand-over).
    fn handing_gate(&self) -> Option<Arc<RwLock<()>>> {
        if self.handing {
            None
        } else {
            self.batch.as_ref().map(|b| b.gate.clone())
        }
    }

    /// After the King's army agents' day: the searches not taken called off,
    /// those under way waited for.
    fn end(&mut self) {
        if let Some(batch) = self.batch.take() {
            settle(batch);
        }
    }

    /// The army's search worked out ahead, if any.
    fn job_of(&self, agent: usize) -> Option<Arc<Job>> {
        self.batch.as_ref()?.jobs.iter().find(|j| j.agent == agent).cloned()
    }
}

fn settle(batch: Batch) {
    for job in &batch.jobs {
        let thought = job.thought.load(Ordering::Relaxed);
        add(if thought { Count::NotSearched } else { Count::NotThought }, 1);
        job.settle();
    }
}

thread_local! {
    static COURSE: RefCell<Course> = const { RefCell::new(Course::new()) };
}

/// Once the King has thought (`super::children_begin`).
pub(super) fn begin(king: usize, argument: usize) {
    COURSE.with(|c| c.borrow_mut().begin(&mut Real, king, argument));
}

/// The army agent's think (`super::think`).
pub(super) fn think(agent: usize) {
    COURSE.with(|c| c.borrow_mut().think(agent));
}

/// The framework's search (`super::search`).
pub(super) fn search(agent: usize, given: Call, argument: usize) -> Option<Outcome> {
    COURSE.with(|c| c.borrow_mut().search(&mut Real, agent, given, argument))
}

/// A unit handed to an agent (`super::unit_to_agent`). The course is let go
/// while the game runs the hand-over, which may hand another unit: that one
/// sees this thread already handing, and does not take the gate again.
pub(super) fn unit_to_agent(f: impl FnOnce() -> usize) -> usize {
    let gate = COURSE.with(|c| c.borrow().handing_gate());
    let Some(gate) = gate else { return f() };
    let _writing = gate.write().unwrap_or_else(|e| e.into_inner());
    COURSE.with(|c| c.borrow_mut().handing = true);
    let result = f();
    COURSE.with(|c| c.borrow_mut().handing = false);
    result
}

/// After the King's army agents' day (`super::armies_end`).
pub(super) fn end() {
    COURSE.with(|c| c.borrow_mut().end());
}

/// The real game's search and memory: the functions of `super` and the pool
/// that works the searches out.
pub(super) struct Real;

impl Game for Real {
    fn thinking_agents(&mut self, king: usize) -> Vec<usize> {
        let base = base();
        let mut agents = Vec::new();
        let mut node = unsafe { read::<usize>(king + ARMIES) };
        while node != 0 {
            let agent = unsafe { read::<usize>(node) };
            node = unsafe { read::<usize>(node + NEXT) };
            let table = unsafe { read::<usize>(agent) };
            // Army agents only, which think today.
            if unsafe { read::<usize>(table + THINK) } != base + ARMY_THINK || unsafe { read::<i32>(agent + COUNTDOWN) } != 1 {
                continue;
            }
            let thinks_today: unsafe extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(read::<usize>(table + THINKS_TODAY)) };
            if unsafe { thinks_today(agent) } == 0 {
                continue;
            }
            agents.push(agent);
        }
        agents
    }

    fn prepare(&mut self, agent: usize, argument: usize) -> Option<Call> {
        unsafe { super::prepare(agent, argument) }
    }

    fn state(&mut self, agent: usize, unit: usize) -> u64 {
        unsafe { super::state(agent, unit) }
    }

    fn today(&mut self) -> u32 {
        today()
    }

    fn search_fn(&self) -> SearchFn {
        Arc::new(|agent, call, cache| unsafe { search_as_the_game(agent, call, cache) })
    }

    fn ready(&self) -> bool {
        pool().is_some()
    }

    fn hand_out(&mut self, jobs: &[Arc<Job>]) {
        let Some(pool) = pool() else { return };
        pool.queue.lock().unwrap_or_else(|e| e.into_inner()).extend(jobs.iter().cloned());
        pool.ready.notify_all();
    }

    fn game_path(&self) -> bool {
        speed::game_path(&SPEED_UP)
    }

    fn checked(&self) -> bool {
        speed::checked(&SPEED_UP)
    }

    fn compared(&mut self, same: bool, detail: String) {
        speed::compared(&SPEED_UP, same, || detail);
    }
}

// --- The pool ---

struct Pool {
    queue: Mutex<VecDeque<Arc<Job>>>,
    ready: Condvar,
}

static POOL: OnceLock<Option<&'static Pool>> = OnceLock::new();
const MOST_THREADS: usize = 8;
/// The search lays out arrays as long as the provinces on its stack.
const STACK: usize = 16 << 20;

fn pool() -> Option<&'static Pool> {
    *POOL.get_or_init(|| {
        let wanted = std::thread::available_parallelism().map_or(1, |n| n.get()).saturating_sub(2).min(MOST_THREADS);
        if wanted == 0 {
            return None;
        }
        let pool: &'static Pool = Box::leak(Box::new(Pool { queue: Mutex::new(VecDeque::new()), ready: Condvar::new() }));
        let started = (0..wanted)
            .filter(|i| {
                std::thread::Builder::new()
                    .name(format!("eu4-harmony army search {i}"))
                    .stack_size(STACK)
                    .spawn(move || crate::guarded("army search", || worker(pool)))
                    .is_ok()
            })
            .count();
        log!("army searches: {started} threads to search with");
        (started > 0).then_some(pool)
    })
}

fn worker(pool: &'static Pool) -> ! {
    loop {
        let job = {
            let mut queue = pool.queue.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(job) = queue.pop_front() {
                    break job;
                }
                queue = pool.ready.wait(queue).unwrap_or_else(|e| e.into_inner());
            }
        };
        job.work();
    }
}

// --- What came of the searches, for the log ---

/// What came of the searches, for the log.
#[derive(Clone, Copy)]
enum Count {
    /// Searches handed to the pool, and the time it took to prepare them on
    /// the King's thread, in microseconds.
    Prepared,
    PreparingMicros,
    /// At the agent's think: its search worked out, under way (and the
    /// time the think waited for it, in microseconds), or not begun and so
    /// called off.
    Ready,
    UnderWay,
    WaitedMicros,
    NotBegun,
    /// At the search: the one worked out taken, or not, the think giving
    /// other arguments, or the agent or its army changed.
    Taken,
    OtherArguments,
    OtherState,
    /// Searches the think made that none was prepared for.
    Unprepared,
    /// After the armies' day: prepared for an agent that did not think, or
    /// thought without searching.
    NotThought,
    NotSearched,
}

static COUNTS: [AtomicU64; 12] = [const { AtomicU64::new(0) }; 12];
static LOGGED: AtomicUsize = AtomicUsize::new(0);
const LOG_EVERY: u64 = 20_000;
const MOST_LOGGED: usize = 200;

fn add(count: Count, n: u64) -> u64 {
    COUNTS[count as usize].fetch_add(n, Ordering::Relaxed)
}

fn get(count: Count) -> u64 {
    COUNTS[count as usize].load(Ordering::Relaxed)
}

/// Logs what came of the searches, every so many prepared.
fn note_prepared(count: u64) {
    let before = add(Count::Prepared, count);
    if (before + count) / LOG_EVERY != before / LOG_EVERY && LOGGED.fetch_add(1, Ordering::Relaxed) < MOST_LOGGED {
        use Count::*;
        log!(
            "army searches: {} prepared in {} ms; at the think {} ready, {} under way (waited {} ms), {} not begun; {} taken, {} with other arguments, {} with another state; {} not thought, {} thought without a search; {} searched unprepared{}",
            before + count,
            get(PreparingMicros) / 1000,
            get(Ready),
            get(UnderWay),
            get(WaitedMicros) / 1000,
            get(NotBegun),
            get(Taken),
            get(OtherArguments),
            get(OtherState),
            get(NotThought),
            get(NotSearched),
            get(Unprepared),
            if speed::checked(&SPEED_UP) { format!(", {} differing from the game's", speed::differences(&SPEED_UP).1) } else { String::new() }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn call(unit: usize) -> Call {
        Call { depth: 99_999, unit, argument: 0, score: 10_000_000.0, a: 0, b: 0, c: 0, d: 0, flag: 1, other: 0, out: [0; 0x15] }
    }

    fn outcome(province: i32) -> Outcome {
        Outcome { province, score: 0.0, a: 0, b: 0, c: 0, d: 0, out: [0; 0x15] }
    }

    /// The game as the tests tell it: which agents think, what each search's
    /// arguments and state are, the date, and what the search returns — the
    /// prepared outcome with no access cache, the game's with one.
    struct Fake {
        agents: Vec<usize>,
        calls: HashMap<usize, Option<Call>>,
        states: HashMap<usize, u64>,
        date: u32,
        prepared: Outcome,
        game: Outcome,
        ready: bool,
        handed: Vec<Arc<Job>>,
        game_path: bool,
        checked: bool,
        compared: Vec<(bool, String)>,
    }

    impl Default for Fake {
        fn default() -> Self {
            Self {
                agents: Vec::new(),
                calls: HashMap::new(),
                states: HashMap::new(),
                date: 5,
                prepared: outcome(7),
                game: outcome(7),
                ready: true,
                handed: Vec::new(),
                game_path: false,
                checked: false,
                compared: Vec::new(),
            }
        }
    }

    impl Game for Fake {
        fn thinking_agents(&mut self, _king: usize) -> Vec<usize> {
            self.agents.clone()
        }

        fn prepare(&mut self, agent: usize, _argument: usize) -> Option<Call> {
            self.calls.get(&agent).copied().unwrap_or(Some(call(10)))
        }

        fn state(&mut self, agent: usize, _unit: usize) -> u64 {
            self.states.get(&agent).copied().unwrap_or(0)
        }

        fn today(&mut self) -> u32 {
            self.date
        }

        fn search_fn(&self) -> SearchFn {
            let (prepared, game) = (self.prepared, self.game);
            Arc::new(move |_agent, _call, cache| if cache == 0 { prepared } else { game })
        }

        fn ready(&self) -> bool {
            self.ready
        }

        fn hand_out(&mut self, jobs: &[Arc<Job>]) {
            self.handed.extend(jobs.iter().cloned());
        }

        fn game_path(&self) -> bool {
            self.game_path
        }

        fn checked(&self) -> bool {
            self.checked
        }

        fn compared(&mut self, same: bool, detail: String) {
            self.compared.push((same, detail));
        }
    }

    impl Fake {
        /// Works out every search handed over, as the pool would.
        fn run(&self) {
            for job in &self.handed {
                job.work();
            }
        }
    }

    fn begun(g: &mut Fake) -> Course {
        let mut c = Course::new();
        c.begin(g, 0x1000, 0);
        c
    }

    #[test]
    fn a_prepared_search_is_taken_when_it_matches() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        let mut c = begun(&mut g);
        g.run();
        assert_eq!(c.search(&mut g, 1, call(10), 0).map(|o| o.province), Some(7));
        assert!(g.compared.is_empty());
    }

    #[test]
    fn other_arguments_leave_it_to_the_game() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        let mut c = begun(&mut g);
        g.run();
        assert_eq!(c.search(&mut g, 1, call(11), 0), None);
    }

    #[test]
    fn another_date_leaves_it_to_the_game() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        let mut c = begun(&mut g);
        g.run();
        g.date = 6;
        assert_eq!(c.search(&mut g, 1, call(10), 0), None);
    }

    #[test]
    fn another_state_leaves_it_to_the_game() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        g.states.insert(1, 100);
        let mut c = begun(&mut g);
        g.run();
        g.states.insert(1, 200);
        assert_eq!(c.search(&mut g, 1, call(10), 0), None);
    }

    #[test]
    fn a_search_none_was_prepared_for_is_left_to_the_game() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        let mut c = begun(&mut g);
        g.run();
        assert_eq!(c.search(&mut g, 2, call(10), 0), None);
    }

    #[test]
    fn no_batch_leaves_every_search_to_the_game() {
        let mut g = Fake::default();
        let mut c = Course::new();
        assert_eq!(c.search(&mut g, 1, call(10), 0), None);
    }

    #[test]
    fn a_job_called_off_before_it_begins_has_no_outcome() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        let mut c = begun(&mut g);
        // Not run: the day's end calls it off.
        c.end();
        assert_eq!(g.handed[0].progress.load(Ordering::Acquire), CALLED_OFF);
        assert_eq!(g.handed[0].settle(), None);
    }

    #[test]
    fn the_days_end_waits_for_a_search_worked_out() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        let mut c = begun(&mut g);
        g.run();
        c.end();
        assert_eq!(g.handed[0].progress.load(Ordering::Acquire), DONE);
    }

    #[test]
    fn the_think_settles_its_own_search() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        let mut c = begun(&mut g);
        g.run();
        c.think(1);
        assert!(g.handed[0].thought.load(Ordering::Relaxed));
        assert_eq!(g.handed[0].progress.load(Ordering::Acquire), DONE);
        // An agent with no search of its own: nothing to settle.
        c.think(2);
    }

    #[test]
    fn a_unit_handed_holds_the_gate_and_nesting_does_not_deadlock() {
        let mut g = Fake { agents: vec![1], ..Default::default() };
        COURSE.with(|c| c.borrow_mut().begin(&mut g, 0x1000, 0));
        let gate = COURSE.with(|c| c.borrow().batch.as_ref().unwrap().gate.clone());
        let held = gate.clone();
        let result = unit_to_agent(|| {
            // The gate is held to write: a search cannot read meanwhile.
            assert!(held.try_write().is_err());
            // A unit handed again within the hand-over does not take it again.
            unit_to_agent(|| 7)
        });
        assert_eq!(result, 7);
        // Once handed, the gate is free again.
        assert!(gate.try_write().is_ok());
    }

    #[test]
    fn a_unit_handed_with_no_batch_runs_at_once() {
        assert_eq!(unit_to_agent(|| 42), 42);
    }

    #[test]
    fn checked_a_search_that_differs_is_reported() {
        let mut g = Fake { agents: vec![1], checked: true, prepared: outcome(7), game: outcome(8), ..Default::default() };
        let mut c = begun(&mut g);
        g.run();
        // The comparison runs the game's search with the country's access
        // cache (nonzero), the job worked out ahead with none.
        assert_eq!(c.search(&mut g, 1, call(10), 0x100).map(|o| o.province), Some(7));
        assert_eq!(g.compared.len(), 1);
        assert!(!g.compared[0].0);
        assert!(g.compared[0].1.contains("the province"));
    }

    #[test]
    fn checked_a_search_that_agrees_is_reported_the_same() {
        let mut g = Fake { agents: vec![1], checked: true, ..Default::default() };
        let mut c = begun(&mut g);
        g.run();
        c.search(&mut g, 1, call(10), 0x100);
        assert_eq!(g.compared.len(), 1);
        assert!(g.compared[0].0);
    }

    #[test]
    fn searches_left_to_the_game_prepare_nothing() {
        let mut g = Fake { agents: vec![1], game_path: true, ..Default::default() };
        let c = begun(&mut g);
        assert!(g.handed.is_empty());
        assert!(c.batch.is_none());
    }

    #[test]
    fn no_pool_prepares_nothing() {
        let mut g = Fake { agents: vec![1], ready: false, ..Default::default() };
        let c = begun(&mut g);
        assert!(g.handed.is_empty());
        assert!(c.batch.is_none());
    }

    #[test]
    fn an_agent_that_would_not_search_is_not_prepared() {
        let mut g = Fake { agents: vec![1, 2], ..Default::default() };
        g.calls.insert(1, None);
        let c = begun(&mut g);
        assert_eq!(g.handed.len(), 1);
        assert_eq!(g.handed[0].agent, 2);
        assert!(c.batch.is_some());
    }

    // --- The job's state machine, on its own ---

    fn job(search: SearchFn) -> Arc<Job> {
        Arc::new(Job {
            agent: 1,
            date: 5,
            call: call(10),
            state: 0,
            gate: Arc::new(RwLock::new(())),
            search,
            progress: AtomicU8::new(PENDING),
            thought: AtomicBool::new(false),
            outcome: Mutex::new(None),
            finished: Condvar::new(),
        })
    }

    #[test]
    fn a_job_not_begun_is_called_off_by_settle() {
        let j = job(Arc::new(|_, _, _| outcome(7)));
        assert_eq!(j.settle(), None);
        assert_eq!(j.progress.load(Ordering::Acquire), CALLED_OFF);
        // Called off: working it out does nothing.
        j.work();
        assert_eq!(j.progress.load(Ordering::Acquire), CALLED_OFF);
        assert_eq!(j.settle(), None);
    }

    #[test]
    fn a_job_worked_out_is_taken_by_settle() {
        let j = job(Arc::new(|_, _, _| outcome(7)));
        j.work();
        assert_eq!(j.progress.load(Ordering::Acquire), DONE);
        assert_eq!(j.settle().map(|o| o.province), Some(7));
    }

    #[test]
    fn a_search_under_way_is_waited_for() {
        let (send, recv) = std::sync::mpsc::channel::<()>();
        let recv = Mutex::new(recv);
        let j = job(Arc::new(move |_, _, _| {
            recv.lock().unwrap().recv().unwrap();
            outcome(7)
        }));
        let worker = {
            let j = j.clone();
            std::thread::spawn(move || j.work())
        };
        // Wait until the search is under way.
        while j.progress.load(Ordering::Acquire) != RUNNING {
            std::thread::yield_now();
        }
        let settler = {
            let j = j.clone();
            std::thread::spawn(move || j.settle())
        };
        // The settle waits until the search is worked out.
        send.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(settler.join().unwrap().map(|o| o.province), Some(7));
    }
}
