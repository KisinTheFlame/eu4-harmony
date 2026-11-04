//! What the process holds in memory, split by where it lies, for remote
//! control's `memory` (the `dev-memory` feature: a player's build has none
//! of this; docs/leak-finding.md). It reads only what Windows keeps of the
//! process and its heaps, so it costs the game nothing between two asks:
//! a run can measure without the heap census, which slows the game and holds
//! hundreds of megabytes of its own.
//!
//! The game's C runtime allocates from the heap whose handle its global
//! [`CRT_HEAP`] holds, the process's default heap; the framework allocates
//! from the default heap too, so what the game holds there is what the heap
//! has allocated less the framework's own heap.
use crate::points::Point;
use eu4_harmony_remote::memory::{Heap, Memory, MemoryAnswer, MemoryRequest};
use eu4_harmony_remote::Error;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The game's C runtime's heap handle, which `free` passes to `HeapFree`.
const CRT_HEAP: usize = 0x233d850;
/// `free`, where it reads [`CRT_HEAP`].
const FREE_READS_HEAP: usize = 0x1a6834b;

static BASE: AtomicUsize = AtomicUsize::new(0);

pub fn points(m: &crate::memory::Memory) -> Vec<Point> {
    BASE.store(m.base, Ordering::Relaxed);
    vec![Point::keep(FREE_READS_HEAP, "48 8b 0d fe 54 8d 00 ff 15", "free passes the C runtime's heap").uses(&[CRT_HEAP])]
}

/// The game's C runtime's heap, once the game has made it; 0 before.
fn game_heap() -> usize {
    let base = BASE.load(Ordering::Relaxed);
    if base == 0 {
        return 0;
    }
    crate::memory::Memory { base }.ptr(base + CRT_HEAP).unwrap_or(0)
}

/// What the process holds now; the census's generations are left for it to
/// fill.
pub fn memory() -> Memory {
    use windows_sys::Win32::System::Memory::GetProcessHeap;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
    let mut each = crate::heap::each_heap();
    each.sort_by_key(|(_, allocated, _)| std::cmp::Reverse(*allocated));
    let (game, process) = (game_heap(), unsafe { GetProcessHeap() } as usize);
    let private = crate::heap::private_bytes();
    let framework = crate::heap::live();
    let (allocated, committed) = each.iter().fold((0, 0), |(a, c), (_, ha, hc)| (a + ha, c + hc));
    let game_heap = each.iter().find(|(h, ..)| *h == game).map_or(0, |(_, a, _)| if game == process { a.saturating_sub(framework) } else { *a });
    let mut handles = 0u32;
    unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut handles) };
    Memory {
        private: private as u64,
        heaps_allocated: allocated as u64,
        heaps_committed: committed as u64,
        free_in_heaps: committed.saturating_sub(allocated) as u64,
        outside_heaps: private.saturating_sub(committed) as u64,
        framework_heap: framework as u64,
        game_heap: game_heap as u64,
        heaps: each
            .iter()
            .map(|(h, a, c)| Heap { handle: format!("{h:#x}"), game: *h == game, process: *h == process, allocated: *a as u64, committed: *c as u64 })
            .collect(),
        handles,
        census: cfg!(feature = "dev-heap-census"),
        generation: 0,
        generations: Vec::new(),
    }
}

/// Answers remote control's memory group: the census's own capabilities
/// when it is built in, and without it only `memory`.
pub fn answer(request: MemoryRequest) -> Result<MemoryAnswer, Error> {
    #[cfg(feature = "dev-heap-census")]
    return crate::census::answer(request);
    #[cfg(not(feature = "dev-heap-census"))]
    match request {
        MemoryRequest::Get {} => Ok(MemoryAnswer::Get(memory())),
        _ => Err(Error::new(eu4_harmony_remote::Code::Unavailable, "the heap census is not built in: build with the dev-heap-census feature")),
    }
}
