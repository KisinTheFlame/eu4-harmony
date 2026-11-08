//! What the AI's countries draw at once on several threads, in vanilla too:
//! the world's random generator (docs/research/random.md, ADR
//! random-draws-may-change-hands).
//!
//! In the AI stage every AI country's task (`0xb0fa0`) runs on one of the
//! game's workers or the main thread, and its agents draw from the world's
//! one generator (a Mersenne twister at `world+0xb90`, stepped by
//! `0x14c71b0`), with no lock: which numbers a country gets depends on the
//! threads' timing, and two threads stepping it at once can hand them one
//! number or tear its 624 words. A game started with `-seed` gives each agent
//! a generator of its own instead and runs the stage on the main thread, so
//! it draws nothing from the world's.
//!
//! So each AI country's task draws from a stream of its own: as the stage
//! begins, on the main thread, one number is drawn from the world's
//! generator the game's way, and each country's stream is seeded from it and
//! the country's index; while a task runs, the step of the world's generator
//! answers from that task's stream on its thread. A thread that draws from it
//! outside any task, but the main thread, draws from a generator of its own.
//! Every other generator, and the world's on the main thread outside the
//! stage, are stepped as the game steps them. With `-seed`, and in a
//! developer build with the test switch `game-shares-ai-draws`, every draw is
//! the game's.
use crate::called_by_game;
use crate::hooks::Call;
use crate::objects::campaign::globals;
use crate::points::{Function, Point};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// The generator's step, `(generator) -> u32`, the next tempered number.
const STEP: usize = 0x14c71b0;
/// The AI's phase of the day, on the main thread: it runs the stage.
const AI_PHASE: usize = 0x75b4b0;
/// An AI country's task, `(AI)`: the country's tag at `+0x20` of its AI.
const COUNTRY_TASK: usize = 0xb0fa0;
const AI_TAG: usize = 0x20;
/// The world's generator, inside the world.
const WORLD_GENERATOR: usize = 0xb90;
/// Set by `-seed`: agents draw from their own generators, and the stage runs
/// on the main thread.
const SEEDED: usize = 0x23581c0;

type Step = unsafe extern "C" fn(usize) -> u32;
static ORIGINAL_STEP: Function<Step> = Function::new();

/// Where the pointer to the world is, once the first stage has begun.
static WORLD_AT: AtomicUsize = AtomicUsize::new(0);
/// Streams are given for the stage under way, from this number.
static ACTIVE: AtomicBool = AtomicBool::new(false);
static STAGE_SEED: AtomicU32 = AtomicU32::new(0);
/// Every draw is the game's: `-seed`, or the test switch.
static GAME_DRAWS: AtomicBool = AtomicBool::new(false);

/// What was drawn, for the report and a developer build's log.
static STAGES: AtomicU64 = AtomicU64::new(0);
static TASKS: AtomicU64 = AtomicU64::new(0);
static STREAM_DRAWS: AtomicU64 = AtomicU64::new(0);
static OWN_DRAWS: AtomicU64 = AtomicU64::new(0);

/// A small generator of 32-bit numbers (PCG32, XSH RR).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stream {
    state: u64,
    increment: u64,
}

/// SplitMix64's step: a 64-bit number mixed into another.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Stream {
    /// The stream of object `id` in the work whose number is `seed`.
    fn of(seed: u32, id: u64) -> Self {
        let key = mix(u64::from(seed) << 32 ^ id);
        let mut stream = Self { state: 0, increment: mix(key) << 1 | 1 };
        stream.next();
        stream.state = stream.state.wrapping_add(key);
        stream.next();
        stream
    }

    /// A number as the game's step gives one: below 2^31, which its callers
    /// take as a non-negative `i32` (its tempering ends in an arithmetic
    /// shift, `sar eax, 0x12; xor eax, r8d`, so its top bit is always 0).
    fn draw(&mut self) -> u32 {
        self.next() >> 1
    }

    fn next(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(self.increment);
        let shifted = (((old >> 18) ^ old) >> 27) as u32;
        shifted.rotate_right((old >> 59) as u32)
    }
}

thread_local! {
    /// The streams of the tasks this thread is in, the innermost last; None
    /// for a task that draws as the game does.
    static TASK_STREAMS: RefCell<Vec<Option<Stream>>> = const { RefCell::new(Vec::new()) };
    /// This thread's draws from its task's stream, until the task ends.
    static TASK_DRAWS: Cell<u64> = const { Cell::new(0) };
    /// The generator of a thread drawing outside any task.
    static OWN: Cell<Option<Stream>> = const { Cell::new(None) };
}

/// The world's generator's step from the stream of the task this thread
/// runs, or this thread's own outside one but on the main thread; None to
/// step it as the game does.
fn drawn_apart() -> Option<u32> {
    let from_task = TASK_STREAMS
        .try_with(|streams| match streams.borrow_mut().last_mut() {
            Some(Some(stream)) => Some(Some(stream.draw())),
            Some(None) => Some(None),
            None => None,
        })
        .ok()
        .flatten();
    match from_task {
        Some(Some(n)) => {
            let _ = TASK_DRAWS.try_with(|d| d.set(d.get() + 1));
            Some(n)
        }
        Some(None) => None,
        None if crate::thread_id() == crate::main_thread() => None,
        None => OWN
            .try_with(|own| {
                let mut stream = own.get().unwrap_or_else(|| Stream::of(STAGE_SEED.load(Ordering::Relaxed), u64::from(crate::thread_id()) | 1 << 48));
                let n = stream.draw();
                own.set(Some(stream));
                OWN_DRAWS.fetch_add(1, Ordering::Relaxed);
                n
            })
            .ok(),
    }
}

#[called_by_game]
unsafe extern "C" fn step(generator: usize) -> u32 {
    let world_at = WORLD_AT.load(Ordering::Relaxed);
    if world_at != 0 && !GAME_DRAWS.load(Ordering::Relaxed) {
        let world = unsafe { *(world_at as *const usize) };
        if world != 0 && generator == world + WORLD_GENERATOR {
            if let Some(n) = drawn_apart() {
                return n;
            }
        }
    }
    unsafe { ORIGINAL_STEP.get()(generator) }
}

/// The AI's phase begins, on the main thread: the stage's number drawn.
fn phase_begins(call: &mut Call) {
    let m = call.memory();
    let world_at = m.at(globals::WORLD);
    let game_draws = game_shares_ai_draws() || m.u8(m.at(SEEDED)).unwrap_or(0) != 0;
    GAME_DRAWS.store(game_draws, Ordering::Relaxed);
    let world = m.ptr(world_at).unwrap_or(0);
    if game_draws || world == 0 {
        ACTIVE.store(false, Ordering::Release);
        return;
    }
    let seed = unsafe { ORIGINAL_STEP.get()(world + WORLD_GENERATOR) };
    STAGE_SEED.store(seed, Ordering::Relaxed);
    WORLD_AT.store(world_at, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Release);
}

/// The phase is over: every task of the stage has ended.
fn phase_ended(_: &mut Call) {
    if !ACTIVE.swap(false, Ordering::AcqRel) {
        return;
    }
    let stages = STAGES.fetch_add(1, Ordering::Relaxed) + 1;
    #[cfg(dev_build)]
    if stages.is_multiple_of(100) {
        log!("repair: the AI's countries' draws: {}", counted());
    }
    #[cfg(not(dev_build))]
    let _ = stages;
}

/// An AI country's task begins: its stream, from the stage's number and the
/// country's index.
fn task_begins(call: &mut Call) {
    let stream = ACTIVE.load(Ordering::Acquire).then(|| {
        let tag = call.memory().u64(call.arg(0) + AI_TAG).unwrap_or(0);
        Stream::of(STAGE_SEED.load(Ordering::Relaxed), (tag >> 32) as u16 as u64)
    });
    let _ = TASK_STREAMS.try_with(|streams| streams.borrow_mut().push(stream));
}

/// The task has ended, or faulted: its stream goes, its draws counted.
fn task_ended(_: &mut Call) {
    let had = TASK_STREAMS.try_with(|streams| streams.borrow_mut().pop()).ok().flatten();
    if let Some(Some(_)) = had {
        TASKS.fetch_add(1, Ordering::Relaxed);
        let draws = TASK_DRAWS.try_with(|d| d.replace(0)).unwrap_or(0);
        STREAM_DRAWS.fetch_add(draws, Ordering::Relaxed);
    }
}

/// What was drawn since the counts last started.
fn counted() -> String {
    let (stages, tasks) = (STAGES.load(Ordering::Relaxed), TASKS.load(Ordering::Relaxed));
    let (streams, own) = (STREAM_DRAWS.load(Ordering::Relaxed), OWN_DRAWS.load(Ordering::Relaxed));
    format!(
        "{stages} AI stages, {tasks} countries' tasks drew {streams} numbers from streams of their own ({:.0} a stage), {own} drawn outside a task from a thread's own",
        streams as f64 / stages.max(1) as f64
    )
}

/// What it drew since the last world reset; the counts start again.
fn since_reset() -> String {
    let said = counted();
    for count in [&STAGES, &TASKS, &STREAM_DRAWS, &OWN_DRAWS] {
        count.store(0, Ordering::Relaxed);
    }
    said
}

/// A developer build's test switch `game-shares-ai-draws`: every AI country
/// draws from the world's one generator as the game's do, so that the repair
/// can be measured against it.
fn game_shares_ai_draws() -> bool {
    test_switch!(game_shares_ai_draws)
}

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset), ..super::Repair::NONE };

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::replace(STEP, "48 89 5c 24 08 8b 91 c0 09 00 00 48 8b d9 85 d2", "random generator stepped", step as Step, &ORIGINAL_STEP),
        Point::observe(
            AI_PHASE,
            "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 10 f0 ff ff",
            "AI's phase of the day",
            1,
            Some(phase_begins),
            Some(phase_ended),
        ),
        Point::observe(COUNTRY_TASK, "40 55 53 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 f8", "AI country's task", 1, Some(task_begins), Some(task_ended)),
        // An agent draws from its own generator with `-seed`, else from the
        // world's: `cmp [0x23581c0], 0; je`, `lea rcx, [rbp + 0x78]`, then
        // `mov rcx, [world]; add rcx, 0xb90; call 0x14c71b0`.
        Point::keep(
            0xe9fd3usize,
            "80 3d e6 e1 26 02 00 74 06 48 8d 4d 78 eb 0e 48 8b 0d 8f 5e 25 02 48 81 c1 90 0b 00 00 e8",
            "army agent draws from the world's or its own generator",
        )
        .uses(&[SEEDED, globals::WORLD, WORLD_GENERATOR]),
        // `mov r8, [rsi + 0x20]; mov r9, r8; shr r9, 0x20`: the task reads its
        // country's tag, and of it the index.
        Point::keep(0xb1028usize, "4c 8b 46 20 4d 8b c8 49 c1 e9 20", "AI country's task reads its tag").uses(&[AI_TAG]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_is_its_seed_and_its_object() {
        let draws = |seed, id| {
            let mut s = Stream::of(seed, id);
            (0..64).map(|_| s.next()).collect::<Vec<_>>()
        };
        assert_eq!(draws(7, 33), draws(7, 33));
        assert_ne!(draws(7, 33), draws(7, 34));
        assert_ne!(draws(7, 33), draws(8, 33));
        assert_ne!(draws(7, 33)[..8], draws(7, 33)[8..16]);
    }

    #[test]
    fn a_stream_draws_evenly() {
        // Of a million numbers from many objects' streams, each of 16 ranges
        // and each bit gets its share within a few per mille.
        let (mut ranges, mut bits) = ([0u32; 16], [0u32; 32]);
        let n = 1 << 20;
        for id in 0..256 {
            let mut s = Stream::of(0x1234_5678, id);
            for _ in 0..n / 256 {
                let x = s.next();
                ranges[(x >> 28) as usize] += 1;
                for (b, count) in bits.iter_mut().enumerate() {
                    *count += x >> b & 1;
                }
            }
        }
        for count in ranges {
            assert!((f64::from(count) / f64::from(n / 16) - 1.0).abs() < 0.02, "{ranges:?}");
        }
        for count in bits {
            assert!((f64::from(count) / f64::from(n / 2) - 1.0).abs() < 0.01, "{bits:?}");
        }
    }

    #[test]
    fn a_draw_is_never_negative_as_the_games_are() {
        let mut s = Stream::of(1, 2);
        let draws: Vec<u32> = (0..100_000).map(|_| s.draw()).collect();
        assert!(draws.iter().all(|&x| (x as i32) >= 0));
        // And it reaches near the top of its range, as the game's does.
        assert!(draws.iter().any(|&x| x > 0x7ff0_0000));
    }

    #[test]
    fn mix_is_splitmix64() {
        // SplitMix64's first outputs from the seed 0.
        let mut state = 0u64;
        let mut next = || {
            let z = mix(state);
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            z
        };
        assert_eq!(next(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(next(), 0x6e78_9e6a_a1b9_65f4);
    }
}
