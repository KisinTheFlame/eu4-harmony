//! Replace the text lexer's ternary search tree token lookup (`0x6e4880` ->
//! `0x6e4800`) with a lock-free flat open-addressed hash table for the global
//! token table at `0x2326720` (docs/game/program/saves.md, "The tokenizer").
//!
//! Vanilla `0x170e700` calls `0x6e4880(&0x2326720, text)` for every non-numeric
//! word in a text save or script file. `0x6e4880` + `0x6e4800` chase pointers
//! across a ternary search tree and call CRT `tolower((int)*p)` on every byte.
//! In a 70 MB late-game text save this alone takes ~200-240 ms.
//!
//! `0x6e4880` has only two callers in `eu4.exe`: `0x170e700` (text tokenizer)
//! and `0x170d410` (dynamic token creation), both targeting `0x2326720`.
//! Once `0x170d250` finishes populating `0x2326720` (`[0x2357b30] != 0`), this
//! module traverses the native ternary tree once to build a compact
//! case-insensitive ASCII open-addressed hash table mapping each lowercased
//! keyword to the exact native `*mut u32` token-id pointer stored in the tree.
//! Because `0x2326720` is append-only while `[0x2357b30] != 0` (existing keys
//! and their value pointers are never deleted or overwritten until teardown in
//! `0x170d910`, which clears `root` and `TOKENS_READY`), a hit in the active
//! snapshot is always valid even while startup database loading is appending
//! thousands of dynamic tokens to `[0x242b8a0..0x242b8a8]`. On a miss while
//! `dyn_end` differs from the snapshot, lookup falls back to native `0x6e4880`
//! and waits for `dyn_end` to stay unchanged across a burst of lookups before
//! publishing one fresh snapshot atomically through `AtomicPtr`.
//! Non-ASCII inputs fall back to `0x6e4880` to preserve CRT locale semantics.
use crate::base::called_by_game;
use crate::native::objects::commands_rva::TOKENS_READY;
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::ffi::c_char;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::Mutex;

const LOOKUP: usize = 0x6e4880;
const TOKEN_TREE: usize = 0x2326720;
const TOKEN_TREE_ROOT: usize = 0x2326728;
const DYNAMIC_TOKENS_BEGIN: usize = 0x242b8a0;
const DYNAMIC_TOKENS_END: usize = 0x242b8a8;

type Lookup = unsafe extern "C" fn(usize, *const c_char) -> usize;

static ORIGINAL_LOOKUP: Function<Lookup> = Function::new();

pub const SPEED_UP: SpeedUp = SpeedUp::new("token-lookup", "loading", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(LOOKUP, "40 57 48 83 ec 20 48 83 79 08 00 48 8b fa 75 08", "ternary tree lookup", lookup_token as Lookup, &ORIGINAL_LOOKUP),
        Point::keep(0x170ea0busize, "48 8d 0d 0e 7d c1 00 e8 69 5e fd fe", "text lexer looks up token in global table").uses(&[TOKEN_TREE]),
        Point::keep(0x170d48fusize, "48 8b 0d 12 e4 d1 00 48 2b 0d 03 e4 d1 00", "dynamic token vector bounds")
            .uses(&[DYNAMIC_TOKENS_BEGIN, DYNAMIC_TOKENS_END]),
        Point::keep(0x170d285usize, "80 3d a4 a8 c4 00 00", "token table initialized flag").uses(&[TOKENS_READY]),
        Point::keep(0x170d34ausize, "48 8d 15 d7 93 c1 00", "token table root pointer").uses(&[TOKEN_TREE_ROOT]),
    ]
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[inline]
fn ascii_lower(b: u8) -> u8 {
    if b.is_ascii_uppercase() {
        b | 0x20
    } else {
        b
    }
}

/// Computes the lowercase ASCII FNV-1a hash and length of a NUL-terminated C
/// string. Returns `None` if any byte is non-ASCII (`>= 0x80`) or length is 0.
#[inline]
unsafe fn hash_cstr_ascii_lower(mut p: *const u8) -> Option<(u64, usize)> {
    let mut h = FNV_OFFSET;
    let mut len = 0usize;
    loop {
        let b = *p;
        if b == 0 {
            return if len == 0 { None } else { Some((h, len)) };
        }
        if b >= 0x80 {
            return None;
        }
        h ^= ascii_lower(b) as u64;
        h = h.wrapping_mul(FNV_PRIME);
        p = p.add(1);
        len += 1;
    }
}

#[inline]
fn hash_lower_bytes(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

#[derive(Clone, Copy)]
struct Slot {
    hash: u64,
    offset: u32,
    len: u16,
    value: usize,
}

impl Slot {
    const EMPTY: Self = Self { hash: 0, offset: 0, len: 0, value: 0 };
}

struct FlatTokenTable {
    root: usize,
    dyn_end: usize,
    mask: usize,
    /// All lowercased keys packed contiguously so probe hits stay cache-hot.
    arena: Box<[u8]>,
    slots: Box<[Slot]>,
}

impl FlatTokenTable {
    fn from_entries(root: usize, dyn_end: usize, entries: &[(Vec<u8>, usize)]) -> Self {
        let cap = (entries.len().max(16) * 2).next_power_of_two();
        let mask = cap - 1;
        let total_bytes: usize = entries.iter().map(|(k, _)| k.len()).sum();
        let mut arena = Vec::with_capacity(total_bytes);
        let mut slots = vec![Slot::EMPTY; cap];

        for (key, value) in entries {
            if key.is_empty() || key.len() > u16::MAX as usize || *value == 0 {
                continue;
            }
            let h = hash_lower_bytes(key);
            let mut idx = (h as usize) & mask;
            loop {
                let s = slots[idx];
                if s.value == 0 {
                    let offset = arena.len() as u32;
                    arena.extend_from_slice(key);
                    slots[idx] = Slot { hash: h, offset, len: key.len() as u16, value: *value };
                    break;
                }
                if s.hash == h && s.len as usize == key.len() {
                    let existing = &arena[s.offset as usize..s.offset as usize + s.len as usize];
                    if existing == key.as_slice() {
                        slots[idx].value = *value;
                        break;
                    }
                }
                idx = (idx + 1) & mask;
            }
        }

        Self { root, dyn_end, mask, arena: arena.into_boxed_slice(), slots: slots.into_boxed_slice() }
    }

    #[inline]
    unsafe fn get(&self, text: *const u8, hash: u64, len: usize) -> usize {
        let mut idx = (hash as usize) & self.mask;
        loop {
            let s = *self.slots.get_unchecked(idx);
            if s.value == 0 {
                return 0;
            }
            if s.hash == hash && s.len as usize == len {
                let stored = self.arena.as_ptr().add(s.offset as usize);
                let mut eq = true;
                for i in 0..len {
                    if *stored.add(i) != ascii_lower(*text.add(i)) {
                        eq = false;
                        break;
                    }
                }
                if eq {
                    return s.value;
                }
            }
            idx = (idx + 1) & self.mask;
        }
    }
}

/// Active immutable snapshot for lock-free lookups.
static TABLE: AtomicPtr<FlatTokenTable> = AtomicPtr::new(std::ptr::null_mut());
/// Last observed `dyn_end` and consecutive lookups where it stayed unchanged,
/// used to coalesce thousands of dynamic-token insertions during startup into
/// a single snapshot rebuild once the burst settles.
static LAST_SEEN_DYN_END: AtomicUsize = AtomicUsize::new(0);
static STABLE_LOOKUPS: AtomicUsize = AtomicUsize::new(0);
const REBUILD_AFTER_STABLE_LOOKUPS: usize = 256;
/// Serializes rebuilds and keeps retired snapshots at stable heap addresses
/// for the process lifetime (at most a handful of rebuilds when dynamic tokens
/// are registered) so concurrent lock-free readers never observe moved or freed
/// memory.
#[expect(clippy::vec_box, reason = "TABLE holds raw pointers to live snapshots that must stay pinned across Vec growth")]
static BUILD_LOCK: Mutex<Vec<Box<FlatTokenTable>>> = Mutex::new(Vec::new());

/// Iterative traversal of the game's ternary search tree rooted at `root`.
/// Node layout (`0x6e4800`): `+0x00` value ptr, `+0x08` i8 char, `+0x10` left,
/// `+0x18` right, `+0x20` mid.
unsafe fn collect_tst(root: usize, out: &mut Vec<(Vec<u8>, usize)>) {
    enum Step {
        Visit(usize),
        PushAndMid { ch: u8, value: usize, mid: usize },
        PopChar,
    }
    let mut stack = Vec::with_capacity(256);
    let mut prefix: Vec<u8> = Vec::with_capacity(64);
    stack.push(Step::Visit(root));

    while let Some(step) = stack.pop() {
        match step {
            Step::PopChar => {
                prefix.pop();
            }
            Step::PushAndMid { ch, value, mid } => {
                prefix.push(ch);
                if value != 0 {
                    out.push((prefix.clone(), value));
                }
                if mid != 0 {
                    stack.push(Step::PopChar);
                    stack.push(Step::Visit(mid));
                } else {
                    prefix.pop();
                }
            }
            Step::Visit(node) => {
                if node == 0 {
                    continue;
                }
                let value = (node as *const usize).read();
                let ch = ((node + 0x08) as *const u8).read();
                let left = ((node + 0x10) as *const usize).read();
                let right = ((node + 0x18) as *const usize).read();
                let mid = ((node + 0x20) as *const usize).read();

                // Push in reverse order so traversal is left -> current/mid -> right.
                if right != 0 {
                    stack.push(Step::Visit(right));
                }
                if ch < 0x80 {
                    stack.push(Step::PushAndMid { ch: ascii_lower(ch), value, mid });
                }
                if left != 0 {
                    stack.push(Step::Visit(left));
                }
            }
        }
    }
}

unsafe fn rebuild_table(root: usize, dyn_end: usize) -> *const FlatTokenTable {
    let mut retired = BUILD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cur = TABLE.load(Ordering::Acquire);
    if !cur.is_null() {
        let t = &*cur;
        if t.root == root && t.dyn_end == dyn_end {
            return cur;
        }
    }
    let mut entries = Vec::with_capacity(16_384);
    collect_tst(root, &mut entries);
    let mut boxed = Box::new(FlatTokenTable::from_entries(root, dyn_end, &entries));
    let ptr: *mut FlatTokenTable = &mut *boxed;
    TABLE.store(ptr, Ordering::Release);
    retired.push(boxed);
    ptr
}

#[called_by_game]
unsafe extern "C" fn lookup_token(tree: usize, text: *const c_char) -> usize {
    if text.is_null() {
        return ORIGINAL_LOOKUP.get()(tree, text);
    }
    let m = crate::native::hooked_memory();
    if tree != m.at(TOKEN_TREE) {
        return ORIGINAL_LOOKUP.get()(tree, text);
    }
    if (m.at(TOKENS_READY) as *const u8).read() == 0 {
        return ORIGINAL_LOOKUP.get()(tree, text);
    }
    let root = (m.at(TOKEN_TREE_ROOT) as *const usize).read();
    if root == 0 {
        return 0;
    }

    let Some((hash, len)) = hash_cstr_ascii_lower(text as *const u8) else {
        return ORIGINAL_LOOKUP.get()(tree, text);
    };

    let dyn_end = (m.at(DYNAMIC_TOKENS_END) as *const usize).read();
    let mut tbl = TABLE.load(Ordering::Acquire);
    if tbl.is_null() || (*tbl).root != root {
        tbl = rebuild_table(root, dyn_end) as *mut FlatTokenTable;
        LAST_SEEN_DYN_END.store(dyn_end, Ordering::Relaxed);
        STABLE_LOOKUPS.store(0, Ordering::Relaxed);
    }

    let mut fast = (*tbl).get(text as *const u8, hash, len);
    if (*tbl).dyn_end != dyn_end {
        // Dynamic tokens were appended since `tbl` was built. Wait until
        // `dyn_end` stops changing across a burst of lookups so startup's
        // thousands of individual insertions trigger only one rebuild.
        let ready_to_rebuild = if LAST_SEEN_DYN_END.load(Ordering::Relaxed) == dyn_end {
            STABLE_LOOKUPS.fetch_add(1, Ordering::Relaxed) + 1 >= REBUILD_AFTER_STABLE_LOOKUPS
        } else {
            LAST_SEEN_DYN_END.store(dyn_end, Ordering::Relaxed);
            STABLE_LOOKUPS.store(0, Ordering::Relaxed);
            false
        };

        if ready_to_rebuild {
            tbl = rebuild_table(root, dyn_end) as *mut FlatTokenTable;
            STABLE_LOOKUPS.store(0, Ordering::Relaxed);
            if fast == 0 {
                fast = (*tbl).get(text as *const u8, hash, len);
            }
        } else if fast == 0 {
            // Snapshot hit is always valid (tree is append-only); only a miss
            // on a stale snapshot needs the native tree while insertions are in flight.
            return ORIGINAL_LOOKUP.get()(tree, text);
        }
    }

    if speed::checked(&SPEED_UP) {
        let game = ORIGINAL_LOOKUP.get()(tree, text);
        speed::compared(&SPEED_UP, fast == game, || {
            let s = std::ffi::CStr::from_ptr(text).to_string_lossy();
            format!("token lookup {s:?}: fast={fast:#x} game={game:#x}")
        });
        return game;
    }
    fast
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn flat_table_matches_case_insensitive_ascii_keys() {
        let entries = vec![
            (b"owner".to_vec(), 0x1001),
            (b"controller".to_vec(), 0x1002),
            (b"add_core".to_vec(), 0x1003),
            (b"history".to_vec(), 0x1004),
            (b"a".to_vec(), 0x1005),
        ];
        let table = FlatTokenTable::from_entries(1, 2, &entries);

        let query = |s: &str| {
            let c = CString::new(s).unwrap();
            unsafe {
                let (h, len) = hash_cstr_ascii_lower(c.as_ptr() as *const u8).unwrap();
                table.get(c.as_ptr() as *const u8, h, len)
            }
        };

        assert_eq!(query("owner"), 0x1001);
        assert_eq!(query("OWNER"), 0x1001);
        assert_eq!(query("Add_Core"), 0x1003);
        assert_eq!(query("A"), 0x1005);
        assert_eq!(query("missing_token"), 0);
        assert_eq!(query("own"), 0);
    }

    #[test]
    fn non_ascii_or_empty_falls_back_to_native() {
        let empty = CString::new("").unwrap();
        assert!(unsafe { hash_cstr_ascii_lower(empty.as_ptr() as *const u8) }.is_none());
        let non_ascii = CString::new("caf\u{00e9}").unwrap();
        assert!(unsafe { hash_cstr_ascii_lower(non_ascii.as_ptr() as *const u8) }.is_none());
    }

    #[test]
    fn collect_tst_handles_left_mid_right_branches() {
        #[repr(C)]
        struct TstNode {
            value: usize,
            ch: u8,
            _pad: [u8; 7],
            left: usize,
            right: usize,
            mid: usize,
        }

        // Build a TST containing "be", "ba", "do", "ca" where 'c' has both left ('b'), mid ('a'), and right ('d').
        let n_e = TstNode { value: 1, ch: b'e', _pad: [0; 7], left: 0, right: 0, mid: 0 };
        let n_ba = TstNode { value: 2, ch: b'a', _pad: [0; 7], left: 0, right: &n_e as *const _ as usize, mid: 0 };
        let n_b = TstNode { value: 0, ch: b'b', _pad: [0; 7], left: 0, right: 0, mid: &n_ba as *const _ as usize };
        let n_o = TstNode { value: 3, ch: b'o', _pad: [0; 7], left: 0, right: 0, mid: 0 };
        let n_d = TstNode { value: 0, ch: b'd', _pad: [0; 7], left: 0, right: 0, mid: &n_o as *const _ as usize };
        let n_ca = TstNode { value: 4, ch: b'a', _pad: [0; 7], left: 0, right: 0, mid: 0 };
        let root =
            TstNode { value: 0, ch: b'c', _pad: [0; 7], left: &n_b as *const _ as usize, right: &n_d as *const _ as usize, mid: &n_ca as *const _ as usize };

        let mut out = Vec::new();
        unsafe { collect_tst(&root as *const _ as usize, &mut out) };
        let mut pairs: Vec<(String, usize)> = out.into_iter().map(|(k, v)| (String::from_utf8(k).unwrap(), v)).collect();
        pairs.sort();
        assert_eq!(pairs, vec![("ba".into(), 2), ("be".into(), 1), ("ca".into(), 4), ("do".into(), 3)]);
    }
}
