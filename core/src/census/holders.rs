//! Who still points at a leftover (docs/leak-finding.md, M2): the memory the
//! census reads, word by word, and any eight bytes that hold an address in
//! the first bytes of a leftover taken as a pointer to it. Reading is the
//! census's to do; this module only takes what was read and tells, for each
//! leftover, what pointed at it.
use super::SiteKey;
use std::collections::HashMap;

/// How far into a block a pointer may point and still be taken for the
/// block's: an object's bases, and a large buffer's alignment, lie in its
/// first bytes.
const INTO: usize = 64;
/// The bitmap of the pages that hold a leftover's first bytes, which most
/// words are told apart by, is by pages of this many bits.
const PAGE_BITS: u32 = 12;

/// A leftover to look for.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub start: usize,
    pub size: usize,
    pub site: SiteKey,
}

/// Where the memory read lies.
#[derive(Clone, Copy, Debug)]
pub enum Source {
    /// The game's writable data, from this RVA.
    Data { rva: u32 },
    /// A live block the census counts, from its start.
    Block { start: usize, site: SiteKey },
}

/// What points at a leftover.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Holder {
    /// The game's data, at an RVA.
    Data(u32),
    /// A block of a site.
    Site(SiteKey),
}

/// The best a leftover is held by: the game's data, a live block that is not
/// a leftover looked for, only leftovers looked for, or nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Held {
    #[default]
    Nothing,
    Leftovers,
    Blocks,
    Data,
}

/// The first bytes of a leftover a pointer may point into.
fn span(t: &Target) -> usize {
    t.size.clamp(1, INTO)
}

/// By the site of the leftovers: how many are held best by each kind of
/// holder, and the pointers each holder has to them.
pub type Found = HashMap<SiteKey, (HashMap<Held, u64>, HashMap<Holder, u64>)>;

pub struct Scan {
    /// The leftovers, by their start.
    targets: Vec<Target>,
    low: usize,
    pages: Vec<u64>,
    held: Vec<Held>,
    /// By the site of the leftovers pointed at: how many pointers each holder
    /// has to them.
    references: HashMap<SiteKey, HashMap<Holder, u64>>,
}

impl Scan {
    pub fn new(mut targets: Vec<Target>) -> Self {
        targets.sort_by_key(|t| t.start);
        let low = targets.first().map_or(0, |t| t.start);
        let high = targets.last().map_or(0, |t| t.start + INTO);
        let mut pages = vec![0u64; ((high - low) >> PAGE_BITS) / 64 + 1];
        for t in &targets {
            for page in (t.start - low) >> PAGE_BITS..=(t.start + span(t) - 1 - low) >> PAGE_BITS {
                pages[page / 64] |= 1 << (page % 64);
            }
        }
        let held = vec![Held::Nothing; targets.len()];
        Self { targets, low, pages, held, references: HashMap::new() }
    }

    /// The leftover an address points into, by its index.
    fn target_at(&self, address: usize) -> Option<usize> {
        let page = address.checked_sub(self.low)? >> PAGE_BITS;
        if self.pages.get(page / 64)? & (1 << (page % 64)) == 0 {
            return None;
        }
        let i = self.targets.partition_point(|t| t.start <= address).checked_sub(1)?;
        let t = &self.targets[i];
        (address < t.start + span(t)).then_some(i)
    }

    /// Whether a block starting at `start` is a leftover looked for.
    pub fn is_target(&self, start: usize) -> bool {
        self.targets.binary_search_by_key(&start, |t| t.start).is_ok()
    }

    /// Takes in what was read at `offset` bytes into `source`: its words,
    /// eight bytes each from the start of `bytes`, which lies on an eight-byte
    /// boundary.
    pub fn read(&mut self, source: Source, offset: usize, bytes: &[u8]) {
        for (i, word) in bytes.chunks_exact(8).enumerate() {
            let address = u64::from_le_bytes(word.try_into().unwrap_or_default()) as usize;
            let Some(t) = self.target_at(address) else { continue };
            let (holder, held) = match source {
                Source::Data { rva } => (Holder::Data(rva + (offset + i * 8) as u32), Held::Data),
                Source::Block { start, .. } if start == self.targets[t].start => continue,
                Source::Block { start, site } => (Holder::Site(site), if self.is_target(start) { Held::Leftovers } else { Held::Blocks }),
            };
            self.held[t] = self.held[t].max(held);
            *self.references.entry(self.targets[t].site).or_default().entry(holder).or_default() += 1;
        }
    }

    /// For each site: how many of its leftovers are held best by each kind
    /// of holder, and the pointers each holder has to them.
    pub fn found(self) -> Found {
        let mut found = Found::new();
        for (t, held) in self.targets.iter().zip(&self.held) {
            *found.entry(t.site).or_default().0.entry(*held).or_default() += 1;
        }
        for (site, references) in self.references {
            found.entry(site).or_default().1 = references;
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(of: &[usize]) -> Vec<u8> {
        of.iter().flat_map(|w| (*w as u64).to_le_bytes()).collect()
    }

    #[test]
    fn a_leftover_is_held_by_the_best_that_points_into_its_first_bytes() {
        let a = Target { start: 0x10_0000, size: 0x100, site: 1 };
        let b = Target { start: 0x10_0200, size: 0x20, site: 1 };
        let c = Target { start: 0x90_0000, size: 0x40, site: 2 };
        let d = Target { start: 0x90_1000, size: 0x40, site: 2 };
        let mut scan = Scan::new(vec![c, a, b, d]);
        // The game's data points at a, 8 bytes in; a live block at b and,
        // past its first bytes, at a again.
        scan.read(Source::Data { rva: 0x2000 }, 0x10, &words(&[7, a.start + 8]));
        scan.read(Source::Block { start: 0x50_0000, site: 9 }, 0, &words(&[b.start, a.start + 0x80]));
        // c points at d, and at itself.
        scan.read(Source::Block { start: c.start, site: 2 }, 0, &words(&[d.start, c.start]));
        let found = scan.found();
        let (held, references) = &found[&1];
        assert_eq!((held[&Held::Data], held[&Held::Blocks]), (1, 1));
        assert_eq!(references[&Holder::Data(0x2018)], 1);
        assert_eq!(references[&Holder::Site(9)], 1);
        let (held, references) = &found[&2];
        assert_eq!((held[&Held::Leftovers], held[&Held::Nothing]), (1, 1));
        assert_eq!(references.len(), 1);
        assert_eq!(references[&Holder::Site(2)], 1);
    }

    #[test]
    fn a_word_outside_every_leftover_is_nothing() {
        let mut scan = Scan::new(vec![Target { start: 0x1000, size: 0x10, site: 1 }]);
        scan.read(Source::Block { start: 0x8000, site: 3 }, 0, &words(&[0, 0xfff, 0x1010, usize::MAX, 0x1000_0000]));
        assert_eq!(scan.found()[&1].0[&Held::Nothing], 1);
        assert!(Scan::new(Vec::new()).found().is_empty());
    }
}
