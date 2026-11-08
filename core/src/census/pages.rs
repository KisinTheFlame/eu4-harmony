//! What keeps the heap's free pages committed (docs/plans/leak-finding.md, M5): a
//! walk of the game's heap gives its committed pages and every busy block on
//! them, the small blocks of its low-fragmentation heap one by one; a page
//! that holds blocks stays committed however little of it they use, so its
//! free bytes are shared out among the blocks on it, and through the census
//! among their sites. A page no block is on is free as a whole.
//!
//! The walk holds the heap's lock, and the framework allocates from the
//! same heap: everything it fills is allocated before.
use super::SiteKey;
use std::collections::HashMap;

pub const PAGE: usize = 4096;
/// How full a page in use is, in eighths: `by_use[0]` is up to an eighth.
pub const PARTS: usize = 8;

/// What a walk of a heap found: its committed ranges, its uncommitted ranges
/// inside them, and its busy blocks, each by start and bytes.
#[derive(Debug, Default)]
pub struct Walk {
    pub regions: Vec<(usize, usize)>,
    pub uncommitted: Vec<(usize, usize)>,
    pub busy: Vec<(usize, usize)>,
    /// False when there were more entries than the buffers held.
    pub complete: bool,
}

/// Walks `heap` under its lock, keeping at most `most` busy blocks and as
/// many ranges.
pub fn walk(heap: usize, most: usize) -> Walk {
    use windows_sys::Win32::System::Memory::{HeapLock, HeapUnlock, HeapWalk, PROCESS_HEAP_ENTRY};
    // An entry's kind, in its flags (winnt.h).
    const PROCESS_HEAP_REGION: u32 = 1;
    const PROCESS_HEAP_UNCOMMITTED_RANGE: u32 = 2;
    const PROCESS_HEAP_ENTRY_BUSY: u32 = 4;
    let ranges = most / 64 + 1024;
    let mut walk = Walk { regions: Vec::with_capacity(ranges), uncommitted: Vec::with_capacity(ranges), busy: Vec::with_capacity(most), complete: true };
    let heap = heap as _;
    if unsafe { HeapLock(heap) } == 0 {
        walk.complete = false;
        return walk;
    }
    let mut entry: PROCESS_HEAP_ENTRY = unsafe { std::mem::zeroed() };
    // Nothing here allocates: each push is within the capacity.
    while unsafe { HeapWalk(heap, &mut entry) } != 0 {
        let flags = entry.wFlags as u32;
        let (list, at) = if flags & PROCESS_HEAP_REGION != 0 {
            let region = unsafe { entry.Anonymous.Region };
            (&mut walk.regions, (region.lpFirstBlock as usize, region.lpLastBlock as usize - region.lpFirstBlock as usize))
        } else if flags & PROCESS_HEAP_UNCOMMITTED_RANGE != 0 {
            (&mut walk.uncommitted, (entry.lpData as usize, entry.cbData as usize))
        } else if flags & PROCESS_HEAP_ENTRY_BUSY != 0 {
            (&mut walk.busy, (entry.lpData as usize, entry.cbData as usize + entry.cbOverhead as usize))
        } else {
            continue;
        };
        if list.len() == list.capacity() {
            walk.complete = false;
            break;
        }
        list.push(at);
    }
    unsafe { HeapUnlock(heap) };
    walk
}

/// A committed page: the bytes busy blocks use of it, and how many are on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub used: u32,
    pub blocks: u32,
}

/// The pages of a range, with the bytes of it on each.
fn spread(start: usize, bytes: usize) -> impl Iterator<Item = (usize, usize)> {
    let end = start + bytes;
    (start / PAGE..end.div_ceil(PAGE)).map(move |page| (page, end.min((page + 1) * PAGE) - start.max(page * PAGE)))
}

/// The committed pages, by number: those of the regions, less their
/// uncommitted ranges, and those busy blocks lie on.
pub fn pages(walk: &Walk) -> HashMap<usize, Page> {
    let mut pages: HashMap<usize, Page> = HashMap::new();
    for (start, bytes) in &walk.regions {
        for (page, _) in spread(*start, *bytes) {
            pages.entry(page).or_default();
        }
    }
    for (start, bytes) in &walk.uncommitted {
        // Whole pages only: an uncommitted range begins and ends on them.
        for page in start.div_ceil(PAGE)..(start + bytes) / PAGE {
            pages.remove(&page);
        }
    }
    for (start, bytes) in &walk.busy {
        for (page, on) in spread(*start, (*bytes).max(1)) {
            let p = pages.entry(page).or_default();
            p.used = (p.used + on as u32).min(PAGE as u32);
            p.blocks += 1;
        }
    }
    pages
}

/// The committed pages in all: their bytes, the bytes blocks use, the bytes
/// of pages no block is on, the free bytes of pages blocks are on, and those
/// pages by how full they are.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub committed: u64,
    pub used: u64,
    pub free_pages: u64,
    pub free_on_used_pages: u64,
    pub by_use: [u64; PARTS],
}

pub fn totals(pages: &HashMap<usize, Page>) -> Totals {
    let mut t = Totals::default();
    for p in pages.values() {
        t.committed += PAGE as u64;
        t.used += p.used as u64;
        if p.blocks == 0 {
            t.free_pages += PAGE as u64;
        } else {
            t.free_on_used_pages += (PAGE as u32 - p.used) as u64;
            t.by_use[((p.used as usize * PARTS).saturating_sub(1) / PAGE).min(PARTS - 1)] += 1;
        }
    }
    t
}

/// What a site's blocks keep committed: how many lie on pages in use, their
/// bytes, the free bytes of those pages that fall to them, shared out among
/// a page's blocks, and how many of them are leftovers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pin {
    pub blocks: u64,
    pub bytes: u64,
    pub pinned: f64,
    pub leftovers: u64,
}

/// By site, what busy blocks pin, `None` for blocks the census does not
/// count; `site_of` gives a block's site and whether it is a leftover. And
/// the free bytes leftovers pin, of all of them.
pub fn pinning(
    pages: &HashMap<usize, Page>,
    busy: &[(usize, usize)],
    site_of: impl Fn(usize) -> Option<(SiteKey, bool)>,
) -> (HashMap<Option<SiteKey>, Pin>, f64) {
    let mut by_site: HashMap<Option<SiteKey>, Pin> = HashMap::new();
    let mut by_leftovers = 0.0;
    for (start, bytes) in busy {
        let found = site_of(*start);
        let leftover = found.is_some_and(|(_, l)| l);
        let pinned: f64 =
            spread(*start, (*bytes).max(1)).filter_map(|(page, _)| pages.get(&page)).map(|p| (PAGE as u32 - p.used) as f64 / p.blocks.max(1) as f64).sum();
        let pin = by_site.entry(found.map(|(site, _)| site)).or_default();
        pin.blocks += 1;
        pin.bytes += *bytes as u64;
        pin.pinned += pinned;
        if leftover {
            pin.leftovers += 1;
            by_leftovers += pinned;
        }
    }
    (by_site, by_leftovers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: usize = 0x10_0000;

    fn walk(busy: Vec<(usize, usize)>) -> Walk {
        // Eight pages, the last three uncommitted.
        Walk { regions: vec![(BASE, 8 * PAGE)], uncommitted: vec![(BASE + 5 * PAGE, 3 * PAGE)], busy, complete: true }
    }

    #[test]
    fn a_page_is_as_full_as_the_blocks_on_it() {
        // Page 0: two blocks of 512; page 1: one of 256; pages 2 and 3: one
        // block of 6000 across them; page 4: nothing.
        let w = walk(vec![(BASE, 512), (BASE + 1024, 512), (BASE + PAGE, 256), (BASE + 2 * PAGE + 96, 6000)]);
        let pages = pages(&w);
        assert_eq!(pages.len(), 5);
        assert_eq!(pages[&(BASE / PAGE)], Page { used: 1024, blocks: 2 });
        assert_eq!(pages[&(BASE / PAGE + 2)], Page { used: 4000, blocks: 1 });
        assert_eq!(pages[&(BASE / PAGE + 3)], Page { used: 2000, blocks: 1 });
        let t = totals(&pages);
        assert_eq!(t.committed, 5 * PAGE as u64);
        assert_eq!(t.used, 1024 + 256 + 6000);
        assert_eq!(t.free_pages, PAGE as u64);
        assert_eq!(t.free_on_used_pages, (4 * PAGE - 1024 - 256 - 6000) as u64);
        // 1024 and 256 of 4096 in the first eighths, 4000 in the last, 2000
        // in the fourth.
        assert_eq!(t.by_use, [1, 1, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn a_page_s_free_bytes_fall_to_its_blocks_alike() {
        let busy = vec![(BASE, 512), (BASE + 1024, 512), (BASE + PAGE, 256), (BASE + 2 * PAGE, 96)];
        let w = walk(busy.clone());
        let pages = pages(&w);
        // Sites: 1 and 2 share page 0, 1 has page 1 alone; the last block is
        // not counted.
        let site_of = |start: usize| match start - BASE {
            0 => Some((1, true)),
            1024 => Some((2, false)),
            x if x == PAGE => Some((1, false)),
            _ => None,
        };
        let (by_site, by_leftovers) = pinning(&pages, &busy, site_of);
        let half = (PAGE - 1024) as f64 / 2.0;
        assert_eq!(by_site[&Some(1)], Pin { blocks: 2, bytes: 768, pinned: half + (PAGE - 256) as f64, leftovers: 1 });
        assert_eq!(by_site[&Some(2)], Pin { blocks: 1, bytes: 512, pinned: half, leftovers: 0 });
        assert_eq!(by_site[&None].pinned, (PAGE - 96) as f64);
        assert_eq!(by_leftovers, half);
    }
}
