//! The mesh types loaded beside the databases (docs/loading.md, L6).
//!
//! At the end of the gfx stage the LoadingThread loads the meshes
//! (`0x14dc5d0`): the mesh objects not loaded yet, then every mesh type and
//! the particles (`0x14dc390`: each type's `.mesh` file, its buffers, textures
//! and skeleton, then the particles' shaders), then the meshes' shaders and
//! the sprites' vertex declaration. Nearly all of its 2.2 s is `0x14dc390`.
//! Nothing the databases load reads what it makes, and what both use is safe
//! from two threads (docs/research/loading.md, "Which stretches can run side
//! by side"); the rest of the meshes' load neither needs it nor is read by it.
//!
//! So at the startup's first call, `0x14dc390` runs on a thread of the
//! framework's while the LoadingThread goes on. The LoadingThread waits for
//! that thread as the databases end, at the DLC's metadata (`0x2060f0`),
//! which InitGame calls next, before the events, the flags, the main menu and
//! any drawing of a mesh; so does a check of the device that finds it lost,
//! before the game lets every device object go and makes it again. A later
//! call loads the mesh types on its caller's thread, as the game does.
use crate::called_by_game;
use crate::hooks::Call;
use crate::points::{Function, GameFunction, Point};
use crate::speed::SpeedUp;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The meshes' load (`graphics`), and in it the call of the mesh types' load
/// (`mov rcx, rdi; call 0x14dc390`), `rdi` the graphics object throughout.
const MESHES: usize = 0x14dc5d0;
const TYPES_CALLED: usize = 0x14dc684;
const TYPES_DONE: usize = 0x14dc68c;
/// The mesh types' load and the particles' shaders (`graphics`).
const TYPES: GameFunction<1> = GameFunction::at(0x14dc390);
/// The DLC's metadata, which InitGame reads right after the databases (and a
/// game's start again).
const DLC_METADATA: usize = 0x2060f0;
/// The device's check (`device -> state`): 0 for a device that draws, 2 for
/// one lost and ready to be reset, 1 for one lost (`TestCooperativeLevel`).
const DEVICE_STATE: usize = 0x16d8e20;

/// Left to the game, the mesh types load on the LoadingThread before the
/// databases; what they load is the same either way, so there is nothing to
/// check.
pub const SPEED_UP: SpeedUp = SpeedUp { name: "meshes", group: "loading", points, checkable: false };

fn points() -> Vec<Point> {
    vec![
        Point::keep(MESHES, "48 89 5c 24 18 88 54 24 10 55 56 57 48 83 ec 30 80 b9 0c 26 03 00", "the meshes' load"),
        Point::keep(TYPES, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 4c 89 74 24 20 55 48", "the mesh types' load"),
        Point::jump(
            TYPES_CALLED,
            "48 8b cf e8 04 fd ff ff",
            "the meshes' load calls the mesh types' load",
            types_called_stub as *const () as usize,
            &TYPES_RESUME,
        ),
        Point::observe(DLC_METADATA, "40 53 48 83 ec 40 e8 65 41 3b 00 48 8b d8", "the DLC's metadata after the databases", 0, Some(databases_done), None),
        Point::replace(
            DEVICE_STATE,
            "48 83 ec 28 48 8b 49 08 48 8b 01 ff 50 18 8b c8 85 c0 75 05",
            "the device's check",
            device_state as DeviceState,
            &ORIGINAL_DEVICE_STATE,
        ),
    ]
}

crate::resume!(TYPES_RESUME: TYPES_DONE_AT = TYPES_DONE);

type DeviceState = extern "C" fn(usize) -> u32;
static ORIGINAL_DEVICE_STATE: Function<DeviceState> = Function::new();

/// The thread loading the mesh types, until it is waited for.
static LOADING: Mutex<Option<(JoinHandle<()>, Instant)>> = Mutex::new(None);
/// The mesh types have been loaded once: every later load is the game's.
static LOADED_ONCE: AtomicBool = AtomicBool::new(false);

/// Replaces the meshes' call of the mesh types' load (`rdi` the graphics
/// object).
#[unsafe(naked)]
extern "C" fn types_called_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, rdi", call load_types),
        "jmp qword ptr [rip + {done}]",
        load_types = sym load_types,
        done = sym TYPES_DONE_AT,
    );
}

/// The mesh types' load: on a thread of its own the first time, else here.
#[called_by_game]
extern "C" fn load_types(graphics: usize) {
    let m = crate::hooked_memory();
    if LOADED_ONCE.swap(true, Ordering::AcqRel) {
        wait("a later load");
        if let Err(e) = TYPES.call(&m, [graphics]) {
            log!("meshes: the mesh types' load failed: {e}");
        }
        return;
    }
    let began = Instant::now();
    let spawned = std::thread::Builder::new().name("eu4-harmony meshes".into()).stack_size(16 << 20).spawn(move || {
        crate::guarded("meshes", || {
            if let Err(e) = TYPES.call(&m, [graphics]) {
                log!("meshes: the mesh types' load failed: {e}");
            }
        });
    });
    match spawned {
        Ok(handle) => *LOADING.lock().unwrap_or_else(|e| e.into_inner()) = Some((handle, began)),
        Err(e) => {
            log!("meshes: no thread for the mesh types ({e}): loaded here");
            if let Err(e) = TYPES.call(&m, [graphics]) {
                log!("meshes: the mesh types' load failed: {e}");
            }
        }
    }
}

/// Waits for the mesh types' thread, if one runs, and says how long it took
/// and how long `why` waited for it.
fn wait(why: &str) {
    let Some((handle, began)) = LOADING.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    let waited = Instant::now();
    let finished = handle.is_finished();
    if handle.join().is_err() {
        log!("meshes: the mesh types' thread ended in a panic");
    }
    let (now, total) = (Instant::now(), began.elapsed());
    log!(
        "meshes: the mesh types loaded beside the LoadingThread, {} since they began; {why} waited {}",
        secs(total),
        if finished { "for none of it".to_string() } else { format!("{} for them", secs(now - waited)) }
    );
}

fn secs(d: Duration) -> String {
    format!("{:.2} s", d.as_secs_f64())
}

fn databases_done(_: &mut Call) {
    wait("the end of the databases");
}

/// Replaces the device's check: a device lost waits for the mesh types'
/// thread first, since the game lets every device object go when it resets
/// the device.
#[called_by_game]
extern "C" fn device_state(device: usize) -> u32 {
    let state = ORIGINAL_DEVICE_STATE.get()(device);
    if state != 0 {
        wait("a check of the lost device");
    }
    state
}
