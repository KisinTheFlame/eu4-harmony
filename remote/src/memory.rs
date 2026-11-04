//! What the game holds in memory, and what the heap census found: a
//! developer build's, with the `dev-memory` feature (docs/leak-finding.md).
//! A player's build has none of it; memory.leftovers and memory.generation
//! need the heap census built in too.
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
    remote_anywhere Get "" {} -> Memory, "private bytes; heaps_allocated and heaps_committed, what the process's heaps hold of them; free_in_heaps, what they committed and do not use; outside_heaps, the rest (Direct3D's, the driver's); framework_heap; game_heap, what the game's own heap holds for the game; heaps, each heap; handles; census, whether the heap census is built in; generation, the census's generation now; generations, the census's live blocks and bytes by the generation they were allocated in";
    /// The census's leftovers, the blocks still live two generations after
    /// their own, by site, those that leave the most per generation first.
    remote_anywhere Leftovers "leftovers" {
        /// How many sites to give; 40 when left out.
        sites: Option<u32>,
        /// Whether to look for what still points at the sites' leftovers,
        /// which stops the game for seconds; false when left out.
        holders: Option<bool>,
    } -> Leftovers, "generation, the generation now; blocks and bytes, every leftover; sites, by site: site and function (RVAs in eu4.exe), trend (steady, first or sometimes), per_generation (the bytes the last three generations left there, on average), blocks, bytes, generations (what each generation left there), freed_at (the places that let blocks of the site go, with how many), and with holders, holders (how many of its leftovers the game's data, other live blocks, only the leftovers given, or nothing points at, and what points at them most)";
    /// Watches sites from now on: every block the game lets go is read
    /// first, and for each live block of these sites the last block let go
    /// while it pointed at it is kept, with the code that let it go; memory.leftovers
    /// with holders then names, for the leftovers of these sites nothing
    /// points at, the owners that were let go without them. It slows every
    /// letting go of a block; no sites stop it.
    remote_anywhere Watch "watch" {
        /// The sites, as memory.leftovers writes them, separated by commas,
        /// such as "0x1529f85,0x15a2658 < 0x152a00a"; at most 16.
        sites: String,
    } -> Watching, "sites, the sites watched; blocks, how many of their blocks were live as watching began";
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
    /// Bytes the process's heaps have committed and do not use.
    pub free_in_heaps: u64,
    /// Private bytes beyond what the heaps committed: Direct3D's, the
    /// driver's and other allocations of whole pages.
    pub outside_heaps: u64,
    /// Bytes the framework's own heap holds.
    pub framework_heap: u64,
    /// Bytes the game's own heap, its C runtime's, holds for the game: what
    /// it allocated, less the framework's when the two share it.
    pub game_heap: u64,
    /// Each heap of the process, the largest first.
    pub heaps: Vec<Heap>,
    pub handles: u32,
    /// Whether the heap census is built in; without it generation is 0 and
    /// generations empty, and memory.leftovers and memory.generation are
    /// unavailable.
    pub census: bool,
    /// The census's generation now: the world resets, and the ends
    /// `memory.generation` asked for, so far.
    pub generation: u32,
    /// The census's live blocks, by the generation they were allocated in,
    /// oldest first.
    pub generations: Vec<Generation>,
}

/// One heap of the process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Heap {
    /// Its handle, in hex.
    pub handle: String,
    /// The game's C runtime allocates from it.
    pub game: bool,
    /// It is the process's default heap, which the framework allocates from.
    pub process: bool,
    pub allocated: u64,
    pub committed: u64,
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
    /// What still points at its leftovers, when asked for.
    pub holders: Option<Holders>,
}

/// What still points at a site's leftovers: each leftover counted once, by
/// the best that points at it. A pointer is any eight bytes in the game's
/// writable data or in a live block of the game's heap that hold an address
/// in a leftover's first 64 bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Holders {
    /// Leftovers the game's data points at: a registry or a cache the game
    /// keeps.
    pub by_data: u64,
    /// Leftovers that live blocks point at, other than the leftovers given:
    /// their owners.
    pub by_blocks: u64,
    /// Leftovers that only leftovers given point at, such as the next one in
    /// a list nothing else reaches.
    pub by_leftovers: u64,
    /// Leftovers nothing points at: memory something forgot to let go.
    pub by_nothing: u64,
    /// What points at them most, with how many pointers; at most five.
    pub top: Vec<Holder>,
    /// Whether the site is watched (memory.watch).
    pub watched: bool,
    /// For a watched site, of its leftovers nothing points at: the owners
    /// let go while they last pointed at them, and the code that let them go,
    /// the most first; at most five.
    pub lost_with: Vec<Lost>,
    /// For a watched site, its leftovers nothing points at with no owner
    /// let go: the pointer was written over, or they were lost before
    /// watching began.
    pub unrecorded: u64,
}

/// Owners let go while they last pointed at a site's leftovers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Lost {
    /// The owners' site, and the start of the function it is in.
    pub owner: String,
    pub owner_function: Option<String>,
    /// The code that let them go, an RVA in hex, and the start of its
    /// function: the destructor, or the code that deletes the owner.
    pub let_go_at: String,
    pub let_go_function: Option<String>,
    /// How many of the leftovers they last pointed at.
    pub blocks: u64,
}

/// What memory.watch began watching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Watching {
    pub sites: Vec<String>,
    /// The live blocks of those sites as watching began.
    pub blocks: u64,
}

/// One thing that points at a site's leftovers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Holder {
    /// `data`, the game's writable data, or `block`, a block of the game's
    /// heap.
    pub kind: String,
    /// For data its RVA, for a block its site, in hex.
    pub at: String,
    /// For a block, the start of the function its site is in.
    pub function: Option<String>,
    pub references: u64,
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
