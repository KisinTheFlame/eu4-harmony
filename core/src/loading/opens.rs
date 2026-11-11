//! Files opened and looked up side by side (docs/plans/loading.md, L4).
//!
//! Two global locks let one thread at a time open or look up a file: the
//! game's own spin lock around every PhysFS open (`0x1718300`) and every
//! close, and PhysFS's `stateLock`, held through each open's and each
//! lookup's walk of the search path, which asks every folder and zip in turn.
//! While the databases load, the LoadingThread waited about half its time on
//! them, for the data checksum's thread, the picture checks and the pool
//! threads that read the history (docs/research/loading.md, "The databases").
//!
//! - An open to read no longer takes the game's lock; an open to write or
//!   append, and every close, take it as before.
//! - PhysFS's lock is held shared by an open to read, a lookup (under
//!   `PHYSFS_exists` and `PHYSFS_getRealDir`) and a close, and alone by every
//!   other PhysFS call that takes it, as before; a thread that holds it alone
//!   and opens, looks up or closes holds it alone still.
//! - What the walks write is made safe among readers: the list of open files
//!   is changed under a lock of its own; a zip's lookup of a name leaves the
//!   entry it finds where it is in its bucket, instead of moving it to the
//!   front; an entry of a zip is read the first time under a lock of its own,
//!   and once read it is answered as the game answers it.
//!
//! The files opened, what is read from them and what each lookup answers stay
//! as they were.
use crate::called_by_game;
use crate::points::{Function, Point};
use crate::speed::SpeedUp;
use std::cell::{Cell, UnsafeCell};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use windows_sys::Win32::System::Threading::{
    AcquireSRWLockExclusive, AcquireSRWLockShared, EnterCriticalSection, LeaveCriticalSection, ReleaseSRWLockExclusive, ReleaseSRWLockShared, CRITICAL_SECTION,
    SRWLOCK, SRWLOCK_INIT,
};

/// The game's open (`file, mode, buffer`; mode 0 reads, 1 writes, 2
/// appends): where it takes its lock (`mov rcx, [0x242b9c0]; call
/// 0x1581820`, then the guard's flag at `[rsp + 0x38]`) and gives it back
/// (`mov rax, [0x242b9c0]; mov byte ptr [rax], 0`); the mode is in `r14d`
/// throughout.
const OPEN: usize = 0x1718300;
const OPEN_TAKES: usize = 0x1718346;
const OPEN_TAKEN: usize = 0x1718357;
const OPEN_READS: usize = 0x171839d;
const OPEN_GIVES: usize = 0x17183a8;
const OPEN_GIVEN: usize = 0x17183b3;
/// The global that holds the address of the game's lock, a byte, and the spin
/// lock that takes it.
const OPEN_LOCK: usize = 0x242b9c0;
pub(crate) const SPIN: usize = 0x1581820;
/// PhysFS's lock (a critical section's address) and its platform's grab and
/// release of a mutex, which every PhysFS mutex goes through.
const STATE_LOCK: usize = 0x2339238;
const GRAB: usize = 0x1a05e60;
const RELEASE: usize = 0x1a06150;
/// `PHYSFS_openRead`: its grab, its link of the new file into the list of
/// open files (`[0x23391f8]`, the next at `+0x38`), and its release.
const READ_GRABS: usize = 0x19f8026;
const READ_GRABBED: usize = 0x19f8035;
const READ_LINKS: usize = 0x19f80cb;
const READ_RELEASES: usize = 0x19f80dd;
const READ_RELEASED: usize = 0x19f80e9;
const OPEN_FILES: usize = 0x23391f8;
const NEXT_OPEN: usize = 0x38;
/// The lookup under `PHYSFS_exists` and `PHYSFS_getRealDir`: the search
/// path's entry that has a file.
const FIND: usize = 0x19fad70;
const FIND_GRABS: usize = 0x19fae30;
const FIND_GRABBED: usize = 0x19fae3c;
const FIND_RELEASES: usize = 0x19fae8d;
const FIND_RELEASED: usize = 0x19fae99;
/// `PHYSFS_close`: its grab, then its walk of the lists of open files, and
/// its two releases.
const CLOSE_GRABS: usize = 0x19f66cd;
const CLOSE_GRABBED: usize = 0x19f66d9;
const CLOSE_RELEASES: usize = 0x19f66ef;
const CLOSE_RELEASED: usize = 0x19f66fb;
const CLOSE_RELEASES_AFTER: usize = 0x19f6722;
const CLOSE_RELEASED_AFTER: usize = 0x19f672e;
/// PhysFS's tree of a zip's names: a lookup that finds its name moves the
/// entry to the front of its bucket, unless it is there already (`test r11,
/// r11; je`).
const TREE_FOUND: usize = 0x19f93a1;
/// A zip's entry read (`io, zip, entry -> found`): its local header read the
/// first time, through the zip's own file; the entry's state at `+0x30`, 3
/// once read and 4 for a folder, which it answers at once.
const ENTRY_READ: usize = 0x1a0b420;
const ENTRY_STATES: usize = 0x1a0b441;
const ENTRY_STATE: usize = 0x30;
const ENTRY_READ_DONE: u32 = 3;
const ENTRY_FOLDER: u32 = 4;

/// Left to the game, one thread at a time opens, looks up or closes a file;
/// the answers are the same either way, so there is nothing to check.
pub const SPEED_UP: SpeedUp = SpeedUp { name: "opens", group: "loading", points, checkable: false };

fn points() -> Vec<Point> {
    vec![
        Point::keep(OPEN, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55", "the game's open"),
        Point::jump(
            OPEN_TAKES,
            "48 8b 0d 73 36 d1 00 e8 ce 94 e6 ff c6 44 24 38 01",
            "the game's open takes its lock",
            open_takes_stub as *const () as usize,
            &OPEN_TAKES_RESUME,
        )
        .uses(&[OPEN_LOCK]),
        Point::keep(OPEN_TAKEN, "41 8b ce 45 85 f6 74 3e", "the game's open by its mode"),
        Point::keep(OPEN_READS, "48 8b cf e8 bb fb 2d 00", "the game's open to read"),
        Point::jump(
            OPEN_GIVES,
            "48 8b 05 11 36 d1 00 90 c6 00 00",
            "the game's open gives its lock back",
            open_gives_stub as *const () as usize,
            &OPEN_GIVES_RESUME,
        )
        .uses(&[OPEN_LOCK]),
        Point::keep(SPIN, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 be 01 00 00 00", "the game's spin lock").recorded(SPIN_LOCK.slot()),
        Point::replace(GRAB, "48 83 ec 28 ff 15 e6 08 16 00 b8 01 00 00 00 48 83 c4 28 c3", "PhysFS grabs a mutex", grab as Grab, &ORIGINAL_GRAB),
        Point::replace(RELEASE, "48 ff 25 01 06 16 00 cc cc cc cc cc cc cc cc cc", "PhysFS releases a mutex", release as Release, &ORIGINAL_RELEASE),
        Point::jump(
            READ_GRABS,
            "48 8b 0d 0b 12 94 00 48 8b f7 e8 2b de 00 00",
            "PhysFS open grabs its lock",
            read_grabs_stub as *const () as usize,
            &READ_GRABS_RESUME,
        )
        .uses(&[STATE_LOCK]),
        Point::jump(
            READ_LINKS,
            "48 8b 05 26 11 94 00 48 89 47 38 48 89 3d 1b 11 94 00",
            "PhysFS open links its file",
            read_links_stub as *const () as usize,
            &READ_LINKS_RESUME,
        )
        .uses(&[OPEN_FILES, NEXT_OPEN]),
        Point::jump(
            READ_RELEASES,
            "48 8b 0d 54 11 94 00 e8 67 e0 00 00",
            "PhysFS open releases its lock",
            read_releases_stub as *const () as usize,
            &READ_RELEASES_RESUME,
        )
        .uses(&[STATE_LOCK]),
        Point::keep(FIND, "40 55 48 83 ec 60 48 8d 6c 24 20 48 89 5d 58 48", "PhysFS's lookup of a file"),
        Point::jump(
            FIND_GRABS,
            "48 8b 0d 01 e4 93 00 e8 24 b0 00 00",
            "PhysFS lookup grabs its lock",
            find_grabs_stub as *const () as usize,
            &FIND_GRABS_RESUME,
        )
        .uses(&[STATE_LOCK]),
        Point::jump(
            FIND_RELEASES,
            "48 8b 0d a4 e3 93 00 e8 b7 b2 00 00",
            "PhysFS lookup releases its lock",
            find_releases_stub as *const () as usize,
            &FIND_RELEASES_RESUME,
        )
        .uses(&[STATE_LOCK]),
        Point::jump(
            CLOSE_GRABS,
            "48 8b 0d 64 2b 94 00 e8 87 f7 00 00",
            "PhysFS close grabs its lock",
            close_grabs_stub as *const () as usize,
            &CLOSE_GRABS_RESUME,
        )
        .uses(&[STATE_LOCK]),
        Point::jump(
            CLOSE_RELEASES,
            "48 8b 0d 42 2b 94 00 e8 55 fa 00 00",
            "PhysFS close releases its lock",
            close_releases_stub as *const () as usize,
            &CLOSE_RELEASES_RESUME,
        )
        .uses(&[STATE_LOCK]),
        Point::jump(
            CLOSE_RELEASES_AFTER,
            "48 8b 0d 0f 2b 94 00 e8 22 fa 00 00",
            "PhysFS close releases its lock after the lists",
            close_releases_after_stub as *const () as usize,
            &CLOSE_RELEASES_AFTER_RESUME,
        )
        .uses(&[STATE_LOCK]),
        Point::write(TREE_FOUND, "4d 85 db 74 1c 49 8b 48 08", "PhysFS's tree moves what it finds to the front", "4d 85 db eb 1c 49 8b 48 08"),
        Point::replace(
            ENTRY_READ,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 41 8b 78 30",
            "PhysFS reads a zip's entry",
            entry_read as EntryRead,
            &ORIGINAL_ENTRY_READ,
        )
        .uses(&[ENTRY_STATE]),
        Point::keep(ENTRY_STATES, "41 b9 01 00 00 00 83 ff 04 74 7f", "a zip's entry read answers a folder at once").uses(&[ENTRY_FOLDER as usize]),
    ]
}

type Spin = unsafe extern "C" fn(usize) -> u8;
static SPIN_LOCK: Function<Spin> = Function::new();
type Grab = extern "C" fn(*mut CRITICAL_SECTION) -> i32;
static ORIGINAL_GRAB: Function<Grab> = Function::new();
type Release = extern "C" fn(*mut CRITICAL_SECTION);
static ORIGINAL_RELEASE: Function<Release> = Function::new();
type EntryRead = extern "C" fn(usize, usize, usize) -> i32;
static ORIGINAL_ENTRY_READ: Function<EntryRead> = Function::new();

crate::resume!(OPEN_TAKES_RESUME: OPEN_TAKEN_AT = OPEN_TAKEN);
crate::resume!(OPEN_GIVES_RESUME: OPEN_GIVEN_AT = OPEN_GIVEN);
crate::resume!(READ_GRABS_RESUME: READ_GRABBED_AT = READ_GRABBED);
crate::resume!(READ_LINKS_RESUME: READ_LINKED_AT = READ_RELEASES);
crate::resume!(READ_RELEASES_RESUME: READ_RELEASED_AT = READ_RELEASED);
crate::resume!(FIND_GRABS_RESUME: FIND_GRABBED_AT = FIND_GRABBED);
crate::resume!(FIND_RELEASES_RESUME: FIND_RELEASED_AT = FIND_RELEASED);
crate::resume!(CLOSE_GRABS_RESUME: CLOSE_GRABBED_AT = CLOSE_GRABBED);
crate::resume!(CLOSE_RELEASES_RESUME: CLOSE_RELEASED_AT = CLOSE_RELEASED);
crate::resume!(CLOSE_RELEASES_AFTER_RESUME: CLOSE_RELEASED_AFTER_AT = CLOSE_RELEASED_AFTER);

/// An SRW lock in a static.
struct Lock(UnsafeCell<SRWLOCK>);
// The lock is the SRW lock's own.
unsafe impl Sync for Lock {}

impl Lock {
    const fn new() -> Self {
        Self(UnsafeCell::new(SRWLOCK_INIT))
    }
    fn shared(&self) {
        unsafe { AcquireSRWLockShared(self.0.get()) }
    }
    fn shared_done(&self) {
        unsafe { ReleaseSRWLockShared(self.0.get()) }
    }
    fn alone(&self) {
        unsafe { AcquireSRWLockExclusive(self.0.get()) }
    }
    fn alone_done(&self) {
        unsafe { ReleaseSRWLockExclusive(self.0.get()) }
    }
}

/// PhysFS's lock as readers and the one that changes share it; the list of
/// open files; a zip's entries as they are first read.
static STATE: Lock = Lock::new();
static OPEN_FILES_LOCK: Lock = Lock::new();
static ENTRIES: Lock = Lock::new();
/// The thread reading an entry, which may read another inside (a link's
/// target), by its id; 0 for none.
static ENTRIES_HOLDER: AtomicU32 = AtomicU32::new(0);
static CHANGED_INSIDE_READ: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// How many times this thread holds PhysFS's lock alone, and shared.
    static ALONE: Cell<u32> = const { Cell::new(0) };
    static SHARED: Cell<u32> = const { Cell::new(0) };
    /// Grabs of the lock to change PhysFS's state made while this thread
    /// held it shared: none in the game's code.
    static INSIDE_SHARED: Cell<u32> = const { Cell::new(0) };
    static ENTRIES_DEPTH: Cell<u32> = const { Cell::new(0) };
}

fn base() -> usize {
    crate::hooked_memory().base
}

/// PhysFS's lock: 0 before PhysFS starts and after it ends.
fn state_lock() -> usize {
    unsafe { ((base() + STATE_LOCK) as *const usize).read() }
}

fn hold_alone() {
    if SHARED.get() > 0 {
        INSIDE_SHARED.set(INSIDE_SHARED.get() + 1);
        if !CHANGED_INSIDE_READ.swap(true, Ordering::Relaxed) {
            log!("opens: PhysFS's lock grabbed to change its state inside a read");
        }
        return;
    }
    let alone = ALONE.get();
    if alone == 0 {
        STATE.alone();
    }
    ALONE.set(alone + 1);
}

fn give_back_alone() {
    let inside = INSIDE_SHARED.get();
    if inside > 0 {
        INSIDE_SHARED.set(inside - 1);
        return;
    }
    // None held: a grab made before the part was installed.
    match ALONE.get() {
        0 => {}
        1 => {
            ALONE.set(0);
            STATE.alone_done();
        }
        n => ALONE.set(n - 1),
    }
}

/// Replaces PhysFS's grab of a mutex: PhysFS's lock held alone as well.
#[called_by_game]
extern "C" fn grab(mutex: *mut CRITICAL_SECTION) -> i32 {
    if mutex as usize == state_lock() {
        hold_alone();
    }
    unsafe { EnterCriticalSection(mutex) };
    1
}

/// Replaces PhysFS's release of a mutex.
#[called_by_game]
extern "C" fn release(mutex: *mut CRITICAL_SECTION) {
    let state = mutex as usize == state_lock();
    unsafe { LeaveCriticalSection(mutex) };
    if state {
        give_back_alone();
    }
}

/// PhysFS's lock held to read: shared, or alone again by a thread that holds
/// it alone.
#[called_by_game]
extern "C" fn hold_to_read() {
    if ALONE.get() > 0 {
        grab(state_lock() as *mut CRITICAL_SECTION);
        return;
    }
    let shared = SHARED.get();
    if shared == 0 {
        STATE.shared();
    }
    SHARED.set(shared + 1);
}

#[called_by_game]
extern "C" fn give_back_read() {
    let shared = SHARED.get();
    if shared == 0 {
        release(state_lock() as *mut CRITICAL_SECTION);
        return;
    }
    SHARED.set(shared - 1);
    if shared == 1 {
        STATE.shared_done();
    }
}

/// A close holds the lock to read and the list of open files, which it walks
/// and takes its file out of.
#[called_by_game]
extern "C" fn hold_to_close() {
    hold_to_read();
    OPEN_FILES_LOCK.alone();
}

#[called_by_game]
extern "C" fn give_back_close() {
    OPEN_FILES_LOCK.alone_done();
    give_back_read();
}

/// What the open's link did: the new file first in the list of open files.
#[called_by_game]
extern "C" fn link_open_file(file: usize) {
    OPEN_FILES_LOCK.alone();
    let head = (base() + OPEN_FILES) as *mut usize;
    unsafe {
        ((file + NEXT_OPEN) as *mut usize).write(head.read());
        head.write(file);
    }
    OPEN_FILES_LOCK.alone_done();
}

/// The game's open takes its lock, for a mode other than reading.
#[called_by_game]
extern "C" fn take_open_lock() {
    let lock = unsafe { ((base() + OPEN_LOCK) as *const usize).read() };
    unsafe { SPIN_LOCK.get()(lock) };
}

/// The game's open gives its lock back, if it took it; the lock, which the
/// game's code leaves in `rax`.
#[called_by_game]
extern "C" fn give_open_lock(mode: u32) -> usize {
    let lock = unsafe { ((base() + OPEN_LOCK) as *const usize).read() };
    if mode != 0 {
        unsafe { &*(lock as *const AtomicU8) }.store(0, Ordering::Release);
    }
    lock
}

/// Replaces a zip's entry read: an entry read already, or a folder, answered
/// as the game answers it; any other read under the entries' lock, which a
/// read inside it (of a link's target) holds again.
#[called_by_game]
extern "C" fn entry_read(io: usize, zip: usize, entry: usize) -> i32 {
    let state = unsafe { &*((entry + ENTRY_STATE) as *const AtomicU32) }.load(Ordering::Acquire);
    if state == ENTRY_READ_DONE || state == ENTRY_FOLDER {
        return 1;
    }
    let me = crate::thread_id();
    if ENTRIES_HOLDER.load(Ordering::Acquire) != me {
        ENTRIES.alone();
        ENTRIES_HOLDER.store(me, Ordering::Release);
    }
    ENTRIES_DEPTH.set(ENTRIES_DEPTH.get() + 1);
    let found = ORIGINAL_ENTRY_READ.get()(io, zip, entry);
    let depth = ENTRIES_DEPTH.get() - 1;
    ENTRIES_DEPTH.set(depth);
    if depth == 0 {
        ENTRIES_HOLDER.store(0, Ordering::Release);
        ENTRIES.alone_done();
    }
    found
}

/// Replaces the game's open taking its lock: taken for a mode other than
/// reading, and the guard's flag set to whether it was.
#[unsafe(naked)]
extern "C" fn open_takes_stub() {
    std::arch::naked_asm!(
        "test r14d, r14d",
        "je 2f",
        crate::call_rust!(call take_open_lock),
        "mov byte ptr [rsp + 0x38], 1",
        "jmp qword ptr [rip + {taken}]",
        "2:",
        "mov byte ptr [rsp + 0x38], 0",
        "jmp qword ptr [rip + {taken}]",
        take_open_lock = sym take_open_lock,
        taken = sym OPEN_TAKEN_AT,
    );
}

/// Replaces the game's open giving its lock back: given back if it was
/// taken, and the lock in `rax` as the game's code leaves it.
#[unsafe(naked)]
extern "C" fn open_gives_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov ecx, r14d", call give_open_lock, "mov rax, [rsp + 0x48]"),
        "jmp qword ptr [rip + {given}]",
        give_open_lock = sym give_open_lock,
        given = sym OPEN_GIVEN_AT,
    );
}

/// Replaces PhysFS open's grab, and the `mov rsi, rdi` among it.
#[unsafe(naked)]
extern "C" fn read_grabs_stub() {
    std::arch::naked_asm!(
        "mov rsi, rdi",
        crate::call_rust!(call hold_to_read),
        "jmp qword ptr [rip + {grabbed}]",
        hold_to_read = sym hold_to_read,
        grabbed = sym READ_GRABBED_AT,
    );
}

/// Replaces PhysFS open's link of its file (`rdi`).
#[unsafe(naked)]
extern "C" fn read_links_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, rdi", call link_open_file),
        "jmp qword ptr [rip + {linked}]",
        link_open_file = sym link_open_file,
        linked = sym READ_LINKED_AT,
    );
}

#[unsafe(naked)]
extern "C" fn read_releases_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call give_back_read),
        "jmp qword ptr [rip + {released}]",
        give_back_read = sym give_back_read,
        released = sym READ_RELEASED_AT,
    );
}

#[unsafe(naked)]
extern "C" fn find_grabs_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call hold_to_read),
        "jmp qword ptr [rip + {grabbed}]",
        hold_to_read = sym hold_to_read,
        grabbed = sym FIND_GRABBED_AT,
    );
}

#[unsafe(naked)]
extern "C" fn find_releases_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call give_back_read),
        "jmp qword ptr [rip + {released}]",
        give_back_read = sym give_back_read,
        released = sym FIND_RELEASED_AT,
    );
}

#[unsafe(naked)]
extern "C" fn close_grabs_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call hold_to_close),
        "jmp qword ptr [rip + {grabbed}]",
        hold_to_close = sym hold_to_close,
        grabbed = sym CLOSE_GRABBED_AT,
    );
}

#[unsafe(naked)]
extern "C" fn close_releases_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call give_back_close),
        "jmp qword ptr [rip + {released}]",
        give_back_close = sym give_back_close,
        released = sym CLOSE_RELEASED_AT,
    );
}

#[unsafe(naked)]
extern "C" fn close_releases_after_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call give_back_close),
        "jmp qword ptr [rip + {released}]",
        give_back_close = sym give_back_close,
        released = sym CLOSE_RELEASED_AFTER_AT,
    );
}
