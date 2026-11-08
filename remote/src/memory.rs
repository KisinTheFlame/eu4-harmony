//! What the game holds in memory, and what the heap census found: a
//! developer build's, with the `dev-memory` feature (docs/plans/leak-finding.md).
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
    /// The quarantine of what the framework's repairs let go: while it is
    /// on, every block a repair lets go, and every block freed while it does,
    /// is kept instead of freed and filled with a pattern; every two
    /// seconds the blocks are compared with it, and the first write to each
    /// is kept with the repair that let it go. Off by default; turning it
    /// off frees what it kept.
    remote_anywhere Quarantine "quarantine" {
        /// Whether it is on from now; left out, it stays as it is.
        on: Option<bool>,
    } -> Quarantine, "on; blocks and bytes, what it keeps now; freed, the blocks it kept and freed again to stay under its most; checks, how many times it compared them; repairs, by repair, the blocks and bytes it kept; written, the blocks written to after a repair let them go: repair, site and function, size, offset (of the first byte changed), bytes (from it, in hex), generation (the census's, as it was kept) and found (as the write was found)";
    /// What keeps the game's heap's free pages committed: the heap walked
    /// page by page, a page that holds blocks committed however little of it
    /// they use, its free bytes shared out among them, and through the
    /// census among their sites. It holds the heap, and so the game, for
    /// seconds.
    remote_anywhere Pages "pages" {
        /// How many sites to give; 40 when left out.
        sites: Option<u32>,
    } -> Pages, "committed, the bytes of the heap's committed pages; used, what its busy blocks use of them; free_pages, the pages no block is on; free_on_used_pages, the free bytes of the others; by_leftovers, the part of those the census's leftovers keep; by_use, the pages in use by how full, in eighths; sites, by site, the most free bytes first: site and function (or \"not counted\" for blocks the census does not count, the framework's own among them), blocks, bytes, pinned (the free bytes that fall to them) and leftovers (how many of the blocks are); complete, false when the heap had more entries than were read; took_ms";
    /// The game's heap's blocks the census does not count: other modules'
    /// (Direct3D's, the driver's, the libraries' the game is linked with),
    /// the game's own made before the framework started or not through its
    /// C runtime, and the framework's. Each ask marks them; the next gives,
    /// size by size, how many more there are, and samples of those made
    /// since: their first bytes, and the modules their first words point
    /// into. It holds the heap, and so the game, for seconds.
    remote_anywhere Uncounted "uncounted" {
        /// How many sizes to give; 20 when left out.
        sizes: Option<u32>,
        /// How many samples of each size; 3 when left out.
        samples: Option<u32>,
    } -> Uncounted, "blocks and bytes, every block the census does not count, with the heap's overhead; marked, whether an earlier ask marked them; made_blocks and made_bytes, those made since the mark; sizes, by size, the most bytes grown since the mark first (or the most bytes, unmarked): size, blocks, grew (how many more than at the mark), made (how many of them were made since), and samples of those made since: address, hex and text (the first 64 bytes), points (what their words point into, module+offset); complete, false when the heap had more entries than were read; took_ms";
}

/// The quarantine of what the framework's repairs let go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Quarantine {
    pub on: bool,
    /// What it keeps now.
    pub blocks: u64,
    pub bytes: u64,
    /// The blocks it kept and freed again, the oldest first, to keep no
    /// more than its most.
    pub freed: u64,
    /// How many times it compared what it keeps with the pattern.
    pub checks: u64,
    /// By repair, what it kept since it was turned on, the most bytes first.
    pub repairs: Vec<Kept>,
    /// The blocks written to after a repair let them go, the first found
    /// first; at most 100.
    pub written: Vec<Written>,
}

/// What the quarantine kept of a repair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Kept {
    pub repair: String,
    pub blocks: u64,
    pub bytes: u64,
}

/// A block written to after a repair let it go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Written {
    pub repair: String,
    /// The block's site, as memory.leftovers writes it, and its function.
    pub site: String,
    pub function: Option<String>,
    pub size: u64,
    /// The first byte that changed, and the bytes from it, at most 16, in
    /// hex.
    pub offset: u64,
    pub bytes: String,
    /// The census's generation as the block was kept, and as the write was
    /// found.
    pub generation: u32,
    pub found: u32,
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

/// What keeps the game's heap's free pages committed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Pages {
    /// The bytes of the heap's committed pages.
    pub committed: u64,
    /// What its busy blocks use of them, their heap's overhead with them.
    pub used: u64,
    /// The bytes of the pages no block is on.
    pub free_pages: u64,
    /// The free bytes of the pages blocks are on, and of those what the
    /// census's leftovers keep.
    pub free_on_used_pages: u64,
    pub by_leftovers: u64,
    /// The pages blocks are on, by how full they are: up to an eighth, up
    /// to two, and so on.
    pub by_use: Vec<u64>,
    /// By site, the most free bytes first.
    pub sites: Vec<Pinning>,
    /// False when the heap had more entries than were read.
    pub complete: bool,
    pub took_ms: u64,
}

/// The game's heap's blocks the census does not count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Uncounted {
    /// Every block the census does not count, with the heap's overhead.
    pub blocks: u64,
    pub bytes: u64,
    /// Whether an earlier ask marked the blocks, which the rest compares
    /// with.
    pub marked: bool,
    /// The blocks made since the mark.
    pub made_blocks: u64,
    pub made_bytes: u64,
    /// By size, the most bytes grown since the mark first.
    pub sizes: Vec<UncountedSize>,
    /// False when the heap had more entries than were read.
    pub complete: bool,
    pub took_ms: u64,
}

/// The blocks of one size the census does not count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct UncountedSize {
    /// With the heap's overhead.
    pub size: u64,
    pub blocks: u64,
    /// How many more than at the mark.
    pub grew: i64,
    /// How many of them were made since the mark.
    pub made: u64,
    pub samples: Vec<UncountedSample>,
}

/// A block made since the mark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct UncountedSample {
    pub address: String,
    /// Its first 64 bytes, in hex, and as text, with a dot for each byte
    /// that is not printable.
    pub hex: String,
    pub text: String,
    /// What its first words point into, as module+offset, such as
    /// "d3d9.dll+0x1a2b0".
    pub points: Vec<String>,
}

/// What a site's blocks keep committed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Pinning {
    /// As memory.leftovers writes it, or "not counted" for the blocks the
    /// census does not count.
    pub site: String,
    pub function: Option<String>,
    pub blocks: u64,
    pub bytes: u64,
    /// The free bytes of the pages its blocks are on that fall to them,
    /// a page's shared out among its blocks alike.
    pub pinned: u64,
    /// How many of its blocks are leftovers.
    pub leftovers: u64,
}