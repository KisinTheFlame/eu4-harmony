//! Counts the game's live heap blocks by where they were allocated and when,
//! for finding what keeps the game's memory growing (the `dev-heap-census`
//! feature: a player's build has none of this). The C runtime's allocation
//! functions in `eu4.exe` are observed, and each live block is kept with its
//! call site and its generation, the number of world resets before it was
//! allocated. At every world reset the blocks still live from two generations
//! before are logged by call site: they outlived two resets of the world they
//! were made in.
//!
//! A block `operator new` allocates is counted at the call of `operator new`.
//! Blocks allocated before the framework started are not counted, nor are
//! blocks of other modules' heaps and allocators.
use crate::hooks::HookFrame;
use crate::points::Point;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

/// `(size)`, `(count, size)`, `(block, size)`, `(block)`: the C runtime's
/// allocation functions, on its heap.
const MALLOC: usize = 0x1a6837c;
const CALLOC: usize = 0x1a6989c;
const REALLOC: usize = 0x1a6dd98;
const FREE: usize = 0x1a6833c;
/// `(size)`: `operator new`, which calls [`MALLOC`]; and its end.
const NEW: usize = 0x1a332d4;
const NEW_END: usize = 0x1a33310;

#[derive(Clone, Copy)]
struct Block {
    site: usize,
    size: usize,
    generation: u32,
}

const SHARDS: usize = 64;
type Shard = Mutex<Option<HashMap<usize, Block>>>;
static BLOCKS: [Shard; SHARDS] = [const { Mutex::new(None) }; SHARDS];
static GENERATION: AtomicU32 = AtomicU32::new(0);
static BASE: AtomicUsize = AtomicUsize::new(0);

fn shard(block: usize) -> std::sync::MutexGuard<'static, Option<HashMap<usize, Block>>> {
    BLOCKS[((block >> 4) ^ (block >> 12)) % SHARDS].lock().unwrap_or_else(|e| e.into_inner())
}

fn add(block: usize, site: usize, size: usize) {
    if block != 0 {
        let generation = GENERATION.load(Ordering::Relaxed);
        shard(block).get_or_insert_with(HashMap::new).insert(block, Block { site, size, generation });
    }
}

fn remove(block: usize) {
    if block != 0 {
        if let Some(blocks) = shard(block).as_mut() {
            blocks.remove(&block);
        }
    }
}

/// Whether `site` is inside `operator new`, which counts its own blocks.
fn in_new(site: usize) -> bool {
    let base = BASE.load(Ordering::Relaxed);
    (base + NEW..base + NEW_END).contains(&site)
}

extern "C" fn malloc_after(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault == 0 && !in_new(h.return_address) {
        add(h.result, h.return_address, h.args[0]);
    }
}

extern "C" fn calloc_after(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault == 0 {
        add(h.result, h.return_address, h.args[0].saturating_mul(h.args[1]));
    }
}

extern "C" fn realloc_after(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault == 0 && h.result != 0 {
        remove(h.args[0]);
        add(h.result, h.return_address, h.args[1]);
    }
}

extern "C" fn free_before(frame: *mut HookFrame) {
    remove(unsafe { (*frame).args[0] });
}

extern "C" fn new_after(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault == 0 {
        add(h.result, h.return_address, h.args[0]);
    }
}

pub fn points(m: &crate::memory::Memory) -> Vec<Point> {
    BASE.store(m.at(0), Ordering::Relaxed);
    log!("census: counting the C runtime's heap blocks by call site");
    vec![
        Point::observe(MALLOC, "40 53 48 83 ec 20 48 8b d9 48 83 f9 e0", "census malloc", 1, None, Some(malloc_after)),
        Point::observe(CALLOC, "40 53 48 83 ec 20 4c 8b c2 48 8b d9", "census calloc", 2, None, Some(calloc_after)),
        Point::observe(REALLOC, "48 89 5c 24 08 57 48 83 ec 20 48 8b da 48 8b f9", "census realloc", 2, None, Some(realloc_after)),
        Point::observe(FREE, "48 85 c9 74 37 53 48 83 ec 20", "census free", 1, Some(free_before), None),
        Point::observe(NEW, "40 53 48 83 ec 20 48 8b d9 eb 0f", "census new", 1, None, Some(new_after)),
    ]
}

const MB: f64 = 1024.0 * 1024.0;

/// A world reset: the generation ends, and the blocks of the generation two
/// before that are still live are logged, by call site.
pub fn world_reset() {
    let now = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let base = BASE.load(Ordering::Relaxed);
    let mut generations: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let mut sites: HashMap<usize, (usize, usize)> = HashMap::new();
    let looked_at = now.checked_sub(2);
    for shard in &BLOCKS {
        let shard = shard.lock().unwrap_or_else(|e| e.into_inner());
        for block in shard.iter().flat_map(|blocks| blocks.values()) {
            let g = generations.entry(block.generation).or_default();
            *g = (g.0 + 1, g.1 + block.size);
            if Some(block.generation) == looked_at {
                let s = sites.entry(block.site).or_default();
                *s = (s.0 + 1, s.1 + block.size);
            }
        }
    }
    let by_generation: Vec<String> = generations.iter().map(|(g, (n, bytes))| format!("{g}: {n} blocks {:.1} MB", *bytes as f64 / MB)).collect();
    log!("census: generation {now} begins; live by generation: {}", by_generation.join(", "));
    let Some(looked_at) = looked_at else { return };
    let mut sites: Vec<_> = sites.into_iter().collect();
    sites.sort_by_key(|(_, (_, bytes))| std::cmp::Reverse(*bytes));
    for (site, (count, bytes)) in sites.iter().take(40) {
        let at = if *site >= base && *site - base < 0x4000000 { format!("eu4+{:#x}", site - base) } else { format!("{site:#x}") };
        log!("census: generation {looked_at} left {count} blocks {:.2} MB at {at}", *bytes as f64 / MB);
    }
}
