//! Localisation loaded once at startup, and its `localisation/replace` pass
//! and duplicate check made linear (docs/research/localisation-loading.md,
//! docs/plans/large-mod-startup.md, M1).
//!
//! At startup `main` loads the whole localisation (`0x873fc0`), sets the text
//! editor's strings, and at `0x877a79` immediately calls `0x14b4a20`, which
//! destroys `[0x2357b20]` and loads `languages.yml`, `localisation` and
//! `localisation/replace` a second time from scratch. Inside each pass,
//! `localisation/replace` (`0x16fa8d0` → `0x16faae0`) walks the entire
//! `collection + 0x38` provenance vector of ~280,000 records from 0 on every
//! replaced key (and `0x16fd600` checks `+5` instead of `+4` for `replace`, so
//! its intended per-file sort never ran anyway). Then `0x16fd7a0` walks
//! `collection + 0x38` from 0 again for every run of equal hashes to format
//! category-5 lines that are dropped unless `-debug` registered `text.log`.
//!
//! - At `0x877a79` in `main`, `0x14b4a20` is skipped (`0x14b4a20` itself stays
//!   as it is for `reload_loc`, `-reload` and `-update_loc`).
//! - In `0x16faae0`, when a replacement hits an existing hash in
//!   `collection + 0x20`, the framework answers the `(hash, file_index)`
//!   provenance lookup in `collection + 0x38` through a per-collection open-
//!   addressing index that catches up with any records appended since its last
//!   use (and resets when `0x16fb7c0` clears `[0x2357b20]` or when a vector's
//!   base moves back or shrinks), updating the first matching record or
//!   appending a new one and copying into the arena just as `0x16faae0` does.
//! - When category 5 (`text.log`) is not registered in `[0x2349598]`,
//!   `0x16fd7a0` checks each collection's sorted `+0x20` vector in one pass
//!   for adjacent equal hashes and returns the same boolean without the
//!   quadratic scan and discarded formatting; with `-debug` it runs the game's
//!   own `0x16fd7a0`.
//!
//! Checked (`checked = ["localisation"]`), every replacement in `0x16faae0`
//! searches `collection + 0x38` both ways, every `0x16fd7a0` compares the
//! fast scan's boolean with the game's `0x16fd7a0`, and at `0x877a79` the
//! store before `0x14b4a20` is snapshotted, `0x14b4a20` runs, and every
//! collection's lookup entries, provenance records and resolved strings are
//! compared with what skipping `0x14b4a20` would have kept.
use crate::called_by_game;
use crate::hooks::Call;
use crate::memory::Memory;
use crate::points::{Function, GameFunction, Point};
use crate::speed::{self, SpeedUp};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::CStr;

/// In `main` (`0x8757e0`): `call 0x14b4a20` right after `0x873fc0` and the
/// editor's strings, and the instruction after it.
const MAIN_RELOAD_CALL: usize = 0x877a79;
const MAIN_AFTER_RELOAD: usize = 0x877a7e;
const RELOAD_LOC: GameFunction<0> = GameFunction::at(0x14b4a20);
/// `0x16fb7c0()`: destroys `[0x2357b20]` and clears the pointer.
const CLEAR_STORE: usize = 0x16fb7c0;
/// In `0x16faae0`, where a hit in `collection + 0x20` (`rax`) searches
/// `collection + 0x38` (`r9`, base `r10`) for `(rsi = hash, r11d = file_idx)`:
/// `mov r11d, [rsp + 0xa8]; lea r9, [r15 + 0x38]; lea ebx, [r12 + rdi]`,
/// resuming at `0x16fac0a` when found (`rdx` = record index) or `0x16fac22`
/// when not found (`edx == count`).
const REPLACE_ENTRY: usize = 0x16faae0;
const REPLACE_SEARCH: usize = 0x16fabb6;
const REPLACE_FOUND: usize = 0x16fac0a;
const REPLACE_NOT_FOUND: usize = 0x16fac22;
/// `0x16fd7a0() -> collision_found`: checks every collection for duplicate or
/// colliding hashes.
const CHECK_COLLISIONS: usize = 0x16fd7a0;

use crate::game_string::LOCALISATION;
/// Offsets in `[LOCALISATION]` (`+8..+0x10` collections, `+0x20` shared text
/// arena).
const STORE_COLLECTIONS: usize = 0x08;
const STORE_COLLECTIONS_END: usize = 0x10;
const STORE_ARENA: usize = 0x20;
/// Offsets in a `0x68`-byte language collection.
const COL_ENTRIES: usize = 0x20;
const COL_ENTRIES_END: usize = 0x28;
const COL_RECORDS: usize = 0x38;
const COL_RECORDS_END: usize = 0x40;
const COL_FILES: usize = 0x50;
const ENTRY_SIZE: usize = 0x10;
const RECORD_SIZE: usize = 0x18;
const RECORD_KEY: usize = 0x08;
const RECORD_VAL: usize = 0x0c;
const RECORD_FILE: usize = 0x10;
const RECORD_LINE: usize = 0x14;

/// `[0x2349598]`: the logger category table (`+4` bucket count, `+8` buckets
/// of `(entry: *const u32, next: usize)`). Category 5 (`text.log`) is
/// registered only when `-debug` is passed (`0x872863`).
const LOGGERS: usize = 0x2349598;
const LOGGERS_BUCKETS: usize = 0x04;
const LOGGERS_TABLE: usize = 0x08;
const TEXT_LOG_CATEGORY: u32 = 5;

pub const SPEED_UP: SpeedUp = SpeedUp { name: "localisation", group: "loading", points, checkable: true };

type CheckCollisions = unsafe extern "C" fn() -> u8;
static ORIGINAL_CHECK_COLLISIONS: Function<CheckCollisions> = Function::new();

crate::resume!(MAIN_RELOAD_RESUME: MAIN_AFTER_RELOAD_AT = MAIN_AFTER_RELOAD);
crate::resume!(REPLACE_RESUME: REPLACE_FOUND_AT = REPLACE_FOUND, REPLACE_NOT_FOUND_AT = REPLACE_NOT_FOUND);

fn points() -> Vec<Point> {
    vec![
        Point::jump(
            MAIN_RELOAD_CALL,
            "e8 a2 cf c3 00",
            "main reloads localisation right after loading it",
            main_reload_stub as *const () as usize,
            &MAIN_RELOAD_RESUME,
        ),
        Point::keep(MAIN_AFTER_RELOAD, "41 b9 05 00 01 00", "main after the localisation reload"),
        Point::keep(RELOAD_LOC, "40 53 48 83 ec 40 48 8b 15 f3 30 ea 00 48 85 d2", "localisation reload").uses(&[LOCALISATION]),
        Point::observe(CLEAR_STORE, "40 55 48 83 ec 20 48 8b 2d 53 c3 c5 00 48 85 ed", "localisation store cleared", 0, Some(store_cleared), None)
            .uses(&[LOCALISATION]),
        Point::keep(REPLACE_ENTRY, "48 89 5c 24 08 44 89 4c 24 20 4c 89 44 24 18 55", "localisation entry replacement"),
        Point::jump(
            REPLACE_SEARCH,
            "44 8b 9c 24 a8 00 00 00 4d 8d 4f 38 41 8d 1c 3c",
            "localisation replacement searches provenance records",
            replace_search_stub as *const () as usize,
            &REPLACE_RESUME,
        )
        .uses(&[COL_RECORDS]),
        Point::keep(
            REPLACE_FOUND,
            "48 8d 04 52 48 8d 0c c5 00 00 00 00 46 89 64 11 08 49 8b 01 89 5c 01 0c",
            "localisation replacement updates a provenance record",
        )
        .uses(&[RECORD_KEY, RECORD_VAL]),
        Point::keep(
            REPLACE_NOT_FOUND,
            "49 8b 49 08 48 8b c1 49 2b 01 48 c1 f8 03 49 0f af c7 3b d0 0f 85 e6 00 00 00",
            "localisation replacement checks whether to append a provenance record",
        ),
        // The linear loop replaced: `cmp [r10 + rax * 8], rsi; ...; cmp [r10 + rax * 8 + 0x10], r11d`.
        Point::keep(0x16fabf0usize, "48 8d 04 52 49 39 34 c2 75 07 45 39 5c c2 10 74 09", "localisation provenance record fields").uses(&[RECORD_FILE]),
        Point::replace(
            CHECK_COLLISIONS,
            "40 55 48 83 ec 20 48 8d 6c 24 20 48 8b 05 6e a3 c5 00 48 89 5d 10",
            "localisation duplicate and collision check",
            check_collisions as CheckCollisions,
            &ORIGINAL_CHECK_COLLISIONS,
        )
        .uses(&[LOCALISATION]),
        // `0x16fd7a0` walks `[store + 8]..[store + 0x10]` and `0x16fb1c0`
        // compares adjacent hashes in `[collection + 0x20]..[collection + 0x28]`.
        Point::keep(0x16fd7ceusize, "48 8b 50 08 48 8b 40 10 48 2b c2 48 c1 f8 03", "localisation collections walked")
            .uses(&[STORE_COLLECTIONS, STORE_COLLECTIONS_END]),
        Point::keep(0x16fb1dausize, "48 8b 41 28 48 8b f1 48 2b 41 20 45 32 d2 48 c1 f8 04", "localisation entries counted for collision check")
            .uses(&[COL_ENTRIES, COL_ENTRIES_END]),
        // Logger category lookup in `0x15632d0`: `[0x2349598]`, `+4` buckets, `+8` table.
        Point::keep(0x15632f5usize, "4c 8b 05 9c 62 de 00 4d 85 c0 74 27 33 d2 8b c7 41 f7 70 04 49 8b 40 08", "logger category table lookup").uses(&[
            LOGGERS,
            LOGGERS_BUCKETS,
            LOGGERS_TABLE,
        ]),
    ]
}

/// First record index in `records` (`count` 24-byte items) with `(hash, file)`,
/// as `0x16fabf0` finds it.
unsafe fn linear_record_search(records: usize, count: u32, hash: u64, file: u32) -> Option<u32> {
    for i in 0..count {
        let at = records + i as usize * RECORD_SIZE;
        let held_hash = unsafe { *(at as *const u64) };
        if held_hash == hash {
            let held_file = unsafe { *((at + RECORD_FILE) as *const u32) };
            if held_file == file {
                return Some(i);
            }
        }
    }
    None
}

/// Open-addressing index of the first record index for each `(hash, file)` in
/// one collection's `+0x38` vector.
#[derive(Default)]
struct RecordIndex {
    /// `(occupied_stamp, hash, file, place)`.
    slots: Vec<(u32, u64, u32, u32)>,
    stamp: u32,
    known: u32,
    first_hash: u64,
    first_file: u32,
}

impl RecordIndex {
    fn slot(&self, hash: u64, file: u32) -> usize {
        let mixed = hash.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (u64::from(file).wrapping_mul(0x517c_c1b7_2722_0a95));
        ((mixed ^ (mixed >> 32)) as usize) & (self.slots.len() - 1)
    }

    fn reset(&mut self, capacity_for: u32) {
        let wanted = (capacity_for as usize * 2).next_power_of_two().max(256);
        if self.slots.len() < wanted {
            self.slots = vec![(0, 0, 0, 0); wanted];
            self.stamp = 0;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.slots.iter_mut().for_each(|s| s.0 = 0);
            self.stamp = 1;
        }
        self.known = 0;
    }

    /// Indexes records in `known..count`, keeping the earliest index for any
    /// duplicate `(hash, file)`.
    unsafe fn catch_up(&mut self, records: usize, count: u32) {
        if count > 0 {
            let h0 = unsafe { *(records as *const u64) };
            let f0 = unsafe { *((records + RECORD_FILE) as *const u32) };
            if self.known > 0 && (h0 != self.first_hash || f0 != self.first_file) {
                self.reset(count);
            }
            self.first_hash = h0;
            self.first_file = f0;
        }
        if count < self.known || (count as usize) * 2 > self.slots.len() {
            self.reset(count);
        }
        for place in self.known..count {
            let at_mem = records + place as usize * RECORD_SIZE;
            let hash = unsafe { *(at_mem as *const u64) };
            let file = unsafe { *((at_mem + RECORD_FILE) as *const u32) };
            let mut slot = self.slot(hash, file);
            loop {
                let (stamp, h, f, _) = self.slots[slot];
                if stamp != self.stamp {
                    self.slots[slot] = (self.stamp, hash, file, place);
                    break;
                }
                if h == hash && f == file {
                    break;
                }
                slot = (slot + 1) & (self.slots.len() - 1);
            }
        }
        self.known = count;
    }

    fn get(&self, hash: u64, file: u32) -> Option<u32> {
        if self.slots.is_empty() {
            return None;
        }
        let mut slot = self.slot(hash, file);
        loop {
            let (stamp, h, f, place) = self.slots[slot];
            if stamp != self.stamp {
                return None;
            }
            if h == hash && f == file {
                return Some(place);
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }
}

thread_local! {
    /// Per-collection record indexes, keyed by collection pointer.
    static INDEXES: RefCell<HashMap<usize, RecordIndex>> = RefCell::new(HashMap::new());
    /// Set while `checked` runs `0x14b4a20` at `0x877a79` with the game's own
    /// linear paths for comparison.
    static FORCE_GAME: Cell<bool> = const { Cell::new(false) };
}

fn store_cleared(_: &mut Call) {
    INDEXES.with(|m| m.borrow_mut().clear());
}

/// Finds `(hash, file)` in `collection + 0x38`: returns the record index, or
/// `count` when absent (matching `edx == count` at `0x16fac22`).
#[called_by_game]
extern "C" fn find_record(collection: usize, hash: u64, file: u32) -> u64 {
    unsafe {
        let base = *((collection + COL_RECORDS) as *const usize);
        let end = *((collection + COL_RECORDS_END) as *const usize);
        let count = end.saturating_sub(base) / RECORD_SIZE;
        let count_u32 = count as u32;
        if FORCE_GAME.with(Cell::get) {
            return u64::from(linear_record_search(base, count_u32, hash, file).unwrap_or(count_u32));
        }
        let found = INDEXES.with(|map| {
            let mut map = map.borrow_mut();
            let index = map.entry(collection).or_default();
            index.catch_up(base, count_u32);
            index.get(hash, file)
        });
        if speed::checked(&SPEED_UP) {
            let game = linear_record_search(base, count_u32, hash, file);
            speed::compared(&SPEED_UP, found == game, || format!("record ({hash:#x}, file {file}): index answered {found:?}, game {game:?}"));
            return u64::from(game.unwrap_or(count_u32));
        }
        u64::from(found.unwrap_or(count_u32))
    }
}

/// Replaces `0x16fabb6..0x16fac22` inside `0x16faae0`.
///
/// On entry (at `0x16fabb6`):
/// - `r15` = `collection`, `rax` = found entry in `collection + 0x20`,
/// - `rsi` = 32-bit hash (zero-extended), `r12d` = key offset in arena,
/// - `rdi` = `strlen(key) + 1`, `[rsp + 0xa8]` = `file_index`.
///
/// Sets:
/// - `r11d = [rsp + 0xa8]`, `r9 = r15 + 0x38`, `ebx = r12d + edi`,
/// - `[rax + 8] = ebx`, `r14b = 0`, `r15 = 0xaaaaaaaa_aaaaaaab`,
/// - `r10 = [r9]`, `rdx` = found index (jumping to `0x16fac0a`) or `count`
///   (jumping to `0x16fac22`).
#[unsafe(naked)]
extern "C" fn replace_search_stub() {
    std::arch::naked_asm!(
        "mov r11d, dword ptr [rsp + 0xa8]",
        "lea ebx, [r12 + rdi]",
        "mov dword ptr [rax + 8], ebx",
        "xor r14d, r14d",
        crate::call_rust!(
            "mov rcx, r15",
            "mov rdx, rsi",
            "mov r8d, r11d",
            call find_record,
            "mov rax, [rsp + 0x48]"
        ),
        "mov r11d, dword ptr [rsp + 0xa8]",
        "lea r9, [r15 + 0x38]",
        "mov r15, 0xaaaaaaaaaaaaaaab",
        "mov r10, qword ptr [r9]",
        "mov rdx, rax",
        "mov rcx, qword ptr [r9 + 8]",
        "sub rcx, r10",
        "sar rcx, 3",
        "imul rcx, r15",
        "cmp edx, ecx",
        "je 2f",
        "jmp qword ptr [rip + {found}]",
        "2:",
        "jmp qword ptr [rip + {not_found}]",
        find_record = sym find_record,
        found = sym REPLACE_FOUND_AT,
        not_found = sym REPLACE_NOT_FOUND_AT,
    );
}

/// Whether `category` has a logger registered in `[0x2349598]`, as `0x15632d0`
/// looks it up (`category % buckets`).
fn category_logged(m: &Memory, category: u32) -> bool {
    let Ok(table_obj) = m.ptr(m.at(LOGGERS)) else { return false };
    if table_obj == 0 {
        return false;
    }
    let Ok(buckets) = m.u32(table_obj + LOGGERS_BUCKETS) else { return false };
    let Ok(table) = m.ptr(table_obj + LOGGERS_TABLE) else { return false };
    if buckets == 0 || table == 0 {
        return false;
    }
    let bucket = (category % buckets) as usize;
    let mut node = m.ptr(table + bucket * 8).unwrap_or(0);
    while node != 0 {
        if let Ok(entry) = m.ptr(node) {
            if entry != 0 && m.u32(entry).ok() == Some(category) && m.ptr(entry + 8).unwrap_or(0) != 0 {
                return true;
            }
        }
        node = m.ptr(node + 8).unwrap_or(0);
    }
    false
}

/// Whether any collection in `[0x2357b20]` has two adjacent entries in its
/// sorted `+0x20` vector with the same 64-bit hash word, which is the exact
/// condition under which `0x16fd7a0` → `0x16fb1c0` returns `1`.
unsafe fn any_adjacent_equal_hash(store: usize) -> Option<bool> {
    if store == 0 {
        return Some(false);
    }
    let cols = unsafe { *((store + STORE_COLLECTIONS) as *const usize) };
    let cols_end = unsafe { *((store + STORE_COLLECTIONS_END) as *const usize) };
    if cols_end < cols || (cols_end - cols) % 8 != 0 {
        return None;
    }
    let n_cols = (cols_end - cols) / 8;
    let mut found = false;
    for i in 0..n_cols {
        let col = unsafe { *((cols + i * 8) as *const usize) };
        if col == 0 {
            continue;
        }
        let entries = unsafe { *((col + COL_ENTRIES) as *const usize) };
        let entries_end = unsafe { *((col + COL_ENTRIES_END) as *const usize) };
        let count = entries_end.saturating_sub(entries) / ENTRY_SIZE;
        for j in 0..count.saturating_sub(1) {
            let h0 = unsafe { *((entries + j * ENTRY_SIZE) as *const u64) };
            let h1 = unsafe { *((entries + (j + 1) * ENTRY_SIZE) as *const u64) };
            if h0 == h1 {
                found = true;
                break;
            }
        }
    }
    Some(found)
}

#[called_by_game]
unsafe extern "C" fn check_collisions() -> u8 {
    let m = crate::hooked_memory();
    if FORCE_GAME.with(Cell::get) || category_logged(&m, TEXT_LOG_CATEGORY) {
        return unsafe { ORIGINAL_CHECK_COLLISIONS.get()() };
    }
    let store = m.ptr(m.at(LOCALISATION)).unwrap_or(0);
    let Some(ours) = (unsafe { any_adjacent_equal_hash(store) }) else {
        return unsafe { ORIGINAL_CHECK_COLLISIONS.get()() };
    };
    if speed::checked(&SPEED_UP) {
        let game = unsafe { ORIGINAL_CHECK_COLLISIONS.get()() };
        speed::compared(&SPEED_UP, u8::from(ours) == game, || format!("collision check: ours={ours}, game={game}"));
        return game;
    }
    u8::from(ours)
}

/// Snapshot of one collection's observable contents, for `checked` at
/// `0x877a79`: sorted `(hash, value_bytes)` in `+0x20` and `(hash, key_bytes,
/// value_bytes, file_path, line)` in `+0x38`.
#[derive(Debug, PartialEq, Eq)]
struct RecordSnap {
    hash: u64,
    key: Vec<u8>,
    val: Vec<u8>,
    file: Vec<u8>,
    line: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct CollectionSnap {
    entries: Vec<(u64, Vec<u8>)>,
    records: Vec<RecordSnap>,
}

unsafe fn c_bytes(at: usize) -> Vec<u8> {
    if at == 0 {
        return Vec::new();
    }
    unsafe { CStr::from_ptr(at as *const i8) }.to_bytes().to_vec()
}

unsafe fn std_string_bytes(at: usize) -> Vec<u8> {
    crate::game_string::text(at as *const u8).to_vec()
}

unsafe fn snapshot_store(store: usize) -> Vec<(Vec<u8>, CollectionSnap)> {
    if store == 0 {
        return Vec::new();
    }
    let arena = unsafe { *((store + STORE_ARENA) as *const usize) };
    let cols = unsafe { *((store + STORE_COLLECTIONS) as *const usize) };
    let cols_end = unsafe { *((store + STORE_COLLECTIONS_END) as *const usize) };
    let count = cols_end.saturating_sub(cols) / 8;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let col = unsafe { *((cols + i * 8) as *const usize) };
        if col == 0 {
            continue;
        }
        let lang = unsafe { std_string_bytes(col) };
        let entries_base = unsafe { *((col + COL_ENTRIES) as *const usize) };
        let entries_end = unsafe { *((col + COL_ENTRIES_END) as *const usize) };
        let n_entries = entries_end.saturating_sub(entries_base) / ENTRY_SIZE;
        let mut entries = Vec::with_capacity(n_entries);
        for j in 0..n_entries {
            let e = entries_base + j * ENTRY_SIZE;
            let hash = unsafe { *(e as *const u64) };
            let val_off = unsafe { *((e + 8) as *const u32) } as usize;
            entries.push((hash, unsafe { c_bytes(arena + val_off) }));
        }
        let files_base = unsafe { *((col + COL_FILES) as *const usize) };
        let records_base = unsafe { *((col + COL_RECORDS) as *const usize) };
        let records_end = unsafe { *((col + COL_RECORDS_END) as *const usize) };
        let n_records = records_end.saturating_sub(records_base) / RECORD_SIZE;
        let mut records = Vec::with_capacity(n_records);
        for j in 0..n_records {
            let r = records_base + j * RECORD_SIZE;
            let hash = unsafe { *(r as *const u64) };
            let key_off = unsafe { *((r + RECORD_KEY) as *const u32) } as usize;
            let val_off = unsafe { *((r + RECORD_VAL) as *const u32) } as usize;
            let file_idx = unsafe { *((r + RECORD_FILE) as *const u32) };
            let line = unsafe { *((r + RECORD_LINE) as *const u32) };
            let file = if file_idx == u32::MAX || files_base == 0 { Vec::new() } else { unsafe { std_string_bytes(files_base + file_idx as usize * 0x20) } };
            records.push(RecordSnap { hash, key: unsafe { c_bytes(arena + key_off) }, val: unsafe { c_bytes(arena + val_off) }, file, line });
        }
        out.push((lang, CollectionSnap { entries, records }));
    }
    out
}

#[called_by_game]
extern "C" fn main_reload() {
    if !speed::checked(&SPEED_UP) {
        return;
    }
    let m = crate::hooked_memory();
    let before = unsafe { snapshot_store(m.ptr(m.at(LOCALISATION)).unwrap_or(0)) };
    FORCE_GAME.with(|f| f.set(true));
    let _ = RELOAD_LOC.call(&m, []);
    FORCE_GAME.with(|f| f.set(false));
    let after = unsafe { snapshot_store(m.ptr(m.at(LOCALISATION)).unwrap_or(0)) };
    speed::compared(&SPEED_UP, before == after, || format!("localisation store differed after main's reload: {} vs {} language(s)", before.len(), after.len()));
}

#[unsafe(naked)]
extern "C" fn main_reload_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call main_reload),
        "mov al, 1",
        "jmp qword ptr [rip + {after}]",
        main_reload = sym main_reload,
        after = sym MAIN_AFTER_RELOAD_AT,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawRecord {
        hash: u64,
        key: u32,
        val: u32,
        file: u32,
        line: u32,
    }

    #[test]
    fn record_index_matches_linear_search_and_catches_up_on_appends() {
        let mut raw = vec![
            RawRecord { hash: 0x1111, key: 0, val: 2, file: 0, line: 1 },
            RawRecord { hash: 0x2222, key: 4, val: 6, file: 0, line: 2 },
            RawRecord { hash: 0x1111, key: 8, val: 10, file: 0, line: 3 },
            RawRecord { hash: 0x1111, key: 12, val: 14, file: 1, line: 1 },
        ];
        let mut index = RecordIndex::default();
        unsafe { index.catch_up(raw.as_ptr() as usize, raw.len() as u32) };
        for (h, f) in [(0x1111, 0), (0x1111, 1), (0x1111, 2), (0x2222, 0), (0x3333, 0)] {
            assert_eq!(index.get(h, f), unsafe { linear_record_search(raw.as_ptr() as usize, raw.len() as u32, h, f) }, "({h:#x}, {f})");
        }
        assert_eq!(index.get(0x1111, 0), Some(0), "keeps the first record of the same (hash, file)");

        // Appending hundreds of records triggers growth and preserves earlier answers.
        for i in 0..400u32 {
            raw.push(RawRecord { hash: 0x1_0000 + u64::from(i), key: 0, val: 0, file: i % 7, line: i });
        }
        unsafe { index.catch_up(raw.as_ptr() as usize, raw.len() as u32) };
        for (h, f) in [(0x1111, 0), (0x1111, 1), (0x1_0000, 0), (0x1_018f, 399 % 7), (0x1_018f, 6)] {
            assert_eq!(index.get(h, f), unsafe { linear_record_search(raw.as_ptr() as usize, raw.len() as u32, h, f) });
        }
    }

    #[test]
    fn adjacent_equal_hashes_detects_duplicates_in_sorted_entries() {
        #[repr(C)]
        struct Entry {
            hash: u64,
            val: u32,
            pad: u32,
        }
        #[repr(C)]
        struct FakeCol {
            _lang: [u8; 0x20],
            entries: usize,
            entries_end: usize,
        }
        #[repr(C)]
        struct FakeStore {
            _active: usize,
            cols: usize,
            cols_end: usize,
        }

        let distinct = [Entry { hash: 1, val: 0, pad: 0 }, Entry { hash: 2, val: 4, pad: 0 }, Entry { hash: 5, val: 8, pad: 0 }];
        let col1 = FakeCol { _lang: [0; 0x20], entries: distinct.as_ptr() as usize, entries_end: unsafe { distinct.as_ptr().add(distinct.len()) } as usize };
        let ptrs1 = [&raw const col1 as usize];
        let store1 = FakeStore { cols: ptrs1.as_ptr() as usize, cols_end: unsafe { ptrs1.as_ptr().add(1) } as usize, _active: 0 };
        assert_eq!(unsafe { any_adjacent_equal_hash(&raw const store1 as usize) }, Some(false));

        let dup = [Entry { hash: 1, val: 0, pad: 0 }, Entry { hash: 2, val: 4, pad: 0 }, Entry { hash: 2, val: 8, pad: 0 }];
        let col2 = FakeCol { _lang: [0; 0x20], entries: dup.as_ptr() as usize, entries_end: unsafe { dup.as_ptr().add(dup.len()) } as usize };
        let ptrs2 = [&raw const col1 as usize, &raw const col2 as usize];
        let store2 = FakeStore { cols: ptrs2.as_ptr() as usize, cols_end: unsafe { ptrs2.as_ptr().add(2) } as usize, _active: 0 };
        assert_eq!(unsafe { any_adjacent_equal_hash(&raw const store2 as usize) }, Some(true));
    }
}
