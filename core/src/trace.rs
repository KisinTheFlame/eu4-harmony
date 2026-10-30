//! Logs calls of game functions, for finding out what the game passes them
//! (the `trace` feature: a player's build has none of this). Set
//! `EU4HARMONY_TRACE` to the functions' RVAs, comma-separated, before the game
//! starts, say `0x15988e0,0x159c600`. Each function's first calls (20, or
//! `EU4HARMONY_TRACE_CALLS`) are logged: the arguments, the text any of them
//! points at (a C string or a `std::string`, in the game's text encoding),
//! where the call came from, and the result.
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

struct Traced {
    address: usize,
    rva: usize,
    argc: u32,
    calls: AtomicUsize,
}

static TRACED: OnceLock<Vec<Traced>> = OnceLock::new();
static LIMIT: OnceLock<usize> = OnceLock::new();

thread_local! {
    /// This thread's traced calls still running, innermost last: the number
    /// of each that was logged.
    static RUNNING: RefCell<Vec<Option<usize>>> = const { RefCell::new(Vec::new()) };
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
        RUNNING.with(|r| r.borrow_mut().push(logged.then_some(n)));
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
        let Some(Some(n)) = RUNNING.with(|r| r.borrow_mut().pop()) else { return };
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

/// Observe points for the functions `EU4HARMONY_TRACE` names; none when it is
/// not set. A function whose argument count cannot be told is left out, as
/// are those past the `room` observer slots the framework leaves.
pub fn points(m: &Memory, room: usize) -> Vec<Point> {
    let Ok(spec) = std::env::var("EU4HARMONY_TRACE") else { return Vec::new() };
    let limit = std::env::var("EU4HARMONY_TRACE_CALLS").ok().and_then(|n| n.parse().ok()).unwrap_or(20);
    LIMIT.get_or_init(|| limit);
    let image = unsafe { analysis::loaded(m.base) };
    let hex = |s: &str| usize::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok();
    let mut traced = Vec::new();
    for item in spec.split(',').filter(|s| !s.trim().is_empty()) {
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
        if traced.len() == room {
            log!("trace: {rva:#x} left out: only {room} functions can be traced besides the framework's own");
            continue;
        }
        log!("trace: {rva:#x}, {argc} argument(s), first {limit} calls");
        traced.push(Traced { address: m.at(rva), rva, argc: argc.min(12), calls: AtomicUsize::new(0) });
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
    points
}
