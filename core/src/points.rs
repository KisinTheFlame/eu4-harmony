//! Dependency points (GLOSSARY): the places in the game image the framework
//! relies on. Each is declared once, with the bytes it must hold and what the
//! framework does there, and that one declaration drives checking, hooking and
//! undoing (ADR 0012). [`install`] applies every point or none.
use crate::{
    hooks::{self, Callback},
    memory::Memory,
    Result,
};
use std::ffi::CStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows_sys::Win32::System::{
    Diagnostics::Debug::FlushInstructionCache,
    LibraryLoader::{GetModuleHandleW, GetProcAddress},
    Threading::GetCurrentProcess,
};

/// Where a dependency point is.
#[derive(Clone, Copy, Debug)]
pub enum Place {
    Rva(usize),
    /// A function `eu4.exe` exports by name.
    Export(&'static CStr),
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

/// What the framework does at a dependency point.
pub enum Action {
    /// Relies on it only: calls it, reads it or continues there.
    Keep,
    /// Runs `before` and `after` around every call of the function there.
    Observe { argc: u32, before: Option<Callback>, after: Option<Callback> },
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

    pub fn observe(place: impl Into<Place>, bytes: &'static str, what: &'static str, argc: u32, before: Option<Callback>, after: Option<Callback>) -> Self {
        Self::new(place, bytes, what, Action::Observe { argc, before, after })
    }

    pub fn replace(place: impl Into<Place>, bytes: &'static str, what: &'static str, with: usize, original: &'static AtomicUsize) -> Self {
        Self::new(place, bytes, what, Action::Replace { with, original })
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
    text.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).ok().filter(|_| b.len() == 2).ok_or_else(|| format!("{b:?} is not a byte in hex")))
        .collect()
}

fn spaced(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// Finds a function exported by name.
type Exports<'a> = &'a dyn Fn(&CStr) -> Result<usize>;

/// `eu4.exe`'s own exports, in the running game.
fn game_export(name: &CStr) -> Result<usize> {
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    unsafe { GetProcAddress(module, name.as_ptr() as *const u8) }
        .map(|f| f as usize)
        .ok_or_else(|| format!("eu4.exe does not export {}", name.to_string_lossy()))
}

fn resolve(m: Memory, place: Place, exports: Exports) -> Result<usize> {
    match place {
        Place::Rva(rva) => Ok(m.at(rva)),
        Place::Export(name) => exports(name),
    }
}

fn describe(point: &Point) -> String {
    match point.place {
        Place::Rva(rva) => format!("{} at {rva:#x}", point.what),
        Place::Export(name) => format!("{} ({})", point.what, name.to_string_lossy()),
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
fn check(m: Memory, points: &[Point], exports: Exports) -> Result<Vec<usize>> {
    let mut addresses = Vec::with_capacity(points.len());
    let mut problems = Vec::new();
    for point in points {
        let wrong = malformed(point);
        if !wrong.is_empty() {
            problems.extend(wrong);
            continue;
        }
        let expected = match point.method {
            Some(function) => (m.at(function) as u64).to_le_bytes().to_vec(),
            None => hex(point.bytes)?,
        };
        let found = resolve(m, point.place, exports).and_then(|at| m.bytes(at, expected.len()).map(|found| (at, found)));
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
        return Err(format!("{} of {} dependency points do not match:
{}", problems.len(), points.len(), problems.join("
")));
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
            Action::Observe { argc, before, after } => made(hooks::observe(at, *argc, *before, *after))?,
            Action::Replace { with, original } => made(hooks::create(at, *with).map(|t| original.store(t, Ordering::Release)))?,
            Action::Jump { stub, resume } => {
                made(hooks::create(at, *stub).map(|_| ()))?;
                for r in *resume {
                    r.address.store(m.at(r.rva), Ordering::Release);
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

/// Checks every point, then hooks and writes all of them and enables the
/// hooks. If anything fails, nothing stays changed and the error says why.
pub fn install(m: Memory, points: &[Point]) -> Result<Installed> {
    let addresses = check(m, points, &game_export)?;
    let mut undo = Undo::default();
    match apply(m, points, &addresses, &mut undo) {
        Ok(()) => Ok(Installed { checked: points.len(), hooks: undo.hooks.len(), writes: undo.writes.len() }),
        Err(e) => {
            undo.run(m);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookFrame;
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

    static RESUME: AtomicUsize = AtomicUsize::new(0);
    static RESUME_AT: [Resume; 1] = [Resume { rva: 15, address: &RESUME }];

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
    extern "C" fn remember(frame: *mut HookFrame) {
        SEEN.store(unsafe { (*frame).args[0] }, Ordering::SeqCst);
    }

    static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn doubled(x: usize) -> usize {
        let original: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(ORIGINAL.load(Ordering::SeqCst)) };
        original(x) * 2
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
            Point::replace(replaced.base, "48 89 c8 48 83 c0 01", "replaced", doubled as *const () as usize, &ORIGINAL),
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
        let points = [
            Point::write(3usize, "48 83 c0 01", "one", "90 90 90 90"),
            Point::observe(0usize, "48 89 c8 48 83 c0 01", "two", 1, None, None),
        ];
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
            Point::replace(good.base, "48 89 c8 48 83 c0 01", "good", doubled as *const () as usize, &ORIGINAL),
            Point::replace(data.base, "48 89 c8 48 83 c0 01", "not code", doubled as *const () as usize, &ORIGINAL),
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
        let points = crate::points();
        let problems: Vec<String> = points.iter().flat_map(malformed).collect();
        assert!(problems.is_empty(), "{problems:#?}");
        // Exports are far from the image and from each other.
        let addresses: Vec<usize> = points
            .iter()
            .enumerate()
            .map(|(i, p)| match p.place {
                Place::Rva(rva) => rva,
                Place::Export(_) => 0x1_0000_0000 + 0x1000 * i,
            })
            .collect();
        disjoint(&points, &addresses).unwrap();
    }

    /// Every point carries in its own bytes, decoded where they lie, the
    /// numbers it says it uses: a named field or global and the instruction
    /// kept for it cannot drift apart.
    #[test]
    fn every_point_uses_what_it_names() {
        let mut wrong = Vec::new();
        for point in crate::points().iter().filter(|p| !p.uses.is_empty()) {
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

    #[test]
    fn a_method_is_checked_by_its_function_address() {
        let m = code(ADDER, PAGE_EXECUTE_READWRITE);
        // A vtable at RVA 0x100 whose method at 8 is the function at RVA 3.
        m.write(m.at(0x108), &(m.at(3) as u64).to_le_bytes()).unwrap();
        let lookup = |_: &CStr| -> Result<usize> { Err("no exports".into()) };
        assert_eq!(check(m, &[Point::method(0x100, 8, 3, "method")], &lookup).unwrap(), [m.at(0x108)]);
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
        let dir = std::path::PathBuf::from(std::env::var("EU4_GAME_DIR").expect("set EU4_GAME_DIR"));
        let (m, exports) = map_game(&dir.join("eu4.exe"));
        assert_eq!(crate::baseline::recognize(&m).unwrap(), "1.37.5");
        let lookup = |name: &CStr| exports.get(name.to_str().unwrap()).map(|&rva| m.at(rva)).ok_or_else(|| format!("no export {name:?}"));
        let points = crate::points();
        let addresses = check(m, &points, &lookup).unwrap();
        assert_eq!(addresses.len(), points.len());
    }

    /// Every observe point forwards at least as many arguments as its function
    /// reads, by the function's code: same environment variable as above.
    #[test]
    #[ignore]
    fn observers_forward_every_argument_their_function_reads() {
        let dir = std::path::PathBuf::from(std::env::var("EU4_GAME_DIR").expect("set EU4_GAME_DIR"));
        let (m, exports) = map_game(&dir.join("eu4.exe"));
        let lookup = |name: &CStr| exports.get(name.to_str().unwrap()).map(|&rva| m.at(rva)).ok_or_else(|| format!("no export {name:?}"));
        let points = crate::points();
        let addresses = check(m, &points, &lookup).unwrap();
        let image = unsafe { crate::analysis::loaded(m.base) };
        let mut short = Vec::new();
        for (point, &at) in points.iter().zip(&addresses) {
            let Action::Observe { argc, .. } = point.action else { continue };
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
        for (rva, count) in [(0x15988e0, 11), (0x1596660, 15)] {
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
