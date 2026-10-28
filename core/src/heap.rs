//! What the framework holds in memory, for the log: every allocation the core
//! makes goes through [`Counting`], which keeps a running total of the bytes
//! asked for, and [`private_bytes`] reads what the whole process has
//! committed. The difference between two moments shows how much of a change
//! was the framework's own heap and how much the game's, Direct3D's or the
//! driver's. Counting costs two relaxed atomic operations per allocation.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

pub struct Counting;

#[global_allocator]
static HEAP: Counting = Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static BLOCKS: AtomicUsize = AtomicUsize::new(0);

fn grew(size: usize) {
    let now = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            grew(layout.size());
            BLOCKS.fetch_add(1, Ordering::Relaxed);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            grew(layout.size());
            BLOCKS.fetch_add(1, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        BLOCKS.fetch_sub(1, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = System.realloc(p, layout, size);
        if !q.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            grew(size);
        }
        q
    }
}

/// Bytes the framework's heap holds now, not counting the allocator's own
/// overhead per block.
pub fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// Bytes the whole process has committed for itself (private bytes).
pub fn private_bytes() -> usize {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..unsafe { std::mem::zeroed() } };
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS, counters.cb) };
    if ok == 0 { 0 } else { counters.PrivateUsage }
}

const MB: f64 = 1024.0 * 1024.0;

/// Both, now, for the log.
pub fn summary() -> String {
    format!(
        "process private {:.0} MB; framework heap {:.1} MB in {} blocks (peak {:.1} MB)",
        private_bytes() as f64 / MB,
        live() as f64 / MB,
        BLOCKS.load(Ordering::Relaxed),
        PEAK.load(Ordering::Relaxed) as f64 / MB
    )
}

/// What a piece of work changed: private bytes and framework heap, in MB.
pub struct Change {
    private: usize,
    live: usize,
}

impl Change {
    pub fn begin() -> Self {
        Self { private: private_bytes(), live: live() }
    }

    pub fn end(&self) -> String {
        let delta = |now: usize, then: usize| (now as f64 - then as f64) / MB;
        format!("private {:+.1} MB, framework heap {:+.1} MB", delta(private_bytes(), self.private), delta(live(), self.live))
    }
}
