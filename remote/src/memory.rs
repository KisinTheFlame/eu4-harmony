//! What the game holds in memory, and what the heap census found: a
//! developer build's, with the `dev-memory` feature (docs/leak-finding.md).
//! A player's build has none of it.
//!
//! The census counts the live blocks of the game's C runtime heap by their
//! site, the code that allocated them, and their generation. A generation
//! ends at every world reset and when `memory.generation` asks; a leftover is
//! a block still live two generations after its own.
use serde::{Deserialize, Serialize};

group! {
    /// What the game holds in memory, and what the heap census found: a
    /// developer build's.
    "memory": MemoryRequest, MemoryAnswer;

    /// What the process holds now, and the census's live blocks by
    /// generation.
    remote_anywhere Get "" {} -> Memory, "private bytes; heaps_allocated and heaps_committed, what the process's heaps hold of them; outside_heaps, the rest (Direct3D's, the driver's); framework_heap; handles; generation, the census's generation now; generations, the census's live blocks and bytes by the generation they were allocated in";
    /// The census's leftovers, the blocks still live two generations after
    /// their own, by site, those that leave the most per generation first.
    remote_anywhere Leftovers "leftovers" {
        /// How many sites to give; 40 when left out.
        sites: Option<u32>,
    } -> Leftovers, "generation, the generation now; blocks and bytes, every leftover; sites, by site: site and function (RVAs in eu4.exe), trend (steady, first or sometimes), per_generation (the bytes the last three generations left there, on average), blocks, bytes, generations (what each generation left there), freed_at (the places that let blocks of the site go, with how many)";
    /// Ends the census's generation now, as a world reset ends one, for
    /// leftovers inside one long game.
    remote_anywhere Generation "generation" {} -> u32, "the generation that begins";
}

/// What the process holds now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Memory {
    /// Bytes the process has committed for itself.
    pub private: u64,
    /// Bytes allocated from the process's heaps, the game's and the
    /// framework's alike.
    pub heaps_allocated: u64,
    /// Bytes the process's heaps have committed.
    pub heaps_committed: u64,
    /// Private bytes beyond what the heaps committed: Direct3D's, the
    /// driver's and other allocations of whole pages.
    pub outside_heaps: u64,
    /// Bytes the framework's own heap holds.
    pub framework_heap: u64,
    pub handles: u32,
    /// The census's generation now: the world resets, and the ends
    /// `memory.generation` asked for, so far.
    pub generation: u32,
    /// The census's live blocks, by the generation they were allocated in,
    /// oldest first.
    pub generations: Vec<Generation>,
}

/// Blocks of one generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Generation {
    pub generation: u32,
    pub blocks: u64,
    pub bytes: u64,
}

/// The census's leftovers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Leftovers {
    /// The generation now.
    pub generation: u32,
    /// Every leftover, of every site.
    pub blocks: u64,
    pub bytes: u64,
    /// By site, those that leave the most per generation first.
    pub sites: Vec<Site>,
}

/// The leftovers of one site.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Site {
    /// The code that allocated them, an RVA in eu4.exe in hex, such as
    /// `"0x155c294"`; for a block of the game's generic allocation of a
    /// buffer, the code that called it.
    pub site: String,
    /// The start of the function the site is in, an RVA in hex; null when
    /// the exception directory has none.
    pub function: Option<String>,
    pub trend: Trend,
    /// The bytes each of the last three generations that are old enough to
    /// leave any left there, on average.
    pub per_generation: u64,
    pub blocks: u64,
    pub bytes: u64,
    /// What each generation left there, oldest first; generations that left
    /// nothing are left out.
    pub generations: Vec<Generation>,
    /// The places that let blocks of the site go, by RVA, with how many each
    /// let go, the most first; at most five.
    pub freed_at: Vec<Freed>,
}

/// How the leftovers of a site are spread over the generations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    /// Each of the last generations left some: a leak of every session.
    Steady,
    /// Only the first generations left any: a cache filled once.
    First,
    /// Anything else.
    Sometimes,
}

/// A place that let blocks of a site go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Freed {
    /// The code that let them go, an RVA in hex.
    pub site: String,
    /// The start of the function it is in, an RVA in hex.
    pub function: Option<String>,
    pub blocks: u64,
}
