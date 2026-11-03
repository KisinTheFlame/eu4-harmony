//! Logs every command the game executes, in its written form, for finding out
//! what a command carries (the `dev-command-log` feature: a player's build has
//! none of this). dev.toml's `[command-log]` can name the types to log, by the
//! game's names (`types = ["buildunit", "move_order"]`); every type is logged
//! when it names none.
//!
//! The session's call of each command's execute is observed, so a command is
//! logged once it has passed its check and just before it runs, the player's,
//! the AI's and the game's own alike ([commands.md]). The command writes
//! itself with the game's own writer, in text instead of the binary the
//! session writes it in; the text goes into a memory stream laid out as the
//! local handler's post lays out its own.
//!
//! [commands.md]: ../../docs/research/commands.md
use crate::game_string;
use crate::hooks::HookFrame;
use crate::memory::Memory;
use crate::points::Point;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;

/// `(lambda)`: the session's call of a command's execute; the command is at
/// the lambda's `+8`.
const EXECUTE: usize = 0x14cea10;
const LAMBDA_COMMAND: usize = 0x8;
/// A command's type token (`+0x50`), its write as a block (`+0x08`, `(command,
/// writer)`), its ID.
const TYPE: usize = 0x50;
const WRITE_BLOCK: usize = 0x08;
const ID: usize = 0x4c;
/// `(writer, token)`: writes a token as a key.
const WRITE_KEY: usize = 0x1709280;
/// `(block, size)`: the game's `operator delete`.
const DELETE: usize = 0x17394f0;
/// The token table: whether it is set up, its last index, and its names, one
/// `std::string` each.
const TOKENS_READY: usize = 0x2357b30;
const TOKENS_LAST: usize = 0x2357b28;
const TOKEN_NAMES: usize = 0x242b888;
/// The memory stream's and the writer's vtables.
//
// Addresses are the base plus these: `Memory::at` knows only the framework's
// own points, and these are the command log's, checked as they are installed.
const STREAM_VTABLE: usize = 0x1dc0358;
const WRITER_VTABLE: usize = 0x1dbf7d8;

/// The memory stream, as the local handler's post makes it: a mode, a name,
/// and the byte vector it writes to or reads from.
mod stream {
    pub const SIZE: usize = 0x58;
    pub const LINE: usize = 0x08;
    pub const MODE: usize = 0x10;
    /// 1 to write, 0 to read.
    pub const WRITING: usize = 0x14;
    pub const NAME: usize = 0x18;
    pub const BYTES: usize = 0x38;
}
/// The writer: its depth, its stream, and whether it writes binary.
mod writer {
    pub const SIZE: usize = 0x20;
    pub const STREAM: usize = 0x10;
    pub const BINARY: usize = 0x18;
}

/// The game's objects are built in these, aligned as the game's own are.
#[repr(C, align(16))]
struct Object<const N: usize>([u8; N]);

impl<const N: usize> Object<N> {
    fn new() -> Self {
        Object([0; N])
    }
    fn set(&mut self, offset: usize, value: &[u8]) {
        self.0[offset..offset + value.len()].copy_from_slice(value);
    }
    fn at(&mut self) -> usize {
        self.0.as_mut_ptr() as usize
    }
}

static FILTER: OnceLock<Option<Vec<String>>> = OnceLock::new();
static BROKEN: AtomicBool = AtomicBool::new(false);
static LOGGED: AtomicUsize = AtomicUsize::new(0);
/// The time spent logging, in microseconds, logged every [`EVERY`] commands.
static SPENT: AtomicUsize = AtomicUsize::new(0);
const EVERY: usize = 1000;

pub fn points() -> Vec<Point> {
    let names = &crate::dev::options().command_log.types;
    log!("commands: logging {}", if names.is_empty() { "every executed command".to_string() } else { names.join(", ") });
    FILTER.get_or_init(|| (!names.is_empty()).then(|| names.clone()));
    vec![
        Point::observe(
            EXECUTE,
            "48 89 5c 24 10 48 89 74 24 18 57 48 83 ec 30 48 8b f1 48 8b 49 08 48 8b 01 ff 50 50",
            "command execution",
            1,
            Some(before),
            None,
        ),
        Point::keep(0x14cea3cusize, "3b 1d e6 90 e8 00 7f 14 85 db 78 10 48 8b fb 48 c1 e7 05 48 03 3d 32 ce f5 00", "command type names")
            .uses(&[TOKENS_LAST, TOKEN_NAMES]),
        Point::keep(0x14cea2eusize, "80 3d fb 90 e8 00 00", "token table set up").uses(&[TOKENS_READY]),
        Point::keep(0x15804f9usize, "48 8d 3d 58 fe 83 00 48 89 7c 24 50 4c 8d 25 cc f2 83 00", "command stream and writer")
            .uses(&[STREAM_VTABLE, WRITER_VTABLE]),
        Point::keep(WRITE_KEY, "48 89 5c 24 08 57 48 81 ec 30 02 00 00 8b da 48 8b f9 e8 c9 f2 ff ff 80 7f 18 00", "write a key"),
        Point::keep(DELETE, "e9 2b 45 02 00", "operator delete"),
    ]
}

/// The name of type token `token`.
fn type_name(m: &Memory, token: u32) -> Option<String> {
    if m.u8(m.base + TOKENS_READY).ok()? == 0 || token as i32 > m.i32(m.base + TOKENS_LAST).ok()? {
        return None;
    }
    let names = m.ptr(m.base + TOKEN_NAMES).ok()?;
    let bytes = game_string::read(m, names + token as usize * game_string::OBJECT).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// A memory stream writing to the byte vector `bytes` (begin, end, capacity).
fn memory_stream(m: &Memory, bytes: &mut [usize; 3]) -> Object<{ stream::SIZE }> {
    let mut s = Object::new();
    s.set(0, &(m.base + STREAM_VTABLE).to_le_bytes());
    s.set(stream::LINE, &1u32.to_le_bytes());
    s.set(stream::MODE, &0u32.to_le_bytes());
    s.set(stream::WRITING, &1u32.to_le_bytes());
    // An empty name, its text held inline.
    s.set(stream::NAME + game_string::CAPACITY, &(game_string::INLINE as u64).to_le_bytes());
    s.set(stream::BYTES, &(bytes.as_mut_ptr() as usize).to_le_bytes());
    s
}

/// The command at `command` as the game writes it in text.
fn written(m: &Memory, command: usize, token: u32) -> crate::Result<Vec<u8>> {
    let mut bytes = [0usize; 3];
    let mut stream = memory_stream(m, &mut bytes);
    let mut writer = Object::<{ writer::SIZE }>::new();
    writer.set(0, &(m.base + WRITER_VTABLE).to_le_bytes());
    writer.set(writer::STREAM, &stream.at().to_le_bytes());
    writer.0[writer::BINARY] = 0;
    let writer = writer.at();
    let result = m.call(m.base + WRITE_KEY, &[writer, token as usize]).and_then(|_| m.call(m.vslot(command, WRITE_BLOCK)?, &[command, writer])).map(|_| ());
    // The game grew the vector through the stream.
    let bytes = unsafe { std::ptr::read_volatile(&bytes) };
    let result = result.and_then(|_| m.bytes(bytes[0], bytes[1] - bytes[0]));
    if bytes[0] != 0 {
        // As the game frees a vector's buffer: a large one is aligned, with
        // the block it was cut from just before it.
        let mut size = bytes[2] - bytes[0];
        let mut block = bytes[0];
        if size >= 0x1000 {
            size += 0x27;
            block = m.ptr(block - 8)?;
        }
        m.call(m.base + DELETE, &[block, size])?;
    }
    result
}

/// `text` on one line.
fn one_line(text: &[u8]) -> String {
    String::from_utf8_lossy(text).split_whitespace().collect::<Vec<_>>().join(" ")
}

extern "C" fn before(frame: *mut HookFrame) {
    crate::guarded("commands", || {
        if BROKEN.load(Ordering::Relaxed) {
            return;
        }
        let start = std::time::Instant::now();
        let m = crate::hooked_memory();
        let Ok(command) = m.ptr(unsafe { (*frame).args[0] } + LAMBDA_COMMAND) else { return };
        let Ok(token) = m.vslot(command, TYPE).and_then(|f| m.call(f, &[command])) else { return };
        let token = token as u32;
        let name = type_name(&m, token).unwrap_or_else(|| format!("token {token}"));
        if let Some(Some(names)) = FILTER.get() {
            if !names.contains(&name) {
                return;
            }
        }
        let id = m.u32(command + ID).unwrap_or(0);
        let n = LOGGED.fetch_add(1, Ordering::Relaxed);
        match written(&m, command, token) {
            Ok(text) => log!("command #{n} id {id} {name}: {}", one_line(&text)),
            Err(e) => {
                BROKEN.store(true, Ordering::Relaxed);
                log!("command #{n} id {id} {name}: could not be written ({e}); no more commands are logged");
            }
        }
        let spent = SPENT.fetch_add(start.elapsed().as_micros() as usize, Ordering::Relaxed);
        if (n + 1).is_multiple_of(EVERY) {
            log!("commands: {} logged, {} us each on average", n + 1, spent / (n + 1));
        }
    })
}
