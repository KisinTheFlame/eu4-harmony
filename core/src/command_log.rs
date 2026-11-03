//! Logs every command the game executes, in its written form, for finding out
//! what a command carries (the `trace` feature: a player's build has none of
//! this). Set `EU4HARMONY_COMMANDS` before the game starts: to `all`, or to
//! the commands' type names, comma-separated (`buildunit,move_order`). With
//! `EU4HARMONY_COMMANDS_READ` set too, each logged command's text is read
//! back into a new command with the game's text reader and written again,
//! and the two texts are compared; the new command is destroyed unexecuted.
//! `EU4HARMONY_COMMANDS_TEXTS` and `EU4HARMONY_COMMANDS_POST` name files of
//! commands in text, one a line, which at the first `daily_tick` are read
//! back and logged, and read, checked and posted to the session.
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
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

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
// own points, and these are the trace build's, checked as they are installed.
const STREAM_VTABLE: usize = 0x1dc0358;
const WRITER_VTABLE: usize = 0x1dbf7d8;

/// Reading back, as the local handler's post does: a text tokenizer (its base
/// `(tokenizer, stream, owns)`, then its own vtable), a reader `(reader,
/// tokenizer, vector, name, owns)` with its destructor, the reader's first
/// token, read into `+0x30`, and the factory `(factory, reader, token)`, an
/// object of its own. A command reads itself at `+0x18`, `(command, reader)`,
/// and is destroyed at `+0`.
const TOKENIZER: usize = 0x170d1d0;
const TEXT_TOKENIZER_VTABLE: usize = 0x1dbf958;
const TOKENIZER_SIZE: usize = 0x220;
const READER: usize = 0x17096f0;
const END_READER: usize = 0x1709950;
const READER_SIZE: usize = 0x6a8;
const READ_TOKEN: usize = 0x170ada0;
const READER_TOKEN: usize = 0x30;
const MAKE: usize = 0x14c6500;
const FACTORY_VTABLE: usize = 0x1c448b8;
const READ_BLOCK: usize = 0x18;
/// A command's check, `(command)`, which the session asks before it executes
/// the command, and the framework's recruit before it posts it.
const CHECK: usize = 0x40;

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
static READ: AtomicBool = AtomicBool::new(false);
static READ_BROKEN: AtomicBool = AtomicBool::new(false);
static READ_SAME: AtomicUsize = AtomicUsize::new(0);
static READ_DIFFERENT: AtomicUsize = AtomicUsize::new(0);
static READ_NONE: AtomicUsize = AtomicUsize::new(0);
/// How many of each type's differing read-backs were logged in full.
static DIFFERENT_SHOWN: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);
const SHOWN: usize = 3;
static LOGGED: AtomicUsize = AtomicUsize::new(0);
/// The time spent logging, in microseconds, logged every [`EVERY`] commands.
static SPENT: AtomicUsize = AtomicUsize::new(0);
const EVERY: usize = 1000;

pub fn points() -> Vec<Point> {
    let Ok(spec) = std::env::var("EU4HARMONY_COMMANDS") else { return Vec::new() };
    let names: Vec<String> = spec.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != "all").collect();
    log!("commands: logging {}", if names.is_empty() { "every executed command".to_string() } else { names.join(", ") });
    FILTER.get_or_init(|| (!names.is_empty()).then_some(names));
    let read = std::env::var_os("EU4HARMONY_COMMANDS_READ").is_some();
    READ.store(read, Ordering::Relaxed);
    if read {
        log!("commands: reading each logged command back from its text");
    }
    let mut points = vec![
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
    ];
    if read {
        points.extend([
            Point::keep(TOKENIZER, "48 89 5c 24 08 57 48 83 ec 20 48 89 51 08 48 8d 05 bb 27 6b 00 48 89 01 41 0f b6 d8", "make a tokenizer"),
            Point::keep(0x8ded9usize, "48 8d 05 78 1a d3 01", "the text tokenizer").uses(&[TEXT_TOKENIZER_VTABLE]),
            Point::keep(READER, "48 89 5c 24 10 4c 89 44 24 18 48 89 4c 24 08 55 56 57 48 83 ec 20 49 8b f9 49 8b f0", "make a reader"),
            Point::keep(END_READER, "40 53 48 83 ec 20 80 b9 80 06 00 00 00 48 8d 05 6c 5e 6b 00", "end a reader"),
            Point::keep(0x17096b0usize, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 8c 02 00 00 f6 c3 01 74 0d ba a8 06", "a reader's size"),
            Point::keep(READ_TOKEN, "48 89 5c 24 10 57 48 81 ec 30 02 00 00 48 8b 59 28 48 8b f9", "read a token"),
            Point::keep(MAKE, "48 89 5c 24 08 55 56 57 48 8d 6c 24 c0 48 81 ec 40 01 00 00 41 8b f8", "make a command"),
            Point::keep(0x15809afusize, "48 8d 05 02 3f 6c 00", "the command factory").uses(&[FACTORY_VTABLE]),
        ]);
    }
    points
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

/// A memory stream over the byte vector `bytes` (begin, end, capacity).
fn memory_stream(m: &Memory, bytes: &mut [usize; 3], writing: bool) -> Object<{ stream::SIZE }> {
    let mut s = Object::new();
    s.set(0, &(m.base + STREAM_VTABLE).to_le_bytes());
    s.set(stream::LINE, &1u32.to_le_bytes());
    s.set(stream::MODE, &0u32.to_le_bytes());
    s.set(stream::WRITING, &(writing as u32).to_le_bytes());
    // An empty name, its text held inline.
    s.set(stream::NAME + game_string::CAPACITY, &(game_string::INLINE as u64).to_le_bytes());
    s.set(stream::BYTES, &(bytes.as_mut_ptr() as usize).to_le_bytes());
    s
}

/// The command at `command` as the game writes it in text.
fn written(m: &Memory, command: usize, token: u32) -> crate::Result<Vec<u8>> {
    let mut bytes = [0usize; 3];
    let mut stream = memory_stream(m, &mut bytes, true);
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

/// The command `text` holds, read by the game's text reader, with the type
/// token read; `None` when the factory made no command. The command is the
/// caller's to post or destroy.
fn read_command(m: &Memory, text: &str) -> crate::Result<Option<(u32, usize)>> {
    let start = text.as_ptr() as usize;
    let mut bytes = [start, start + text.len(), start + text.len()];
    let mut stream = memory_stream(m, &mut bytes, false);
    let mut tokenizer = Object::<TOKENIZER_SIZE>::new();
    let mut reader = Object::<READER_SIZE>::new();
    // An empty vector and an empty name for the reader.
    let mut vector = [0usize; 3];
    let mut name = Object::<{ game_string::OBJECT }>::new();
    name.set(game_string::CAPACITY, &(game_string::INLINE as u64).to_le_bytes());
    let mut factory = (m.base + FACTORY_VTABLE).to_le_bytes();
    let (tokenizer, reader) = (tokenizer.at(), reader.at());
    m.call(m.base + TOKENIZER, &[tokenizer, stream.at(), 0])?;
    unsafe { *(tokenizer as *mut usize) = m.base + TEXT_TOKENIZER_VTABLE };
    let made = m.call(m.base + READER, &[reader, tokenizer, vector.as_mut_ptr() as usize, name.at(), 0]).and_then(|_| {
        let made = m.call(m.base + READ_TOKEN, &[reader]).and_then(|_| {
            let token = m.u32(reader + READER_TOKEN)?;
            let command = m.call(m.base + MAKE, &[factory.as_mut_ptr() as usize, reader, token as usize])?;
            if command != 0 {
                if let Err(e) = m.call(m.vslot(command, READ_BLOCK)?, &[command, reader]) {
                    m.call(m.vslot(command, 0)?, &[command, 1])?;
                    return Err(e);
                }
            }
            Ok((token, command))
        });
        m.call(m.base + END_READER, &[reader])?;
        made
    });
    m.call(m.vslot(tokenizer, 0)?, &[tokenizer, 0])?;
    let (token, command) = made?;
    Ok((command != 0).then_some((token, command)))
}

/// The command `command` written again by its own type.
fn written_again(m: &Memory, command: usize) -> crate::Result<Vec<u8>> {
    let token = m.call(m.vslot(command, TYPE)?, &[command])?;
    written(m, command, token as u32)
}

/// The command `text` holds, read by the game's text reader and written
/// again, with the type token read; `None` when the factory made no command.
fn read_back(m: &Memory, text: &str) -> crate::Result<Option<(u32, Vec<u8>)>> {
    let Some((token, command)) = read_command(m, text)? else { return Ok(None) };
    let again = written_again(m, command);
    m.call(m.vslot(command, 0)?, &[command, 1])?;
    Ok(Some((token, again?)))
}

/// Reads each line of the file `EU4HARMONY_COMMANDS_POST` names into a
/// command, once, asks the command's check and, when it passes, posts it to
/// the session as the framework's recruit does; logs what became of each.
fn post_texts(m: &Memory) {
    let Some(path) = std::env::var_os("EU4HARMONY_COMMANDS_POST") else { return };
    let texts = match std::fs::read_to_string(&path) {
        Ok(texts) => texts,
        Err(e) => return log!("commands: could not read {}: {e}", path.to_string_lossy()),
    };
    let session = match crate::game::context_any(m).and_then(|ctx| crate::game::command_session(m, &ctx)) {
        Ok(session) => session,
        Err(e) => return log!("commands: no session to post to: {}", e.message),
    };
    for text in texts.lines().filter(|l| !l.trim().is_empty()) {
        match post_text(m, session, text) {
            Ok(what) => log!("text posted: {text:?}: {what}"),
            Err(e) => log!("text posted: {text:?} failed: {e}"),
        }
    }
}
static TEXTS_POSTED: AtomicBool = AtomicBool::new(false);

fn post_text(m: &Memory, session: usize, text: &str) -> crate::Result<String> {
    let Some((_, command)) = read_command(m, text)? else { return Ok("made no command".to_string()) };
    let read = written_again(m, command).map(|t| one_line(&t)).unwrap_or_else(|e| format!("(not written: {e})"));
    let checked = m.call(m.vslot(command, CHECK)?, &[command])? as u8 != 0;
    let taken = checked && crate::game::post(m, session, command).map_err(|e| e.message)?;
    if !taken {
        m.call(m.vslot(command, 0)?, &[command, 1])?;
    }
    let what = match (checked, taken) {
        (true, true) => "its check passed; the session took it",
        (false, _) => "its check refused it",
        (true, false) => "its check passed; the session did not take it",
    };
    Ok(format!("read as {read}; {what}"))
}

/// Reads back each line of the file `EU4HARMONY_COMMANDS_TEXTS` names, once,
/// and logs what each line became.
fn read_texts(m: &Memory) {
    let Some(path) = std::env::var_os("EU4HARMONY_COMMANDS_TEXTS") else { return };
    let texts = match std::fs::read_to_string(&path) {
        Ok(texts) => texts,
        Err(e) => return log!("commands: could not read {}: {e}", path.to_string_lossy()),
    };
    for text in texts.lines().filter(|l| !l.trim().is_empty()) {
        match read_back(m, text) {
            Ok(None) => log!("text read back: {text:?} made no command"),
            Ok(Some((token, again))) => log!("text read back: {text:?} as token {token}, written again: {:?}", String::from_utf8_lossy(&again)),
            Err(e) => log!("text read back: {text:?} failed: {e}"),
        }
    }
}
static TEXTS_READ: AtomicBool = AtomicBool::new(false);

/// Reads the command `text` back and counts whether it came back the same.
fn check_read_back(m: &Memory, n: usize, name: &str, text: &str) {
    match read_back(m, text) {
        Ok(None) => {
            READ_NONE.fetch_add(1, Ordering::Relaxed);
            log!("command #{n} {name}: read back, the factory made no command");
        }
        Ok(Some((token, again))) => {
            let again = one_line(&again);
            if again == text {
                READ_SAME.fetch_add(1, Ordering::Relaxed);
                return;
            }
            READ_DIFFERENT.fetch_add(1, Ordering::Relaxed);
            let mut shown = DIFFERENT_SHOWN.lock().unwrap_or_else(|e| e.into_inner());
            let count = shown.get_or_insert_with(HashMap::new).entry(name.to_string()).or_insert(0);
            *count += 1;
            if *count <= SHOWN {
                log!("command #{n} {name}: read back as token {token}, written again differently: {again}");
            }
        }
        Err(e) => {
            READ_BROKEN.store(true, Ordering::Relaxed);
            log!("command #{n} {name}: could not be read back ({e}); no more are read back");
        }
    }
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
        if name == "daily_tick" && !TEXTS_READ.swap(true, Ordering::Relaxed) {
            read_texts(&m);
        }
        if name == "daily_tick" && !TEXTS_POSTED.swap(true, Ordering::Relaxed) {
            post_texts(&m);
        }
        if let Some(Some(names)) = FILTER.get() {
            if !names.contains(&name) {
                return;
            }
        }
        let id = m.u32(command + ID).unwrap_or(0);
        let n = LOGGED.fetch_add(1, Ordering::Relaxed);
        match written(&m, command, token) {
            Ok(text) => {
                let text = one_line(&text);
                log!("command #{n} id {id} {name}: {text}");
                if READ.load(Ordering::Relaxed) && !READ_BROKEN.load(Ordering::Relaxed) {
                    check_read_back(&m, n, &name, &text);
                }
            }
            Err(e) => {
                BROKEN.store(true, Ordering::Relaxed);
                log!("command #{n} id {id} {name}: could not be written ({e}); no more commands are logged");
            }
        }
        let spent = SPENT.fetch_add(start.elapsed().as_micros() as usize, Ordering::Relaxed);
        if (n + 1).is_multiple_of(EVERY) {
            log!("commands: {} logged, {} us each on average", n + 1, spent / (n + 1));
            if READ.load(Ordering::Relaxed) {
                let (same, different, none) = (READ_SAME.load(Ordering::Relaxed), READ_DIFFERENT.load(Ordering::Relaxed), READ_NONE.load(Ordering::Relaxed));
                log!("commands: read back {same} the same, {different} differently, {none} made none");
            }
        }
    })
}
