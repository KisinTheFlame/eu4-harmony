//! A crash's record in the framework log. As the game's crash handling
//! begins to write a crash's files (`0x1654250`), the log gets what it is,
//! where it happened, on which thread, the game's date, and the calls it was
//! in, so that a player's logs say where the game crashed when its crash
//! folder is not sent with them (docs/research/crash-handling.md).
//!
//! The crash handling comes there from the game's unhandled exception filter
//! (`0x1654730`, a stack overflow's from a thread of its own), from a pure
//! virtual function's call (`0x1654ac0`) and from `abort` (`0x1654b50`),
//! each once a process. What ends the game without its filter, as the
//! heap's own end of a corrupted heap does, is not recorded.
use crate::hooks::Call;
use crate::memory::Memory;
use crate::points::Point;
use crate::stack::Context;
use std::time::Duration;
use windows_sys::Win32::System::Diagnostics::Debug::CONTEXT;

/// Writes a crash's files and starts the crash reporter: (what crashed, as
/// text; the exception's pointers, or null when there was no exception; the
/// thread, as a handle).
const WRITE_CRASH: usize = 0x1654250;
/// How many calls a record names.
const FRAMES: usize = 32;
/// How long the record waits for the log, which the crashed thread may hold.
const LOG_WAIT: Duration = Duration::from_secs(1);

pub fn points() -> Vec<Point> {
    vec![Point::observe(WRITE_CRASH, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18", "write a crash", 3, Some(crashed), None)]
}

/// What a crash's exception record says, as one line's part.
fn exception(code: u32, parameters: &[u64]) -> String {
    let access = |verb: &str| match parameters {
        [kind, address, ..] => {
            let kind = match kind {
                0 => "reading",
                1 => "writing",
                8 => "executing",
                _ => verb,
            };
            format!(" {kind} {address:#x}")
        }
        _ => String::new(),
    };
    let name = match code {
        0xc000_0005 => format!("an access violation{}", access("at")),
        0xc000_0006 => format!("a page not read in{}", access("at")),
        0xc000_00fd => "a stack overflow".into(),
        0xc000_0409 => "a stack buffer overrun".into(),
        0xc000_0094 => "an integer division by zero".into(),
        0xc000_001d => "an illegal instruction".into(),
        0xc000_0374 => "a corrupted heap".into(),
        0xe06d_7363 => "a C++ exception".into(),
        0x8000_0003 => "a breakpoint".into(),
        _ => "an exception".into(),
    };
    format!("{code:#010x}, {name}")
}

/// The thread a crash is recorded on.
fn thread() -> String {
    let id = crate::thread_id();
    if id == crate::main_thread() {
        format!("the main thread ({id})")
    } else if crate::drawing::is_drawing_thread() {
        format!("the drawing thread ({id})")
    } else {
        format!("thread {id}")
    }
}

/// The text at `address`, a C string of the game's.
fn text(m: &Memory, address: usize) -> String {
    let mut bytes = [0u8; 64];
    if m.read_into(address, &mut bytes).is_err() {
        return "a crash".into();
    }
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// The exception's code, its parameters and its context, read from its
/// pointers with reads that cannot fault.
fn read_exception(m: &Memory, pointers: usize) -> Option<(u32, u64, Vec<u64>, Context)> {
    let record = m.u64(pointers).ok()? as usize;
    let context_at = m.u64(pointers + 8).ok()? as usize;
    let code = m.u32(record).ok()?;
    let address = m.u64(record + 0x10).ok()?;
    let count = (m.u32(record + 0x18).ok()? as usize).min(15);
    let parameters = (0..count).map(|i| m.u64(record + 0x20 + 8 * i).unwrap_or(0)).collect();
    let mut context = Context::zeroed();
    let bytes = unsafe { std::slice::from_raw_parts_mut(&mut context.0 as *mut CONTEXT as *mut u8, size_of::<CONTEXT>()) };
    m.read_into(context_at, bytes).ok()?;
    Some((code, address, parameters, context))
}

/// The game's crash handling begins to write a crash's files.
fn crashed(call: &mut Call) {
    let m = call.memory();
    let what = text(&m, call.arg(0));
    let date = match crate::game::world_context_any(&m) {
        Ok(world) => format!(", the game at {}", eu4_harmony_checks::Date(world.date)),
        Err(_) => String::new(),
    };
    let mut frames = [0u64; FRAMES];
    let line = match read_exception(&m, call.arg(1)) {
        Some((code, address, parameters, context)) => {
            crate::stack::walk(&context.0, &mut frames);
            format!("{what}: {} at {}", exception(code, &parameters), crate::stack::place(address))
        }
        None => {
            // No exception: the calls that came to the crash handling.
            crate::stack::walk(&Context::capture().0, &mut frames);
            what
        }
    };
    crate::log::write_within(format_args!("crash: {line}, on {}{date}", thread()), LOG_WAIT);
    crate::log::write_within(format_args!("crash: in the calls {}", crate::stack::describe(&frames)), LOG_WAIT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exception_is_named_with_what_it_did_where() {
        assert_eq!(exception(0xc000_0005, &[0, 0x10]), "0xc0000005, an access violation reading 0x10");
        assert_eq!(exception(0xc000_0005, &[1, 0x7ff0]), "0xc0000005, an access violation writing 0x7ff0");
        assert_eq!(exception(0xc000_0005, &[8, 0x20]), "0xc0000005, an access violation executing 0x20");
        assert_eq!(exception(0xc000_0005, &[]), "0xc0000005, an access violation");
        assert_eq!(exception(0xc000_00fd, &[]), "0xc00000fd, a stack overflow");
        assert_eq!(exception(0x1234, &[]), "0x00001234, an exception");
    }

    #[test]
    fn a_crash_s_exception_is_read_from_its_pointers() {
        let mut context = Context::zeroed();
        context.0.Rip = 0x1234;
        let mut record = [0u64; 20];
        record[0] = 0xc000_0005;
        record[2] = 0x5678;
        record[3] = 2;
        record[4] = 1;
        record[5] = 0x10;
        let pointers = [record.as_ptr() as usize, &context.0 as *const CONTEXT as usize];
        let m = Memory { base: 0 };
        let (code, address, parameters, read) = read_exception(&m, pointers.as_ptr() as usize).unwrap();
        assert_eq!((code, address, parameters, read.0.Rip), (0xc000_0005, 0x5678, vec![1, 0x10], 0x1234));
        assert!(read_exception(&m, 0).is_none(), "no pointers, no exception");
    }
}
