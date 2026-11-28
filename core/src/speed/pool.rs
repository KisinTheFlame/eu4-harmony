//! General-purpose worker pool used by parallel saving speed-ups, decoupled
//! from save-writer `Holders`.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// A cooperative work item executed by every thread of a [`WorkerPool`] plus
/// the submitting thread.
pub(crate) trait Work: Send + Sync {
    /// Claims and executes work units until none remain. `on_worker` is `true`
    /// on pool worker threads and `false` on the submitting thread.
    fn work(&self, on_worker: bool);
    /// Called when the pool times out waiting for workers so the task stops
    /// producing results.
    fn abandon(&self);
}

struct BatchState {
    work: Option<Arc<dyn Work>>,
    round: u64,
    busy: usize,
    shutdown: bool,
}

/// Reusable one-batch-at-a-time worker pool where the submitting thread works
/// alongside the pool threads (`saving::parallel`, `saving::stored`, `saving::sum`).
pub(crate) struct WorkerPool {
    worker_count: usize,
    #[cfg(feature = "dev")]
    threads: Vec<u32>,
    handles: Mutex<Vec<std::thread::JoinHandle<()>>>,
    inner: Arc<BatchInner>,
    in_use: AtomicBool,
    broken: AtomicBool,
}

struct BatchInner {
    state: Mutex<BatchState>,
    start: Condvar,
    done: Condvar,
}

impl WorkerPool {
    /// Spawns up to `wanted` worker threads named `"{name_prefix} {i}"`,
    /// running `on_worker_start` once on each worker before entering the work
    /// loop.
    pub(crate) fn new(name_prefix: &'static str, guard_label: &'static str, wanted: usize, on_worker_start: impl Fn() + Send + Sync + 'static) -> Self {
        let inner = Arc::new(BatchInner {
            state: Mutex::new(BatchState { work: None, round: 0, busy: 0, shutdown: false }),
            start: Condvar::new(),
            done: Condvar::new(),
        });
        let init = Arc::new(on_worker_start);
        let ids = Arc::new(Mutex::new(Vec::new()));
        let handles: Vec<_> = (0..wanted)
            .filter_map(|i| {
                let ids = ids.clone();
                let inner = inner.clone();
                let init = init.clone();
                std::thread::Builder::new()
                    .name(format!("{name_prefix} {i}"))
                    .spawn(move || {
                        ids.lock().unwrap_or_else(|e| e.into_inner()).push(crate::base::thread_id());
                        init();
                        batch_worker(&inner, guard_label);
                    })
                    .ok()
            })
            .collect();
        let worker_count = handles.len();
        let deadline = Instant::now() + Duration::from_secs(5);
        while ids.lock().unwrap_or_else(|e| e.into_inner()).len() < worker_count && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        #[cfg(feature = "dev")]
        let threads = ids.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Self {
            worker_count,
            #[cfg(feature = "dev")]
            threads,
            handles: Mutex::new(handles),
            inner,
            in_use: AtomicBool::new(false),
            broken: AtomicBool::new(false),
        }
    }

    pub(crate) fn is_broken(&self) -> bool {
        self.broken.load(Ordering::SeqCst)
    }

    /// Total concurrency: worker threads plus the submitting thread.
    pub(crate) fn width(&self) -> usize {
        self.worker_count + 1
    }

    /// OS thread IDs of the pool's worker threads.
    #[cfg(feature = "dev")]
    pub(crate) fn thread_ids(&self) -> &[u32] {
        &self.threads
    }

    /// Hands `work` to the pool's workers unless another batch is in progress
    /// or the pool has timed out.
    pub(crate) fn begin(&self, work: Arc<dyn Work>) -> bool {
        if self.is_broken() || self.in_use.swap(true, Ordering::SeqCst) {
            return false;
        }
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.work = Some(work);
        state.round += 1;
        state.busy = self.worker_count;
        self.inner.start.notify_all();
        true
    }

    /// Helps finish the current batch on the calling thread, waits up to
    /// `timeout` for all workers, and if `timeout` elapses, abandons remaining
    /// chunks and waits until every active worker exits `work()` before
    /// returning `None`. Use this when the caller will mutate or free memory
    /// read by workers after this call returns.
    pub(crate) fn finish_draining(&self, work: &dyn Work, timeout: Duration, timeout_note: &str) -> Option<usize> {
        work.work(false);
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        let deadline = Instant::now() + timeout;
        let mut timed_out = false;
        while state.busy > 0 {
            if timed_out {
                state = self.inner.done.wait(state).unwrap_or_else(|e| e.into_inner());
                continue;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.broken.store(true, Ordering::SeqCst);
                work.abandon();
                log!("{timeout_note}");
                timed_out = true;
                continue;
            }
            state = self.inner.done.wait_timeout(state, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        state.work = None;
        self.in_use.store(false, Ordering::SeqCst);
        (!timed_out).then(|| self.width())
    }

    /// Runs `work` synchronously and, if `timeout` elapses, abandons remaining
    /// chunks and waits until every active worker exits `work()` before
    /// returning `None`. Use this when the caller will immediately mutate or
    /// free memory read by workers after this call returns.
    pub(crate) fn run_draining(&self, work: Arc<dyn Work>, timeout: Duration, timeout_note: &str) -> Option<usize> {
        if !self.begin(work.clone()) {
            return None;
        }
        self.finish_draining(&*work, timeout, timeout_note)
    }

    /// Evaluates `f(0..count)` across the pool and calling thread, returning
    /// `None` if the pool is busy, broken, or any part was skipped.
    pub(crate) fn each<T: Send + 'static>(
        &self,
        count: usize,
        timeout: Duration,
        timeout_note: &str,
        f: impl Fn(usize) -> T + Send + Sync + 'static,
    ) -> Option<Vec<T>> {
        let task = Arc::new(EachWork {
            next: AtomicUsize::new(0),
            abandoned: AtomicBool::new(false),
            f: Box::new(f),
            out: (0..count).map(|_| Mutex::new(None)).collect(),
        });
        self.run_draining(task.clone(), timeout, timeout_note)?;
        task.out.iter().map(|o| o.lock().unwrap_or_else(|e| e.into_inner()).take()).collect()
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        if self.is_broken() {
            return;
        }
        {
            let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
            state.shutdown = true;
            self.inner.start.notify_all();
        }
        for handle in std::mem::take(&mut *self.handles.lock().unwrap_or_else(|e| e.into_inner())) {
            let _ = handle.join();
        }
    }
}

fn batch_worker(inner: &BatchInner, guard_label: &str) {
    let mut seen = 0;
    loop {
        let work = {
            let mut state = inner.state.lock().unwrap_or_else(|e| e.into_inner());
            while state.round == seen && !state.shutdown {
                state = inner.start.wait(state).unwrap_or_else(|e| e.into_inner());
            }
            if state.shutdown {
                return;
            }
            seen = state.round;
            state.work.clone()
        };
        if let Some(work) = work {
            crate::base::guarded(guard_label, || work.work(true));
        }
        let mut state = inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.busy = state.busy.saturating_sub(1);
        inner.done.notify_all();
    }
}

struct EachWork<T> {
    next: AtomicUsize,
    abandoned: AtomicBool,
    f: Box<dyn Fn(usize) -> T + Send + Sync>,
    out: Vec<Mutex<Option<T>>>,
}

impl<T: Send> Work for EachWork<T> {
    fn work(&self, _: bool) {
        loop {
            if self.abandoned.load(Ordering::Acquire) {
                return;
            }
            let i = self.next.fetch_add(1, Ordering::SeqCst);
            let Some(out) = self.out.get(i) else { return };
            let value = (self.f)(i);
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
        }
    }

    fn abandon(&self) {
        self.abandoned.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_pool_runs_each_and_begin_finish_across_threads() {
        let init_hits = Arc::new(AtomicUsize::new(0));
        let hits = init_hits.clone();
        let pool = WorkerPool::new("test-pool", "test pool", 2, move || {
            hits.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(pool.width(), 3);

        let squares = pool.each(64, Duration::from_secs(5), "timeout", |i| i * i).unwrap();
        assert_eq!(squares, (0..64).map(|i| i * i).collect::<Vec<_>>());
        assert_eq!(init_hits.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn worker_pool_timeout_abandons_task_and_marks_pool_broken() {
        struct Stuck {
            abandoned: AtomicBool,
            release: Arc<(Mutex<bool>, Condvar)>,
        }
        impl Work for Stuck {
            fn work(&self, on_worker: bool) {
                if on_worker {
                    let (lock, cv) = &*self.release;
                    let mut done = lock.lock().unwrap();
                    while !*done {
                        done = cv.wait(done).unwrap();
                    }
                }
            }
            fn abandon(&self) {
                self.abandoned.store(true, Ordering::SeqCst);
                let (lock, cv) = &*self.release;
                *lock.lock().unwrap() = true;
                cv.notify_all();
            }
        }
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let task = Arc::new(Stuck { abandoned: AtomicBool::new(false), release });
        let pool = WorkerPool::new("test-timeout", "test timeout", 1, || {});
        let width = pool.run_draining(task.clone(), Duration::from_millis(20), "test timed out");
        assert_eq!(width, None);
        assert!(task.abandoned.load(Ordering::SeqCst));
        assert!(pool.is_broken());
        // Broken pool refuses further batches.
        assert!(pool.each(4, Duration::from_secs(1), "timeout", |i| i).is_none());
    }

    #[test]
    fn finish_draining_waits_for_abandoned_worker_to_exit_before_returning() {
        struct Slow {
            abandoned: AtomicBool,
            exited: AtomicBool,
        }
        impl Work for Slow {
            fn work(&self, on_worker: bool) {
                if on_worker {
                    while !self.abandoned.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    std::thread::sleep(Duration::from_millis(15));
                    self.exited.store(true, Ordering::Release);
                }
            }
            fn abandon(&self) {
                self.abandoned.store(true, Ordering::Release);
            }
        }
        let task = Arc::new(Slow { abandoned: AtomicBool::new(false), exited: AtomicBool::new(false) });
        let pool = WorkerPool::new("test-drain", "test drain", 1, || {});
        assert!(pool.begin(task.clone()));
        assert_eq!(pool.finish_draining(&*task, Duration::from_millis(10), "drain timed out"), None);
        assert!(task.exited.load(Ordering::Acquire), "finish_draining returned while worker was still active");
        assert!(pool.is_broken());
    }
}
