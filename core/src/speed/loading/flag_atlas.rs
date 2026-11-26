//! Cache flag-atlas validation (`0x10c5ff0`) and reuse already-uploaded flag
//! atlas page textures across in-game world resets (`0x10b2b70` -> `0x10c7280`)
//! (docs/game/program/load-preparation.md, "Flag cache verification is ordered
//! input work").
//!
//! Every world reset calls `0x10b2b70(graphics, 1)`, which destroys the flag
//! atlas textures (`0x10c5ea0`), re-validates every country/rebel flag `.tga`
//! on disk with a streaming MD5 digest plus an O(N^2) name check (`0x10c5ff0`,
//! ~175 ms), then decodes and re-uploads every `flagfiles_N.tga` page (~110 ms),
//! while also re-populating each newly constructed country object's 3-color flag
//! data (`0x453850`) and re-registering `flagmap0..N` in the texture manager.
//!
//! When the ordered flag-name list, directory, flag-dir mode, and page capacity
//! match the last validated run (and no D3D device reset intervened), this
//! module:
//! 1. Answers `0x10c5ff0` as valid (`0`) without re-hashing every flag file.
//! 2. Stashes the existing D3D9 page texture handles right before `0x10c5ea0`,
//!    lets `0x10c7280` rebuild country colors (`0x453850`), UV grids, and
//!    metadata normally, and re-attaches each stashed page texture handle at
//!    `0x10c9f8f` so `flagfiles_N.tga` is not decoded and uploaded again.
//! 3. Releases any unused stashed texture handles through `0x1fd1330` as soon
//!    as `0x10c7280` finishes, and invalidates the texture stash on any D3D
//!    device reset (`0x14ddc70`).
use crate::base::called_by_game;
use crate::native::game_string;
use crate::native::hooks::Call;
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const VALIDATE: usize = 0x10c5ff0;
const LOAD_ATLAS: usize = 0x10c7280;
const CLEAR_ATLAS: usize = 0x10c5ea0;
const CLEAR_AT_LOAD: usize = 0x10b2b97;
const AFTER_CLEAR_AT_LOAD: usize = 0x10b2ba6;
const PAGE_LOAD_CHECK: usize = 0x10c9f8f;
const PAGE_LOAD_NEEDS_FILE: usize = 0x10c9fab;
const PAGE_LOAD_SKIP: usize = 0x10ca110;
const CHECK_RELOAD_NEEDED: usize = 0x16c10a0;
const BEFORE_DEVICE_RESET: usize = 0x14ddc70;
const FLAG_CONFIG_GLOBAL: usize = 0x233fe28;
const DESTROY_TEXTURE_SLOT: usize = 0x1fd1330;

type Validate = unsafe extern "C" fn(usize, usize, usize) -> u8;
type CheckReload = unsafe extern "C" fn() -> u8;
type DestroyTexture = unsafe extern "C" fn(usize);

static ORIGINAL_VALIDATE: Function<Validate> = Function::new();
static CLEAR_ATLAS_FN: Function<unsafe extern "C" fn(usize)> = Function::new();
static CHECK_RELOAD_FN: Function<CheckReload> = Function::new();

/// Incremented whenever the graphics object invalidates device resources for a
/// D3D9 device reset (`0x14ddc70`).
static DEVICE_RESET_GEN: AtomicU64 = AtomicU64::new(0);

#[derive(PartialEq, Eq)]
struct ValidationKey {
    dir_path: Vec<u8>,
    mode: i32,
    mod_dir: Vec<u8>,
    flags_per_page: i32,
    flag_names: Vec<Vec<u8>>,
}

static LAST_VALID: Mutex<Option<(ValidationKey, u64)>> = Mutex::new(None);

#[derive(Default)]
struct Stash {
    /// Device reset generation when the textures were stashed.
    gen: u64,
    /// Texture handles stolen from `flagmap`'s pages before `0x10c5ea0`.
    handles: Vec<usize>,
}

thread_local! {
    static STASH: RefCell<Stash> = RefCell::new(Stash::default());
}

resume!(CLEAR_AT_LOAD_RESUME: AFTER_CLEAR_AT = AFTER_CLEAR_AT_LOAD);
resume!(PAGE_LOAD_RESUME: LOAD_FILE_AT = PAGE_LOAD_NEEDS_FILE, SKIP_FILE_AT = PAGE_LOAD_SKIP);

pub const SPEED_UP: SpeedUp = SpeedUp::new("flag-atlas", "loading", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(
            VALIDATE,
            "48 89 5c 24 18 48 89 54 24 10 48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57",
            "validate flag atlas cache",
            validate_cache as Validate,
            &ORIGINAL_VALIDATE,
        ),
        Point::keep(CLEAR_ATLAS, "48 89 5c 24 20 55 48 83 ec 20 48 8b 41 20 48 8b d9 48 2b 41 18", "clear flag atlas textures").recorded(CLEAR_ATLAS_FN.slot()),
        Point::jump(
            CLEAR_AT_LOAD,
            "48 8d b1 10 26 03 00 48 8b ce e8 fa 32 01 00",
            "flag atlas cleared at load",
            stash_before_clear_stub as *const () as usize,
            &CLEAR_AT_LOAD_RESUME,
        ),
        Point::keep(AFTER_CLEAR_AT_LOAD, "48 8b 05 23 ad 28 01", "flag atlas load continues after clear"),
        Point::jump(
            PAGE_LOAD_CHECK,
            "49 8b 44 24 18 4a 8b 04 f8 48 83 38 00 74 0d",
            "flag atlas page texture load check",
            reuse_page_texture_stub as *const () as usize,
            &PAGE_LOAD_RESUME,
        ),
        Point::keep(PAGE_LOAD_NEEDS_FILE, "45 33 c9 45 33 c0 48 8d 94 24 98 00 00 00", "flag atlas page loads cached tga"),
        Point::keep(PAGE_LOAD_SKIP, "48 8d 8c 24 98 00 00 00 e8 43 b5 fc fe", "flag atlas page skips cached tga load"),
        Point::keep(CHECK_RELOAD_NEEDED, "48 83 ec 28 ff 15 76 01 91 00", "flag atlas check texture reload needed").recorded(CHECK_RELOAD_FN.slot()),
        Point::observe(LOAD_ATLAS, "4c 89 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08", "load flag atlas", 5, None, Some(after_load_atlas)),
        Point::observe(
            BEFORE_DEVICE_RESET,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 33 ff 48 8b f1",
            "invalidate resources before device reset",
            1,
            Some(on_device_reset),
            None,
        ),
        Point::keep(0x10c602dusize, "48 8b 15 f4 9d 27 01", "flag validation config global").uses(&[FLAG_CONFIG_GLOBAL]),
        Point::keep(0x10c5ef0usize, "ff 15 3a b4 f0 00", "flag atlas destroys texture via graphics table").uses(&[DESTROY_TEXTURE_SLOT]),
    ]
}

fn on_device_reset(_call: &mut Call) {
    DEVICE_RESET_GEN.fetch_add(1, Ordering::AcqRel);
}

unsafe fn read_validation_key(flagmap: usize, dir_str: usize, flag_names_vec: usize) -> Option<ValidationKey> {
    let m = crate::native::hooked_memory();
    let cfg = (m.at(FLAG_CONFIG_GLOBAL) as *const usize).read();
    if cfg == 0 || dir_str == 0 || flag_names_vec == 0 {
        return None;
    }
    let mode = ((cfg + 0x40) as *const i32).read();
    let mod_dir = game_string::text((cfg + 0xa8) as *const u8).to_vec();
    let dir_path = game_string::text(dir_str as *const u8).to_vec();
    let flags_per_page = ((flagmap + 0x58) as *const i32).read();
    let begin = (flag_names_vec as *const usize).read();
    let end = ((flag_names_vec + 8) as *const usize).read();
    if end < begin || !(end - begin).is_multiple_of(game_string::OBJECT) {
        return None;
    }
    let count = (end - begin) / game_string::OBJECT;
    let mut flag_names = Vec::with_capacity(count);
    for i in 0..count {
        let entry = (begin + i * game_string::OBJECT) as *const u8;
        flag_names.push(game_string::text(entry).to_vec());
    }
    Some(ValidationKey { dir_path, mode, mod_dir, flags_per_page, flag_names })
}

#[called_by_game]
unsafe extern "C" fn validate_cache(flagmap: usize, dir_str: usize, flag_names_vec: usize) -> u8 {
    let key = read_validation_key(flagmap, dir_str, flag_names_vec);
    let cur_gen = DEVICE_RESET_GEN.load(Ordering::Acquire);

    if let Some(ref k) = key {
        let hit = LAST_VALID.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|(cached, _)| cached == k);
        if hit {
            if speed::checked(&SPEED_UP) {
                let game = ORIGINAL_VALIDATE.get()(flagmap, dir_str, flag_names_vec);
                speed::compared(&SPEED_UP, game == 0, || format!("flag cache validation returned {game} where cached valid"));
                if game != 0 {
                    *LAST_VALID.lock().unwrap_or_else(|e| e.into_inner()) = None;
                }
                return game;
            }
            return 0;
        }
    }

    let result = ORIGINAL_VALIDATE.get()(flagmap, dir_str, flag_names_vec);
    if result == 0 {
        if let Some(k) = key {
            *LAST_VALID.lock().unwrap_or_else(|e| e.into_inner()) = Some((k, cur_gen));
        }
    } else {
        *LAST_VALID.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    result
}

/// Releases any texture handles remaining in `STASH` via the graphics table's
/// `DestroyTexture` slot (`0x1fd1330`).
unsafe fn drain_stash() {
    let leftover: Vec<usize> = STASH.with(|cell| {
        let mut s = cell.borrow_mut();
        s.handles.drain(..).filter(|&h| h != 0).collect()
    });
    if leftover.is_empty() {
        return;
    }
    let m = crate::native::hooked_memory();
    let destroy_ptr = (m.at(DESTROY_TEXTURE_SLOT) as *const usize).read();
    if destroy_ptr != 0 {
        let destroy: DestroyTexture = std::mem::transmute(destroy_ptr);
        for handle in leftover {
            destroy(handle);
        }
    }
}

/// Called right before `0x10c5ea0(flagmap)` inside `0x10b2b70(graphics, is_reset)`.
/// On a world reset (`is_reset != 0`), if we already validated the atlas in this
/// device generation, steal the existing D3D9 page texture handles so `0x10c5ea0`
/// frees only the page wrappers and does not destroy the GPU textures.
#[called_by_game]
unsafe extern "C" fn stash_before_clear(flagmap: usize, is_reset: u8) {
    drain_stash();
    if is_reset == 0 || speed::checked(&SPEED_UP) {
        return;
    }
    let cur_gen = DEVICE_RESET_GEN.load(Ordering::Acquire);
    let can_reuse = LAST_VALID.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|(_, valid_gen)| *valid_gen == cur_gen);
    if !can_reuse {
        return;
    }

    let begin = ((flagmap + 0x18) as *const usize).read();
    let end = ((flagmap + 0x20) as *const usize).read();
    if begin == 0 || end <= begin || !(end - begin).is_multiple_of(8) {
        return;
    }
    let count = (end - begin) / 8;
    let pages = std::slice::from_raw_parts(begin as *const usize, count);
    let mut handles = Vec::with_capacity(count);
    for &page_ptr in pages {
        if page_ptr == 0 {
            return;
        }
        let tex = (page_ptr as *const usize).read();
        if tex == 0 {
            return;
        }
        handles.push(tex);
    }
    // Zero out the texture slot in every page wrapper so `0x10c5ea0` skips
    // `DestroyTexture` while still freeing the wrapper structs normally.
    for &page_ptr in pages {
        (page_ptr as *mut usize).write(0);
    }
    STASH.with(|cell| {
        let mut s = cell.borrow_mut();
        s.gen = cur_gen;
        s.handles = handles;
    });
}

/// Called at `0x10c9f8f` inside `0x10c7280`'s per-page loop.
/// Returns `1` if the page's texture handle is already valid (either native or
/// re-attached from `STASH`) and does not need `flagfiles_i.tga` to be decoded,
/// or `0` if native should load `flagfiles_i.tga`.
#[called_by_game]
unsafe extern "C" fn try_reuse_page_texture(flagmap: usize, page_idx: usize, needs_rebuild: u8) -> u8 {
    let begin = ((flagmap + 0x18) as *const usize).read();
    let end = ((flagmap + 0x20) as *const usize).read();
    let page_count = end.saturating_sub(begin) / 8;
    if page_idx >= page_count {
        return 0;
    }
    let page_ptr = ((begin + page_idx * 8) as *const usize).read();
    let existing = (page_ptr as *const usize).read();

    if existing != 0 {
        return u8::from(CHECK_RELOAD_FN.get()() == 0);
    }

    if needs_rebuild != 0 || CHECK_RELOAD_FN.get()() != 0 {
        return 0;
    }

    let cur_gen = DEVICE_RESET_GEN.load(Ordering::Acquire);
    let reused = STASH.with(|cell| {
        let mut s = cell.borrow_mut();
        if s.gen != cur_gen || s.handles.len() != page_count {
            return 0usize;
        }
        std::mem::take(&mut s.handles[page_idx])
    });

    if reused != 0 {
        (page_ptr as *mut usize).write(reused);
        1
    } else {
        0
    }
}

fn after_load_atlas(_call: &mut Call) {
    unsafe {
        drain_stash();
    }
}

/// Replaces `lea rsi, [rcx+0x32610]; mov rcx, rsi; call 0x10c5ea0` at `0x10b2b97`.
#[unsafe(naked)]
extern "C" fn stash_before_clear_stub() {
    std::arch::naked_asm!(
        "lea rsi, [rcx + 0x32610]",
        call_rust!("mov rcx, rsi", "mov edx, ebx", call stash),
        "mov rcx, rsi",
        "call qword ptr [rip + {clear}]",
        "jmp qword ptr [rip + {next}]",
        stash = sym stash_before_clear,
        clear = sym CLEAR_ATLAS_FN,
        next = sym AFTER_CLEAR_AT,
    );
}

/// Replaces `0x10c9f8f..0x10c9f9e`: checks whether page `r15` of `r12` can skip
/// loading `flagfiles_i.tga` (either already populated or re-attached from stash).
#[unsafe(naked)]
extern "C" fn reuse_page_texture_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rcx, r12", "mov rdx, r15", "movzx r8d, sil", call reuse),
        "test al, al",
        "jnz 2f",
        "jmp qword ptr [rip + {load}]",
        "2:",
        "jmp qword ptr [rip + {skip}]",
        reuse = sym try_reuse_page_texture,
        load = sym LOAD_FILE_AT,
        skip = sym SKIP_FILE_AT,
    );
}
