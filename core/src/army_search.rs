//! The searches of an AI country's armies, run on idle threads
//! (docs/research/ai.md, "The army and fleet agents"; ADR
//! the-game-may-be-made-faster-with-reasoned-differences).
//!
//! Late in a game the AI stage lasts as long as its longest country's task,
//! and much of that task is its armies' thinks one after another, most of
//! each the best-first search `0x1022b0` for where the army should go. The
//! search writes its stack, its outputs, the agent's own region scores
//! (`+0xbb0`, a hash map it adds keys to), caches the game keeps for
//! threads, and the country's access cache it is given (its fourth
//! argument, the agent's day's): the game already runs it on several
//! threads at once, one country on each.
//!
//! So once a country's King has thought, before its ministers do their day
//! (`0x14b622` in its day `0x14b370`), the framework finds the army agents
//! that think today and will search, works out for each the arguments the
//! think gives the search (`0xeab50`, `0xeb81f`..`0xebb5f`, reproduced in
//! [`prepare`]), and hands the searches to a pool of its own. Where the
//! think decides by a random draw whether it searches, as an army at peace
//! does nine thinks in ten, the search is left to it. The King goes
//! on at once, its ministers' days and its armies' running while the pool
//! searches. When an army's think calls the search, the framework's search
//! (which replaces `0x1022b0`) takes the result worked out for it, provided
//! the think gives the same arguments and the agent and its army are as
//! they were; anything else, it searches as the game does. After the armies'
//! day (`0x14b675`) the searches not taken are called off and the King waits
//! for those still under way, so nothing of the pool runs past the
//! country's task.
//!
//! Three things keep the pool from what the King's thread changes: its
//! searches are given no access cache, which the country's other agents add
//! to at the same time, and look access up each time;
//! an army's think (which replaces `0xe9f30`) first waits for its own
//! search, so that the two never use the agent's region scores at once; and
//! a unit handed from one agent to another (`0xe4990`, which changes both
//! agents' lists of units, read by the searches of armies near them) waits
//! for the searches under way, and no search begins until it is done.
//!
//! The difference from the game's own code is one of time: a search worked
//! out ahead reads the world as it stood when the King had thought, not
//! after its ministers and the armies before it did their day. The
//! arguments, the army's own state, the war plan and the offensive the
//! country follows are checked; what the others changed elsewhere (units
//! handed between agents, other armies' targets, the strength planned to
//! arrive in provinces) is not, and is what the developer build's
//! `check-army-searches` counts.
use crate::points::{Function, Point};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};

/// The search: `(callback, depth, unit, think argument, &score, &a, &b, &c,
/// &d, flag, flag, &out) -> province id`.
const SEARCH: usize = 0x1022b0;
/// The search's callback object's vtable: `{vtable, agent}`.
const CALLBACK: usize = 0x1c3cd98;
/// In the King's day, once it has thought: `mov rbx, [rdi + 0x40]; test rbx,
/// rbx; je` (`rdi` the King, `rbp` the day's argument), the walk of its
/// children, which goes on at `+9` and at the walk of its army agents
/// (`0x14b645`); and what follows that walk: `mov edx, esi; mov rcx, rdi`.
pub(crate) const CHILDREN_WALK: usize = 0x14b622;
const CHILDREN_WALK_ON: usize = 0x14b62b;
pub(crate) const ARMIES_WALK: usize = 0x14b645;
pub(crate) const ARMIES_WALKED: usize = 0x14b675;
const ARMIES_WALKED_ON: usize = 0x14b67a;
/// The King's list of army agents: first node; a node's agent at `+0` and
/// the next node at `+0x10`. Its children's list, `+0x40`. The record of the
/// offensive its country follows, `+0xcf8`, its fields `+0x10`..`+0x38`.
const ARMIES: usize = 0xc50;
const CHILDREN: usize = 0x40;
const FOLLOWED: usize = 0xcf8;
const OFFENSIVE: std::ops::Range<usize> = 0x10..0x38;
const NEXT: usize = 0x10;
/// A unit handed to an agent: `(agent, unit)`.
const UNIT_TO_AGENT: usize = 0xe4990;

/// An army agent: its days to its next think, its country (a handle, the
/// country's index in `+0x2c`), its King, its units' count.
const COUNTDOWN: usize = 0x20;
const COUNTRY: usize = 0x28;
const COUNTRY_INDEX: usize = 0x2c;
const KING: usize = 0xa40;
const UNITS: usize = 0xab8;
/// Its vtable's "thinks today" and think.
const THINKS_TODAY: usize = 0x50;
const THINK: usize = 0x60;
const ARMY_THINK: usize = 0xe9f30;
/// The agent's fields its search and its score read, checked unchanged
/// before a search worked out ahead is taken: its country, its King
/// `+0xa40`, its base and area `+0xaf0`..`+0xb00`, its targets
/// `+0xb00`..`+0xb08`, its flag `+0xb1c`, its region scores' map `+0xbb0`.
/// Not the fields its think itself writes before it searches (`+0xb14`,
/// `+0xb1e`, `+0xb28`).
const AGENT_STATE: [std::ops::Range<usize>; 5] = [0x28..0x30, 0xa40..0xa48, 0xaf0..0xb08, 0xb1c..0xb1d, 0xbb0..0xbd0];
/// A unit: `+0x24`, which the search's depth reads, its regiments and whom
/// it follows (`+0x20`..`+0x58`), its location, its path and its battle
/// (`+0xe8`..`+0x118`), its mission and flags (`+0x1f8`..`+0x210`), its AI
/// fields (`+0x248`..`+0x268`).
const UNIT_STATE: [std::ops::Range<usize>; 4] = [0x20..0x58, 0xe8..0x118, 0x1f8..0x210, 0x248..0x268];
const UNIT_LOCATION: usize = 0xe8;
const UNIT_BATTLE: usize = 0x110;
const UNIT_DAYS: usize = 0x24;
/// A unit's battle, its two bytes of retreating, and a field whose being
/// set ends the think before it searches; the agent's target over the sea.
const UNIT_IN_BATTLE: usize = 0xc0;
const UNIT_RETREATING: usize = 0x203;
const UNIT_STOPS: usize = 0x218;
/// An army agent's thinks at peace without a search, and how many lead to
/// one.
const THINKS_WITHOUT_SEARCH: usize = 0xbf0;
const PEACEFUL_THINKS: i32 = 15;
/// A unit locked in its move, and its method that answers it.
const UNIT_LOCKED: usize = 0x202;
const LOCKED_IN_MOVE: usize = 0x118;
const SEA_TARGET_PROVINCE: usize = 0xb00;

/// The world, its date at `+0x1dd0`; the countries (`+0x118` of the
/// country manager); a country's flags `+0x289b`, `+0x1988`, the
/// one that keeps its armies from sea targets, and its war plan.
use crate::objects::campaign::globals::{COUNTRY_MANAGER as COUNTRIES, WORLD};
const DATE: usize = 0x1dd0;
const COUNTRY_LIST: usize = 0x118;
const AT_WAR_FLAG: usize = 0x289b;
const WARS: usize = 0x1988;
const NO_SEA_TARGETS: usize = 0x2883;
const WAR_PLAN: std::ops::Range<usize> = 0x5ec..0x604;

/// The constants the think gives: the score it starts the search with
/// (`0x1dc19a8`), what it divides the army's strength by (`0x1dc1938`), and
/// a weight of the location's score (`0x1dc1604`).
const START_SCORE: f32 = 10_000_000.0;
const STRENGTH_SCALE: f32 = 1000.0;
const LOCATION_WEIGHT: f32 = 1.0;
const DEEP: i32 = 99_999;
const SHALLOW: i32 = 6;

type Search = unsafe extern "C" fn(usize, i32, usize, usize, *mut f32, *mut i32, *mut u8, *mut u8, *mut u8, u8, u8, *mut u8) -> i32;
static ORIGINAL_SEARCH: Function<Search> = Function::new();
type Think = unsafe extern "C" fn(usize, usize) -> usize;
static ORIGINAL_THINK: Function<Think> = Function::new();
type UnitToAgent = unsafe extern "C" fn(usize, usize) -> usize;
static ORIGINAL_UNIT_TO_AGENT: Function<UnitToAgent> = Function::new();
type MainArmy = unsafe extern "C" fn(usize) -> usize;
static MAIN_ARMY: Function<MainArmy> = Function::new();
type UnitFlag = unsafe extern "C" fn(usize) -> u8;
static UNIT_FLAG: Function<UnitFlag> = Function::new();
type Threat = unsafe extern "C" fn(usize, usize, usize, u8, u8) -> f32;
static THREAT: Function<Threat> = Function::new();
type SeaTarget = unsafe extern "C" fn(usize, usize, *mut u8, usize) -> u8;
static SEA_TARGET: Function<SeaTarget> = Function::new();
type Always = unsafe extern "C" fn(usize) -> u8;
static SEARCHES_EVERY_THINK: Function<Always> = Function::new();
type UnitAnswer = unsafe extern "C" fn(usize) -> u8;
static HIRED: Function<UnitAnswer> = Function::new();
static NAVY_OVER_SIZE: Function<UnitAnswer> = Function::new();
type Friend = unsafe extern "C" fn(usize, usize, usize) -> usize;
static FRIEND: Function<Friend> = Function::new();
type Destination = unsafe extern "C" fn(usize) -> usize;
static DESTINATION: Function<Destination> = Function::new();
type AtWar = unsafe extern "C" fn(usize, u8) -> i32;
static WARS_FOUGHT: Function<AtWar> = Function::new();
type RouteCheck = unsafe extern "C" fn(usize, usize, usize, usize) -> u8;
static ROUTE_CHECK: Function<RouteCheck> = Function::new();
type Strength = unsafe extern "C" fn(usize, *mut i32, u8) -> *const i32;
static STRENGTH: Function<Strength> = Function::new();
type Score =
    unsafe extern "C" fn(usize, usize, f32, usize, *mut i32, *mut u8, *mut u8, *mut u8, u8, f32, f32, f32, *mut u8, f32, *mut u8, *mut u8, usize, usize) -> f32;
static SCORE: Function<Score> = Function::new();
type DatePart = unsafe extern "C" fn(usize) -> i32;
static YEAR: Function<DatePart> = Function::new();
static DAY_OF_YEAR: Function<DatePart> = Function::new();

fn base() -> usize {
    crate::hooked_memory().base
}

unsafe fn read<T: Copy>(at: usize) -> T {
    unsafe { (at as *const T).read_unaligned() }
}

/// What the think gives the search.
#[derive(Clone, Copy, PartialEq)]
struct Call {
    depth: i32,
    unit: usize,
    argument: usize,
    score: f32,
    a: i32,
    b: u8,
    c: u8,
    d: u8,
    flag: u8,
    other: u8,
    out: [u8; 0x15],
}

/// What the search gives back.
#[derive(Clone, Copy)]
struct Outcome {
    province: i32,
    score: f32,
    a: i32,
    b: u8,
    c: u8,
    d: u8,
    out: [u8; 0x15],
}

impl Outcome {
    fn same(&self, other: &Outcome) -> bool {
        self.province == other.province
            && self.score.to_bits() == other.score.to_bits()
            && (self.a, self.b, self.c, self.d) == (other.a, other.b, other.c, other.d)
            && self.out == other.out
    }
}

const PENDING: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;
const CALLED_OFF: u8 = 3;

/// One army's search, worked out ahead.
struct Job {
    agent: usize,
    date: u32,
    call: Call,
    /// The agent's and its army's state when the search was prepared.
    state: u64,
    /// Held to read while the search runs; held to write while a unit is
    /// handed between the country's agents.
    gate: Arc<RwLock<()>>,
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
            unsafe { search_as_the_game(self.agent, &self.call, 0) }
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

thread_local! {
    static BATCH: RefCell<Option<Batch>> = const { RefCell::new(None) };
    /// This thread holds its batch's gate to write.
    static HANDING: Cell<bool> = const { Cell::new(false) };
}

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
    /// With `check-army-searches`, taken but not what the game's search gives.
    Different,
}

static COUNTS: [AtomicU64; 13] = [const { AtomicU64::new(0) }; 13];
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
            if checking() { format!(", {} differing from the game's", get(Different)) } else { String::new() }
        );
    }
}

/// A developer build's test switch `check-army-searches`: every search
/// taken is worked out again as the game does, and a difference counted.
fn checking() -> bool {
    #[cfg(feature = "dev-test-switches")]
    return crate::dev::options().test_switches.check_army_searches;
    #[cfg(not(feature = "dev-test-switches"))]
    false
}

/// A developer build's test switch `game-searches-armies`: the framework
/// leaves the searches to the game.
fn left_to_the_game() -> bool {
    #[cfg(feature = "dev-test-switches")]
    return crate::dev::options().test_switches.game_searches_armies;
    #[cfg(not(feature = "dev-test-switches"))]
    false
}

/// A hash of the state the search reads of the agent and its army, with its
/// country's war plan and the offensive its King follows.
unsafe fn state(agent: usize, unit: usize) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |at: usize, len: usize| {
        let bytes = unsafe { std::slice::from_raw_parts(at as *const u8, len) };
        for &b in bytes {
            hash = (hash ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    };
    for range in AGENT_STATE {
        mix(agent + range.start, range.len());
    }
    for range in UNIT_STATE {
        mix(unit + range.start, range.len());
    }
    let followed = unsafe { read::<usize>(read::<usize>(agent + KING) + FOLLOWED) };
    if followed != 0 {
        mix(followed + OFFENSIVE.start, OFFENSIVE.len());
    }
    mix(unsafe { country_of(agent) } + WAR_PLAN.start, WAR_PLAN.len());
    hash
}

fn today() -> u32 {
    unsafe { read::<u32>(read::<usize>(base() + WORLD) + DATE) }
}

/// The agent's country.
unsafe fn country_of(agent: usize) -> usize {
    unsafe {
        let countries = read::<usize>(read::<usize>(base() + COUNTRIES) + COUNTRY_LIST);
        read::<usize>(countries + 8 * ((read::<u64>(agent + COUNTRY) >> 32) as i16 as isize as usize))
    }
}

/// The search's arguments as the think `0xeab50` gives them
/// (`0xeb81f`..`0xebb5f`), worked out from the agent as it is: None when the
/// think would not search today, or would search in a battle, which is left
/// to the game.
unsafe fn prepare(agent: usize, argument: usize) -> Option<Call> {
    unsafe {
        let base = base();
        if read::<i32>(agent + UNITS) == 0 {
            return None;
        }
        let unit = MAIN_ARMY.get()(agent);
        if unit == 0 || read::<usize>(unit + UNIT_LOCATION) == 0 || read::<i32>(unit + UNIT_BATTLE) != 0 {
            return None;
        }
        // The think does not come to the search for an agent with a target
        // over the sea (`0xea250` sails it, `0xf1390`), or for an army
        // retreating or with `+0x218` (`0xeb2f8`..`0xeb31e`) or in a battle
        // (`0xeb63d`): of 140,000 searches prepared, none of these searched
        // (docs/prototype-log/2026-11-08-armies-searches-idle-threads.md).
        if read::<i32>(agent + SEA_TARGET_PROVINCE) > 0
            || read::<usize>(unit + UNIT_IN_BATTLE) != 0
            || read::<u8>(unit + UNIT_RETREATING) != 0
            || read::<u8>(unit + UNIT_RETREATING + 1) != 0
            || read::<usize>(unit + UNIT_STOPS) != 0
        {
            return None;
        }
        let country = country_of(agent);
        // `0xeae58`..`0xeae8b`: out of a battle, the think ends at once for
        // an army locked in its move or with the unit's flag; `0xeb3c5`: the
        // threat where it is.
        let flag = UNIT_FLAG.get()(unit);
        if flag != 0 || read::<u8>(unit + UNIT_LOCKED) != 0 {
            return None;
        }
        // `0xeb475`..`0xeb572`: an army at peace (its country in no war,
        // not hired, not a navy over its size) goes on to the search only
        // on one think in ten at random, or once its thinks without one
        // (`+0xbf0`) reach 15. Its searches are left to the think, rather
        // than worked out nine times in ten for nothing.
        if read::<u8>(country + AT_WAR_FLAG) == 0
            && WARS_FOUGHT.get()(country, 1) <= 0
            && HIRED.get()(unit) == 0
            && NAVY_OVER_SIZE.get()(agent) == 0
            && read::<i32>(agent + THINKS_WITHOUT_SEARCH) < PEACEFUL_THINKS
        {
            return None;
        }
        let at_peace = read::<u8>(country + AT_WAR_FLAG) == 0 && read::<i32>(country + WARS) == 0;
        let here = THREAT.get()(0, unit, read::<usize>(unit + UNIT_LOCATION), flag, at_peace as u8);
        // `0xeb6e5`..`0xeb711`: whether it has a sea target.
        let mut sea = 0u8;
        let mut sea_target = (read::<u8>(country + NO_SEA_TARGETS) == 0 && SEA_TARGET.get()(agent, unit, &mut sea, argument) != 0) as u8;
        // `0xeb749`..`0xeb7a8`: for most countries one day in three.
        let world = read::<usize>(base + WORLD);
        let days = YEAR.get()(world + DATE) * 365 + DAY_OF_YEAR.get()(world + DATE);
        if SEARCHES_EVERY_THINK.get()(agent) == 0 && days % 3 != read::<i16>(agent + COUNTRY_INDEX) as i32 % 3 {
            return None;
        }
        // `0xeb64b`..`0xeb692`: an army with a friendly army in its province
        // to follow, not locked in its move, ends its think there, following
        // it on one think in three.
        let friend = FRIEND.get()(agent, unit, read::<usize>(unit + UNIT_LOCATION));
        if friend != 0 {
            let locked: unsafe extern "C" fn(usize) -> u8 = std::mem::transmute(read::<usize>(read::<usize>(friend) + LOCKED_IN_MOVE));
            if locked(friend) == 0 {
                return None;
            }
        }
        // `0xeb852`..`0xeb993`: where it goes, the threat there, the route,
        // its strength, and the score of where it goes.
        let destination = match DESTINATION.get()(unit) {
            0 => read::<usize>(unit + UNIT_LOCATION),
            d => d,
        };
        let not_at_war = read::<u8>(country + AT_WAR_FLAG) == 0 && WARS_FOUGHT.get()(country, 0) == 0;
        let threat = THREAT.get()(0, unit, destination, flag, not_at_war as u8);
        // `0xeb8e3`..`0xeb8ea`, `0xebaa2`: with no route, it searches deep
        // without the score of where it goes.
        if ROUTE_CHECK.get()(agent, unit, destination, argument) == 0 {
            return Some(Call { depth: DEEP, unit, argument, score: START_SCORE, a: 0, b: 0, c: 0, d: 0, flag: 1, other: 0, out: [0; 0x15] });
        }
        let mut strength = 0i32;
        let strength = *STRENGTH.get()(unit, &mut strength, 0) as f32 / STRENGTH_SCALE;
        let (mut a, mut seen_enemy, mut e, mut f, mut g, mut stays) = (0i32, 0u8, 0u8, 0u8, 0u8, 0u8);
        SCORE.get()(
            agent,
            unit,
            strength,
            destination,
            &mut a,
            &mut seen_enemy,
            &mut e,
            &mut f,
            1,
            threat,
            threat,
            here,
            &mut g,
            LOCATION_WEIGHT,
            &mut sea_target,
            &mut stays,
            0,
            argument,
        );
        // `0xeb9a0`..`0xeba9d`: shallow on three days of four out of a
        // battle, when nothing threatens it where it is.
        let mut depth = DEEP;
        if here <= 0.0 && stays == 0 && (days + read::<i32>(unit + UNIT_DAYS)) & 3 != 0 {
            depth = SHALLOW;
        }
        Some(Call { depth, unit, argument, score: START_SCORE, a, b: 0, c: 0, d: 0, flag: (seen_enemy == 0 && stays == 0) as u8, other: stays, out: [0; 0x15] })
    }
}

/// The game's search with these arguments, its think's argument, the
/// country's access cache, given as `cache`: the country's own on the
/// country's thread, none on the pool's, where the country's other agents
/// would use it at the same time.
unsafe fn search_as_the_game(agent: usize, call: &Call, cache: usize) -> Outcome {
    let callback = [base() + CALLBACK, agent];
    let mut call = *call;
    let province = unsafe {
        ORIGINAL_SEARCH.get()(
            callback.as_ptr() as usize,
            call.depth,
            call.unit,
            cache,
            &mut call.score,
            &mut call.a,
            &mut call.b,
            &mut call.c,
            &mut call.d,
            call.flag,
            call.other,
            call.out.as_mut_ptr(),
        )
    };
    Outcome { province, score: call.score, a: call.a, b: call.b, c: call.c, d: call.d, out: call.out }
}

/// Once the King has thought: its army agents' searches of today, handed
/// to the pool. The developer build's `agents-order` calls it from its own
/// walk.
pub(crate) extern "C" fn children_begin(king: usize, argument: usize) {
    if let Some(old) = BATCH.with(|b| b.borrow_mut().take()) {
        settle(old);
    }
    if left_to_the_game() {
        return;
    }
    let Some(pool) = pool() else { return };
    let began = std::time::Instant::now();
    let base = base();
    let date = today();
    let gate = Arc::new(RwLock::new(()));
    let mut jobs = Vec::new();
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
        let Some(call) = (unsafe { prepare(agent, argument) }) else { continue };
        let state = unsafe { state(agent, call.unit) };
        jobs.push(Arc::new(Job {
            agent,
            date,
            call,
            state,
            gate: gate.clone(),
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
    pool.queue.lock().unwrap_or_else(|e| e.into_inner()).extend(jobs.iter().cloned());
    pool.ready.notify_all();
    BATCH.with(|b| *b.borrow_mut() = Some(Batch { jobs, gate }));
}

/// After the King's army agents' day: the searches not taken called off,
/// those under way waited for.
extern "C" fn armies_end() {
    if let Some(batch) = BATCH.with(|b| b.borrow_mut().take()) {
        settle(batch);
    }
}

fn settle(batch: Batch) {
    for job in &batch.jobs {
        let thought = job.thought.load(Ordering::Relaxed);
        add(if thought { Count::NotSearched } else { Count::NotThought }, 1);
        job.settle();
    }
}

/// The army's search worked out ahead, if any.
fn job_of(agent: usize) -> Option<Arc<Job>> {
    BATCH.with(|b| b.borrow().as_ref()?.jobs.iter().find(|j| j.agent == agent).cloned())
}

/// The army agent's think: its own search settled first, so that its think
/// and its search never use its region scores at once.
unsafe extern "C" fn think(agent: usize, argument: usize) -> usize {
    if let Some(job) = job_of(agent) {
        job.thought.store(true, Ordering::Relaxed);
        match job.progress.load(Ordering::Acquire) {
            DONE => add(Count::Ready, 1),
            PENDING => add(Count::NotBegun, 1),
            _ => {
                let began = std::time::Instant::now();
                job.settle();
                add(Count::WaitedMicros, began.elapsed().as_micros() as u64);
                add(Count::UnderWay, 1)
            }
        };
        job.settle();
    }
    unsafe { ORIGINAL_THINK.get()(agent, argument) }
}

/// A unit handed to an agent: no search of the country runs meanwhile.
unsafe extern "C" fn unit_to_agent(agent: usize, unit: usize) -> usize {
    let gate = if HANDING.with(Cell::get) { None } else { BATCH.with(|b| b.borrow().as_ref().map(|b| b.gate.clone())) };
    let Some(gate) = gate else { return unsafe { ORIGINAL_UNIT_TO_AGENT.get()(agent, unit) } };
    let _writing = gate.write().unwrap_or_else(|e| e.into_inner());
    HANDING.with(|h| h.set(true));
    let result = unsafe { ORIGINAL_UNIT_TO_AGENT.get()(agent, unit) };
    HANDING.with(|h| h.set(false));
    result
}

/// The framework's search: the one worked out ahead when it matches.
unsafe extern "C" fn search(
    callback: usize,
    depth: i32,
    unit: usize,
    argument: usize,
    score: *mut f32,
    a: *mut i32,
    b: *mut u8,
    c: *mut u8,
    d: *mut u8,
    flag: u8,
    other: u8,
    out: *mut u8,
) -> i32 {
    let original = || unsafe { ORIGINAL_SEARCH.get()(callback, depth, unit, argument, score, a, b, c, d, flag, other, out) };
    let agent = unsafe { read::<usize>(callback + 8) };
    let job = BATCH.with(|batch| {
        let mut batch = batch.borrow_mut();
        let jobs = &mut batch.as_mut()?.jobs;
        let at = jobs.iter().position(|j| j.agent == agent)?;
        Some(jobs.swap_remove(at))
    });
    let Some(job) = job else {
        if BATCH.with(|b| b.borrow().is_some()) {
            add(Count::Unprepared, 1);
        }
        return original();
    };
    let given = unsafe {
        let mut given = Call { depth, unit, argument, score: *score, a: *a, b: *b, c: *c, d: *d, flag, other, out: [0; 0x15] };
        std::ptr::copy_nonoverlapping(out, given.out.as_mut_ptr(), given.out.len());
        given
    };
    let outcome = job.settle();
    if outcome.is_some() && (job.date != today() || given != job.call) {
        add(Count::OtherArguments, 1);
        return original();
    }
    if outcome.is_some() && unsafe { state(agent, unit) } != job.state {
        add(Count::OtherState, 1);
        return original();
    }
    // Called off at the think, not begun, and counted there.
    let Some(outcome) = outcome else { return original() };
    if checking() && !unsafe { search_as_the_game(agent, &given, argument) }.same(&outcome) {
        add(Count::Different, 1);
    }
    unsafe {
        *score = outcome.score;
        *a = outcome.a;
        *b = outcome.b;
        *c = outcome.c;
        *d = outcome.d;
        std::ptr::copy_nonoverlapping(outcome.out.as_ptr(), out, outcome.out.len());
    }
    add(Count::Taken, 1);
    outcome.province
}

crate::resume!(CHILDREN_RESUME: CHILDREN_ON = CHILDREN_WALK_ON, NO_CHILDREN_ON = ARMIES_WALK);
crate::resume!(WALKED_RESUME: WALKED_ON = ARMIES_WALKED_ON);

/// Replaces `mov rbx, [rdi + 0x40]; test rbx, rbx; je` with the searches
/// handed out first.
#[unsafe(naked)]
extern "C" fn children_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, rdi", "mov rdx, [rbp]", call children_begin),
        "mov rbx, [rdi + {children}]",
        "test rbx, rbx",
        "jnz 2f",
        "jmp qword ptr [rip + {none}]",
        "2:",
        "jmp qword ptr [rip + {on}]",
        children = const CHILDREN,
        children_begin = sym children_begin,
        none = sym NO_CHILDREN_ON,
        on = sym CHILDREN_ON,
    );
}

/// Replaces `mov edx, esi; mov rcx, rdi` with the searches settled first.
#[unsafe(naked)]
extern "C" fn walked_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call armies_end),
        "mov edx, esi",
        "mov rcx, rdi",
        "jmp qword ptr [rip + {on}]",
        armies_end = sym armies_end,
        on = sym WALKED_ON,
    );
}

/// Whether the developer build's `agents-order` walks the King's agents, and
/// calls [`children_begin`] itself.
fn walked_by_agents_order() -> bool {
    #[cfg(feature = "dev-test-switches")]
    return crate::dev::options().test_switches.agents_order != crate::dev::AgentsOrder::Game;
    #[cfg(not(feature = "dev-test-switches"))]
    false
}

pub fn points() -> Vec<Point> {
    let mut points = vec![
        Point::replace(SEARCH, "4c 89 4c 24 20 89 54 24 10 48 89 4c 24 08 55 53", "army search", search as Search, &ORIGINAL_SEARCH),
        Point::method(CALLBACK, 0, 0x103200, "army search's callback"),
        Point::method(CALLBACK, 8, 0x96f860, "army search's callback's step"),
        Point::replace(ARMY_THINK, "40 53 48 83 ec 20 48 8b d9 c6 81 28 0b 00 00 01", "army agent's think", think as Think, &ORIGINAL_THINK),
        Point::replace(
            UNIT_TO_AGENT,
            "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 41 56",
            "unit handed to an agent",
            unit_to_agent as UnitToAgent,
            &ORIGINAL_UNIT_TO_AGENT,
        ),
        Point::keep(CHILDREN_WALK_ON, "0f 1f 44 00 00", "King's walk of its children goes on"),
        Point::keep(ARMIES_WALK, "48 8b 9f 50 0c 00 00", "King's walk of its army agents").uses(&[ARMIES]),
        Point::jump(ARMIES_WALKED, "8b d6 48 8b cf", "King's day after its army agents", walked_stub as *const () as usize, &WALKED_RESUME),
        Point::keep(ARMIES_WALKED_ON, "e8 c1 50 00 00", "King's missions after its army agents"),
        // The think's search, as `prepare` gives it: the day's gate, the
        // strength, the score of where it goes, the depth and the call.
        Point::keep(
            0xeb77cusize,
            "0f bf 4f 2c b8 56 55 55 55 f7 eb 8b c2 c1 e8 1f 03 d0 8d 04 52 2b d8 b8 56 55 55 55 f7 e9 8b c2 c1 e8 1f 03 d0 8d 04 52 2b c8 3b d9 0f 85",
            "army think's day of search",
        )
        .uses(&[COUNTRY_INDEX]),
        Point::keep(0xeb8f0usize, "45 33 c0 48 8d 55 00 48 8b 4d 80 e8 60 07 c6 00 66 0f 6e 10 0f 5b d2 f3 41 0f 5e d0", "army think's strength"),
        Point::keep(0xeb989usize, "4d 8b ce 48 8b 55 80 48 8b cf e8 08 d5 00 00", "army think's score of where it goes"),
        Point::keep(
            0xeba62usize,
            "4c 8b 15 0f 44 25 02 49 8d 8a d0 1d 00 00 e8 fb 42 fe ff 69 d8 6d 01 00 00 49 8d 8a d0 1d 00 00 e8 89 43 fe ff 03 d8 8b 46 24 03 c3 a8 03 41 8b c4 41 bc 06 00 00 00 41 0f 45 c4 44 8b e0",
            "army think's search depth",
        )
        .uses(&[WORLD, DATE, UNIT_DAYS]),
        Point::keep(0xebb52usize, "4d 8b cd 4c 8b c6 41 8b d4 48 8d 4d d8 e8 4c 67 01 00", "army think's search"),
        Point::keep(0x1dc19a8usize, "80 96 18 4b", "army search's starting score"),
        Point::keep(0x1dc1938usize, "00 00 7a 44", "army strength's scale"),
        Point::keep(0x1dc1604usize, "00 00 80 3f", "army location score's weight"),
        Point::keep(0x107ec0usize, "83 b9 b8 0a 00 00 00 4c 8b c9 7e 56", "army agent's main army").recorded(MAIN_ARMY.slot()),
        Point::keep(0xd4cbc0usize, "48 89 5c 24 10 48 89 74 24 18 57 48", "unit's flag for the threat").recorded(UNIT_FLAG.slot()),
        Point::keep(0x102140usize, "48 89 5c 24 10 48 89 4c 24 08 55 56", "army threat at a province").recorded(THREAT.slot()),
        Point::keep(0x1034f0usize, "4c 89 4c 24 20 4c 89 44 24 18 48 89", "army agent's sea target").recorded(SEA_TARGET.slot()),
        Point::keep(0x10bdb0usize, "48 89 5c 24 08 48 89 6c 24 18 48 89", "army searches every think").recorded(SEARCHES_EVERY_THINK.slot()),
        Point::keep(
            0xeb475usize,
            "41 80 bf 9b 28 00 00 00 74 04 b3 1a eb 31 b2 01 49 8b cf e8 93 e9 26 00 85 c0 7e 04 b3 24 eb 1f 48 8b 4d 80 e8 62 d6 0b 00 84 c0 74 04 b3 0f eb 0e 48 8b cf e8 02 23 00 00 84 c0 74 20",
            "army think's war, hire and size",
        )
        .uses(&[AT_WAR_FLAG]),
        Point::keep(
            0xeb538usize,
            "48 8b cf e8 90 aa ff ff 44 8b c0 b8 67 66 66 66 41 f7 e8 c1 fa 02 8b ca c1 e9 1f 03 d1 8d 0c 92 03 c9 44 3b c1 74 18 8b 87 f0 0b 00 00 83 f8 0f 7d 0d",
            "army think's one think in ten at peace",
        )
        .uses(&[THINKS_WITHOUT_SEARCH]),
        Point::keep(0xeb8e3usize, "e8 08 7a 01 00 84 c0 0f 84 b2 01 00 00", "army think's route check"),
        Point::keep(0x1a8b00usize, "40 57 48 83 ec 20 80 3d 2f 74 19 02 00 48 8b f9", "unit hired").recorded(HIRED.slot()),
        Point::keep(0xed7b0usize, "48 83 ec 28 80 b9 60 0b 00 00 00 4c 8b d9 74 25", "army agent's navy over its size").recorded(NAVY_OVER_SIZE.slot()),
        Point::keep(0xf70d0usize, "48 8b c4 4c 89 40 18 48 89 50 10 48 89 48 08 55", "army's friend to follow").recorded(FRIEND.slot()),
        Point::keep(
            0xeb64busize,
            "4c 8b 82 e8 00 00 00 48 8b cf e8 76 ba 00 00 48 8b d8 48 85 c0 74 65 48 8b 10 48 8b c8 ff 92 18 01 00 00 84 c0 75 55",
            "army think's friend to follow",
        )
        .uses(&[UNIT_LOCATION, LOCKED_IN_MOVE]),
        Point::keep(
            0xeb2f8usize,
            "80 b9 03 02 00 00 00 0f 85 21 10 00 00 e8 96 ec c6 00 84 c0 0f 85 14 10 00 00 48 8b 55 80 48 83 ba 18 02 00 00 00 0f 85 02 10 00 00",
            "army think's end for a retreating army",
        )
        .uses(&[UNIT_RETREATING, UNIT_STOPS]),
        Point::keep(0xd511a0usize, "83 b9 10 01 00 00 00 7e 16 48 8b 81", "unit's destination").recorded(DESTINATION.slot()),
        Point::keep(0x359e20usize, "40 57 48 83 ec 20 8b b9 88 19 00 00", "country's wars").recorded(WARS_FOUGHT.slot()),
        Point::keep(0x1032f0usize, "48 89 5c 24 08 48 89 74 24 18 48 89", "army route check").recorded(ROUTE_CHECK.slot()),
        Point::keep(0xd4c060usize, "48 89 5c 24 18 55 56 57 48 83 ec 20", "unit's strength").recorded(STRENGTH.slot()),
        Point::keep(0xf8ea0usize, "48 89 5c 24 18 4c 89 4c 24 20 48 89", "army score of a province").recorded(SCORE.slot()),
        Point::keep(crate::wording::dates::YEAR, "44 8b 01 41 b9 01 00 00 00 41 81 e8", "date's year").recorded(YEAR.slot()),
        Point::keep(0xcfe10usize, "48 83 ec 28 44 8b 11 41 81 ea c0 55", "date's day of the year").recorded(DAY_OF_YEAR.slot()),
    ];
    if !walked_by_agents_order() {
        points.push(
            Point::jump(CHILDREN_WALK, "48 8b 5f 40 48 85 db 74 1a", "King's walk of its children", children_stub as *const () as usize, &CHILDREN_RESUME)
                .uses(&[CHILDREN]),
        );
    }
    points
}
