//! MinHook binding, used only by [`points`](crate::points): hooks are created
//! from dependency point declarations, enabled together, and removed again
//! only when startup is undone. Once startup succeeds they stay for the life of
//! the game process.
//!
//! [`create`] installs a plain detour (used for replacements and jumps).
//! [`observe`] installs a C++ observer thunk that calls Rust before/after the
//! original game function without a Rust frame on the stack while it runs.
use crate::Result;
use std::sync::Mutex;

extern "C" {
    fn MH_Initialize() -> i32;
    fn MH_CreateHook(target: *mut u8, detour: *mut u8, original: *mut *mut u8) -> i32;
    fn MH_RemoveHook(target: *mut u8) -> i32;
    fn MH_QueueEnableHook(target: *mut u8) -> i32;
    fn MH_ApplyQueued() -> i32;
    fn eu4h_observer_thunk(slot: u32, argc: u32) -> usize;
    fn eu4h_observer_setup(slot: u32, original: usize, before: Option<Callback>, after: Option<Callback>);
}
const MH_OK: i32 = 0;
const MH_ERROR_ALREADY_INITIALIZED: i32 = 1;
const OBSERVER_SLOTS: usize = 48;

/// Arguments and result of an observed call; `fault` is 1 when the original
/// left through an exception, in which case `result` is meaningless.
#[repr(C)]
pub struct HookFrame {
    pub args: [usize; 12],
    pub result: usize,
    pub return_address: usize,
    pub slot: usize,
    pub fault: usize,
}
pub type Callback = extern "C" fn(*mut HookFrame);

struct Registry {
    targets: Vec<usize>,
    observers: usize,
}
static REGISTRY: Mutex<Registry> = Mutex::new(Registry { targets: Vec::new(), observers: 0 });

fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

fn create_locked(registry: &mut Registry, target: usize, detour: usize) -> Result<usize> {
    if registry.targets.contains(&target) {
        return Err(format!("duplicate hook on {target:#x}"));
    }
    if registry.targets.is_empty() {
        let code = unsafe { MH_Initialize() };
        if code != MH_OK && code != MH_ERROR_ALREADY_INITIALIZED {
            return Err(format!("MH_Initialize failed: {code}"));
        }
    }
    let mut original = std::ptr::null_mut();
    let code = unsafe { MH_CreateHook(target as *mut u8, detour as *mut u8, &mut original) };
    if code != MH_OK {
        return Err(format!("MH_CreateHook({target:#x}) failed: {code}"));
    }
    registry.targets.push(target);
    Ok(original as usize)
}

/// Installs `detour` on `target` and returns the trampoline to the original code.
/// Hooks are created disabled; [`enable_all`] turns them on together.
pub fn create(target: usize, detour: usize) -> Result<usize> {
    create_locked(&mut registry(), target, detour)
}

/// Observes a game function taking `argc` integer/pointer arguments.
pub fn observe(target: usize, argc: u32, before: Option<Callback>, after: Option<Callback>) -> Result<()> {
    let mut registry = registry();
    let slot = registry.observers;
    if slot >= OBSERVER_SLOTS || argc > 12 {
        return Err("observer slots exhausted or too many arguments".into());
    }
    let thunk = unsafe { eu4h_observer_thunk(slot as u32, argc) };
    let original = create_locked(&mut registry, target, thunk)?;
    unsafe { eu4h_observer_setup(slot as u32, original, before, after) };
    registry.observers += 1;
    Ok(())
}

/// Enables the hooks on `targets` in one step, with the game's threads
/// suspended once.
pub fn enable(targets: &[usize]) -> Result<()> {
    for &target in targets {
        let code = unsafe { MH_QueueEnableHook(target as *mut u8) };
        if code != MH_OK {
            return Err(format!("MH_QueueEnableHook({target:#x}) failed: {code}"));
        }
    }
    match unsafe { MH_ApplyQueued() } {
        MH_OK => Ok(()),
        code => Err(format!("MH_ApplyQueued failed: {code}")),
    }
}

/// Removes a hook created by [`create`] or [`observe`], disabling it first if
/// it is enabled, which restores the original code. An observer's slot is not
/// reused.
pub fn remove(target: usize) {
    let mut registry = registry();
    let code = unsafe { MH_RemoveHook(target as *mut u8) };
    if code != MH_OK {
        log!("MH_RemoveHook({target:#x}) failed: {code}");
    }
    registry.targets.retain(|&t| t != target);
}
