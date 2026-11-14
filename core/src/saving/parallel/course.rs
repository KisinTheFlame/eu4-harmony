//! The parallel save's course: the provinces and the countries handed to the
//! pool at their loops, the writer the game writes on with, and the join
//! where the world has written its game state (`super`).
//!
//! The hooks tell the course their moments — the provinces' loop, the
//! countries' loop, the end of the game's own loop when it is checked, and
//! the join — and the course decides what the pool writes, what goes into the
//! game's stream and where, and when the save is not what the game would have
//! written and the game ends. The game's stream, writer and pool are reached
//! through [`Game`]: the real one in [`Real`], and a recorder in the tests
//! that says what each run wrote and what fails.
use super::{append, comparing, pool, position, read, splice, Ahead, Loop, Task, Writing, LAST_RUN, SPEED_UP, WRITER_SIZE};
use crate::speed;
use crate::Result;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The game's save writing, as the course reaches it: a loop prepared to be
/// written, the pool that writes it, and the stream its bytes go into.
pub(super) trait Game {
    /// A loop prepared to be written, which the course keeps until the join.
    type Writing;

    /// The loop to write, or None when it is better left to the game; Err
    /// when it cannot be written beside the game.
    fn prepare(&mut self, which: Loop, world: usize, writer: usize) -> Result<Option<Self::Writing>>;
    /// Where the loop's stream stands: where its bytes go.
    fn at(&mut self, writing: &Self::Writing) -> Result<u32>;
    /// Whether the writer is the one the loop was prepared for: its vtable,
    /// depth, stream and form, as the runs start from.
    fn same_writer(&mut self, writing: &Self::Writing, writer: usize) -> bool;
    /// Hands both loops to the pool, which writes them while the game writes
    /// on; false when there is no pool or it is busy.
    fn ahead(&mut self, loops: &[Self::Writing; 2]) -> bool;
    /// Takes the runs left and waits for the pool's threads; how many threads
    /// worked on the loops.
    fn gather(&mut self, loops: &[Self::Writing; 2]) -> usize;
    /// Writes one loop now, on the pool's threads and this one; how many
    /// worked, or None when there is no pool or it is busy.
    fn alone(&mut self, writing: &Self::Writing) -> Option<usize>;
    /// Every run's bytes, in order, once every thread is done.
    fn runs(&mut self, writing: &Self::Writing) -> Result<Vec<Vec<u8>>>;
    /// The whole loop written again on this thread, when a run was not.
    fn whole(&mut self, writing: &Self::Writing) -> Result<Vec<u8>>;
    /// How many items the loop writes.
    fn items(&self, writing: &Self::Writing) -> usize;
    /// The runs' bytes into the stream, in their order, after what it holds.
    fn append(&mut self, writing: &Self::Writing, runs: &[Vec<u8>]) -> Result<()>;
    /// The runs' bytes into the stream at the places given, in order, and
    /// after them what it held from each place on.
    fn splice(&mut self, writing: &Self::Writing, inserts: &[(u32, Vec<Vec<u8>>)]) -> Result<()>;
    /// The bytes the game's own loop wrote, from one place to another.
    fn written(&mut self, writing: &Self::Writing, from: u32, to: u32) -> Result<Vec<u8>>;
    /// The largest run of a loop, the next save's first guess at its buffer.
    fn remember(&mut self, which: Loop, largest: usize);
    /// Whether the game's own loops write as well, to be compared with.
    fn comparing(&self) -> bool;
    /// A loop's bytes were compared with the game's: whether they agreed, and
    /// what differed when they did not.
    fn compared(&mut self, same: bool, detail: String);
    /// What the course did, for the log.
    fn note(&mut self, message: String);
    /// The save is not what the game would have written: the game ends.
    fn end(&mut self, why: String);
}

/// A world write whose loops the pool writes: the writer they were prepared
/// for, where each loop's bytes go, and when it began.
struct InFlight<W> {
    writer: usize,
    loops: [W; 2],
    /// Where each loop's bytes go: where the stream stood when the loop
    /// began, once it was reached and left to the pool.
    at: [Option<u32>; 2],
    /// Checked, where the stream stood when the game's own loop ended.
    ended: [Option<u32>; 2],
    began: Instant,
}

/// The save in flight, if any, and what the hooks' moments make of it.
pub(super) struct Course<G: Game> {
    in_flight: Option<InFlight<G::Writing>>,
}

impl<G: Game> Course<G> {
    pub(super) const fn new() -> Self {
        Self { in_flight: None }
    }

    /// At the provinces' loop: both loops go to the pool and the game writes
    /// on past them; or the game writes them itself, as the return says.
    fn provinces(&mut self, g: &mut G, world: usize, writer: usize) -> bool {
        let began = Instant::now();
        if self.in_flight.is_some() {
            g.end("saves: the provinces and countries the threads wrote for the last save never went into it".into());
            return false;
        }
        let made = g.prepare(Loop::Provinces, world, writer).and_then(|p| Ok((p, g.prepare(Loop::Countries, world, writer)?)));
        let loops = match made {
            Ok((Some(provinces), Some(countries))) => [provinces, countries],
            Ok(_) => return false,
            Err(e) => {
                g.note(format!("saves: the game writes the provinces and countries itself: {e}"));
                return false;
            }
        };
        let at = match g.at(&loops[0]) {
            Ok(at) => at,
            Err(e) => {
                g.note(format!("saves: the game writes the provinces and countries itself: {e}"));
                return false;
            }
        };
        if !g.ahead(&loops) {
            return false;
        }
        self.in_flight = Some(InFlight { writer, loops, at: [Some(at), None], ended: [None, None], began });
        !g.comparing()
    }

    /// At the countries' loop: the game writes on past it while the pool
    /// writes the countries, from the writer as it was at the provinces'
    /// loop; or the countries are written now, or left to the game.
    fn countries(&mut self, g: &mut G, world: usize, writer: usize) -> bool {
        let Some(flight) = self.in_flight.as_mut() else {
            return alone(g, Loop::Countries, world, writer);
        };
        if flight.writer != writer || !g.same_writer(&flight.loops[Loop::Countries as usize], writer) {
            g.note("saves: the writer changed after the provinces; the game writes the countries itself".into());
            return false;
        }
        match g.at(&flight.loops[Loop::Countries as usize]) {
            Ok(at) => flight.at[Loop::Countries as usize] = Some(at),
            Err(e) => {
                g.note(format!("saves: the game writes the countries itself: {e}"));
                return false;
            }
        }
        !g.comparing()
    }

    /// Where the world has written its game state: the main thread takes the
    /// runs left and waits for the pool, and each loop's bytes go into the
    /// stream where the loop began.
    fn join(&mut self, g: &mut G, writer: usize) {
        let Some(flight) = self.in_flight.take() else { return };
        if flight.writer != writer {
            g.end("saves: the world wrote its game state with another writer than its loops'".into());
            return;
        }
        let waited = Instant::now();
        let threads = g.gather(&flight.loops);
        let waited = waited.elapsed();
        if g.comparing() {
            compare(g, &flight);
            return;
        }
        let mut inserts = Vec::new();
        for (which, at) in [Loop::Provinces, Loop::Countries].into_iter().zip(flight.at) {
            let Some(at) = at else { continue };
            let writing = &flight.loops[which as usize];
            let outputs = g.runs(writing).or_else(|e| {
                g.note(format!("saves: {e}; the main thread writes the {} again", which.name()));
                g.whole(writing).map(|all| vec![all])
            });
            match outputs {
                Ok(outputs) => {
                    g.remember(which, outputs.iter().map(Vec::len).max().unwrap_or(0));
                    inserts.push((at, outputs));
                }
                Err(e) => {
                    g.end(format!("saves: writing the {}: {e}", which.name()));
                    return;
                }
            }
        }
        if let Err(e) = g.splice(&flight.loops[0], &inserts) {
            // The stream holds part of the game state: the save is not what
            // the game would have written, so the game ends rather than keep
            // it.
            g.end(format!("saves: putting the provinces and countries into the save: {e}"));
            return;
        }
        let bytes: usize = inserts.iter().flat_map(|(_, runs)| runs).map(Vec::len).sum();
        g.note(format!(
            "saves: wrote {} provinces and {} countries on {threads} threads beside the game's writing, {bytes} bytes; waited {waited:.1?} for them, {:.1?} after the provinces' loop",
            g.items(&flight.loops[0]),
            g.items(&flight.loops[1]),
            flight.began.elapsed()
        ));
    }

    /// Checked, where the game's own loop ended: where the stream stands, to
    /// compare the pool's bytes with what the loop wrote.
    #[cfg(feature = "dev-test-switches")]
    fn loop_ended(&mut self, g: &mut G, writer: usize, which: usize) {
        let Some(flight) = self.in_flight.as_mut().filter(|f| f.writer == writer && f.at[which].is_some()) else { return };
        match g.at(&flight.loops[which]) {
            Ok(at) => flight.ended[which] = Some(at),
            Err(e) => g.note(format!("saves: where the game's loop ended: {e}")),
        }
    }
}

/// One loop written now, on the pool's threads and this one, its bytes
/// appended to the stream; or the game writes it itself.
fn alone<G: Game>(g: &mut G, which: Loop, world: usize, writer: usize) -> bool {
    let start = Instant::now();
    let writing = match g.prepare(which, world, writer) {
        Ok(Some(writing)) => writing,
        Ok(None) => return false,
        Err(e) => {
            g.note(format!("saves: the game writes the {} itself: {e}", which.name()));
            return false;
        }
    };
    let Some(threads) = g.alone(&writing) else { return false };
    let outputs = match g.runs(&writing) {
        Ok(outputs) => outputs,
        Err(e) => {
            g.note(format!("saves: the game writes the {} itself: {e}", which.name()));
            return false;
        }
    };
    let bytes: usize = outputs.iter().map(Vec::len).sum();
    g.remember(which, outputs.iter().map(Vec::len).max().unwrap_or(0));
    if let Err(e) = g.append(&writing, &outputs) {
        // Part of the bytes may be in the stream: the save is not what the
        // game would have written, so the game ends rather than keep it.
        g.end(format!("saves: writing the {} into the save: {e}", which.name()));
        return false;
    }
    g.note(format!("saves: wrote {} {} on {threads} threads, {bytes} bytes, in {:.1?}", g.items(&writing), which.name(), start.elapsed()));
    true
}

/// Checked, the pool's bytes of each loop against what the game's own loop
/// wrote into the save.
fn compare<G: Game>(g: &mut G, flight: &InFlight<G::Writing>) {
    for (which, writing) in [Loop::Provinces, Loop::Countries].into_iter().zip(&flight.loops) {
        let name = which.name();
        let (Some(at), Some(ended)) = (flight.at[which as usize], flight.ended[which as usize]) else {
            g.note(format!("saves: the game's loop of the {name} was not seen, so it is not compared"));
            continue;
        };
        let bytes = g.written(writing, at, ended).and_then(|game| Ok((game, g.runs(writing)?.concat())));
        match bytes {
            Ok((game, parallel)) => {
                let differs = game.iter().zip(&parallel).position(|(a, b)| a != b);
                let same = differs.is_none() && game.len() == parallel.len();
                g.compared(
                    same,
                    format!(
                        "the {name} differ at byte {}, the game wrote {} bytes and the threads {}",
                        differs.unwrap_or(game.len().min(parallel.len())),
                        game.len(),
                        parallel.len()
                    ),
                );
            }
            Err(e) => g.note(format!("saves: the {name} could not be compared: {e}")),
        }
    }
}

/// The course the hooks tell their moments to.
static COURSE: Mutex<Course<Real>> = Mutex::new(Course::new());

/// At the provinces' loop (`super::write_provinces`).
pub(super) fn provinces(world: usize, writer: usize) -> bool {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).provinces(&mut Real, world, writer)
}

/// At the countries' loop (`super::write_countries`).
pub(super) fn countries(world: usize, writer: usize) -> bool {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).countries(&mut Real, world, writer)
}

/// Where the world has written its game state (`super::join`).
pub(crate) fn join(writer: usize) {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).join(&mut Real, writer);
}

/// Checked, where the game's own loop ended (`super::compare::end`).
#[cfg(feature = "dev-test-switches")]
pub(super) fn loop_ended(writer: usize, which: usize) {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).loop_ended(&mut Real, writer, which);
}

/// The game's save writing: the loops of `super`, the pool they are handed
/// to, and the game's stream their bytes go into.
pub(super) struct Real;

impl Game for Real {
    type Writing = Arc<Writing>;

    fn prepare(&mut self, which: Loop, world: usize, writer: usize) -> Result<Option<Self::Writing>> {
        Ok(Writing::new(crate::hooked_memory(), which, world, writer)?.map(Arc::new))
    }

    fn at(&mut self, writing: &Self::Writing) -> Result<u32> {
        position(crate::hooked_memory(), writing.stream)
    }

    fn same_writer(&mut self, writing: &Self::Writing, writer: usize) -> bool {
        let m = crate::hooked_memory();
        m.bytes(writer, WRITER_SIZE).is_ok_and(|now| now[..0xc] == writing.writer[..0xc] && now[0x10..0x1a] == writing.writer[0x10..0x1a])
    }

    fn ahead(&mut self, loops: &[Self::Writing; 2]) -> bool {
        let Some(pool) = pool() else { return false };
        pool.begin(Arc::new(Ahead { writings: loops.clone() }))
    }

    fn gather(&mut self, loops: &[Self::Writing; 2]) -> usize {
        let ahead = Ahead { writings: loops.clone() };
        match pool() {
            Some(pool) => pool.finish(&ahead),
            // Only a finish that gave up waiting breaks the pool, and none
            // runs between the handing out and this: the runs left are
            // written here.
            None => {
                ahead.work(None);
                1
            }
        }
    }

    fn alone(&mut self, writing: &Self::Writing) -> Option<usize> {
        pool()?.run(writing.clone())
    }

    fn runs(&mut self, writing: &Self::Writing) -> Result<Vec<Vec<u8>>> {
        writing.outputs()
    }

    fn whole(&mut self, writing: &Self::Writing) -> Result<Vec<u8>> {
        writing.write_run(0..writing.items.len())
    }

    fn items(&self, writing: &Self::Writing) -> usize {
        writing.items.len()
    }

    fn append(&mut self, writing: &Self::Writing, runs: &[Vec<u8>]) -> Result<()> {
        append(crate::hooked_memory(), writing.stream, runs)
    }

    fn splice(&mut self, writing: &Self::Writing, inserts: &[(u32, Vec<Vec<u8>>)]) -> Result<()> {
        splice(crate::hooked_memory(), writing.stream, inserts)
    }

    fn written(&mut self, writing: &Self::Writing, from: u32, to: u32) -> Result<Vec<u8>> {
        read(crate::hooked_memory(), writing.stream, from, to)
    }

    fn remember(&mut self, which: Loop, largest: usize) {
        LAST_RUN[which as usize].store(largest, Ordering::Relaxed);
    }

    fn comparing(&self) -> bool {
        comparing()
    }

    fn compared(&mut self, same: bool, detail: String) {
        speed::compared(&SPEED_UP, same, || detail);
    }

    fn note(&mut self, message: String) {
        log!("{message}");
    }

    fn end(&mut self, why: String) {
        crate::fatal(&why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORLD: usize = 0x1000;
    const WRITER: usize = 0x2000;

    /// A loop as the tests tell it: how many items it writes, what its runs
    /// wrote, what the whole loop writes when it is written again, and what
    /// the game's own loop wrote.
    #[derive(Clone)]
    struct Told {
        items: usize,
        runs: Result<Vec<Vec<u8>>>,
        whole: Result<Vec<u8>>,
        game: Vec<u8>,
    }

    impl Default for Told {
        fn default() -> Self {
            Self { items: 100, runs: Ok(vec![vec![1, 2, 3], vec![4, 5, 6, 7]]), whole: Ok(vec![0; 7]), game: Vec::new() }
        }
    }

    /// The game as the tests tell it: what each loop writes, where the stream
    /// stands, what fails, and what the course asked for, in order. A handle
    /// is the loop itself.
    #[derive(Default)]
    struct Fake {
        provinces: Told,
        countries: Told,
        stands: u32,
        calls: Vec<String>,
        notes: Vec<String>,
        ended: Option<String>,
        compared: Vec<(bool, String)>,
        remembered: Vec<(Loop, usize)>,
        appended: Vec<Vec<Vec<u8>>>,
        spliced: Vec<Vec<(u32, Vec<Vec<u8>>)>>,
        prepare_fails: Option<String>,
        leaves_to_game: bool,
        at_fails: Option<String>,
        busy: bool,
        writer_changed: bool,
        append_fails: Option<String>,
        splice_fails: Option<String>,
        written_fails: Option<String>,
        comparing: bool,
    }

    impl Fake {
        fn told(&self, which: Loop) -> &Told {
            match which {
                Loop::Provinces => &self.provinces,
                Loop::Countries => &self.countries,
            }
        }

        fn asked(&mut self, what: String) {
            self.calls.push(what);
        }

        fn said(&self, part: &str) -> bool {
            self.notes.iter().any(|n| n.contains(part))
        }
    }

    impl Game for Fake {
        type Writing = Loop;

        fn prepare(&mut self, which: Loop, _world: usize, _writer: usize) -> Result<Option<Self::Writing>> {
            self.asked(format!("prepare {}", which.name()));
            if let Some(e) = &self.prepare_fails {
                return Err(e.clone());
            }
            Ok((!self.leaves_to_game).then_some(which))
        }

        fn at(&mut self, writing: &Self::Writing) -> Result<u32> {
            self.asked(format!("at {}", writing.name()));
            match &self.at_fails {
                Some(e) => Err(e.clone()),
                None => Ok(self.stands),
            }
        }

        fn same_writer(&mut self, writing: &Self::Writing, _writer: usize) -> bool {
            self.asked(format!("same_writer {}", writing.name()));
            !self.writer_changed
        }

        fn ahead(&mut self, _loops: &[Self::Writing; 2]) -> bool {
            self.asked("ahead".into());
            !self.busy
        }

        fn gather(&mut self, _loops: &[Self::Writing; 2]) -> usize {
            self.asked("gather".into());
            8
        }

        fn alone(&mut self, writing: &Self::Writing) -> Option<usize> {
            self.asked(format!("alone {}", writing.name()));
            (!self.busy).then_some(8)
        }

        fn runs(&mut self, writing: &Self::Writing) -> Result<Vec<Vec<u8>>> {
            self.asked(format!("runs {}", writing.name()));
            self.told(*writing).runs.clone()
        }

        fn whole(&mut self, writing: &Self::Writing) -> Result<Vec<u8>> {
            self.asked(format!("whole {}", writing.name()));
            self.told(*writing).whole.clone()
        }

        fn items(&self, writing: &Self::Writing) -> usize {
            self.told(*writing).items
        }

        fn append(&mut self, _writing: &Self::Writing, runs: &[Vec<u8>]) -> Result<()> {
            self.asked("append".into());
            self.appended.push(runs.to_vec());
            match &self.append_fails {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }

        fn splice(&mut self, _writing: &Self::Writing, inserts: &[(u32, Vec<Vec<u8>>)]) -> Result<()> {
            self.asked("splice".into());
            self.spliced.push(inserts.to_vec());
            match &self.splice_fails {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }

        fn written(&mut self, writing: &Self::Writing, _from: u32, _to: u32) -> Result<Vec<u8>> {
            self.asked(format!("written {}", writing.name()));
            match &self.written_fails {
                Some(e) => Err(e.clone()),
                None => Ok(self.told(*writing).game.clone()),
            }
        }

        fn remember(&mut self, which: Loop, largest: usize) {
            self.remembered.push((which, largest));
        }

        fn comparing(&self) -> bool {
            self.comparing
        }

        fn compared(&mut self, same: bool, detail: String) {
            self.compared.push((same, detail));
        }

        fn note(&mut self, message: String) {
            self.notes.push(message);
        }

        fn end(&mut self, why: String) {
            self.ended = Some(why);
        }
    }

    /// A course with both loops handed to the pool, the provinces' place
    /// read at 10 and the countries' at 50.
    fn ahead(g: &mut Fake) -> Course<Fake> {
        let mut c = Course::new();
        g.stands = 10;
        assert!(c.provinces(g, WORLD, WRITER));
        g.stands = 50;
        assert!(c.countries(g, WORLD, WRITER));
        c
    }

    #[test]
    fn both_loops_go_into_the_stream_where_they_began() {
        let mut g = Fake::default();
        let mut c = ahead(&mut g);
        c.join(&mut g, WRITER);
        assert_eq!(g.spliced, vec![vec![(10, g.provinces.runs.clone().unwrap()), (50, g.countries.runs.clone().unwrap())]]);
        assert!(g.appended.is_empty());
        assert_eq!(g.remembered, vec![(Loop::Provinces, 4), (Loop::Countries, 4)]);
        assert_eq!(g.ended, None);
        assert!(g.calls.iter().any(|c| c == "ahead"));
        assert!(g.calls.iter().any(|c| c == "gather"));
        assert!(g.said("wrote 100 provinces and 100 countries on 8 threads"));
    }

    #[test]
    fn a_changed_writer_leaves_the_countries_to_the_game() {
        let mut g = Fake { writer_changed: true, ..Default::default() };
        let mut c = Course::new();
        g.stands = 10;
        assert!(c.provinces(&mut g, WORLD, WRITER));
        assert!(!c.countries(&mut g, WORLD, WRITER));
        assert!(g.said("the writer changed after the provinces"));
        c.join(&mut g, WRITER);
        // The provinces still go in where they began; the countries were the
        // game's to write.
        assert_eq!(g.spliced, vec![vec![(10, g.provinces.runs.clone().unwrap())]]);
        assert_eq!(g.remembered, vec![(Loop::Provinces, 4)]);
    }

    #[test]
    fn another_writer_at_the_countries_leaves_them_to_the_game() {
        let mut g = Fake::default();
        let mut c = Course::new();
        g.stands = 10;
        assert!(c.provinces(&mut g, WORLD, WRITER));
        assert!(!c.countries(&mut g, WORLD, WRITER + 1));
        assert!(g.said("the writer changed after the provinces"));
    }

    #[test]
    fn a_faulted_run_is_written_again_by_the_main_thread() {
        let mut g = Fake::default();
        g.provinces.runs = Err("run 3 of the provinces was not written".into());
        g.provinces.whole = Ok(vec![9; 7]);
        let mut c = ahead(&mut g);
        c.join(&mut g, WRITER);
        assert!(g.calls.iter().any(|c| c == "whole provinces"));
        assert!(g.said("the main thread writes the provinces again"));
        assert_eq!(g.spliced, vec![vec![(10, vec![vec![9; 7]]), (50, g.countries.runs.clone().unwrap())]]);
        assert_eq!(g.ended, None);
    }

    #[test]
    fn a_loop_that_cannot_be_written_again_ends_the_game() {
        let mut g = Fake::default();
        g.provinces.runs = Err("run 3 of the provinces was not written".into());
        g.provinces.whole = Err("a run of the provinces writes more than 1073741824 bytes".into());
        let mut c = ahead(&mut g);
        c.join(&mut g, WRITER);
        assert_eq!(g.ended.as_deref(), Some("saves: writing the provinces: a run of the provinces writes more than 1073741824 bytes"));
        assert!(g.spliced.is_empty());
    }

    #[test]
    fn a_splice_that_fails_ends_the_game() {
        let mut g = Fake { splice_fails: Some("the places to put the loops' bytes are out of order".into()), ..Default::default() };
        let mut c = ahead(&mut g);
        c.join(&mut g, WRITER);
        assert_eq!(g.ended.as_deref(), Some("saves: putting the provinces and countries into the save: the places to put the loops' bytes are out of order"));
    }

    #[test]
    fn a_save_left_in_flight_ends_the_game_at_the_next_provinces() {
        let mut g = Fake::default();
        let mut c = Course::new();
        g.stands = 10;
        assert!(c.provinces(&mut g, WORLD, WRITER));
        assert!(!c.provinces(&mut g, WORLD, WRITER));
        assert_eq!(g.ended.as_deref(), Some("saves: the provinces and countries the threads wrote for the last save never went into it"));
    }

    #[test]
    fn another_writer_at_the_join_ends_the_game() {
        let mut g = Fake::default();
        let mut c = ahead(&mut g);
        c.join(&mut g, WRITER + 1);
        assert_eq!(g.ended.as_deref(), Some("saves: the world wrote its game state with another writer than its loops'"));
        assert!(g.spliced.is_empty());
    }

    #[test]
    fn the_countries_alone_are_written_and_appended() {
        let mut g = Fake::default();
        let mut c = Course::new();
        assert!(c.countries(&mut g, WORLD, WRITER));
        assert!(g.calls.iter().any(|c| c == "alone countries"));
        assert_eq!(g.appended, vec![g.countries.runs.clone().unwrap()]);
        assert!(g.spliced.is_empty());
        assert_eq!(g.remembered, vec![(Loop::Countries, 4)]);
        assert!(g.said("wrote 100 countries on 8 threads"));
    }

    #[test]
    fn a_loop_better_left_to_the_game_is_not_written() {
        let mut g = Fake { leaves_to_game: true, ..Default::default() };
        let mut c = Course::new();
        assert!(!c.countries(&mut g, WORLD, WRITER));
        assert!(!c.provinces(&mut g, WORLD, WRITER));
        c.join(&mut g, WRITER);
        assert!(g.appended.is_empty());
        assert!(g.spliced.is_empty());
        assert_eq!(g.ended, None);
    }

    #[test]
    fn a_prepare_that_fails_leaves_the_loops_to_the_game() {
        let mut g = Fake { prepare_fails: Some("the writer is not the save's".into()), ..Default::default() };
        let mut c = Course::new();
        assert!(!c.provinces(&mut g, WORLD, WRITER));
        assert!(g.said("the game writes the provinces and countries itself: the writer is not the save's"));
        assert!(!c.countries(&mut g, WORLD, WRITER));
        assert!(g.said("the game writes the countries itself: the writer is not the save's"));
    }

    #[test]
    fn a_busy_pool_leaves_the_loops_to_the_game() {
        let mut g = Fake { busy: true, ..Default::default() };
        let mut c = Course::new();
        g.stands = 10;
        assert!(!c.provinces(&mut g, WORLD, WRITER));
        c.join(&mut g, WRITER);
        assert!(g.spliced.is_empty());
        assert!(!c.countries(&mut g, WORLD, WRITER));
        assert!(g.appended.is_empty());
    }

    #[test]
    fn a_place_that_cannot_be_read_leaves_the_loops_to_the_game() {
        let mut g = Fake { at_fails: Some("the stream is not there".into()), ..Default::default() };
        let mut c = Course::new();
        assert!(!c.provinces(&mut g, WORLD, WRITER));
        assert!(g.said("the game writes the provinces and countries itself: the stream is not there"));
        assert!(g.spliced.is_empty());
    }

    #[test]
    fn an_append_that_fails_ends_the_game() {
        let mut g = Fake { append_fails: Some("the stream is full".into()), ..Default::default() };
        let mut c = Course::new();
        assert!(!c.countries(&mut g, WORLD, WRITER));
        assert_eq!(g.ended.as_deref(), Some("saves: writing the countries into the save: the stream is full"));
    }

    #[test]
    fn checked_the_games_loops_write_and_nothing_is_spliced() {
        let mut g = Fake { comparing: true, ..Default::default() };
        let mut c = Course::new();
        g.stands = 10;
        // Checked, the game writes its own loops: the course says so.
        assert!(!c.provinces(&mut g, WORLD, WRITER));
        g.stands = 50;
        assert!(!c.countries(&mut g, WORLD, WRITER));
        c.join(&mut g, WRITER);
        assert!(g.spliced.is_empty());
        assert!(g.appended.is_empty());
        // The game's loops were not seen to end, so they are not compared.
        assert!(g.said("the game's loop of the provinces was not seen"));
        assert!(g.said("the game's loop of the countries was not seen"));
        assert!(g.compared.is_empty());
    }

    #[cfg(feature = "dev-test-switches")]
    #[test]
    fn checked_each_loop_is_compared_with_the_games_own() {
        let mut g = Fake { comparing: true, ..Default::default() };
        g.provinces.game = vec![1, 2, 3, 4, 5, 6, 7];
        g.countries.game = vec![1, 2, 3, 4, 5, 6, 7];
        let mut c = Course::new();
        g.stands = 10;
        assert!(!c.provinces(&mut g, WORLD, WRITER));
        g.stands = 50;
        assert!(!c.countries(&mut g, WORLD, WRITER));
        g.stands = 90;
        c.loop_ended(&mut g, WRITER, Loop::Provinces as usize);
        g.stands = 140;
        c.loop_ended(&mut g, WRITER, Loop::Countries as usize);
        c.join(&mut g, WRITER);
        assert_eq!(g.compared.len(), 2);
        assert!(g.compared.iter().all(|(same, _)| *same));
        assert!(g.spliced.is_empty());
    }

    #[cfg(feature = "dev-test-switches")]
    #[test]
    fn checked_a_loop_that_differs_is_reported() {
        let mut g = Fake { comparing: true, ..Default::default() };
        g.provinces.game = vec![1, 2, 3, 4, 5, 6, 7];
        g.countries.game = vec![1, 2, 9, 4, 5, 6, 7];
        let mut c = Course::new();
        g.stands = 10;
        c.provinces(&mut g, WORLD, WRITER);
        g.stands = 50;
        c.countries(&mut g, WORLD, WRITER);
        g.stands = 90;
        c.loop_ended(&mut g, WRITER, Loop::Provinces as usize);
        g.stands = 140;
        c.loop_ended(&mut g, WRITER, Loop::Countries as usize);
        c.join(&mut g, WRITER);
        assert_eq!(g.compared.len(), 2);
        assert!(g.compared[0].0);
        assert!(!g.compared[1].0);
        assert!(g.compared[1].1.contains("the countries differ at byte 2"));
    }

    #[cfg(feature = "dev-test-switches")]
    #[test]
    fn checked_a_loop_whose_bytes_cannot_be_read_is_not_compared() {
        let mut g = Fake { comparing: true, written_fails: Some("the stream's buffer does not hold what was written".into()), ..Default::default() };
        let mut c = Course::new();
        g.stands = 10;
        c.provinces(&mut g, WORLD, WRITER);
        g.stands = 50;
        c.countries(&mut g, WORLD, WRITER);
        g.stands = 90;
        c.loop_ended(&mut g, WRITER, Loop::Provinces as usize);
        g.stands = 140;
        c.loop_ended(&mut g, WRITER, Loop::Countries as usize);
        c.join(&mut g, WRITER);
        assert!(g.compared.is_empty());
        assert!(g.said("the provinces could not be compared"));
    }
}
