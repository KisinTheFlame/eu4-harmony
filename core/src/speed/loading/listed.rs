//! Every folder of PhysFS's search path answered from its listing, kept up to
//! date (docs/game/program/loading.md, L5).
//!
//! A lookup of a file checks the search path's entries in turn, and for a
//! folder PhysFS asks Windows about the path and about each folder on its
//! way, about 20 times a lookup at 29 µs each: 77% of a lookup's time
//! (docs/game/program/loading.md, "The databases with L4"). L2 answers only a path
//! missing from the game folder or the workshop's.
//!
//! Where PhysFS asks a folder and reads no more of the answer than whether
//! the path is there and whether it is a file, a folder or a link (the path's
//! check, and the lookup under `PHYSFS_exists` and `PHYSFS_getRealDir`), the
//! framework answers ASCII paths from the folder's listings: a file or a
//! folder, or PhysFS's "not found". A link, a non-ASCII path, a name Windows
//! would read otherwise than as written, and every other stat are left to the
//! game; non-ASCII entries in a listed folder are skipped, since Windows's
//! ordinal case folding maps none of them to ASCII.
//!
//! A listing is kept only while it holds. Each folder of the search path is
//! watched from before its first listing (`ReadDirectoryChangesW`) for names
//! made, removed or renamed below it, and the listings of the folders that
//! changed are let go of; when Windows cannot tell what changed, all the
//! folder's are. A file or folder the game makes or removes through PhysFS
//! lets go of its ancestor folders' listings before the call returns.
use super::folder::{lowered_relative, DirectoryCache, Kind};
use crate::base::called_by_game;
use crate::native::hooks::Call;
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{Arc, RwLock};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_INFORMATION, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Threading::CreateEventW;
use windows_sys::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};

/// The folder archive's table of functions; its stat (`root, path, &stat ->
/// found`), at `+0x68`.
const FOLDER_ARCHIVE: usize = 0x1c1d7a0;
const ARCHIVE_STAT: usize = 0x68;
const FOLDER_STAT: usize = 0x1a07080;
/// The path's check (`0x19fb700`) asks the entry (`r15`: its root at `+0`,
/// its archive at `+0x18`) about the path so far (`rsi`), into its stat at
/// `[rsp + 0x20]`, of which it reads the type (`+0x20`, 2 a link) and, when
/// not found, whether PhysFS's error is "not found".
const CHECK_ASKS: usize = 0x19fb7e5;
const CHECK_ASKED: usize = 0x19fb7f8;
/// The lookup (`0x19fad70`) asks the entry (`rbx`) about the path (`[rbp]`),
/// into its stat at `[rbp + 8]`, of which it reads nothing.
const FIND_ASKS: usize = 0x19fae6e;
const FIND_ASKED: usize = 0x19fae80;
const STAT_TYPE: usize = 0x20;
const TYPE_FILE: u32 = 0;
const TYPE_FOLDER: u32 = 1;
/// The game's writes through PhysFS: an open to write or append (`path,
/// appending`), a folder made and a file removed (`path`), each relative to
/// the write folder.
const OPEN_WRITE: usize = 0x19fa240;

/// Answered by the game's own stat as well, the answer is checked: whether
/// the path is there and, if it is, whether a file or a folder; the game's is
/// kept.
pub const SPEED_UP: SpeedUp = SpeedUp::new("listed-folders", "loading", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::method(FOLDER_ARCHIVE, ARCHIVE_STAT, FOLDER_STAT, "the folder archive's stat"),
        Point::keep(FOLDER_STAT, "40 55 41 56 41 57 48 83 ec 40 48 8d 6c 24 30 48", "the folder archive's stat").recorded(GAME_STAT.slot()),
        Point::jump(
            CHECK_ASKS,
            "4d 8b 4f 18 4c 8d 44 24 20 49 8b 0f 48 8b d6 41 ff 51 68",
            "PhysFS's path check asks an entry",
            check_asks_stub as *const () as usize,
            &CHECK_RESUME,
        )
        .uses(&[ARCHIVE_STAT]),
        Point::keep(CHECK_ASKED, "8b e8 85 c0 74 1c 83 7c 24 40 02", "PhysFS's path check reads the type").uses(&[2]),
        Point::jump(
            FIND_ASKS,
            "48 8b 43 18 4c 8d 45 08 48 8b 55 00 48 8b 0b ff 50 68",
            "PhysFS's lookup asks an entry",
            find_asks_stub as *const () as usize,
            &FIND_RESUME,
        )
        .uses(&[ARCHIVE_STAT]),
        Point::keep(FIND_ASKED, "85 c0 75 06", "PhysFS's lookup reads whether found"),
        Point::observe(OPEN_WRITE, "40 55 41 56 41 57 48 83 ec 30 48 8d 6c 24 20 48", "PhysFS opens to write", 2, None, Some(written)),
        Point::observe(c"PHYSFS_mkdir", "40 55 41 54 41 55 41 56 41 57 48 83 ec 60 48 8d", "PhysFS makes a folder", 1, None, Some(written)),
        Point::observe(c"PHYSFS_delete", "40 55 48 83 ec 30 48 8d 6c 24 20 48 89 5d 20 48", "PhysFS removes a file", 1, None, Some(written)),
        Point::keep(c"PHYSFS_getWriteDir", "40 53 48 83 ec 20 48 8b 0d db 1a 94 00 33 db e8", "PhysFS's write folder").recorded(WRITE_DIR.slot()),
    ]
}

type Stat = extern "C" fn(*const c_char, *const c_char, *mut u8) -> i32;
static GAME_STAT: Function<Stat> = Function::new();
type WriteDir = extern "C" fn() -> *const c_char;
static WRITE_DIR: Function<WriteDir> = Function::new();

resume!(CHECK_RESUME: CHECK_ASKED_AT = CHECK_ASKED);
resume!(FIND_RESUME: FIND_ASKED_AT = FIND_ASKED);

/// Not the listings' to answer: the game's stat answers.
const LEFT: i32 = -1;

/// The roots by the address of PhysFS's name for them, None for one that
/// cannot be watched.
static ROOTS: RwLock<Option<HashMap<usize, Option<Arc<DirectoryCache>>>>> = RwLock::new(None);

fn root(name: *const c_char) -> Option<Arc<DirectoryCache>> {
    let path = unsafe { CStr::from_ptr(name) }.to_str().ok()?;
    if let Some(found) = ROOTS.read().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|r| r.get(&(name as usize))) {
        // The same address may be a root mounted after another unmounted.
        match found {
            Some(root) if root.path() == path => return Some(root.clone()),
            None => return None,
            Some(_) => {}
        }
    }
    let mut roots = ROOTS.write().unwrap_or_else(|e| e.into_inner());
    let roots = roots.get_or_insert_with(HashMap::new);
    if let Some(Some(root)) = roots.get(&(name as usize)).filter(|r| r.as_ref().is_some_and(|r| r.path() == path)) {
        return Some(root.clone());
    }
    let made = (path.ends_with('\\') && path.is_ascii()).then(|| Arc::new(DirectoryCache::new(path)));
    let made = made.filter(|root| watch(root.clone()));
    if made.is_none() {
        log!("listed-folders: {path:?} left to the game's stat");
    }
    roots.insert(name as usize, made.clone());
    made
}

/// The path in lower case into `out`, if the listings can answer for it:
/// ASCII, `/`-separated names Windows reads as they are written.
fn lowered<'a>(path: &[u8], out: &'a mut [u8; 512]) -> Option<&'a str> {
    lowered_relative(path, false, out)
}

/// Watches `root` for names made, removed or renamed below it, from before
/// this returns; false if Windows will not.
fn watch(root: Arc<DirectoryCache>) -> bool {
    struct Watch {
        folder: HANDLE,
        overlapped: OVERLAPPED,
        buffer: [u32; 16384],
    }
    let wide: Vec<u16> = root.path().encode_utf16().chain([0]).collect();
    let folder = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    if folder == INVALID_HANDLE_VALUE {
        return false;
    }
    let mut watch = Box::new(Watch { folder, overlapped: unsafe { std::mem::zeroed() }, buffer: [0; 16384] });
    watch.overlapped.hEvent = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    let ask = |w: &mut Watch| unsafe {
        ReadDirectoryChangesW(
            w.folder,
            w.buffer.as_mut_ptr() as *mut c_void,
            std::mem::size_of_val(&w.buffer) as u32,
            1,
            FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME,
            std::ptr::null_mut(),
            &mut w.overlapped,
            None,
        ) != 0
    };
    if watch.overlapped.hEvent.is_null() || !ask(&mut watch) {
        unsafe { CloseHandle(folder) };
        return false;
    }
    let watch = SendWatch(watch);
    std::thread::spawn(move || {
        let mut w = watch.into_inner();
        loop {
            let mut bytes = 0;
            if unsafe { GetOverlappedResult(w.folder, &w.overlapped, &mut bytes, 1) } == 0 {
                root.invalidate_all();
                log!("listed-folders: watching {:?} failed; left to the game's stat from now", root.path());
                forget(&root);
                return;
            }
            if bytes == 0 {
                root.invalidate_all();
            } else {
                for path in changes(&w.buffer, bytes as usize) {
                    match lowered_relative(path.as_bytes(), true, &mut [0; 512]) {
                        Some(lower) => root.invalidate_path(lower),
                        None => root.invalidate_all(),
                    }
                }
            }
            if !ask(&mut w) {
                root.invalidate_all();
                forget(&root);
                return;
            }
        }
    });
    true
}

struct SendWatch<T>(Box<T>);
// The watch's handles and buffer are its thread's alone once it starts.
unsafe impl<T> Send for SendWatch<T> {}

impl<T> SendWatch<T> {
    /// Taken whole, so that a closure moves the watch and not its box alone.
    fn into_inner(self) -> Box<T> {
        self.0
    }
}

/// A root no longer watched: its paths go to the game's stat.
fn forget(root: &Arc<DirectoryCache>) {
    if let Some(roots) = ROOTS.write().unwrap_or_else(|e| e.into_inner()).as_mut() {
        for slot in roots.values_mut() {
            if slot.as_ref().is_some_and(|r| Arc::ptr_eq(r, root)) {
                *slot = None;
            }
        }
    }
}

/// The paths a buffer of `FILE_NOTIFY_INFORMATION` names, from the folder.
fn changes(buffer: &[u32], bytes: usize) -> Vec<String> {
    let base = buffer.as_ptr() as *const u8;
    let mut paths = Vec::new();
    let mut at = 0;
    while at + std::mem::size_of::<FILE_NOTIFY_INFORMATION>() <= bytes {
        let info = unsafe { &*(base.add(at) as *const FILE_NOTIFY_INFORMATION) };
        let name = unsafe { std::slice::from_raw_parts(info.FileName.as_ptr(), info.FileNameLength as usize / 2) };
        paths.push(String::from_utf16_lossy(name));
        if info.NextEntryOffset == 0 {
            break;
        }
        at += info.NextEntryOffset as usize;
    }
    paths
}

/// After the game made or removed something through PhysFS: the listings of
/// the path's ancestor folders are let go of.
fn written(call: &mut Call) {
    let path = call.arg(0) as *const c_char;
    let dir = WRITE_DIR.get()();
    if path.is_null() || dir.is_null() {
        return;
    }
    let dir = unsafe { CStr::from_ptr(dir) }.to_string_lossy();
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
    let Some(roots) = ROOTS.read().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    DirectoryCache::invalidate_written(roots.values().flatten().map(Arc::as_ref), &dir, &path);
}

/// Answers PhysFS's question to an entry of the search path, if it is a
/// folder and its listings can: 1 found, its type in `stat`; 0 not, with
/// PhysFS's "not found"; [`LEFT`] for the game's stat to answer.
#[called_by_game]
extern "C" fn ask(archive: usize, name: *const c_char, path: *const c_char, stat: *mut u8) -> i32 {
    let m = crate::native::hooked_memory();
    if archive != m.base + FOLDER_ARCHIVE || name.is_null() || path.is_null() {
        return LEFT;
    }
    let Some(root) = root(name) else { return LEFT };
    let mut buffer = [0; 512];
    let Some(lower) = lowered(unsafe { CStr::from_ptr(path) }.to_bytes(), &mut buffer) else { return LEFT };
    let kind = match root.kind(lower) {
        Ok(Some(Kind::Other)) | Err(()) => return LEFT,
        Ok(kind) => kind,
    };
    if speed::checked(&SPEED_UP) {
        let game = GAME_STAT.get()(name, path, stat);
        let game_kind = (game != 0).then(|| unsafe { (stat.add(STAT_TYPE) as *const u32).read() });
        let ours = kind.map(|k| if k == Kind::Folder { TYPE_FOLDER } else { TYPE_FILE });
        speed::compared(&SPEED_UP, game_kind == ours, || {
            format!("{}{} is {game_kind:?} to the game's stat, {ours:?} to the listings", root.path(), unsafe { CStr::from_ptr(path) }.to_string_lossy())
        });
        return game;
    }
    match kind {
        Some(kind) => {
            unsafe { (stat.add(STAT_TYPE) as *mut u32).write(if kind == Kind::Folder { TYPE_FOLDER } else { TYPE_FILE }) };
            1
        }
        None => {
            crate::native::physfs::set_not_found(&m);
            0
        }
    }
}

/// Replaces the path check's question: answered from the listings, or by the
/// entry's archive as before.
#[unsafe(naked)]
extern "C" fn check_asks_stub() {
    std::arch::naked_asm!(
        "mov r9, [r15 + 0x18]",
        "lea r8, [rsp + 0x20]",
        "mov rcx, [r15]",
        "mov rdx, rsi",
        call_rust!("mov r10, rcx", "mov rcx, r9", "mov r9, r8", "mov r8, rdx", "mov rdx, r10", call ask -> eax),
        "cmp eax, -1",
        "jne 2f",
        "call qword ptr [r9 + 0x68]",
        "2:",
        "jmp qword ptr [rip + {asked}]",
        ask = sym ask,
        asked = sym CHECK_ASKED_AT,
    );
}

/// Replaces the lookup's question the same way.
#[unsafe(naked)]
extern "C" fn find_asks_stub() {
    std::arch::naked_asm!(
        "mov rax, [rbx + 0x18]",
        "lea r8, [rbp + 8]",
        "mov rdx, [rbp]",
        "mov rcx, [rbx]",
        call_rust!("mov r10, rcx", "mov rcx, rax", "mov r9, r8", "mov r8, rdx", "mov rdx, r10", call ask -> eax),
        "cmp eax, -1",
        "jne 2f",
        "mov rax, [rbx + 0x18]",
        "call qword ptr [rax + 0x68]",
        "2:",
        "jmp qword ptr [rip + {asked}]",
        ask = sym ask,
        asked = sym FIND_ASKED_AT,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_at(dir: &std::path::Path) -> DirectoryCache {
        DirectoryCache::new(&dir.to_string_lossy())
    }

    #[test]
    fn a_path_is_answered_from_the_listings_of_its_folders() {
        let dir = std::env::temp_dir().join(format!("eu4-harmony-listed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("gfx").join("Interface")).unwrap();
        std::fs::write(dir.join("gfx").join("Interface").join("Icon.dds"), b"x").unwrap();
        std::fs::write(dir.join("1.310\u{66f4}\u{65b0}\u{5185}\u{5bb9}.txt"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("\u{7d20}\u{6750}")).unwrap();
        let root = root_at(&dir);
        let mut buffer = [0; 512];
        let mut ask = |path: &str| root.kind(lowered(path.as_bytes(), &mut buffer).unwrap());
        assert_eq!(ask("gfx/interface/icon.dds"), Ok(Some(Kind::File)));
        assert_eq!(ask("GFX/Interface/ICON.DDS"), Ok(Some(Kind::File)));
        assert_eq!(ask("gfx"), Ok(Some(Kind::Folder)));
        assert_eq!(ask("gfx/interface"), Ok(Some(Kind::Folder)));
        assert_eq!(ask("gfx/interface/other.dds"), Ok(None));
        assert_eq!(ask("gfx/missing/icon.dds"), Ok(None));
        assert_eq!(ask("missing/icon.dds"), Ok(None));
        // A file where a folder would be.
        assert_eq!(ask("gfx/interface/icon.dds/more"), Ok(None));
        // Non-ASCII paths stay with the game's stat.
        assert_eq!(lowered("1.310\u{66f4}\u{65b0}\u{5185}\u{5bb9}.txt".as_bytes(), &mut buffer), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_change_lets_go_of_its_folder_and_what_was_below_it() {
        let dir = std::env::temp_dir().join(format!("eu4-harmony-listed-change-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a").join("b")).unwrap();
        let root = root_at(&dir);
        assert_eq!(root.kind("a/b/c.txt"), Ok(None));
        std::fs::write(dir.join("a").join("b").join("c.txt"), b"x").unwrap();
        // Kept until told.
        assert_eq!(root.kind("a/b/c.txt"), Ok(None));
        root.invalidate_path("a/b/c.txt");
        assert_eq!(root.kind("a/b/c.txt"), Ok(Some(Kind::File)));
        std::fs::remove_dir_all(dir.join("a").join("b")).unwrap();
        root.invalidate_path("a/b");
        assert_eq!(root.kind("a/b/c.txt"), Ok(None));
        assert_eq!(root.kind("a/b"), Ok(None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nested_mkdir_invalidates_cached_ancestor_listings() {
        let dir = std::env::temp_dir().join(format!("eu4-harmony-listed-mkdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = root_at(&dir);
        // Populate the root listing before the nested directory exists, as
        // `PHYSFS_mkdir`'s own path check does on a fresh user folder.
        assert_eq!(root.kind("shadercache/dx9/vs_3_0/test.scache"), Ok(None));

        std::fs::create_dir_all(dir.join("shadercache").join("dx9").join("vs_3_0")).unwrap();
        let write_dir = dir.to_string_lossy();
        DirectoryCache::invalidate_written([&root], &write_dir, "shadercache/dx9/vs_3_0");
        assert_eq!(root.kind("shadercache"), Ok(Some(Kind::Folder)));
        assert_eq!(root.kind("shadercache/dx9"), Ok(Some(Kind::Folder)));
        assert_eq!(root.kind("shadercache/dx9/vs_3_0"), Ok(Some(Kind::Folder)));

        // Repeated separators normalize to the same ancestor chain.
        assert_eq!(root.kind("nested/sub/leaf"), Ok(None));
        std::fs::create_dir_all(dir.join("nested").join("sub").join("leaf")).unwrap();
        DirectoryCache::invalidate_written([&root], &write_dir, "nested//sub/leaf");
        assert_eq!(root.kind("nested"), Ok(Some(Kind::Folder)));
        assert_eq!(root.kind("nested/sub/leaf"), Ok(Some(Kind::Folder)));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_windows_reads_otherwise_are_not_the_listings_to_answer() {
        let mut buffer = [0; 512];
        for path in ["gfx/../gfx/x", "gfx//x", "gfx/ICON~1.DDS", "gfx/x.", "gfx/x ", "c:/x", "gfx\\x", "gfx/\u{e9}"] {
            assert_eq!(lowered(path.as_bytes(), &mut buffer), None, "{path}");
        }
        assert_eq!(lowered(b"Gfx/Interface/X.dds", &mut buffer), Some("gfx/interface/x.dds"));
    }

    #[test]
    fn a_folder_watched_tells_of_a_file_made_in_it() {
        let dir = std::env::temp_dir().join(format!("eu4-harmony-listed-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        let root = Arc::new(root_at(&dir));
        assert!(watch(root.clone()));
        assert_eq!(root.kind("logs/game.log"), Ok(None));
        std::fs::write(dir.join("logs").join("game.log"), b"x").unwrap();
        let start = std::time::Instant::now();
        while root.kind("logs/game.log") != Ok(Some(Kind::File)) {
            assert!(start.elapsed() < std::time::Duration::from_secs(5), "never told");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
