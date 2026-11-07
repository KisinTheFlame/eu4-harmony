//! Logs calls of game functions, for finding out what the game passes them
//! (the `dev-call-trace` feature: a player's build has none of this). The
//! functions are the RVAs `functions` in dev.toml's `[call-trace]` lists, say
//! `["0x15988e0", "0x159c600"]`. Each function's first calls (`calls`, 20 by
//! default) are logged: the arguments, the text any of them points at (a C
//! string or a `std::string`, in the game's text encoding), where the call
//! came from, and the result.
//!
//! With `times`, every call of each function is timed as well, into
//! `call-times.tsv` next to the log: its thread, its start and end in
//! microseconds since the tool began, the game's date, its first argument, and
//! the eight bytes at the offset `fields` gives into the object that argument
//! points at. The calls are kept in memory and written by a thread of the
//! tool's own once a second, so that no thread of the game's waits for the
//! file.
//!
//! A traced function is observed, and an observer forwards exactly the
//! arguments its point declares: a function that reads more finds garbage.
//! The count comes from the function's code ([`crate::analysis::arguments`]);
//! `RVA:N` gives it for a function the exception directory has no entry for
//! (a leaf), or overrides it.
use crate::analysis;
use crate::hooks::{self, HookFrame};
use crate::memory::Memory;
use crate::points::Point;
use crate::unicode::game_text;
use std::cell::RefCell;
use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

struct Traced {
    address: usize,
    rva: usize,
    argc: u32,
    calls: AtomicUsize,
    /// The offset into the first argument's object whose bytes the times
    /// record.
    field: Option<usize>,
}

static TRACED: OnceLock<Vec<Traced>> = OnceLock::new();
static LIMIT: OnceLock<usize> = OnceLock::new();
/// When calls are timed, the instant the times count from.
static TIMED: OnceLock<Instant> = OnceLock::new();
/// The timed calls not yet written.
static TIMES: Mutex<Vec<Time>> = Mutex::new(Vec::new());

/// A traced call still running.
struct Running {
    /// The call's number, when it was logged.
    logged: Option<usize>,
    /// When it began, the game's date and what it was given, when calls are
    /// timed.
    time: Option<Time>,
}

/// One timed call.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Time {
    rva: usize,
    thread: u32,
    start: Duration,
    end: Duration,
    date: Option<i32>,
    argument: usize,
    field: Option<u64>,
}

thread_local! {
    /// This thread's traced calls still running, innermost last.
    static RUNNING: RefCell<Vec<Running>> = const { RefCell::new(Vec::new()) };
}

const TIMES_HEADER: &str = "function\tthread\tstart\tend\tdate\targument\tfield\n";

/// A timed call's line in `call-times.tsv`: the main thread is `main`, the
/// times are microseconds, a date or field not read is empty.
fn time_line(t: &Time, main: u32) -> String {
    let thread = if t.thread == main { "main".to_string() } else { t.thread.to_string() };
    let date = t.date.map(|d| eu4_harmony_checks::Date(d).to_string()).unwrap_or_default();
    let field = t.field.map(|f| format!("{f:#x}")).unwrap_or_default();
    format!("{:#x}\t{thread}\t{}\t{}\t{date}\t{:#x}\t{field}\n", t.rva, t.start.as_micros(), t.end.as_micros(), t.argument)
}

/// Writes the timed calls once a second into `call-times.tsv` in the logs
/// folder, a new file each run.
fn write_times() {
    let path = match crate::paths::game_dir().and_then(|dir| crate::paths::user_dir(&dir)) {
        Ok(dir) => eu4_harmony_folders::UserFolder::new(dir).logs().join("call-times.tsv"),
        Err(e) => {
            log!("trace: the times are not written: {e}");
            return;
        }
    };
    let file = std::fs::create_dir_all(path.parent().unwrap_or(&path)).and_then(|()| std::fs::File::create(&path));
    let mut file = match file {
        Ok(file) => std::io::BufWriter::new(file),
        Err(e) => {
            log!("trace: cannot write {}: {e}", path.display());
            return;
        }
    };
    log!("trace: timing every call into {}", path.display());
    let _ = file.write_all(TIMES_HEADER.as_bytes());
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let times = std::mem::take(&mut *TIMES.lock().unwrap_or_else(|e| e.into_inner()));
        let main = crate::main_thread();
        for t in &times {
            let _ = file.write_all(time_line(t, main).as_bytes());
        }
        let _ = file.flush();
    }
}

/// The text at `p`, when it holds some: a `std::string` (`game_string`'s
/// layout) or a C string.
fn text_at(m: &Memory, p: usize) -> Option<String> {
    let readable = |bytes: &[u8]| {
        let text = game_text::to_utf8(bytes);
        (text.chars().count() >= 2 && !text.chars().any(|c| c.is_control() && c != '\n' && c != '\t')).then_some(text)
    };
    use crate::game_string::{CAPACITY, INLINE, SIZE};
    if let (Ok(size), Ok(capacity)) = (m.u64(p + SIZE), m.u64(p + CAPACITY)) {
        if (INLINE as u64..1 << 20).contains(&capacity) && size <= capacity {
            let data = if capacity <= INLINE as u64 { Some(p) } else { m.ptr(p).ok() };
            if let Some(text) = data.and_then(|d| m.bytes(d, size as usize).ok()).and_then(|b| readable(&b)) {
                return Some(format!("string {text:?}"));
            }
        }
    }
    let bytes = m.bytes(p, 64).or_else(|_| m.bytes(p, 16)).ok()?;
    let end = bytes.iter().position(|&b| b == 0)?;
    readable(&bytes[..end]).map(|text| format!("text {text:?}"))
}

fn traced(frame: &HookFrame) -> Option<(&'static Traced, Memory)> {
    let address = hooks::observed(frame.slot)?;
    let traced = TRACED.get()?.iter().find(|t| t.address == address)?;
    Some((traced, *crate::memory()?))
}

extern "C" fn before(frame: *mut HookFrame) {
    crate::guarded("trace", || {
        let frame = unsafe { &*frame };
        let Some((traced, m)) = traced(frame) else { return };
        let n = traced.calls.fetch_add(1, Ordering::Relaxed);
        let limit = *LIMIT.get().unwrap_or(&20);
        let logged = n < limit;
        let time = TIMED.get().map(|origin| {
            let argument = frame.args[0];
            Time {
                rva: traced.rva,
                thread: crate::thread_id(),
                start: origin.elapsed(),
                end: Duration::ZERO,
                date: crate::game::world_context_any(&m).ok().map(|w| w.date),
                argument,
                field: traced.field.and_then(|f| m.u64(argument.wrapping_add(f)).ok()),
            }
        });
        RUNNING.with(|r| r.borrow_mut().push(Running { logged: logged.then_some(n), time }));
        if !logged {
            if n == limit {
                log!("trace {:#x}: later calls not logged", traced.rva);
            }
            return;
        }
        let args: Vec<String> = frame.args[..traced.argc as usize]
            .iter()
            .map(|&a| match text_at(&m, a) {
                Some(text) => format!("{a:#x} {text}"),
                None => format!("{a:#x}"),
            })
            .collect();
        let from = frame.return_address.wrapping_sub(m.base);
        log!("trace {:#x} #{n} from {from:#x}: {}", traced.rva, args.join(", "));
    })
}

extern "C" fn after(frame: *mut HookFrame) {
    crate::guarded("trace", || {
        let frame = unsafe { &*frame };
        let Some((traced, m)) = traced(frame) else { return };
        // The call returning is this thread's innermost traced one.
        let Some(running) = RUNNING.with(|r| r.borrow_mut().pop()) else { return };
        if let (Some(mut time), Some(origin)) = (running.time, TIMED.get()) {
            time.end = origin.elapsed();
            TIMES.lock().unwrap_or_else(|e| e.into_inner()).push(time);
        }
        let Some(n) = running.logged else { return };
        if frame.fault != 0 {
            log!("trace {:#x} #{n}: left through an exception", traced.rva);
            return;
        }
        let result = frame.result;
        match text_at(&m, result) {
            Some(text) => log!("trace {:#x} #{n} returned {result:#x} {text}", traced.rva),
            None => log!("trace {:#x} #{n} returned {result:#x}", traced.rva),
        }
    })
}

/// The most arguments an observer forwards (`HookFrame::args`).
const MOST_ARGUMENTS: u32 = 12;

/// Observe points for the functions dev.toml names. A function whose argument count cannot be told is left out, as
/// are one that reads more arguments than an observer forwards and those past the `room` observer slots the
/// framework leaves.
pub fn points(m: &Memory, room: usize) -> Vec<Point> {
    let options = &crate::dev::options().call_trace;
    if options.functions.is_empty() {
        return Vec::new();
    }
    let limit = options.calls;
    LIMIT.get_or_init(|| limit);
    let image = unsafe { analysis::loaded(m.base) };
    let hex = |s: &str| usize::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok();
    let mut traced = Vec::new();
    for item in &options.functions {
        let (rva, argc) = match item.split_once(':') {
            Some((rva, n)) => (hex(rva), n.trim().parse::<u32>().ok()),
            None => (hex(item), None),
        };
        let Some(rva) = rva.filter(|&r| r < image.len()) else {
            log!("trace: {item:?} is not an RVA in the game");
            continue;
        };
        let Some(argc) = argc.or_else(|| analysis::bounds(image, rva).map(|range| analysis::arguments(&image[range], rva))) else {
            log!("trace: {rva:#x} starts no function in the exception directory; give its argument count as {rva:#x}:N");
            continue;
        };
        if argc > MOST_ARGUMENTS {
            // Forwarding fewer would hand the function garbage for the rest.
            log!("trace: {rva:#x} left out: it reads {argc} arguments, and an observer forwards at most {MOST_ARGUMENTS}");
            continue;
        }
        if traced.len() == room {
            log!("trace: {rva:#x} left out: only {room} functions can be traced besides the framework's own");
            continue;
        }
        let field = match options.fields.iter().find(|(f, _)| hex(f) == Some(rva)) {
            Some((_, offset)) => match hex(offset) {
                Some(offset) => Some(offset),
                None => {
                    log!("trace: {offset:?}, the field of {rva:#x}, is not an offset in hex");
                    None
                }
            },
            None => None,
        };
        log!("trace: {rva:#x}, {argc} argument(s), first {limit} calls");
        traced.push(Traced { address: m.base + rva, rva, argc, calls: AtomicUsize::new(0), field });
    }
    let points = traced
        .iter()
        .map(|t| {
            // The bytes there now: tracing checks nothing about the game.
            let bytes: Vec<String> = image[t.rva..t.rva + 8].iter().map(|b| format!("{b:02x}")).collect();
            let bytes: &'static str = bytes.join(" ").leak();
            Point::observe(t.rva, bytes, "traced function", t.argc, Some(before), Some(after))
        })
        .collect();
    if TRACED.set(traced).is_err() {
        log!("trace: already set up");
        return Vec::new();
    }
    if options.times && TIMED.set(Instant::now()).is_ok() {
        let writer = std::thread::Builder::new().name("eu4-harmony call times".into()).spawn(|| crate::guarded("trace", write_times));
        if let Err(e) = writer {
            log!("trace: the times are not written: {e}");
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timed_call_is_a_line_of_its_thread_times_date_and_argument() {
        let date = eu4_harmony_checks::Date::from_ymd(1700, 1, 2).unwrap().0;
        let call = Time {
            rva: 0xb0fa0,
            thread: 4242,
            start: Duration::from_micros(1500),
            end: Duration::from_micros(1750),
            date: Some(date),
            argument: 0x1234,
            field: Some(0x8000_0021_0052_5554),
        };
        assert_eq!(time_line(&call, 7), "0xb0fa0\t4242\t1500\t1750\t1700.1.2\t0x1234\t0x8000002100525554\n");
        let unread = Time { thread: 7, date: None, field: None, ..call };
        assert_eq!(time_line(&unread, 7), "0xb0fa0\tmain\t1500\t1750\t\t0x1234\t\n");
        assert_eq!(TIMES_HEADER.matches('\t').count(), time_line(&call, 7).matches('\t').count());
    }
}
