//! What the module's own work costs while the game runs: how often, and how
//! long, it worked out line breaks and drew glyphs and pages for filled
//! fonts. Logged once the main menu shows, then again when there is more to
//! say, at most every ten minutes, from the frame the main thread presents.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Cost {
    count: AtomicU64,
    nanos: AtomicU64,
}

impl Cost {
    const fn new() -> Self {
        Self { count: AtomicU64::new(0), nanos: AtomicU64::new(0) }
    }

    /// Runs `work`, counting it and the time it took.
    pub fn time<T>(&self, work: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let out = work();
        self.nanos.fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        out
    }

    /// Counts something that is not timed.
    pub fn add(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    fn get(&self) -> (u64, Duration) {
        (self.count.load(Ordering::Relaxed), Duration::from_nanos(self.nanos.load(Ordering::Relaxed)))
    }
}

/// Questions about where a line may break.
pub static BREAK_QUESTIONS: Cost = Cost::new();
/// A text's breaks worked out by ICU (a question about a text not asked
/// about just before).
pub static BREAKS: Cost = Cost::new();
/// Glyphs drawn into a filled font's page.
pub static GLYPHS: Cost = Cost::new();
/// Pages made for a filled font.
pub static PAGES: Cost = Cost::new();

fn summary() -> String {
    let (questions, _) = BREAK_QUESTIONS.get();
    let (breaks, breaks_time) = BREAKS.get();
    let (glyphs, glyphs_time) = GLYPHS.get();
    let (pages, pages_time) = PAGES.get();
    let each = |n: u64, t: Duration| if n == 0 { Duration::ZERO } else { t / n as u32 };
    format!(
        "{questions} line break questions, {breaks} texts' breaks worked out in {breaks_time:.1?} ({:.1?} each); {glyphs} glyphs drawn in {glyphs_time:.1?} ({:.1?} each); {pages} font pages made in {pages_time:.1?}",
        each(breaks, breaks_time),
        each(glyphs, glyphs_time),
    )
}

const EVERY: Duration = Duration::from_secs(600);

/// When the costs were last looked at, and what was logged then.
static LAST: Mutex<Option<(Instant, String)>> = Mutex::new(None);

/// Logs the costs now.
pub fn log_now() {
    let summary = summary();
    log!("unicode costs: {summary}");
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), summary));
}

/// Logs the costs when they were logged before, ten minutes or more ago, and
/// have changed since. Cheap enough for every frame.
pub fn log_when_due() {
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let Some((at, before)) = last.as_mut() else { return };
    if at.elapsed() < EVERY {
        return;
    }
    *at = Instant::now();
    let summary = summary();
    if *before != summary {
        log!("unicode costs: {summary}");
        *before = summary;
    }
}
