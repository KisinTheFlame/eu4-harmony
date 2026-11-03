//! Which of some blocks a word of memory points into: the one rule what still
//! points at a leftover (M2) and who let a watched block's owner go (M3)
//! both read memory by (docs/leak-finding.md). Eight bytes point into a block
//! when they hold an address in its first [`INTO`] bytes, where an object's
//! bases and a large buffer's alignment lie.
//!
//! Most words point into none, and are told apart without a lock by a count
//! of the blocks' first bytes on each page, by the low 20 bits of the page
//! number; the blocks themselves are kept in order behind a lock.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;

/// How far into a block a pointer may point and still be taken for the
/// block's.
pub const INTO: usize = 64;
const SLOTS: usize = 1 << 20;

/// Some blocks, each with a value of the caller's.
pub struct Index<V> {
    pages: Box<[AtomicU16]>,
    /// By start: the end of the first bytes, and the value.
    blocks: Mutex<BTreeMap<usize, (usize, V)>>,
}

fn pages(start: usize, end: usize) -> impl Iterator<Item = usize> {
    (start >> 12..=(end - 1) >> 12).map(|page| page % SLOTS)
}

impl<V: Copy> Index<V> {
    pub fn new() -> Self {
        Self { pages: (0..SLOTS).map(|_| AtomicU16::new(0)).collect(), blocks: Mutex::new(BTreeMap::new()) }
    }

    fn blocks(&self) -> std::sync::MutexGuard<'_, BTreeMap<usize, (usize, V)>> {
        self.blocks.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn insert(&self, start: usize, size: usize, value: V) {
        let end = start + size.clamp(1, INTO);
        if let Some((old, _)) = self.blocks().insert(start, (end, value)) {
            pages(start, old).for_each(|p| _ = self.pages[p].fetch_sub(1, Ordering::Relaxed));
        }
        pages(start, end).for_each(|p| _ = self.pages[p].fetch_add(1, Ordering::Relaxed));
    }

    /// The value of the block that starts at `start`.
    pub fn get(&self, start: usize) -> Option<V> {
        self.blocks().get(&start).map(|(_, value)| *value)
    }

    /// Whether `word` may point into a block: false for most words, with no
    /// lock taken.
    pub fn may_point(&self, word: usize) -> bool {
        self.pages[(word >> 12) % SLOTS].load(Ordering::Relaxed) != 0
    }

    /// For each word `may_point` let through, with a tag of the caller's, the
    /// block it points into: the tag, the block's start and its value. One
    /// lock for them all.
    pub fn pointed<T: Copy>(&self, words: &[(T, usize)]) -> Vec<(T, usize, V)> {
        let blocks = self.blocks();
        words
            .iter()
            .filter_map(|(tag, word)| {
                let (start, (end, value)) = blocks.range(..=*word).next_back()?;
                (*word < *end).then_some((*tag, *start, *value))
            })
            .collect()
    }

    /// The words of `bytes`, eight bytes each from its start, that may point
    /// into a block, each tagged with its offset in `bytes`.
    pub fn candidates(&self, bytes: &[u8]) -> Vec<(usize, usize)> {
        bytes
            .chunks_exact(8)
            .enumerate()
            .map(|(i, word)| (i * 8, u64::from_le_bytes(word.try_into().unwrap_or_default()) as usize))
            .filter(|(_, word)| self.may_point(*word))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_points_into_a_block_s_first_bytes() {
        let index = Index::new();
        index.insert(0x1000, 0x100, 'a');
        index.insert(0x1100, 8, 'b');
        index.insert(0x2_0000_1000, 0x40, 'c');
        let words = [(0, 0x1000), (1, 0x103f), (2, 0x1040), (3, 0x1104), (4, 0x1108), (5, 0xfff), (6, 0x2_0000_1010)];
        assert_eq!(index.pointed(&words), [(0, 0x1000, 'a'), (1, 0x1000, 'a'), (3, 0x1100, 'b'), (6, 0x2_0000_1000, 'c')]);
        assert!(index.may_point(0x1040) && !index.may_point(0x5000));
        assert_eq!(index.get(0x1100), Some('b'));
    }

    #[test]
    fn the_words_that_may_point_are_found_in_bytes() {
        let index = Index::new();
        index.insert(0x7000_0000, 0x40, ());
        let bytes: Vec<u8> = [5usize, 0x7000_0008, 0x9000_0000].iter().flat_map(|w| (*w as u64).to_le_bytes()).collect();
        assert_eq!(index.candidates(&bytes), [(8, 0x7000_0008)]);
    }
}
