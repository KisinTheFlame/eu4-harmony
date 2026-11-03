//! Who let a leftover's owner go (docs/leak-finding.md, M3): while the census
//! watches some sites, every block the game lets go is read first, and a word
//! of it that points into a live block of a watched site
//! ([`super::pointers`]) is kept with the block's site and the code that let
//! it go, the last such for each watched block. A leftover nothing points at
//! any more, whose last owner was let go so, names the destructor that should
//! have let it go too.
//!
//! Owners let go only after the pointer was copied elsewhere, as a vector's
//! buffer is when it grows, are kept too; the leftover the report names them
//! for is one nothing points at, so its last owner is the one that matters.
use super::pointers::Index;
use super::SiteKey;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// The most sites watched at once.
pub const MOST: usize = 16;
/// The most of a block let go that is read: a buffer of pointers larger than
/// this is read in its first part only.
const READ_AT_MOST: usize = 4 << 20;

static ON: AtomicBool = AtomicBool::new(false);
static SITES: [AtomicU64; MOST] = [const { AtomicU64::new(0) }; MOST];
static COUNT: AtomicUsize = AtomicUsize::new(0);
/// The live blocks of the watched sites, with their site.
static LIVE: OnceLock<Index<SiteKey>> = OnceLock::new();
/// By a live watched block's start: the site of the last block let go while
/// it pointed into it, and the RVA of the code that let it go.
static LOST: Mutex<Option<HashMap<usize, (SiteKey, u32)>>> = Mutex::new(None);

fn live() -> &'static Index<SiteKey> {
    LIVE.get_or_init(Index::new)
}

fn lost() -> std::sync::MutexGuard<'static, Option<HashMap<usize, (SiteKey, u32)>>> {
    LOST.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether blocks of `site` are watched.
pub fn watches(site: SiteKey) -> bool {
    ON.load(Ordering::Relaxed) && SITES[..COUNT.load(Ordering::Relaxed)].iter().any(|s| s.load(Ordering::Relaxed) == site)
}

/// The sites watched now.
pub fn sites() -> Vec<SiteKey> {
    if !ON.load(Ordering::Relaxed) {
        return Vec::new();
    }
    SITES[..COUNT.load(Ordering::Relaxed)].iter().map(|s| s.load(Ordering::Relaxed)).collect()
}

/// Watches `sites` (at most [`MOST`]) from now on, with the live blocks
/// `blocks` gives of them, by start, size and site; no site stops it.
pub fn start(sites: &[SiteKey], blocks: impl Iterator<Item = (usize, usize, SiteKey)>) {
    let mut lost = lost();
    ON.store(false, Ordering::Relaxed);
    *lost = None;
    live().clear();
    let sites = &sites[..sites.len().min(MOST)];
    for (slot, site) in SITES.iter().zip(sites) {
        slot.store(*site, Ordering::Relaxed);
    }
    COUNT.store(sites.len(), Ordering::Relaxed);
    if sites.is_empty() {
        return;
    }
    for (start, size, site) in blocks.filter(|(_, _, site)| sites.contains(site)) {
        live().insert(start, size, site);
    }
    *lost = Some(HashMap::new());
    ON.store(true, Ordering::Relaxed);
}

/// A block of a watched site was allocated.
pub fn added(start: usize, size: usize, site: SiteKey) {
    live().insert(start, size, site);
}

/// A block of a watched site was let go or moved.
pub fn removed(start: usize) {
    live().remove(start);
    if let Some(lost) = lost().as_mut() {
        lost.remove(&start);
    }
}

/// A block of `site` is about to be let go by the code at `by`: what it
/// points into among the watched blocks is kept. `read` copies the block's
/// bytes from an offset into a buffer, false when it cannot.
pub fn letting_go(start: usize, size: usize, site: SiteKey, by: u32, mut read: impl FnMut(usize, &mut [u8]) -> bool) {
    if !ON.load(Ordering::Relaxed) || size < 8 {
        return;
    }
    let mut buffer = [0u8; 4096];
    let mut candidates = Vec::new();
    let end = size.min(READ_AT_MOST) & !7;
    let mut offset = 0;
    while offset < end {
        let n = (end - offset).min(buffer.len());
        if read(offset, &mut buffer[..n]) {
            candidates.extend(live().candidates(&buffer[..n]));
        }
        offset += n;
    }
    if candidates.is_empty() {
        return;
    }
    let pointed = live().pointed(&candidates);
    let mut lost = lost();
    let Some(lost) = lost.as_mut() else { return };
    for (_, child, _) in pointed.into_iter().filter(|(_, child, _)| *child != start) {
        lost.insert(child, (site, by));
    }
}

/// The last owner let go while it pointed into the watched block at
/// `start`, and the code that let it go.
pub fn last_owner(start: usize) -> Option<(SiteKey, u32)> {
    lost().as_ref()?.get(&start).copied()
}

#[cfg(test)]
pub(super) fn reset() {
    start(&[], std::iter::empty());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_from(words: Vec<usize>) -> impl FnMut(usize, &mut [u8]) -> bool {
        let bytes: Vec<u8> = words.iter().flat_map(|w| (*w as u64).to_le_bytes()).collect();
        move |offset, out| {
            out.copy_from_slice(&bytes[offset..offset + out.len()]);
            true
        }
    }

    #[test]
    fn the_last_owner_let_go_is_kept_until_the_block_goes() {
        // Static state: one test does it all.
        let (a, b) = (0x7000_1000usize, 0x7000_2000usize);
        start(&[5], [(a, 0x40, 5), (b, 0x40, 5), (0x7000_3000, 0x40, 6)].into_iter());
        assert!(watches(5) && !watches(6));
        assert_eq!(sites(), [5]);
        // A buffer of site 9 holding a and, past its first bytes, b, let go
        // at 0x100; then an owner of site 7 holding a, let go at 0x200.
        letting_go(0x9000_0000, 24, 9, 0x100, read_from(vec![a + 8, b + 0x80, 3]));
        assert_eq!(last_owner(a), Some((9, 0x100)));
        assert_eq!(last_owner(b), None);
        letting_go(0x9000_1000, 8, 7, 0x200, read_from(vec![a]));
        assert_eq!(last_owner(a), Some((7, 0x200)));
        // A block pointing at itself is no owner of itself.
        letting_go(b, 16, 5, 0x300, read_from(vec![b, 0]));
        assert_eq!(last_owner(b), None);
        // A new block of a watched site is watched; one let go is forgotten.
        added(0x7000_4000, 0x10, 5);
        letting_go(0x9000_2000, 8, 8, 0x400, read_from(vec![0x7000_4000]));
        assert_eq!(last_owner(0x7000_4000), Some((8, 0x400)));
        removed(a);
        assert_eq!(last_owner(a), None);
        reset();
        assert!(sites().is_empty() && !watches(5));
        assert!(live().pointed(&[(0, b), (0, 0x7000_4000)]).is_empty());
    }
}
