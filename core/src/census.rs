//! Counts the game's live heap blocks by where they were allocated and when,
//! for finding what keeps the game's memory growing (the `dev-heap-census`
//! feature: a player's build has none of this; docs/leak-finding.md). The C
//! runtime's allocation functions in `eu4.exe` are observed, and each live
//! block is kept with its site, the code that allocated it, and its
//! generation, the number of generations ended before it was allocated. A
//! generation ends at every world reset and when remote control asks
//! (`memory.generation`). A leftover is a block still live two generations
//! after its own: it outlived two resets of the world it was made in. At
//! every world reset the leftovers of the generation two before are logged by
//! site, and remote control's `memory.leftovers` gives them all.
//!
//! A block `operator new` allocates is counted at the call of `operator new`;
//! one the game's generic allocation of a large buffer allocates, or a
//! string's growing, at the two calls above it. For every site the census counts
//! where its blocks were let go: the game's `operator delete` reaches `free`
//! by jumps alone, so `free` returns to the code that deleted the block.
//! Blocks allocated before the framework started are not counted, nor are
//! blocks of other modules' heaps and allocators.
use crate::hooks::HookFrame;
use crate::points::Point;
use eu4_harmony_remote::memory::{Freed, Generation, Holder, Holders, Leftovers, Memory, MemoryAnswer, MemoryRequest, Site, Trend};
use holders::{Held, Scan, Source, Target};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

mod holders;

/// `(size)`, `(count, size)`, `(block, size)`, `(block)`: the C runtime's
/// allocation functions, on its heap.
const MALLOC: usize = 0x1a6837c;
const CALLOC: usize = 0x1a6989c;
const REALLOC: usize = 0x1a6dd98;
const FREE: usize = 0x1a6833c;
/// `(size)`: `operator new`, which calls [`MALLOC`]; and its end.
const NEW: usize = 0x1a332d4;
const NEW_END: usize = 0x1a33310;
/// Where `operator new` returns in the game's generic allocation of a buffer
/// (`0x950b0`) for a large buffer, and, as the generic allocation jumps to it
/// for a small one, where a string growing calls the generic allocation: a
/// block from either is counted at the two calls above it.
const GENERIC: [usize; 2] = [0x950ce, 0x951a1];

/// A site: an RVA, or for a block of the generic allocation the two RVAs
/// above it, `(first << 32) | second`, with [`ABOVE`] set.
type SiteKey = u64;
const ABOVE: u64 = 1 << 63;

#[derive(Clone, Copy)]
struct Block {
    site: SiteKey,
    size: usize,
    generation: u32,
}

#[derive(Default)]
struct Shard {
    blocks: HashMap<usize, Block>,
    /// How many blocks of a site the code at an RVA let go.
    freed: HashMap<(SiteKey, u32), u64>,
}

const SHARDS: usize = 64;
static BLOCKS: [Mutex<Option<Shard>>; SHARDS] = [const { Mutex::new(None) }; SHARDS];
static GENERATION: AtomicU32 = AtomicU32::new(0);
static BASE: AtomicUsize = AtomicUsize::new(0);

fn shard(block: usize) -> std::sync::MutexGuard<'static, Option<Shard>> {
    BLOCKS[((block >> 4) ^ (block >> 12)) % SHARDS].lock().unwrap_or_else(|e| e.into_inner())
}

/// The RVA of an address in `eu4.exe`, or 0 for one outside it.
fn rva(address: usize) -> u32 {
    let base = BASE.load(Ordering::Relaxed);
    match address.checked_sub(base) {
        Some(rva) if rva < 0x4000000 => rva as u32,
        _ => 0,
    }
}

fn add(block: usize, site: SiteKey, size: usize) {
    if block != 0 {
        let generation = GENERATION.load(Ordering::Relaxed);
        shard(block).get_or_insert_with(Shard::default).blocks.insert(block, Block { site, size, generation });
    }
}

/// Takes a block off the census; `by`, the address the code that let it go
/// returns to, when it was let go rather than moved.
fn remove(block: usize, by: Option<usize>) {
    if block == 0 {
        return;
    }
    let mut shard = shard(block);
    let Some(shard) = shard.as_mut() else { return };
    if let (Some(gone), Some(by)) = (shard.blocks.remove(&block), by) {
        *shard.freed.entry((gone.site, rva(by))).or_default() += 1;
    }
}

/// Whether `site` is inside `operator new`, which counts its own blocks.
fn in_new(site: usize) -> bool {
    let base = BASE.load(Ordering::Relaxed);
    (base + NEW..base + NEW_END).contains(&site)
}

/// The site of a block `operator new` returned to `site`: the site itself,
/// or for the generic allocation the two calls above it on the stack.
fn site_of(site: usize) -> SiteKey {
    let site_rva = rva(site);
    if !GENERIC.contains(&(site_rva as usize)) {
        return site_rva as SiteKey;
    }
    let mut frames = [std::ptr::null_mut(); 24];
    let n = unsafe { windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace(0, 24, frames.as_mut_ptr(), std::ptr::null_mut()) } as usize;
    let frames: Vec<usize> = frames[..n].iter().map(|f| *f as usize).collect();
    let Some(at) = frames.iter().position(|f| *f == site) else { return site_rva as SiteKey };
    let mut above = frames[at + 1..].iter().map(|f| rva(*f)).filter(|r| *r != 0 && !GENERIC.contains(&(*r as usize)));
    match (above.next(), above.next()) {
        (Some(first), second) => ABOVE | ((first as u64) << 32) | second.unwrap_or(0) as u64,
        (None, _) => site_rva as SiteKey,
    }
}

extern "C" fn malloc_after(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault == 0 && !in_new(h.return_address) {
        add(h.result, rva(h.return_address) as SiteKey, h.args[0]);
    }
}

extern "C" fn calloc_after(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault == 0 {
        add(h.result, rva(h.return_address) as SiteKey, h.args[0].saturating_mul(h.args[1]));
    }
}

extern "C" fn realloc_after(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault == 0 && h.result != 0 {
        remove(h.args[0], None);
        add(h.result, rva(h.return_address) as SiteKey, h.args[1]);
    }
}

extern "C" fn free_before(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    remove(h.args[0], Some(h.return_address));
}

extern "C" fn new_after(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault == 0 {
        add(h.result, site_of(h.return_address), h.args[0]);
    }
}

pub fn points(m: &crate::memory::Memory) -> Vec<Point> {
    BASE.store(m.base, Ordering::Relaxed);
    log!("census: counting the C runtime's heap blocks by call site");
    vec![
        Point::observe(MALLOC, "40 53 48 83 ec 20 48 8b d9 48 83 f9 e0", "census malloc", 1, None, Some(malloc_after)),
        Point::observe(CALLOC, "40 53 48 83 ec 20 4c 8b c2 48 8b d9", "census calloc", 2, None, Some(calloc_after)),
        Point::observe(REALLOC, "48 89 5c 24 08 57 48 83 ec 20 48 8b da 48 8b f9", "census realloc", 2, None, Some(realloc_after)),
        Point::observe(FREE, "48 85 c9 74 37 53 48 83 ec 20", "census free", 1, Some(free_before), None),
        Point::observe(NEW, "40 53 48 83 ec 20 48 8b d9 eb 0f", "census new", 1, None, Some(new_after)),
        Point::keep(0x950c9usize, "e8 06 e2 99 01 48 8b c8", "the generic allocation calls operator new"),
        Point::keep(0x9519cusize, "e8 0f ff ff ff 4c 8b c6", "a string's buffer from the generic allocation"),
    ]
}

/// Ends the generation, and answers the one that begins.
fn end_generation() -> u32 {
    GENERATION.fetch_add(1, Ordering::Relaxed) + 1
}

/// What the census counts, gathered from every shard: live blocks and bytes
/// by generation, and by site and generation; and how many blocks of each
/// site each place let go.
#[derive(Default)]
struct Counts {
    generations: BTreeMap<u32, (u64, u64)>,
    sites: HashMap<SiteKey, BTreeMap<u32, (u64, u64)>>,
    freed: HashMap<SiteKey, HashMap<u32, u64>>,
}

fn counts(with_sites: bool) -> Counts {
    let mut counts = Counts::default();
    for shard in &BLOCKS {
        let shard = shard.lock().unwrap_or_else(|e| e.into_inner());
        let Some(shard) = shard.as_ref() else { continue };
        for block in shard.blocks.values() {
            let g = counts.generations.entry(block.generation).or_default();
            *g = (g.0 + 1, g.1 + block.size as u64);
            if with_sites {
                let s = counts.sites.entry(block.site).or_default().entry(block.generation).or_default();
                *s = (s.0 + 1, s.1 + block.size as u64);
            }
        }
        if with_sites {
            for ((site, by), n) in &shard.freed {
                *counts.freed.entry(*site).or_default().entry(*by).or_default() += n;
            }
        }
    }
    counts
}

/// How many recent generations [`per_generation`] averages over.
const RECENT: usize = 3;

/// The leftovers of one site, from its live blocks by generation, when the
/// generation is `now`: what each generation old enough left, its trend and
/// what it leaves per generation. Generation 0, which holds what the game
/// loaded at startup, counts toward the trend only as one of the first.
fn leftovers_of(now: u32, live: &BTreeMap<u32, (u64, u64)>) -> (Vec<Generation>, Trend, u64) {
    let Some(last) = now.checked_sub(2) else { return (Vec::new(), Trend::Sometimes, 0) };
    let left: Vec<Generation> = live.range(..=last).map(|(g, (blocks, bytes))| Generation { generation: *g, blocks: *blocks, bytes: *bytes }).collect();
    let bytes_of = |g: u32| left.iter().find(|l| l.generation == g).map_or(0, |l| l.bytes);
    let recent: Vec<u64> = (1..=last).rev().take(RECENT).map(bytes_of).collect();
    let trend = if !recent.is_empty() && recent.iter().all(|b| *b > 0) {
        Trend::Steady
    } else if left.iter().all(|l| l.generation <= 1) {
        Trend::First
    } else {
        Trend::Sometimes
    };
    let per_generation = if recent.is_empty() { 0 } else { recent.iter().sum::<u64>() / recent.len() as u64 };
    (left, trend, per_generation)
}

fn hex(rva: u32) -> String {
    format!("{rva:#x}")
}

/// The start of the function `rva` is in, from the image's exception
/// directory, following chained entries to the function's first.
fn function_of(rva: u32) -> Option<u32> {
    use windows_sys::Win32::System::Diagnostics::Debug::RtlLookupFunctionEntry;
    let base = BASE.load(Ordering::Relaxed);
    if rva == 0 || base == 0 {
        return None;
    }
    let mut image = 0u64;
    let mut entry = unsafe { RtlLookupFunctionEntry(base as u64 + rva as u64, &mut image, std::ptr::null_mut()) };
    if entry.is_null() || image as usize != base {
        return None;
    }
    // UNW_FLAG_CHAININFO: the unwind information ends with the entry this
    // one continues.
    for _ in 0..32 {
        let (begin, unwind) = unsafe { ((*entry).BeginAddress, (*entry).Anonymous.UnwindData) };
        let info = (base + unwind as usize) as *const u8;
        let (flags, codes) = unsafe { (*info >> 3, *info.add(2) as usize) };
        if flags & 4 == 0 {
            return Some(begin);
        }
        entry = unsafe { info.add(4 + codes.div_ceil(2) * 4) } as *mut _;
    }
    None
}

/// A site as remote control writes it: its RVA, or for a block of the
/// generic allocation `"0xA < 0xB"`, the call above it and the one above
/// that; and the function of the first.
fn named(site: SiteKey) -> (String, Option<String>) {
    if site & ABOVE == 0 {
        let rva = site as u32;
        return (hex(rva), function_of(rva).map(hex));
    }
    let (first, second) = (((site >> 32) & 0x7fff_ffff) as u32, site as u32);
    let text = if second == 0 { hex(first) } else { format!("{} < {}", hex(first), hex(second)) };
    (text, function_of(first).map(hex))
}

/// The leftovers, by site, from the census's counts: the `sites` that leave
/// the most per generation first, with what each generation left there and
/// where the site's blocks were let go; and the sites given, in the same
/// order. `name` writes a site and its function.
fn leftovers_from(now: u32, counts: &Counts, sites: usize, name: impl Fn(SiteKey) -> (String, Option<String>)) -> (Leftovers, Vec<SiteKey>) {
    let mut all: Vec<(SiteKey, Vec<Generation>, Trend, u64)> = counts
        .sites
        .iter()
        .map(|(site, live)| {
            let (left, trend, per_generation) = leftovers_of(now, live);
            (*site, left, trend, per_generation)
        })
        .filter(|(_, left, _, _)| !left.is_empty())
        .collect();
    let total = |left: &[Generation]| (left.iter().map(|l| l.blocks).sum::<u64>(), left.iter().map(|l| l.bytes).sum::<u64>());
    let (blocks, bytes) = all.iter().fold((0, 0), |(n, b), (_, left, _, _)| {
        let (ln, lb) = total(left);
        (n + ln, b + lb)
    });
    all.sort_by_key(|(site, left, _, per_generation)| (std::cmp::Reverse(*per_generation), std::cmp::Reverse(total(left).1), *site));
    all.truncate(sites);
    let keys = all.iter().map(|(site, ..)| *site).collect();
    let sites = all
        .into_iter()
        .map(|(site, generations, trend, per_generation)| {
            let (site_text, function) = name(site);
            let mut freed: Vec<(u32, u64)> = counts.freed.get(&site).map(|by| by.iter().map(|(by, n)| (*by, *n)).collect()).unwrap_or_default();
            freed.sort_by_key(|(by, n)| (std::cmp::Reverse(*n), *by));
            let freed_at = freed
                .into_iter()
                .take(5)
                .map(|(by, n)| {
                    let (by_text, by_function) = name(by as SiteKey);
                    Freed { site: by_text, function: by_function, blocks: n }
                })
                .collect();
            let (blocks, bytes) = total(&generations);
            Site { site: site_text, function, trend, per_generation, blocks, bytes, generations, freed_at, holders: None }
        })
        .collect();
    (Leftovers { generation: now, blocks, bytes, sites }, keys)
}

/// The game's writable sections, by RVA and size, from its headers.
fn writable_sections(m: &crate::memory::Memory) -> Vec<(u32, usize)> {
    const WRITE: u32 = 0x8000_0000;
    let read = || -> crate::Result<Vec<(u32, usize)>> {
        let nt = m.base + m.u32(m.base + 0x3c)? as usize;
        let count = m.u16(nt + 6)? as usize;
        let first = nt + 24 + m.u16(nt + 20)? as usize;
        let mut sections = Vec::new();
        for i in 0..count {
            let header = first + i * 40;
            let (size, rva, flags) = (m.u32(header + 8)? as usize, m.u32(header + 12)?, m.u32(header + 36)?);
            if flags & WRITE != 0 {
                sections.push((rva, size));
            }
        }
        Ok(sections)
    };
    read().unwrap_or_default()
}

/// Reads `size` bytes from `start` into the scan, a megabyte at a time;
/// what cannot be read is passed over.
fn scan_range(m: &crate::memory::Memory, scan: &mut Scan, buffer: &mut [u8], source: Source, start: usize, size: usize) {
    let mut offset = 0;
    while offset + 8 <= size {
        let n = (size - offset).min(buffer.len()) & !7;
        if m.read_into(start + offset, &mut buffer[..n]).is_ok() {
            scan.read(source, offset, &buffer[..n]);
        }
        offset += n;
    }
}

/// What still points at the leftovers of `sites` when the generation is
/// `now`: the game's writable data and every live block the census counts
/// are read, with every shard of the census locked, so that none of those
/// blocks is let go meanwhile; the game's threads wait at their next
/// allocation until the reading is done.
fn holders_of(now: u32, sites: &[SiteKey]) -> HashMap<SiteKey, Holders> {
    let Some(last) = now.checked_sub(2) else { return HashMap::new() };
    let began = std::time::Instant::now();
    let m = crate::memory::Memory { base: BASE.load(Ordering::Relaxed) };
    let wanted: HashSet<SiteKey> = sites.iter().copied().collect();
    let shards: Vec<_> = BLOCKS.iter().map(|s| s.lock().unwrap_or_else(|e| e.into_inner())).collect();
    let live = || shards.iter().filter_map(|s| s.as_ref()).flat_map(|s| s.blocks.iter());
    let targets: Vec<Target> = live()
        .filter(|(_, b)| b.generation <= last && wanted.contains(&b.site))
        .map(|(start, b)| Target { start: *start, size: b.size, site: b.site })
        .collect();
    let looked_for = targets.len();
    let mut scan = Scan::new(targets);
    let mut buffer = vec![0u8; 1 << 20];
    let mut read = 0usize;
    for (rva, size) in writable_sections(&m) {
        scan_range(&m, &mut scan, &mut buffer, Source::Data { rva }, m.base + rva as usize, size);
        read += size;
    }
    for (start, block) in live() {
        scan_range(&m, &mut scan, &mut buffer, Source::Block { start: *start, site: block.site }, *start, block.size);
        read += block.size;
    }
    drop(shards);
    log!(
        "census: looked for what points at {looked_for} leftovers of {} sites in {:.0} MB, in {} ms",
        sites.len(),
        read as f64 / MB,
        began.elapsed().as_millis()
    );
    scan.found()
        .into_iter()
        .map(|(site, (held, references))| {
            let count = |h: Held| held.get(&h).copied().unwrap_or(0);
            let mut top: Vec<(holders::Holder, u64)> = references.into_iter().collect();
            top.sort_by_key(|(holder, n)| (std::cmp::Reverse(*n), *holder));
            let top = top
                .into_iter()
                .take(5)
                .map(|(holder, references)| match holder {
                    holders::Holder::Data(rva) => Holder { kind: "data".into(), at: hex(rva), function: None, references },
                    holders::Holder::Site(site) => {
                        let (at, function) = named(site);
                        Holder { kind: "block".into(), at, function, references }
                    }
                })
                .collect();
            let holders = Holders {
                by_data: count(Held::Data),
                by_blocks: count(Held::Blocks),
                by_leftovers: count(Held::Leftovers),
                by_nothing: count(Held::Nothing),
                top,
            };
            (site, holders)
        })
        .collect()
}

/// What the process holds now, and the census's live blocks by generation.
fn memory() -> Memory {
    let counts = counts(false);
    let (allocated, committed) = crate::heap::heaps();
    let private = crate::heap::private_bytes();
    let mut handles = 0u32;
    unsafe { windows_sys::Win32::System::Threading::GetProcessHandleCount(windows_sys::Win32::System::Threading::GetCurrentProcess(), &mut handles) };
    Memory {
        private: private as u64,
        heaps_allocated: allocated as u64,
        heaps_committed: committed as u64,
        outside_heaps: private.saturating_sub(committed) as u64,
        framework_heap: crate::heap::live() as u64,
        handles,
        generation: GENERATION.load(Ordering::Relaxed),
        generations: counts.generations.iter().map(|(g, (blocks, bytes))| Generation { generation: *g, blocks: *blocks, bytes: *bytes }).collect(),
    }
}

/// Answers remote control's memory group.
pub fn answer(request: MemoryRequest) -> MemoryAnswer {
    match request {
        MemoryRequest::Get {} => MemoryAnswer::Get(memory()),
        MemoryRequest::Leftovers { sites, holders } => {
            let now = GENERATION.load(Ordering::Relaxed);
            let (mut leftovers, keys) = leftovers_from(now, &counts(true), sites.unwrap_or(40) as usize, named);
            if holders == Some(true) {
                let mut found = holders_of(now, &keys);
                for (site, key) in leftovers.sites.iter_mut().zip(&keys) {
                    site.holders = Some(found.remove(key).unwrap_or(Holders { by_data: 0, by_blocks: 0, by_leftovers: 0, by_nothing: 0, top: Vec::new() }));
                }
            }
            MemoryAnswer::Leftovers(leftovers)
        }
        MemoryRequest::Generation {} => {
            let now = end_generation();
            log!("census: generation {now} begins, as remote control asked");
            MemoryAnswer::Generation(now)
        }
    }
}

const MB: f64 = 1024.0 * 1024.0;

/// A world reset: the generation ends, and the leftovers of the generation
/// two before are logged, by site.
pub fn world_reset() {
    let now = end_generation();
    let counts = counts(true);
    let by_generation: Vec<String> = counts.generations.iter().map(|(g, (n, bytes))| format!("{g}: {n} blocks {:.1} MB", *bytes as f64 / MB)).collect();
    log!("census: generation {now} begins; live by generation: {}", by_generation.join(", "));
    let Some(looked_at) = now.checked_sub(2) else { return };
    let mut sites: Vec<(SiteKey, (u64, u64))> = counts.sites.iter().filter_map(|(site, live)| live.get(&looked_at).map(|left| (*site, *left))).collect();
    sites.sort_by_key(|(site, (_, bytes))| (std::cmp::Reverse(*bytes), *site));
    for (site, (count, bytes)) in sites.iter().take(40) {
        log!("census: generation {looked_at} left {count} blocks {:.2} MB at {}", *bytes as f64 / MB, named(*site).0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(of: &[(u32, u64)]) -> BTreeMap<u32, (u64, u64)> {
        of.iter().map(|(g, bytes)| (*g, (1, *bytes))).collect()
    }

    #[test]
    fn a_site_every_generation_leaves_to_is_steady() {
        // Generation 7 now: 0 to 5 are old enough; 6 is not, and is not counted.
        let (left, trend, per_generation) = leftovers_of(7, &live(&[(0, 900), (2, 100), (3, 200), (4, 300), (5, 400), (6, 50)]));
        assert_eq!(trend, Trend::Steady);
        assert_eq!(per_generation, 300);
        assert_eq!(left.iter().map(|l| l.generation).collect::<Vec<_>>(), [0, 2, 3, 4, 5]);
    }

    #[test]
    fn a_site_only_the_first_generations_left_to_is_a_cache() {
        let (_, trend, per_generation) = leftovers_of(6, &live(&[(0, 900), (1, 300), (5, 10)]));
        assert_eq!(trend, Trend::First);
        assert_eq!(per_generation, 0);
        let (_, trend, _) = leftovers_of(6, &live(&[(1, 300), (3, 20)]));
        assert_eq!(trend, Trend::Sometimes);
    }

    #[test]
    fn nothing_is_a_leftover_before_two_generations_have_ended() {
        assert!(leftovers_of(1, &live(&[(0, 900), (1, 10)])).0.is_empty());
        let (left, trend, per_generation) = leftovers_of(2, &live(&[(0, 900), (1, 10)]));
        assert_eq!((left.len(), trend, per_generation), (1, Trend::First, 0));
    }

    #[test]
    fn the_sites_that_leave_most_per_generation_come_first() {
        let mut counts = Counts::default();
        counts.sites.insert(0x10, live(&[(0, 1 << 30)]));
        counts.sites.insert(0x20, live(&[(1, 10), (2, 10), (3, 10)]));
        counts.sites.insert(0x30, live(&[(1, 50), (2, 50), (3, 50), (4, 7)]));
        counts.sites.insert(ABOVE | (0x40 << 32) | 0x41, live(&[(4, 7)]));
        counts.freed.insert(0x30, HashMap::from([(0x90, 2), (0x91, 9)]));
        let name = |site: SiteKey| (format!("{site:#x}"), None);
        let (leftovers, keys) = leftovers_from(5, &counts, 2, name);
        assert_eq!(keys, [0x30, 0x20]);
        assert_eq!(leftovers.sites.iter().map(|s| s.site.as_str()).collect::<Vec<_>>(), ["0x30", "0x20"]);
        assert_eq!(leftovers.sites[0].per_generation, 50);
        assert_eq!(leftovers.sites[0].bytes, 150);
        assert_eq!(leftovers.sites[0].freed_at.iter().map(|f| (f.site.as_str(), f.blocks)).collect::<Vec<_>>(), [("0x91", 9), ("0x90", 2)]);
        assert_eq!((leftovers.blocks, leftovers.bytes), (7, (1 << 30) + 180));
    }
}
