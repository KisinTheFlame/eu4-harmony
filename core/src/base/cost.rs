//! Lock-free call-count and elapsed-time accumulator shared across framework
//! subsystems (Unicode text metrics, signal costs).
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Cost {
    count: AtomicU64,
    nanos: AtomicU64,
}

impl Cost {
    pub const fn new() -> Self {
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

    pub fn get(&self) -> (u64, Duration) {
        (self.count.load(Ordering::Relaxed), Duration::from_nanos(self.nanos.load(Ordering::Relaxed)))
    }

    /// How often, and how long in all, and starts counting again.
    pub fn take(&self) -> (u64, Duration) {
        (self.count.swap(0, Ordering::Relaxed), Duration::from_nanos(self.nanos.swap(0, Ordering::Relaxed)))
    }
}

#[cfg(test)]
mod tests {
    use super::Cost;

    #[test]
    fn counts_timed_and_untimed_work_and_resets_on_take() {
        let cost = Cost::new();
        assert_eq!(cost.get().0, 0);
        cost.add();
        assert_eq!(cost.time(|| 42), 42);
        let (count, _) = cost.get();
        assert_eq!(count, 2);
        let (taken, _) = cost.take();
        assert_eq!(taken, 2);
        assert_eq!(cost.get().0, 0);
    }
}
