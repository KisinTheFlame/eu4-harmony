//! MinHook binding, used only by [`points`](crate::points): hooks are created
//! from dependency point declarations, enabled together, and removed again
//! only when startup is undone. Once startup succeeds they stay for the life of
//! the game process.
//!
//! [`create`] installs a plain detour (used for replacements and jumps).
//! [`observe`] installs a C++ observer thunk that calls Rust before/after the
//! original game function without a Rust frame on the stack while it runs.
//!
//! An observer is the seam's: a plain Rust function given a [`Call`], the
//! observed call's arguments and result and the game's memory. The seam runs
//! it under [`guarded`](crate::guarded), so a panic in it is logged, the
//! player is told and the game ends, and never crosses into the game's code
//! (ADR framework-succeeds-or-fails-as-a-whole). Every other function the
//! game calls is marked `#[called_by_game]`, which does the same.
use crate::called_by_game;
use crate::{memory::Memory, Result};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex, OnceLock,
};

extern "C" {
    fn MH_Initialize() -> i32;
    fn MH_CreateHook(target: *mut u8, detour: *mut u8, original: *mut *mut u8) -> i32;
    fn MH_RemoveHook(target: *mut u8) -> i32;
    fn MH_QueueEnableHook(target: *mut u8) -> i32;
    fn MH_ApplyQueued() -> i32;
    fn eu4h_observer_thunk(slot: u32, argc: u32) -> usize;
    fn eu4h_observer_setup(slot: u32, original: usize, before: Option<Native>, after: Option<Native>);
    fn eu4h_observer_slots() -> usize;
    #[cfg(test)]
    fn eu4h_hook_frame_layout(out: *mut usize);
}
const MH_OK: i32 = 0;
const MH_ERROR_ALREADY_INITIALIZED: i32 = 1;

/// The native code's observer slots (`kSlots`), which are never reused.
const SLOTS: usize = 96;

/// How many functions can be observed in all: the native code's observer
/// slots, which are never reused.
pub fn observer_slots() -> usize {
    unsafe { eu4h_observer_slots() }
}

/// Arguments and result of an observed call, as the native code fills them
/// in; `fault` is 1 when the original left through an exception, in which
/// case `result` is meaningless. The original is called with `args` as the
/// "before" observer leaves them.
#[repr(C)]
pub struct HookFrame {
    args: [usize; 12],
    result: usize,
    return_address: usize,
    slot: usize,
    fault: usize,
}

impl HookFrame {
    /// A call with these arguments, as a test gives an observer.
    pub fn new(args: &[usize]) -> Self {
        let mut all = [0; 12];
        all[..args.len()].copy_from_slice(args);
        Self { args: all, result: 0, return_address: 0, slot: 0, fault: 0 }
    }

    /// The same call, returned from the original with `result`.
    pub fn returned(mut self, result: usize) -> Self {
        self.result = result;
        self
    }

    /// The same call, left through an exception.
    pub fn faulted(mut self) -> Self {
        self.fault = 1;
        self.result = !0;
        self
    }

    /// The same call, made from `return_address`.
    pub fn from(mut self, return_address: usize) -> Self {
        self.return_address = return_address;
        self
    }

    /// Argument `index` as the call now has it.
    pub fn arg(&self, index: usize) -> usize {
        self.args[index]
    }
}

/// A value an argument or a result is read as: the argument's register or
/// stack slot, cut to the type's size.
pub trait Word: Copy {
    fn from_word(word: usize) -> Self;
}

macro_rules! words {
    ($($t:ty),*) => {$(impl Word for $t {
        fn from_word(word: usize) -> Self {
            word as $t
        }
    })*};
}
words!(usize, isize, u64, i64, u32, i32, u16, i16, u8, i8);

impl Word for bool {
    fn from_word(word: usize) -> Self {
        word as u8 != 0
    }
}

impl<T> Word for *const T {
    fn from_word(word: usize) -> Self {
        word as *const T
    }
}

impl<T> Word for *mut T {
    fn from_word(word: usize) -> Self {
        word as *mut T
    }
}

/// What an observer is given: one call of the function it observes.
pub struct Call<'a> {
    frame: &'a mut HookFrame,
    memory: Memory,
}

impl<'a> Call<'a> {
    /// A call of a function in the game whose image is `memory`.
    pub fn new(frame: &'a mut HookFrame, memory: Memory) -> Self {
        Self { frame, memory }
    }

    /// The game's memory.
    pub fn memory(&self) -> Memory {
        self.memory
    }

    /// Argument `index` (from 0), as a machine word.
    pub fn arg(&self, index: usize) -> usize {
        self.frame.args[index]
    }

    /// Argument `index` (from 0), read as `T`.
    pub fn arg_as<T: Word>(&self, index: usize) -> T {
        T::from_word(self.frame.args[index])
    }

    /// Changes argument `index` before the original is called with it; only
    /// a "before" observer's change reaches the original.
    pub fn set_arg(&mut self, index: usize, value: usize) {
        self.frame.args[index] = value;
    }

    /// What the original returned, `None` when it left through an exception;
    /// in a "before" observer, 0.
    pub fn result(&self) -> Option<usize> {
        (self.frame.fault == 0).then_some(self.frame.result)
    }

    /// The result read as `T`, `None` when the original left through an
    /// exception.
    pub fn result_as<T: Word>(&self) -> Option<T> {
        self.result().map(T::from_word)
    }

    /// Whether the original left through an exception.
    pub fn faulted(&self) -> bool {
        self.frame.fault != 0
    }

    /// Where the observed function returns to: in the code that called it.
    pub fn caller(&self) -> usize {
        self.frame.return_address
    }

    /// The function observed, by its address.
    pub fn observed(&self) -> Option<usize> {
        observed(self.frame.slot)
    }
}

/// A function the framework runs before or after every call of an observed
/// function.
pub type Observer = fn(&mut Call);

/// What the native code calls: the seam's own two functions.
type Native = extern "C" fn(*mut HookFrame);

/// The observers of one slot, the game they observe, and what they are for
/// the log.
struct Observers {
    before: AtomicUsize,
    after: AtomicUsize,
    base: AtomicUsize,
    what: OnceLock<&'static str>,
}

static OBSERVERS: [Observers; SLOTS] =
    [const { Observers { before: AtomicUsize::new(0), after: AtomicUsize::new(0), base: AtomicUsize::new(0), what: OnceLock::new() } }; SLOTS];

/// Runs `observer` on one call, guarded.
pub fn run(what: &str, frame: &mut HookFrame, memory: Memory, observer: Observer) {
    crate::guarded(what, || observer(&mut Call::new(frame, memory)));
}

fn dispatch(frame: *mut HookFrame, which: fn(&Observers) -> &AtomicUsize) {
    let frame = unsafe { &mut *frame };
    let Some(slot) = OBSERVERS.get(frame.slot) else { return };
    let observer = which(slot).load(Ordering::Acquire);
    if observer == 0 {
        return;
    }
    let observer = unsafe { std::mem::transmute::<usize, Observer>(observer) };
    let memory = Memory { base: slot.base.load(Ordering::Relaxed) };
    run(slot.what.get().copied().unwrap_or("observer"), frame, memory, observer);
}

#[called_by_game]
extern "C" fn before(frame: *mut HookFrame) {
    dispatch(frame, |o| &o.before);
}

#[called_by_game]
extern "C" fn after(frame: *mut HookFrame) {
    dispatch(frame, |o| &o.after);
}

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

/// Observes a game function taking `argc` integer/pointer arguments, in the
/// game whose image is `memory`; `what` names it in the log.
pub fn observe(target: usize, argc: u32, memory: Memory, what: &'static str, before: Option<Observer>, after: Option<Observer>) -> Result<()> {
    let mut registry = registry();
    let slot = registry.observed.len();
    if slot >= observer_slots() || argc > 12 {
        return Err("observer slots exhausted or too many arguments".into());
    }
    let thunk = unsafe { eu4h_observer_thunk(slot as u32, argc) };
    let original = create_locked(&mut registry, target, thunk)?;
    let observers = &OBSERVERS[slot];
    observers.base.store(memory.base, Ordering::Relaxed);
    let _ = observers.what.set(what);
    observers.before.store(before.map_or(0, |f| f as usize), Ordering::Release);
    observers.after.store(after.map_or(0, |f| f as usize), Ordering::Release);
    let native = |observer: Option<Observer>, seam: Native| observer.map(|_| seam);
    unsafe { eu4h_observer_setup(slot as u32, original, native(before, self::before), native(after, self::after)) };
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

/// The function observer slot `slot` observes.
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
        assert_eq!(observer_slots(), SLOTS);
    }

    fn reads(call: &mut Call) {
        let total = call.arg(0) + call.arg_as::<u32>(1) as usize + call.result().unwrap_or(1000);
        call.set_arg(2, total + call.memory().base);
    }

    /// An observer is a plain function a test calls with a frame of its own.
    #[test]
    fn an_observer_is_given_the_call_and_the_memory() {
        let mut frame = HookFrame::new(&[1, 0x1_0000_0002]).returned(30);
        run("reads", &mut frame, Memory { base: 400 }, reads);
        assert_eq!(frame.arg(2), 433);
        let mut frame = HookFrame::new(&[1, 2]).faulted();
        run("reads", &mut frame, Memory { base: 0 }, reads);
        assert_eq!(frame.arg(2), 1003);
    }

    fn panics(_: &mut Call) {
        panic!("boom");
    }

    /// A panic in an observer ends the game through the fatal path, named
    /// by what it observes, and goes no further.
    #[test]
    fn a_panic_in_an_observer_is_fatal() {
        let mut frame = HookFrame::new(&[]);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run("observed thing", &mut frame, Memory { base: 0 }, panics)));
        let payload = caught.unwrap_err();
        assert_eq!(payload.downcast_ref::<String>().map(String::as_str), Some("fatal: observed thing: boom"));
    }

    /// "C-unwind", so that the test's fatal path, which panics, may leave it.
    #[crate::called_by_game]
    extern "C-unwind" fn halves(value: usize) -> usize {
        assert!(value.is_multiple_of(2), "{value} is odd");
        value / 2
    }

    /// A function the game calls runs guarded: its panic takes the fatal path.
    #[test]
    fn a_panic_in_a_function_the_game_calls_is_fatal() {
        assert_eq!(halves(8), 4);
        let payload = std::panic::catch_unwind(|| halves(3)).unwrap_err();
        assert_eq!(payload.downcast_ref::<String>().map(String::as_str), Some("fatal: eu4_harmony_core::hooks::tests::halves: 3 is odd"));
    }

    /// Every function with a body that the game's code may call, an
    /// `extern` one, is marked `#[called_by_game]`, or is a naked stub that
    /// calls one through `call_rust!`: none can let a panic into the game's
    /// code. The proxy's bootstrap catches its own, as under the loader lock
    /// it cannot tell the player.
    #[test]
    fn every_function_the_game_calls_is_guarded() {
        fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().map(std::result::Result::unwrap) {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        sources(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut bare = Vec::new();
        for file in files {
            // The test's stand-ins for the game's functions, a module only
            // tests build, are called by the framework as the game's are.
            if file.ends_with(std::path::Path::new("game").join("buffers.rs")) {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            // A file's tests, at its end, call nothing of the game's.
            let tests = lines.windows(2).position(|w| w[0].trim() == "#[cfg(test)]" && w[1].starts_with("mod tests")).unwrap_or(lines.len());
            for (n, line) in lines[..tests].iter().enumerate() {
                let code = line.trim_start();
                let code = match code.strip_prefix("pub") {
                    Some(rest) if rest.starts_with('(') => rest.split_once(") ").map_or(code, |(_, after)| after),
                    Some(rest) => rest.trim_start(),
                    None => code,
                };
                let code = code.strip_prefix("unsafe ").unwrap_or(code);
                if !(code.starts_with("extern \"C\" fn ") || code.starts_with("extern \"system\" fn ")) || code.contains("eu4_harmony_bootstrap") {
                    continue;
                }
                // A declaration has no body.
                let ends = lines[n..].iter().find(|l| l.trim_end().ends_with('{') || l.trim_end().ends_with(';'));
                if ends.is_some_and(|l| l.trim_end().ends_with(';')) {
                    continue;
                }
                let attributes = lines[..n].iter().rev().map(|l| l.trim()).take_while(|l| l.starts_with("#[") || l.starts_with("//"));
                if !attributes.into_iter().any(|a| a == "#[called_by_game]" || a == "#[unsafe(naked)]") {
                    bare.push(format!("{}:{}", file.display(), n + 1));
                }
            }
        }
        assert!(bare.is_empty(), "functions the game calls that are not #[called_by_game]: {bare:#?}");
    }
}
