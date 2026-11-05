//! The game's heap's blocks the census does not count (docs/leak-finding.md):
//! a walk of the heap gives every busy block, and those the census has no
//! record of are other modules', the game's own made before the framework
//! started or not through its C runtime, and the framework's. Each ask
//! marks them by start and size; the next tells, size by size, how many
//! more there are, and reads samples of those made since, so that what
//! grows outside the census can be told by size and by what it points into.
//! The buffers of the walk and of the mark are the framework's, made for the
//! ask, and are left out.
use super::pages::Walk;
use eu4_harmony_remote::memory::{Uncounted, UncountedSample, UncountedSize};
use std::collections::HashMap;
use std::sync::Mutex;

/// The blocks of the last ask, `(start, size)` by start.
static MARK: Mutex<Option<Vec<(usize, usize)>>> = Mutex::new(None);

/// How many bytes of a sample are read, and of those how many hold the
/// words looked up.
const READ: usize = 64;

/// What `walk` holds that `counted` does not know, against the last mark,
/// which it then replaces.
pub fn uncounted(walk: &Walk, counted: impl Fn(usize) -> bool, sizes: usize, samples: usize) -> Uncounted {
    let mut mark = MARK.lock().unwrap_or_else(|e| e.into_inner());
    let own =
        [walk.busy.as_ptr() as usize, walk.regions.as_ptr() as usize, walk.uncommitted.as_ptr() as usize, mark.as_ref().map_or(0, |m| m.as_ptr() as usize)];
    let mut now: Vec<(usize, usize)> = walk.busy.iter().copied().filter(|(start, _)| !own.contains(start) && !counted(*start)).collect();
    now.sort_unstable();
    let found = compare(&now, mark.as_deref(), sizes, samples);
    *mark = Some(now);
    let sizes = found
        .sizes
        .into_iter()
        .map(|s| UncountedSize {
            size: s.size as u64,
            blocks: s.blocks,
            grew: s.grew,
            made: s.made,
            samples: s.samples.into_iter().filter_map(|start| sample(start, s.size)).collect(),
        })
        .collect();
    Uncounted {
        blocks: found.blocks,
        bytes: found.bytes,
        marked: found.marked,
        made_blocks: found.made_blocks,
        made_bytes: found.made_bytes,
        sizes,
        complete: walk.complete,
        took_ms: 0,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Compared {
    blocks: u64,
    bytes: u64,
    marked: bool,
    made_blocks: u64,
    made_bytes: u64,
    sizes: Vec<BySize>,
}

#[derive(Debug, PartialEq, Eq)]
struct BySize {
    size: usize,
    blocks: u64,
    grew: i64,
    made: u64,
    /// The starts of blocks made since the mark, the lowest first.
    samples: Vec<usize>,
}

/// The blocks `now` against those of `mark`, each by start, size by size,
/// the most bytes grown first, or with no mark the most bytes.
fn compare(now: &[(usize, usize)], mark: Option<&[(usize, usize)]>, sizes: usize, samples: usize) -> Compared {
    let mut by_size: HashMap<usize, (u64, i64, Vec<usize>)> = HashMap::new();
    let mut found = Compared { marked: mark.is_some(), ..Compared::default() };
    for &(start, size) in now {
        found.blocks += 1;
        found.bytes += size as u64;
        let entry = by_size.entry(size).or_default();
        entry.0 += 1;
        entry.1 += 1;
        if mark.is_some_and(|m| m.binary_search(&(start, size)).is_err()) {
            entry.2.push(start);
            found.made_blocks += 1;
            found.made_bytes += size as u64;
        }
    }
    if let Some(mark) = mark {
        for &(_, size) in mark {
            by_size.entry(size).or_default().1 -= 1;
        }
    }
    let mut ordered: Vec<BySize> = by_size
        .into_iter()
        .filter(|(_, (blocks, _, _))| *blocks > 0)
        .map(|(size, (blocks, grew, mut made))| {
            made.sort_unstable();
            let count = made.len() as u64;
            made.truncate(samples);
            BySize { size, blocks, grew: if mark.is_some() { grew } else { 0 }, made: count, samples: made }
        })
        .collect();
    let weight = |s: &BySize| if mark.is_some() { s.grew * s.size as i64 } else { (s.blocks * s.size as u64) as i64 };
    ordered.sort_by(|a, b| weight(b).cmp(&weight(a)).then(a.size.cmp(&b.size)));
    ordered.truncate(sizes);
    found.sizes = ordered;
    found
}

/// The first bytes of the block at `start` and what its words point into,
/// read so that a block let go since the walk cannot fault.
fn sample(start: usize, size: usize) -> Option<UncountedSample> {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut bytes = [0u8; READ];
    let want = size.min(READ);
    let mut read = 0usize;
    let ok = unsafe { ReadProcessMemory(GetCurrentProcess(), start as _, bytes.as_mut_ptr().cast(), want, &mut read) };
    if ok == 0 || read == 0 {
        return None;
    }
    let bytes = &bytes[..read];
    let points = bytes.chunks_exact(8).filter_map(|w| module_of(usize::from_le_bytes(w.try_into().ok()?))).collect::<Vec<_>>();
    Some(UncountedSample { address: format!("{start:#x}"), hex: hex(bytes), text: text(bytes), points })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

fn text(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }).collect()
}

/// `address` as module+offset, when a loaded module's image holds it.
fn module_of(address: usize) -> Option<String> {
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };
    if address < 0x10000 {
        return None;
    }
    let mut module = std::ptr::null_mut();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    if unsafe { GetModuleHandleExW(flags, address as _, &mut module) } == 0 {
        return None;
    }
    let mut name = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
    let path = String::from_utf16_lossy(&name[..len]);
    let file = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
    Some(format!("{file}+{:#x}", address - module as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(of: &[(usize, usize)]) -> Vec<(usize, usize)> {
        let mut blocks = of.to_vec();
        blocks.sort_unstable();
        blocks
    }

    #[test]
    fn without_a_mark_the_most_bytes_come_first() {
        let now = blocks(&[(0x100, 16), (0x200, 16), (0x300, 64)]);
        let found = compare(&now, None, 10, 3);
        assert!(!found.marked);
        assert_eq!((found.blocks, found.bytes, found.made_blocks), (3, 96, 0));
        assert_eq!(found.sizes.iter().map(|s| (s.size, s.blocks, s.grew, s.made)).collect::<Vec<_>>(), [(64, 1, 0, 0), (16, 2, 0, 0)]);
    }

    #[test]
    fn against_a_mark_what_grew_comes_first_with_what_was_made() {
        let mark = blocks(&[(0x100, 16), (0x200, 16), (0x300, 64), (0x400, 32)]);
        // 0x200 went and 0x500 came, both 16; 0x400 went; two of 48 came.
        let now = blocks(&[(0x100, 16), (0x500, 16), (0x300, 64), (0x600, 48), (0x700, 48)]);
        let found = compare(&now, Some(&mark), 10, 1);
        assert!(found.marked);
        assert_eq!((found.made_blocks, found.made_bytes), (3, 112));
        let sizes: Vec<_> = found.sizes.iter().map(|s| (s.size, s.blocks, s.grew, s.made, s.samples.clone())).collect();
        assert_eq!(sizes, [(48, 2, 2, 2, vec![0x600]), (16, 2, 0, 1, vec![0x500]), (64, 1, 0, 0, vec![])]);
    }

    #[test]
    fn a_block_of_another_size_at_a_start_is_made_since() {
        let mark = blocks(&[(0x100, 16)]);
        let found = compare(&blocks(&[(0x100, 32)]), Some(&mark), 10, 3);
        assert_eq!(found.made_blocks, 1);
        assert_eq!(found.sizes.iter().map(|s| (s.size, s.grew)).collect::<Vec<_>>(), [(32, 1)]);
    }

    #[test]
    fn a_sample_reads_its_bytes_and_names_the_module_its_words_point_into() {
        let here = a_sample_reads_its_bytes_and_names_the_module_its_words_point_into as fn() as usize;
        let mut block = [0u8; 24];
        block[..8].copy_from_slice(&here.to_le_bytes());
        block[8..13].copy_from_slice(b"hello");
        let found = sample(block.as_ptr() as usize, block.len()).unwrap();
        assert_eq!(found.points.len(), 1, "{found:?}");
        assert!(found.points[0].contains(".exe+0x"), "{found:?}");
        assert!(found.text[8..13] == *"hello" && found.hex.split(' ').count() == 24, "{found:?}");
    }
}
