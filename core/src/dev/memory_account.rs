//! What the process holds in memory, split by where it lies, for remote
//! control's `memory` (a developer build: a player build has none
//! of this; docs/dev-tools.md). It reads only what Windows keeps of the
//! process and its heaps, so it costs the game nothing between two asks:
//! a run can measure without the heap census, which slows the game and holds
//! hundreds of megabytes of its own.
//!
//! The game's C runtime allocates from the heap whose handle its global
//! [`CRT_HEAP`] holds, the process's default heap; the framework allocates
//! from the default heap too, so what the game holds there is what the heap
//! has allocated less the framework's own heap.
use crate::native::points::Point;
use eu4_harmony_remote::memory::{Heap, Memory, MemoryAnswer, MemoryRequest};
use eu4_harmony_remote::Error;

/// The game's C runtime's heap handle, which `free` passes to `HeapFree`.
const CRT_HEAP: usize = 0x233d850;
/// `free`, where it reads [`CRT_HEAP`].
const FREE_READS_HEAP: usize = 0x1a6834b;

/// Where `eu4.exe` lies, 0 before startup.
fn base() -> usize {
    crate::native::memory().map_or(0, |m| m.base)
}

pub fn points() -> Vec<Point> {
    vec![Point::keep(FREE_READS_HEAP, "48 8b 0d fe 54 8d 00 ff 15", "free passes the C runtime's heap").uses(&[CRT_HEAP])]
}

/// The game's C runtime's heap, once the game has made it; 0 before.
pub fn game_heap() -> usize {
    let base = base();
    if base == 0 {
        return 0;
    }
    crate::native::memory::Memory { base }.ptr(base + CRT_HEAP).unwrap_or(0)
}

/// What the process holds now; the census's generations are left for it to
/// fill.
pub fn memory() -> Memory {
    use windows_sys::Win32::System::Memory::GetProcessHeap;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
    let mut each = crate::base::heap::each_heap();
    each.sort_by_key(|(_, allocated, _)| std::cmp::Reverse(*allocated));
    let (game, process) = (game_heap(), unsafe { GetProcessHeap() } as usize);
    let private = crate::base::heap::private_bytes();
    let framework = crate::base::heap::live();
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
        census: super::options::options().heap_census.enabled,
        generation: 0,
        generations: Vec::new(),
    }
}

/// Answers remote control's memory group: the census's own capabilities
/// when `[heap-census] enabled = true`, and otherwise only `memory`.
pub fn answer(request: MemoryRequest) -> Result<MemoryAnswer, Error> {
    if super::options::options().heap_census.enabled {
        return super::census::answer(request);
    }
    match request {
        MemoryRequest::Get {} => Ok(MemoryAnswer::Get(memory())),
        _ => Err(Error::new(eu4_harmony_remote::Code::Unavailable, "the heap census is off: set [heap-census] enabled = true in dev.toml")),
    }
}
