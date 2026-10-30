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
    fn eu4h_observer_slots() -> usize;
    #[cfg(test)]
    fn eu4h_hook_frame_layout(out: *mut usize);
}
const MH_OK: i32 = 0;
const MH_ERROR_ALREADY_INITIALIZED: i32 = 1;

/// How many functions can be observed in all: the native code's observer
/// slots, which are never reused.
pub fn observer_slots() -> usize {
    unsafe { eu4h_observer_slots() }
}

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
    /// The function each observer slot observes, by slot.
    observed: Vec<usize>,
}
static REGISTRY: Mutex<Registry> = Mutex::new(Registry { targets: Vec::new(), observed: Vec::new() });

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
    let slot = registry.observed.len();
    if slot >= observer_slots() || argc > 12 {
        return Err("observer slots exhausted or too many arguments".into());
    }
    let thunk = unsafe { eu4h_observer_thunk(slot as u32, argc) };
    let original = create_locked(&mut registry, target, thunk)?;
    unsafe { eu4h_observer_setup(slot as u32, original, before, after) };
    registry.observed.push(target);
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

/// The function observer slot `slot` ([`HookFrame::slot`]) observes.
pub fn observed(slot: usize) -> Option<usize> {
    registry().observed.get(slot).copied()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame the native observers fill in is laid out as Rust reads it.
    #[test]
    fn the_hook_frame_is_the_native_code_s() {
        let mut native = [0usize; 5];
        unsafe { eu4h_hook_frame_layout(native.as_mut_ptr()) };
        use std::mem::{offset_of, size_of};
        let ours = [
            size_of::<HookFrame>(),
            offset_of!(HookFrame, result),
            offset_of!(HookFrame, return_address),
            offset_of!(HookFrame, slot),
            offset_of!(HookFrame, fault),
        ];
        assert_eq!(native, ours);
    }
}
