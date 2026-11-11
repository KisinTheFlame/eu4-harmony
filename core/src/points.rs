//! Dependency points (GLOSSARY): the places in the game's code the framework
//! relies on, in `eu4.exe` or in another module that comes with the game. Each is declared once, with the bytes it must hold and what the
//! framework does there, and that one declaration drives checking, hooking and
//! undoing (ADR framework-succeeds-or-fails-as-a-whole). [`install`] applies
//! every point or none, and [`Memory::at`] gives the address of only what a
//! point checks. [`this_build`] gathers every point of a build.
pub mod this_build;

use crate::{
    hooks::{self, Observer},
    memory::Memory,
    Result,
};
use std::ffi::CStr;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use windows_sys::Win32::System::{
    Diagnostics::Debug::FlushInstructionCache,
    LibraryLoader::{GetModuleHandleW, GetProcAddress},
    Threading::GetCurrentProcess,
};

/// A module other than `eu4.exe` that comes with the game and that a
/// dependency point is in. Its bytes are checked as `eu4.exe`'s are, so a
/// game whose module differs does not start (ADR
/// framework-succeeds-or-fails-as-a-whole).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Module {
    /// Paradox's account, social and telemetry client.
    Pops,
}

impl Module {
    pub const fn file(self) -> &'static str {
        match self {
            Module::Pops => "pops_api.dll",
        }
    }
}

/// Where a dependency point is.
#[derive(Clone, Copy, Debug)]
pub enum Place {
    /// An RVA into `eu4.exe`.
    Rva(usize),
    /// A function `eu4.exe` exports by name.
    Export(&'static CStr),
    /// An RVA into another module.
    In(Module, usize),
}

impl From<usize> for Place {
    fn from(rva: usize) -> Self {
        Self::Rva(rva)
    }
}

impl From<&'static CStr> for Place {
    fn from(name: &'static CStr) -> Self {
        Self::Export(name)
    }
}

/// Where a jump stub continues, and the slot holding that address for it.
pub struct Resume {
    pub rva: usize,
    pub address: &'static AtomicUsize,
}

/// Declares the places a jump stub continues at: a slot for each, which the
/// stub jumps through (`jmp qword ptr [rip + {slot}]`), and the list its
/// [`Point::jump`] takes.
#[macro_export]
macro_rules! resume {
    ($list:ident: $($slot:ident = $rva:expr),+ $(,)?) => {
        $(static $slot: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);)+
        static $list: [$crate::points::Resume; [$(stringify!($slot)),+].len()] =
            [$($crate::points::Resume { rva: $rva, address: &$slot }),+];
    };
}

/// A function of the game's that startup fills in, of the type its callers
/// call it as: the original code of a function a point replaces, or a point
/// recorded to be called.
pub struct Function<F> {
    address: AtomicUsize,
    kind: PhantomData<F>,
}

impl<F: Copy> Function<F> {
    #[expect(clippy::new_without_default, reason = "made in statics, where only a const fn will do")]
    pub const fn new() -> Self {
        Self { address: AtomicUsize::new(0), kind: PhantomData }
    }

    /// The function; startup must have filled it in.
    pub fn get(&self) -> F {
        let address = self.address.load(Ordering::Acquire);
        assert!(address != 0, "a game function was called before startup found it");
        unsafe { std::mem::transmute_copy(&address) }
    }

    /// Its address, 0 until startup has found it.
    pub fn address(&self) -> usize {
        self.address.load(Ordering::Acquire)
    }

    /// Where startup stores its address.
    pub fn slot(&'static self) -> &'static AtomicUsize {
        &self.address
    }
}

/// A function of the game's that the framework calls, at its RVA, and how
/// many arguments it takes: the dependency point that checks it names it
/// (`Point::keep(FUNCTION, ...)`), and calling it through it passes exactly
/// that many, so that a call with one too few does not compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameFunction<const ARGS: usize>(usize);

impl<const ARGS: usize> GameFunction<ARGS> {
    pub const fn at(rva: usize) -> Self {
        Self(rva)
    }

    pub const fn rva(self) -> usize {
        self.0
    }

    /// Calls it with integer or pointer arguments inside an SEH guard; a
    /// point must check it ([`Memory::at`]).
    pub fn call(self, m: &Memory, args: [usize; ARGS]) -> Result<usize> {
        let address = m.at(self.0);
        m.call(address, &args)
    }
}

impl<const ARGS: usize> From<GameFunction<ARGS>> for Place {
    fn from(function: GameFunction<ARGS>) -> Self {
        Self::Rva(function.0)
    }
}

/// Its RVA, for what compares an address with the function's or names it
/// as a vtable's method.
impl<const ARGS: usize> From<GameFunction<ARGS>> for usize {
    fn from(function: GameFunction<ARGS>) -> Self {
        function.0
    }
}

/// The address of the function `f`, a function pointer.
fn address<F: Copy>(f: F) -> usize {
    assert_eq!(std::mem::size_of::<F>(), std::mem::size_of::<usize>(), "not a function pointer");
    unsafe { std::mem::transmute_copy(&f) }
}

/// What the framework does at a dependency point.
pub enum Action {
    /// Relies on it only: calls it, reads it or continues there.
    Keep,
    /// Runs `before` and `after` around every call of the function there.
    Observe { argc: u32, before: Option<Observer>, after: Option<Observer> },
    /// Sends calls to `with`; the original stays callable through `original`.
    Replace { with: usize, original: &'static AtomicUsize },
    /// Jumps from inside a function to `stub`, which does what the point's
    /// bytes did and continues at one of `resume`.
    Jump { stub: usize, resume: &'static [Resume] },
    /// Overwrites the point's bytes with these, of the same length.
    Write(&'static str),
}

pub struct Point {
    pub place: Place,
    /// What must be there, as space-separated hex.
    pub bytes: &'static str,
    pub what: &'static str,
    pub action: Action,
    /// Receives the point's address, for code that calls or reads it.
    pub record: Option<&'static AtomicUsize>,
    /// The field offsets, globals and other numbers its bytes use, by name
    /// where they are named; a test checks the bytes against them.
    pub uses: &'static [usize],
    /// For a vtable's method, the function whose address must be there in
    /// place of bytes.
    pub method: Option<usize>,
}

impl Point {
    fn new(place: impl Into<Place>, bytes: &'static str, what: &'static str, action: Action) -> Self {
        Self { place: place.into(), bytes, what, action, record: None, uses: &[], method: None }
    }

    pub fn keep(place: impl Into<Place>, bytes: &'static str, what: &'static str) -> Self {
        Self::new(place, bytes, what, Action::Keep)
    }

    pub fn observe(place: impl Into<Place>, bytes: &'static str, what: &'static str, argc: u32, before: Option<Observer>, after: Option<Observer>) -> Self {
        Self::new(place, bytes, what, Action::Observe { argc, before, after })
    }

    /// Replaces the function there with `with`, which has its type.
    pub fn replace<F: Copy>(place: impl Into<Place>, bytes: &'static str, what: &'static str, with: F, original: &'static Function<F>) -> Self {
        Self::new(place, bytes, what, Action::Replace { with: address(with), original: original.slot() })
    }

    pub fn jump(place: impl Into<Place>, bytes: &'static str, what: &'static str, stub: usize, resume: &'static [Resume]) -> Self {
        Self::new(place, bytes, what, Action::Jump { stub, resume })
    }

    pub fn write(place: impl Into<Place>, bytes: &'static str, what: &'static str, replacement: &'static str) -> Self {
        Self::new(place, bytes, what, Action::Write(replacement))
    }

    /// The method a class has at `offset` in its vtable at `vtable`: the
    /// function at `function`.
    pub fn method(vtable: usize, offset: usize, function: usize, what: &'static str) -> Self {
        Self { method: Some(function), ..Self::new(vtable + offset, "", what, Action::Keep) }
    }

    /// Stores the point's address in `slot` when it is installed.
    pub fn recorded(mut self, slot: &'static AtomicUsize) -> Self {
        self.record = Some(slot);
        self
    }

    /// Says which numbers its bytes use: offsets, RVAs a RIP-relative operand
    /// reaches, immediates.
    pub fn uses(mut self, values: &'static [usize]) -> Self {
        self.uses = values;
        self
    }
}

/// The RVAs the dependency points check, as ranges from the first to the
/// last: each point's bytes and the address right after them, where a call
/// among them returns or a jump stub continues; a method's function; and the
/// numbers a point's bytes use, which a test finds in them.
struct Checked(Vec<(usize, usize)>);

impl Checked {
    fn of(points: &[Point]) -> Self {
        let mut ranges = Vec::new();
        for point in points {
            let Place::Rva(rva) = point.place else { continue };
            match point.method {
                Some(function) => ranges.extend([(rva, rva + 8), (function, function)]),
                None => ranges.push((rva, rva + point.bytes.split_whitespace().count())),
            }
            ranges.extend(point.uses.iter().map(|&value| (value, value)));
        }
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
        for (start, end) in ranges {
            match merged.last_mut() {
                Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        Self(merged)
    }

    fn contains(&self, rva: usize) -> bool {
        let after = self.0.partition_point(|&(start, _)| start <= rva);
        after > 0 && rva <= self.0[after - 1].1
    }
}

static CHECKED: OnceLock<Checked> = OnceLock::new();

/// Whether a dependency point checks the game's bytes at `rva`, or a point's
/// bytes use it: one of the points this build declares ([`this_build::declared`]),
/// which the offline checks check.
pub(crate) fn checks(rva: usize) -> bool {
    CHECKED.get_or_init(|| Checked::of(&this_build::declared())).contains(rva)
}

/// What [`install`] did, for the log and the startup report.
#[derive(Debug, PartialEq, Eq)]
pub struct Installed {
    pub checked: usize,
    pub hooks: usize,
    pub writes: usize,
}

/// A JMP rel32, which is what a hook writes at its target.
const HOOK_LEN: usize = 5;

fn hex(text: &str) -> Result<Vec<u8>> {
    text.split_whitespace().map(|b| u8::from_str_radix(b, 16).ok().filter(|_| b.len() == 2).ok_or_else(|| format!("{b:?} is not a byte in hex"))).collect()
}

fn spaced(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// Finds what a place names outside `eu4.exe`'s RVAs: a function it exports
/// by name, and where another module is loaded.
struct Lookup<'a> {
    export: &'a dyn Fn(&CStr) -> Result<usize>,
    module: &'a dyn Fn(Module) -> Result<usize>,
}

/// `eu4.exe`'s own exports, in the running game.
fn game_export(name: &CStr) -> Result<usize> {
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    unsafe { GetProcAddress(module, name.as_ptr() as *const u8) }
        .map(|f| f as usize)
        .ok_or_else(|| format!("eu4.exe does not export {}", name.to_string_lossy()))
}

/// Where another module is loaded, in the running game.
fn game_module(module: Module) -> Result<usize> {
    let name: Vec<u16> = module.file().encode_utf16().chain([0]).collect();
    let base = unsafe { GetModuleHandleW(name.as_ptr()) } as usize;
    if base == 0 {
        return Err(format!("{} is not loaded", module.file()));
    }
    Ok(base)
}

fn resolve(m: Memory, place: Place, lookup: &Lookup) -> Result<usize> {
    match place {
        Place::Rva(rva) => Ok(m.base + rva),
        Place::Export(name) => (lookup.export)(name),
        Place::In(module, rva) => Ok((lookup.module)(module)? + rva),
    }
}

fn describe(point: &Point) -> String {
    match point.place {
        Place::Rva(rva) => format!("{} at {rva:#x}", point.what),
        Place::Export(name) => format!("{} ({})", point.what, name.to_string_lossy()),
        Place::In(module, rva) => format!("{} in {} at {rva:#x}", point.what, module.file()),
    }
}

/// The bytes a point's action changes, starting at its address.
fn changed_len(point: &Point, expected: usize) -> usize {
    match point.action {
        Action::Keep => 0,
        Action::Observe { .. } | Action::Replace { .. } => HOOK_LEN,
        Action::Jump { .. } | Action::Write(_) => expected,
    }
}

/// What is wrong with how a point is written, whatever the game holds: bytes
/// that do not parse, a replacement of another length, too little to hook.
fn malformed(point: &Point) -> Vec<String> {
    if point.method.is_some() {
        let keeps = point.bytes.is_empty() && matches!(point.action, Action::Keep);
        return if keeps { Vec::new() } else { vec![format!("{}: a method only names its function", describe(point))] };
    }
    let mut problems = Vec::new();
    let expected = match hex(point.bytes) {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => return vec![format!("{}: no bytes", describe(point))],
        Err(e) => return vec![format!("{}: {e}", describe(point))],
    };
    if let Action::Write(bytes) = point.action {
        match hex(bytes) {
            Ok(replacement) if replacement.len() == expected.len() => {}
            Ok(_) => problems.push(format!("{}: replacement is not {} bytes long", describe(point), expected.len())),
            Err(e) => problems.push(format!("{}: replacement: {e}", describe(point))),
        }
    }
    if let Action::Jump { .. } | Action::Observe { .. } | Action::Replace { .. } = point.action {
        if expected.len() < HOOK_LEN {
            problems.push(format!("{}: fewer than {HOOK_LEN} bytes to hook", describe(point)));
        }
    }
    if let (Place::In(..), Action::Jump { .. }) = (point.place, &point.action) {
        problems.push(format!("{}: a jump stub continues in eu4.exe only", describe(point)));
    }
    problems
}

/// Refuses two points that change the same bytes, given their addresses.
fn disjoint(points: &[Point], addresses: &[usize]) -> Result<()> {
    let mut changed: Vec<(usize, usize, &Point)> = points
        .iter()
        .zip(addresses)
        .map(|(p, &at)| (at, at + changed_len(p, p.bytes.split_whitespace().count()), p))
        .filter(|(start, end, _)| end > start)
        .collect();
    changed.sort_by_key(|&(start, _, _)| start);
    for pair in changed.windows(2) {
        if pair[1].0 < pair[0].1 {
            return Err(format!("{} and {} change the same bytes", describe(pair[0].2), describe(pair[1].2)));
        }
    }
    Ok(())
}

/// Resolves every point and compares its bytes, reporting every mismatch.
fn check(m: Memory, points: &[Point], lookup: &Lookup) -> Result<Vec<usize>> {
    let mut addresses = Vec::with_capacity(points.len());
    let mut problems = Vec::new();
    for point in points {
        let wrong = malformed(point);
        if !wrong.is_empty() {
            problems.extend(wrong);
            continue;
        }
        let expected = match point.method {
            Some(function) => ((m.base + function) as u64).to_le_bytes().to_vec(),
            None => hex(point.bytes)?,
        };
        let found = resolve(m, point.place, lookup).and_then(|at| m.bytes(at, expected.len()).map(|found| (at, found)));
        match (found, point.method) {
            (Ok((at, found)), _) if found == expected => addresses.push(at),
            (Ok((_, found)), Some(function)) => {
                let held = (u64::from_le_bytes(found.try_into().unwrap()) as usize).wrapping_sub(m.base);
                problems.push(format!("{}: expected the function at {function:#x}, found {held:#x}", describe(point)))
            }
            (Ok((_, found)), None) => problems.push(format!("{}: expected {}, found {}", describe(point), point.bytes, spaced(&found))),
            (Err(e), _) => problems.push(format!("{}: {e}", describe(point))),
        }
    }
    if !problems.is_empty() {
        return Err(format!(
            "{} of {} dependency points do not match:
{}",
            problems.len(),
            points.len(),
            problems.join(
                "
"
            )
        ));
    }
    disjoint(points, &addresses)?;
    Ok(addresses)
}

fn write_code(m: Memory, at: usize, bytes: &[u8]) -> Result<()> {
    m.write(at, bytes)?;
    unsafe { FlushInstructionCache(GetCurrentProcess(), at as _, bytes.len()) };
    Ok(())
}

/// What a failed install has to put back.
#[derive(Default)]
struct Undo {
    hooks: Vec<usize>,
    writes: Vec<(usize, Vec<u8>)>,
}

impl Undo {
    fn run(self, m: Memory) {
        for target in self.hooks {
            hooks::remove(target);
        }
        for (at, original) in self.writes.into_iter().rev() {
            if let Err(e) = write_code(m, at, &original) {
                log!("cannot restore {at:#x}: {e}");
            }
        }
    }
}

#[cfg(test)]
static FAIL_BEFORE_ENABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn apply(m: Memory, points: &[Point], addresses: &[usize], undo: &mut Undo) -> Result<()> {
    for (point, &at) in points.iter().zip(addresses) {
        if let Some(slot) = point.record {
            slot.store(at, Ordering::Release);
        }
        let made = |r: Result<()>| r.map_err(|e| format!("{}: {e}", describe(point)));
        match &point.action {
            Action::Keep | Action::Write(_) => continue,
            Action::Observe { argc, before, after } => made(hooks::observe(at, *argc, m, point.what, *before, *after))?,
            Action::Replace { with, original } => made(hooks::create(at, *with).map(|t| original.store(t, Ordering::Release)))?,
            Action::Jump { stub, resume } => {
                made(hooks::create(at, *stub).map(|_| ()))?;
                for r in *resume {
                    r.address.store(m.base + r.rva, Ordering::Release);
                }
            }
        }
        undo.hooks.push(at);
    }
    for (point, &at) in points.iter().zip(addresses) {
        if let Action::Write(bytes) = point.action {
            let replacement = hex(bytes)?;
            let original = m.bytes(at, replacement.len())?;
            write_code(m, at, &replacement).map_err(|e| format!("{}: {e}", describe(point)))?;
            undo.writes.push((at, original));
        }
    }
    #[cfg(test)]
    if FAIL_BEFORE_ENABLE.load(Ordering::SeqCst) {
        return Err("injected failure".into());
    }
    hooks::enable(&undo.hooks)
}

/// How many jump stubs [`JUMP_STUBS`] can name.
const JUMP_STUBS_KEPT: usize = 256;

/// The jump stubs installed, each with the place in the game that jumps to
/// it, for a profiler outside the process: a stub has no unwind data, and
/// was jumped into rather than called, so that the stack above it is
/// walked from the place in the game function that jumped to it
/// (`cargo xtask profile`). `count` entries are filled, each the stub's
/// address and the place's.
#[repr(C)]
pub struct JumpStubs {
    count: AtomicUsize,
    stubs: [[AtomicUsize; 2]; JUMP_STUBS_KEPT],
}

#[unsafe(export_name = "eu4h_jump_stubs")]
pub static JUMP_STUBS: JumpStubs = JumpStubs { count: AtomicUsize::new(0), stubs: [const { [const { AtomicUsize::new(0) }; 2] }; JUMP_STUBS_KEPT] };

/// Names the jump stubs of `points`, installed at `addresses`, in
/// [`JUMP_STUBS`], as many as it keeps.
fn name_jump_stubs(points: &[Point], addresses: &[usize]) {
    for (point, &at) in points.iter().zip(addresses) {
        if let Action::Jump { stub, .. } = point.action {
            let i = JUMP_STUBS.count.load(Ordering::Relaxed);
            if i == JUMP_STUBS_KEPT {
                return;
            }
            JUMP_STUBS.stubs[i][0].store(stub, Ordering::Relaxed);
            JUMP_STUBS.stubs[i][1].store(at, Ordering::Relaxed);
            JUMP_STUBS.count.store(i + 1, Ordering::Release);
        }
    }
}

/// Checks every point, then hooks and writes all of them and enables the
/// hooks. If anything fails, nothing stays changed and the error says why.
pub fn install(m: Memory, points: &[Point]) -> Result<Installed> {
    let addresses = check(m, points, &Lookup { export: &game_export, module: &game_module })?;
    let mut undo = Undo::default();
    match apply(m, points, &addresses, &mut undo) {
        Ok(()) => {
            name_jump_stubs(points, &addresses);
            Ok(Installed { checked: points.len(), hooks: undo.hooks.len(), writes: undo.writes.len() })
        }
        Err(e) => {
            undo.run(m);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::Call;
    use std::sync::Mutex;
    use windows_sys::Win32::System::Memory::{VirtualAlloc, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE, PAGE_READWRITE};

    /// MinHook's state is process-wide.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Code in a fresh page, as a stand-in for the game image.
    fn code(bytes: &str, protection: u32) -> Memory {
        let bytes = hex(bytes).unwrap();
        let base = unsafe { VirtualAlloc(std::ptr::null(), 4096, MEM_COMMIT | MEM_RESERVE, protection) } as usize;
        assert_ne!(base, 0);
        let m = Memory { base };
        write_code(m, base, &bytes).unwrap();
        m
    }

    fn call(m: Memory, arg: usize) -> usize {
        let f: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(m.base) };
        f(arg)
    }

    /// mov rax, rcx; add rax, 1; add rax, 2; add rax, 4; ret
    const ADDER: &str = "48 89 c8 48 83 c0 01 48 83 c0 02 48 83 c0 04 c3";

    resume!(RESUME_AT: RESUME = 15);

    /// Does `add rax, 100` instead of the two additions it replaces.
    #[unsafe(naked)]
    extern "C" fn add_hundred() {
        std::arch::naked_asm!("add rax, 100", "jmp qword ptr [rip + {resume}]", resume = sym RESUME);
    }

    /// Points at absolute addresses, for code spread over several pages.
    const ANYWHERE: Memory = Memory { base: 0 };

    #[test]
    fn jump_runs_the_stub_and_resumes() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let m = code(ADDER, PAGE_EXECUTE_READWRITE);
        assert_eq!(call(m, 10), 17);
        let point = Point::jump(7usize, "48 83 c0 02 48 83 c0 04", "two additions", add_hundred as *const () as usize, &RESUME_AT);
        assert_eq!(install(m, &[point]).unwrap(), Installed { checked: 1, hooks: 1, writes: 0 });
        assert_eq!(call(m, 10), 111);
    }

    static SEEN: AtomicUsize = AtomicUsize::new(0);
    fn remember(call: &mut Call) {
        SEEN.store(call.arg(0), Ordering::SeqCst);
    }

    type Adder = extern "C" fn(usize) -> usize;
    static ORIGINAL: Function<Adder> = Function::new();
    extern "C" fn doubled(x: usize) -> usize {
        ORIGINAL.get()(x) * 2
    }

    static RECORDED: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn observe_replace_write_and_record_apply_together() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let observed = code(ADDER, PAGE_EXECUTE_READWRITE);
        let replaced = code(ADDER, PAGE_EXECUTE_READWRITE);
        // mov eax, 1; ret
        let written = code("b8 01 00 00 00 c3", PAGE_EXECUTE_READWRITE);
        let points = [
            Point::observe(observed.base, "48 89 c8 48 83 c0 01", "observed", 1, Some(remember), None),
            Point::replace(replaced.base, "48 89 c8 48 83 c0 01", "replaced", doubled as Adder, &ORIGINAL),
            Point::write(written.base, "b8 01 00 00 00", "written", "b8 02 00 00 00"),
            Point::keep(observed.base + 15, "c3", "return").recorded(&RECORDED),
        ];
        assert_eq!(install(ANYWHERE, &points).unwrap(), Installed { checked: 4, hooks: 2, writes: 1 });
        assert_eq!(call(observed, 5), 12);
        assert_eq!(SEEN.load(Ordering::SeqCst), 5);
        assert_eq!(call(replaced, 5), 24);
        assert_eq!(call(written, 0), 2);
        assert_eq!(RECORDED.load(Ordering::SeqCst), observed.base + 15);
    }

    static OBSERVED: AtomicUsize = AtomicUsize::new(0);
    static ELSEWHERE: AtomicUsize = AtomicUsize::new(0);
    /// Counts a call of a function given its own address.
    fn counts(call: &mut Call) {
        let counter = if call.observed() == Some(call.arg(0)) { &OBSERVED } else { &ELSEWHERE };
        counter.fetch_add(1, Ordering::SeqCst);
    }

    /// There is no limit on how many functions are observed, and each
    /// observer knows its own.
    #[test]
    fn any_number_of_functions_can_be_observed() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        const FUNCTIONS: usize = 600;
        let pages: Vec<Memory> = (0..FUNCTIONS.div_ceil(256)).map(|_| code(&[ADDER; 256].join(" "), PAGE_EXECUTE_READWRITE)).collect();
        let functions: Vec<usize> = (0..FUNCTIONS).map(|i| pages[i / 256].base + 16 * (i % 256)).collect();
        let points: Vec<Point> = functions.iter().map(|&f| Point::observe(f, "48 89 c8 48 83 c0 01", "counted", 1, Some(counts), None)).collect();
        assert_eq!(install(ANYWHERE, &points).unwrap(), Installed { checked: FUNCTIONS, hooks: FUNCTIONS, writes: 0 });
        for &f in &functions {
            assert_eq!(call(Memory { base: f }, f), f + 7);
        }
        assert_eq!((OBSERVED.load(Ordering::SeqCst), ELSEWHERE.load(Ordering::SeqCst)), (FUNCTIONS, 0));
    }

    static FIRST_AND_LAST: Mutex<Option<(usize, usize)>> = Mutex::new(None);
    static RESULT_AND_CALLER: Mutex<Option<(Option<usize>, usize)>> = Mutex::new(None);
    fn sees_twelve(call: &mut Call) {
        *FIRST_AND_LAST.lock().unwrap() = Some((call.arg(0), call.arg(11)));
        call.set_arg(11, call.arg(11) + 1000);
    }
    fn sees_the_result(call: &mut Call) {
        *RESULT_AND_CALLER.lock().unwrap() = Some((call.result(), call.caller()));
    }

    /// An observer is given the arguments passed on the stack as well as in
    /// registers, and its change reaches the original.
    #[test]
    fn an_observer_forwards_twelve_arguments() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // mov rax, [rsp + 0x60]; add rax, [rsp + 0x28]; ret: the twelfth argument and the fifth.
        let m = code("48 8b 44 24 60 48 03 44 24 28 c3", PAGE_EXECUTE_READWRITE);
        let point = Point::observe(0usize, "48 8b 44 24 60", "twelve", 12, Some(sees_twelve), Some(sees_the_result));
        install(m, &[point]).unwrap();
        let args: Vec<usize> = (1..=12).map(|i| i * 10).collect();
        assert_eq!(m.call(m.base, &args).unwrap(), 50 + 1120);
        assert_eq!(*FIRST_AND_LAST.lock().unwrap(), Some((10, 120)));
        let (result, caller) = RESULT_AND_CALLER.lock().unwrap().unwrap();
        assert_eq!(result, Some(1170));
        assert_ne!(caller, 0);
    }

    /// A caller that knows the function it calls leaves r10 and r11 alone,
    /// as the game's code compiled as a whole may, finds them as they were.
    #[test]
    fn an_observer_leaves_the_caller_s_registers() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let m = code(ADDER, PAGE_EXECUTE_READWRITE);
        // mov r10, 0x1111; mov r11, 0x2222; sub rsp, 0x28; mov rax, <the adder>; call rax;
        // add rsp, 0x28; lea rax, [r10 + r11]; ret
        let mut caller = hex("49 c7 c2 11 11 00 00 49 c7 c3 22 22 00 00 48 83 ec 28 48 b8").unwrap();
        caller.extend(m.base.to_le_bytes());
        caller.extend(hex("ff d0 48 83 c4 28 4b 8d 04 1a c3").unwrap());
        write_code(m, m.base + 0x20, &caller).unwrap();
        let point = Point::observe(0usize, "48 89 c8 48 83 c0 01", "adder", 1, Some(remember), Some(remember));
        install(m, &[point]).unwrap();
        assert_eq!(call(Memory { base: m.base + 0x20 }, 6), 0x3333);
        assert_eq!(SEEN.load(Ordering::SeqCst), 6);
    }

    static FAULTED: AtomicUsize = AtomicUsize::new(0);
    fn sees_the_fault(call: &mut Call) {
        FAULTED.store(usize::from(call.faulted()) + 1, Ordering::SeqCst);
    }

    /// A fault in the original passes through the observer, which sees it,
    /// to the code that called it.
    #[test]
    fn a_fault_passes_through_an_observer() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // mov rax, [rcx]; nop; nop; ret
        let m = code("48 8b 01 90 90 c3", PAGE_EXECUTE_READWRITE);
        let point = Point::observe(0usize, "48 8b 01 90 90", "faulting", 1, None, Some(sees_the_fault));
        install(m, &[point]).unwrap();
        let value = 41usize;
        assert_eq!(m.call(m.base, &[&value as *const usize as usize]).unwrap(), 41);
        assert_eq!(FAULTED.load(Ordering::SeqCst), 1);
        let err = m.call(m.base, &[8]).unwrap_err();
        assert!(err.contains("faulted: 0xc0000005"), "{err}");
        assert_eq!(FAULTED.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn mismatch_changes_nothing_and_names_every_point() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let m = code(ADDER, PAGE_EXECUTE_READWRITE);
        let points = [
            Point::observe(0usize, "48 89 c8 48 83 c0 01", "fine", 1, Some(remember), None),
            Point::keep(3usize, "48 83 c0 09", "first wrong"),
            Point::keep(7usize, "90 90", "second wrong"),
        ];
        let err = install(m, &points).unwrap_err();
        assert!(err.starts_with("2 of 3 dependency points do not match"), "{err}");
        assert!(err.contains("first wrong at 0x3: expected 48 83 c0 09, found 48 83 c0 01"), "{err}");
        assert!(err.contains("second wrong at 0x7"), "{err}");
        assert_eq!(m.bytes(m.base, 16).unwrap(), hex(ADDER).unwrap());
    }

    #[test]
    fn overlapping_changes_are_refused() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let m = code(ADDER, PAGE_EXECUTE_READWRITE);
        let points = [Point::write(3usize, "48 83 c0 01", "one", "90 90 90 90"), Point::observe(0usize, "48 89 c8 48 83 c0 01", "two", 1, None, None)];
        let err = install(m, &points).unwrap_err();
        assert!(err.contains("change the same bytes"), "{err}");
        assert_eq!(m.bytes(m.base, 16).unwrap(), hex(ADDER).unwrap());
    }

    #[test]
    fn a_failed_hook_undoes_the_ones_before_it() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let good = code(ADDER, PAGE_EXECUTE_READWRITE);
        let data = code(ADDER, PAGE_READWRITE);
        let points = [
            Point::replace(good.base, "48 89 c8 48 83 c0 01", "good", doubled as Adder, &ORIGINAL),
            Point::replace(data.base, "48 89 c8 48 83 c0 01", "not code", doubled as Adder, &ORIGINAL),
        ];
        let err = install(ANYWHERE, &points).unwrap_err();
        assert!(err.contains("not code"), "{err}");
        assert_eq!(good.bytes(good.base, 16).unwrap(), hex(ADDER).unwrap());
        assert_eq!(call(good, 10), 17);
        // The target was released, so it can be hooked again.
        assert!(install(ANYWHERE, &points[..1]).is_ok());
        assert_eq!(call(good, 10), 34);
    }

    /// How every point is written, without the game: its bytes parse, a
    /// replacement keeps their length, a hook has room, and no two points
    /// change the same bytes.
    #[test]
    fn every_point_is_well_formed() {
        let points = this_build::declared();
        let problems: Vec<String> = points.iter().flat_map(malformed).collect();
        assert!(problems.is_empty(), "{problems:#?}");
        // Exports are far from the image and from each other.
        let addresses: Vec<usize> = points
            .iter()
            .enumerate()
            .map(|(i, p)| match p.place {
                Place::Rva(rva) => rva,
                Place::Export(_) => 0x1_0000_0000 + 0x1000 * i,
                // Another module lies far from the image and from exports.
                Place::In(_, rva) => 0x10_0000_0000 + rva,
            })
            .collect();
        disjoint(&points, &addresses).unwrap();
    }

    /// A jump stub continues where a point checks the code: inside one's
    /// bytes, or right after them, where the jump's own bytes end.
    #[test]
    fn every_jump_resumes_at_code_a_point_checks() {
        let points = this_build::declared();
        let checked: Vec<(usize, usize)> = points
            .iter()
            .filter(|p| p.method.is_none())
            .filter_map(|p| match p.place {
                Place::Rva(rva) => Some((rva, rva + p.bytes.split_whitespace().count())),
                Place::Export(_) | Place::In(..) => None,
            })
            .collect();
        let mut loose = Vec::new();
        for point in &points {
            if let Action::Jump { resume, .. } = point.action {
                for r in resume.iter().filter(|r| !checked.iter().any(|&(start, end)| (start..=end).contains(&r.rva))) {
                    loose.push(format!("{} resumes at {:#x}, which no point checks", describe(point), r.rva));
                }
            }
        }
        assert!(loose.is_empty(), "{loose:#?}");
    }

    /// Every point carries in its own bytes, decoded where they lie, the
    /// numbers it says it uses: a named field or global and the instruction
    /// kept for it cannot drift apart.
    #[test]
    fn every_point_uses_what_it_names() {
        let mut wrong = Vec::new();
        for point in this_build::declared().iter().filter(|p| !p.uses.is_empty()) {
            let Place::Rva(rva) = point.place else { continue };
            let code = crate::analysis::instructions(&hex(point.bytes).unwrap(), rva);
            let carried: Vec<u64> = code.iter().flat_map(crate::analysis::values).collect();
            for &value in point.uses {
                if !carried.contains(&(value as u64)) {
                    wrong.push(format!("{} does not use {value:#x}", describe(point)));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// The least RVA a constant counts as an address from: above every
    /// field's offset (the largest, 0x32604, is the interface's), below the
    /// game's code.
    const IMAGE: usize = 1 << 18;

    /// The game addresses a source file names: the constants of at least
    /// `IMAGE`, and the places its points are given, as a number or as one
    /// of its constants.
    fn addresses_named(text: &str) -> std::collections::BTreeSet<usize> {
        let hex = |token: &str| token.strip_prefix("0x").and_then(|digits| usize::from_str_radix(&digits.replace('_', ""), 16).ok());
        let mut constants = std::collections::HashMap::new();
        let mut named = std::collections::BTreeSet::new();
        for line in text.lines().map(str::trim).filter(|l| !l.starts_with("//")) {
            // A game function, wherever it is declared: its RVA.
            for (at, _) in line.match_indices("GameFunction::at(") {
                let value = line[at + "GameFunction::at(".len()..].split(')').next().and_then(hex);
                if let Some(value) = value {
                    named.insert(value);
                    if let Some((name, _)) = line.split_once("const ").and_then(|(_, rest)| rest.split_once(": GameFunction<")) {
                        constants.insert(name.to_string(), value);
                    }
                }
            }
            let Some((_, rest)) = line.split_once("const ") else { continue };
            // An array of them: each of its numbers.
            if let Some((_, values)) = rest.split_once("] = [").filter(|(kind, _)| kind.contains(": [usize; ")) {
                named.extend(values.trim_end_matches("];").split(',').filter_map(|v| hex(v.trim())).filter(|&v| v >= IMAGE));
                continue;
            }
            let Some((name, value)) = rest.split_once(": usize = ") else { continue };
            if let Some(value) = hex(value.trim_end_matches(';')) {
                constants.insert(name.to_string(), value);
                if value >= IMAGE {
                    named.insert(value);
                }
            }
        }
        let word = |text: &str| -> String { text.trim_start().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect() };
        let number = |token: &str| {
            let token = token.trim_end_matches("usize");
            hex(token).or_else(|| token.parse().ok()).or_else(|| constants.get(token).copied())
        };
        for (i, _) in text.match_indices("Point::") {
            let Some(open) = text[i..].find('(') else { continue };
            let after = text[i + open + 1..].trim_start();
            let first = word(after);
            let Some(mut place) = number(&first) else { continue };
            // A place written as an offset from another: `NAME + 0x41`.
            let rest = after[first.len()..].trim_start();
            if let Some((sign, operand)) = rest.strip_prefix('+').map(|r| (1, r)).or_else(|| rest.strip_prefix('-').map(|r| (-1, r))) {
                if let Some(offset) = number(&word(operand)) {
                    place = if sign > 0 { place + offset } else { place - offset };
                }
            }
            named.insert(place);
        }
        named
    }

    /// Each function, global and place of the game's code is named in one
    /// file, which every part that relies on it reads it from: a second
    /// declaration of it is a second understanding of it, which a fix would
    /// not reach.
    #[test]
    fn every_game_address_is_named_in_one_file() {
        let (root, files) = sources();
        let mut by_address: std::collections::BTreeMap<usize, Vec<String>> = std::collections::BTreeMap::new();
        for file in &files {
            let name = file.strip_prefix(&root).unwrap().display().to_string();
            for address in addresses_named(&std::fs::read_to_string(file).unwrap()) {
                by_address.entry(address).or_default().push(name.clone());
            }
        }
        let twice: Vec<String> = by_address.iter().filter(|(_, files)| files.len() > 1).map(|(a, files)| format!("{a:#x}: {}", files.join(", "))).collect();
        assert!(twice.is_empty(), "{twice:#?}");
    }

    /// A field of the game's objects that more than one module reads is
    /// named in `objects`, and no other file names it again. A field is
    /// told by its name and its offset together: small offsets are shared
    /// by many unrelated objects, so only those of at least `0x100` count,
    /// where the same name at the same offset is the same field. The same
    /// field under two names is not found here.
    #[test]
    fn every_field_is_named_in_one_file() {
        const LEAST: usize = 0x100;
        let (root, files) = sources();
        let mut by_field: std::collections::BTreeMap<(String, usize), std::collections::BTreeSet<String>> = std::collections::BTreeMap::new();
        for file in &files {
            let name = file.strip_prefix(&root).unwrap().display().to_string();
            for line in std::fs::read_to_string(file).unwrap().lines().map(str::trim).filter(|l| !l.starts_with("//")) {
                let Some((field, value)) = line.split_once("const ").and_then(|(_, rest)| rest.split_once(": usize = 0x")) else { continue };
                let Ok(offset) = usize::from_str_radix(&value.trim_end_matches(';').replace('_', ""), 16) else { continue };
                if (LEAST..IMAGE).contains(&offset) {
                    by_field.entry((field.to_string(), offset)).or_default().insert(name.clone());
                }
            }
        }
        let twice: Vec<String> = by_field
            .iter()
            .filter(|(_, files)| files.len() > 1)
            .map(|((field, offset), files)| format!("{field} = {offset:#x}: {}", files.iter().cloned().collect::<Vec<_>>().join(", ")))
            .collect();
        assert!(twice.is_empty(), "{twice:#?}");
    }

    /// The core's source files, and the folder they are in.
    fn sources() -> (std::path::PathBuf, Vec<std::path::PathBuf>) {
        fn under(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().map(std::result::Result::unwrap) {
                let path = entry.path();
                if path.is_dir() {
                    under(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        under(&root, &mut files);
        (root, files)
    }

    /// A game function is called through its [`GameFunction`], which says
    /// how many arguments it takes, never at its address: what is called at
    /// an address is a method or a pointer the game keeps.
    #[test]
    fn game_functions_are_called_through_their_handles() {
        let (root, files) = sources();
        let mut found = Vec::new();
        for file in &files {
            let text = std::fs::read_to_string(file).unwrap();
            for (i, _) in text.match_indices(".call(") {
                let first = text[i + ".call(".len()..].split([',', ')']).next().unwrap_or("");
                if first.contains(".at(") {
                    found.push(format!("{}: {}", file.strip_prefix(&root).unwrap().display(), text[i..].lines().next().unwrap()));
                }
            }
        }
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn addresses_are_read_from_constants_and_points() {
        let text = "const FIELD: usize = 0x28c0;\nconst F: usize = 0x93d750;\n// const OLD: usize = 0x200000;\n\
                    const SITE: usize = 0x9a1f0;\nconst BOTH: [usize; 2] = [0x300000, 0x300010];\nPoint::keep(SITE, \"c3\", \"x\");\nPoint::keep(\n    0xcd1db1usize, \"c3\", \"y\");\nPoint::keep(FIELD + 1, \"c3\", \"z\");\n\
                    const CALLED: GameFunction<2> = GameFunction::at(0x500000);\nPoint::keep(CALLED, \"c3\", \"w\");\n\
                    const TABLE: [(usize, GameFunction<1>); 1] = [(0x10, GameFunction::at(0x600000))];";
        assert_eq!(addresses_named(text), [0x28c1, 0x9a1f0, 0x93d750, 0xcd1db1, 0x300000, 0x300010, 0x500000, 0x600000].into());
    }

    #[test]
    fn a_point_checks_its_bytes_what_they_use_and_a_method_its_function() {
        let points = [
            Point::keep(0x100usize, "48 89 c8 c3", "four bytes").uses(&[0x5000]),
            Point::keep(0x102usize, "c3 90 90 90 90", "overlapping"),
            Point::method(0x200, 8, 0x300, "method"),
            Point::keep(c"PHYSFS_init", "48 89 5c 24 08", "export"),
        ];
        let checked = Checked::of(&points);
        // The bytes of both, and the address right after the second's.
        assert!((0x100..=0x107).all(|rva| checked.contains(rva)));
        assert!(!checked.contains(0xff) && !checked.contains(0x108));
        assert!(checked.contains(0x5000) && !checked.contains(0x5001));
        // The vtable's slot and the function it holds.
        assert!(checked.contains(0x208) && checked.contains(0x300));
        assert!(!checked.contains(0x200) && !checked.contains(0x301));
    }

    #[test]
    #[should_panic(expected = "the framework used 0x100000000, which no dependency point checks")]
    fn an_address_no_point_checks_is_refused() {
        // Past the end of the game image.
        Memory { base: 0 }.at(0x1_0000_0000usize);
    }

    #[test]
    fn a_point_in_another_module_is_checked_there() {
        let image = code("90 90 90 90 90", PAGE_READWRITE);
        let other = code("b8 01 00 00 00", PAGE_READWRITE);
        let lookup = Lookup { export: &|_| Err("no exports".into()), module: &|_| Ok(other.base) };
        let there = check(image, &[Point::keep(Place::In(Module::Pops, 0), "b8 01 00 00 00", "there")], &lookup).unwrap();
        assert_eq!(there, [other.base]);
        let err = check(image, &[Point::keep(Place::In(Module::Pops, 0), "90 90 90 90 90", "here")], &lookup).unwrap_err();
        assert!(err.contains("here in pops_api.dll at 0x0"), "{err}");
    }

    #[test]
    fn a_method_is_checked_by_its_function_address() {
        let m = code(ADDER, PAGE_EXECUTE_READWRITE);
        // A vtable at RVA 0x100 whose method at 8 is the function at RVA 3.
        m.write(m.base + 0x108, &((m.base + 3) as u64).to_le_bytes()).unwrap();
        let lookup = Lookup { export: &|_| Err("no exports".into()), module: &|_| Err("no modules".into()) };
        assert_eq!(check(m, &[Point::method(0x100, 8, 3, "method")], &lookup).unwrap(), [m.base + 0x108]);
        let err = check(m, &[Point::method(0x100, 8, 5, "method")], &lookup).unwrap_err();
        assert!(err.contains("method at 0x108: expected the function at 0x5, found 0x3"), "{err}");
    }

    #[test]
    fn malformed_points_are_named() {
        let bad = [
            Point::keep(1usize, "48 8g", "not hex"),
            Point::keep(2usize, "488b", "run together"),
            Point::write(3usize, "90 90", "short replacement", "90"),
            Point::observe(4usize, "48 89 c8", "too short to hook", 1, None, None),
        ];
        let problems: Vec<String> = bad.iter().flat_map(malformed).collect();
        assert_eq!(problems.len(), 4, "{problems:#?}");
        for (problem, what) in problems.iter().zip(["not hex", "run together", "short replacement", "too short to hook"]) {
            assert!(problem.starts_with(what), "{problem}");
        }
    }

    /// Maps a module from disk the way the loader lays it out, without running
    /// it, and returns where.
    fn map_module(path: &std::path::Path) -> usize {
        let image = crate::analysis::map_file(&std::fs::read(path).unwrap()).leak();
        crate::analysis::relocate(image, image.as_ptr() as u64);
        image.as_ptr() as usize
    }

    /// The game's files, mapped: `eu4.exe` and the other modules points are
    /// in. Calls `with` with the image and what to look places up with.
    fn with_game(with: impl FnOnce(Memory, &Lookup)) {
        let dir = std::path::PathBuf::from(std::env::var("EU4_GAME_DIR").expect("set EU4_GAME_DIR"));
        let (m, exports) = map_game(&dir.join("eu4.exe"));
        let pops = map_module(&dir.join(Module::Pops.file()));
        let export = |name: &CStr| exports.get(name.to_str().unwrap()).map(|&rva| m.base + rva).ok_or_else(|| format!("no export {name:?}"));
        let module = |module: Module| match module {
            Module::Pops => Ok(pops),
        };
        with(m, &Lookup { export: &export, module: &module });
    }

    /// Maps `eu4.exe` from disk the way the loader lays it out, without running
    /// it, and returns it with its exports.
    fn map_game(path: &std::path::Path) -> (Memory, std::collections::HashMap<String, usize>) {
        let image = crate::analysis::map_file(&std::fs::read(path).unwrap()).leak();
        crate::analysis::relocate(image, image.as_ptr() as u64);
        let rd = |rva: usize| u32::from_le_bytes(image[rva..rva + 4].try_into().unwrap()) as usize;
        let export_dir = rd(rd(0x3c) + 24 + 112);
        let (names, ordinals, functions) = (rd(export_dir + 32), rd(export_dir + 36), rd(export_dir + 28));
        let mut exports = std::collections::HashMap::new();
        for i in 0..rd(export_dir + 24) {
            let name_rva = rd(names + 4 * i);
            let end = image[name_rva..].iter().position(|&b| b == 0).unwrap();
            let name = String::from_utf8_lossy(&image[name_rva..name_rva + end]).into_owned();
            let ordinal = u16::from_le_bytes(image[ordinals + 2 * i..ordinals + 2 * i + 2].try_into().unwrap()) as usize;
            exports.insert(name, rd(functions + 4 * ordinal));
        }
        (Memory { base: image.as_ptr() as usize }, exports)
    }

    /// Every dependency point against the installed game:
    /// `EU4_GAME_DIR=<install dir> cargo test -p eu4-harmony-core -- --ignored`
    #[test]
    #[ignore]
    fn every_point_matches_the_installed_game() {
        with_game(|m, lookup| {
            assert_eq!(crate::baseline::recognize(&m).unwrap(), "1.37.5");
            let points = this_build::declared();
            let addresses = check(m, &points, lookup).unwrap();
            assert_eq!(addresses.len(), points.len());
        });
    }

    /// Every observe point forwards at least as many arguments as its function
    /// reads, by the function's code: same environment variable as above.
    #[test]
    #[ignore]
    fn observers_forward_every_argument_their_function_reads() {
        let mut found = None;
        with_game(|m, lookup| found = Some((m, check(m, &this_build::declared(), lookup).unwrap())));
        let (m, addresses) = found.unwrap();
        let points = this_build::declared();
        let image = unsafe { crate::analysis::loaded(m.base) };
        let mut short = Vec::new();
        for (point, &at) in points.iter().zip(&addresses) {
            let (Place::Rva(_) | Place::Export(_), Action::Observe { argc, .. }) = (point.place, &point.action) else { continue };
            let argc = *argc;
            let rva = at - m.base;
            // Exports (PhysFS) live outside the game's code; their arity is
            // the library's documented one.
            let Some(range) = crate::analysis::bounds(image, rva) else { continue };
            let needed = crate::analysis::arguments(&image[range.clone()], rva);
            if needed > argc {
                short.push(format!("{} at {rva:#x}: forwards {argc}, reads {needed}", point.what));
            }
        }
        assert!(short.is_empty(), "{short:#?}");
        // The check sees stack arguments: the main text method takes eleven,
        // the button text method fifteen (forwarding fewer once crashed the game).
        // The map's markers' update reads its first argument only in the
        // body the compiler split from its prologue.
        for (rva, count) in [(0x15988e0, 11), (0x1596660, 15), (0x10a6d00, 4)] {
            let range = crate::analysis::bounds(image, rva).unwrap();
            assert_eq!(crate::analysis::arguments(&image[range], rva), count, "{rva:#x}");
        }
    }

    #[test]
    fn a_late_failure_restores_written_bytes() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let hooked = code(ADDER, PAGE_EXECUTE_READWRITE);
        let written = code("b8 01 00 00 00 c3", PAGE_EXECUTE_READWRITE);
        let points = [
            Point::observe(hooked.base, "48 89 c8 48 83 c0 01", "hooked", 1, Some(remember), None),
            Point::write(written.base, "b8 01 00 00 00", "written", "b8 02 00 00 00"),
        ];
        FAIL_BEFORE_ENABLE.store(true, Ordering::SeqCst);
        let err = install(ANYWHERE, &points).unwrap_err();
        FAIL_BEFORE_ENABLE.store(false, Ordering::SeqCst);
        assert_eq!(err, "injected failure");
        assert_eq!(call(written, 0), 1);
        assert_eq!(hooked.bytes(hooked.base, 16).unwrap(), hex(ADDER).unwrap());
    }
}
