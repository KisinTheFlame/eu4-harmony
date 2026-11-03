//! The command layer (ADR 0032, docs/commands.md): posts one of the game's
//! commands from its written form, through the game's own reader, check and
//! post, as the local handler reads back every command it posts; and hears
//! every command the game executes, in the same form
//! (docs/research/commands.md). Its vocabulary, the commands' type names and
//! keys, is the game's and stays in the core: capabilities write their
//! command's text and post it here.
//!
//! The game's reader takes almost any text and says nothing of what it could
//! not read, so [`post`] writes the command it read out again and refuses it
//! unless every key given came back with the value given.
use super::{command_session, context, fail, game, Fail, GResult, Status};
use crate::{game_string, hooks::HookFrame, memory::Memory, points::Point, unicode::game_text};
use std::cell::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// `(lambda)`: the session's call of a command's execute, for every command
/// that passed its check, before it runs; the command is at the lambda's
/// `+8`.
const EXECUTE: usize = 0x14cea10;
const LAMBDA_COMMAND: usize = 0x8;
/// A command's slots: `+0` destroys it (`(command, flags)`, flag 1 freeing
/// it), `+0x08` writes it as a block (`(command, writer)`), `+0x18` reads it
/// (`(command, reader)`), `+0x40` its check (`(command)`, whether the game
/// would carry it out now), `+0x50` its type token; `+0x4c` is its ID.
const DESTROY: usize = 0x0;
const FREE: usize = 1;
const WRITE_BLOCK: usize = 0x08;
const READ_BLOCK: usize = 0x18;
const CHECK: usize = 0x40;
const TYPE: usize = 0x50;
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
/// The memory stream's and the writer's vtables, as the local handler's post
/// makes them; the stream's `+0x18` sets its mode and whether it writes.
const STREAM_VTABLE: usize = 0x1dc0358;
const WRITER_VTABLE: usize = 0x1dbf7d8;
const STREAM_MODE: usize = 0x171ed90;
/// Reading, as the local handler's post reads back what it wrote: a text
/// tokenizer (its base `(tokenizer, stream, owns)`, then its own vtable, whose
/// `+0` destroys it, flag 0 leaving the stream), a reader `(reader,
/// tokenizer, strings, name, owns)` with its destructor, the reader's first
/// token, read into `+0x30`, and the factory `(factory, reader, token)`, an
/// object of its own, which makes an empty command of the token's type.
const TOKENIZER: usize = 0x170d1d0;
const TEXT_TOKENIZER_VTABLE: usize = 0x1dbf958;
const TEXT_TOKENIZER_DESTROY: usize = 0x170ccb0;
const READER: usize = 0x17096f0;
const END_READER: usize = 0x1709950;
const READ_TOKEN: usize = 0x170ada0;
const READER_TOKEN: usize = 0x30;
const MAKE: usize = 0x14c6500;
const FACTORY_VTABLE: usize = 0x1c448b8;
/// The local handler's post, which writes, reads back and destroys every
/// command posted: the reader made and read, the command made and read, and
/// the posted one destroyed.
const POST_WRITES: usize = 0x158053d;
const POST_READS: usize = 0x158097a;
const POST_DESTROYS: usize = 0x1580c4a;
/// The session's update asking a queued command's check.
const SESSION_CHECKS: usize = 0x14cab2a;
/// Where the token read stores the token in the reader.
const TOKEN_STORED: usize = 0x170ae2c;

/// The memory stream, as the local handler's post makes it: a line, a mode,
/// whether it writes, a name, and the byte vector it writes to or reads from.
mod stream {
    pub const SIZE: usize = 0x58;
    pub const LINE: usize = 0x08;
    pub const MODE: usize = 0x10;
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
/// The tokenizer's and the reader's sizes: the post makes the tokenizer in
/// the 0x220 bytes before its reader, clearing its 0x204 from `+0x14`; the
/// reader's own deleting destructor frees 0x6a8.
const TOKENIZER_SIZE: usize = 0x220;
const TOKENIZER_CLEARED: usize = 0x204;
const READER_SIZE: usize = 0x6a8;
const READER_DELETE: usize = 0x17096b0;

pub fn points() -> Vec<Point> {
    vec![
        Point::observe(
            EXECUTE,
            "48 89 5c 24 10 48 89 74 24 18 57 48 83 ec 30 48 8b f1 48 8b 49 08 48 8b 01 ff 50 50",
            "command execution",
            1,
            Some(executing),
            None,
        )
        .uses(&[LAMBDA_COMMAND, TYPE]),
        Point::keep(0x14cea3cusize, "3b 1d e6 90 e8 00 7f 14 85 db 78 10 48 8b fb 48 c1 e7 05 48 03 3d 32 ce f5 00", "command type names")
            .uses(&[TOKENS_LAST, TOKEN_NAMES]),
        Point::keep(0x14cea2eusize, "80 3d fb 90 e8 00 00", "token table set up").uses(&[TOKENS_READY]),
        Point::keep(0x15804f9usize, "48 8d 3d 58 fe 83 00 48 89 7c 24 50 4c 8d 25 cc f2 83 00", "command stream and writer")
            .uses(&[STREAM_VTABLE, WRITER_VTABLE]),
        Point::method(STREAM_VTABLE, 0x18, STREAM_MODE, "memory stream mode"),
        Point::keep(STREAM_MODE, "89 51 10 4c 8b c9 44 89 41 14 66 c7 41 0c 00 00 c7 41 08 01 00 00 00", "memory stream mode").uses(&[
            stream::MODE,
            stream::WRITING,
            stream::LINE,
        ]),
        Point::keep(WRITE_KEY, "48 89 5c 24 08 57 48 81 ec 30 02 00 00 8b da 48 8b f9 e8 c9 f2 ff ff 80 7f 18 00", "write a key").uses(&[writer::BINARY]),
        Point::keep(DELETE, "e9 2b 45 02 00", "operator delete"),
        Point::keep(
            POST_WRITES,
            "48 8b 03 48 8b cb ff 50 50 8b d0 48 8d 8c 24 10 01 00 00 e8 2b 8d 18 00 48 8b 03 48 8d 94 24 10 01 00 00 48 8b cb ff 50 08",
            "a posted command written",
        )
        .uses(&[TYPE, WRITE_BLOCK]),
        Point::keep(
            POST_READS,
            "c6 44 24 20 00 4d 8d 8c 24 08 01 00 00 4c 8d 44 24 30 48 8d 94 24 e0 02 00 00 48 8d 8c 24 00 05 00 00 e8 4f 8d 18 00 90 \
             48 8d 8c 24 00 05 00 00 e8 f1 a3 18 00 48 8d 05 02 3f 6c 00 48 89 44 24 48 44 8b 84 24 30 05 00 00 48 8d 94 24 00 05 00 00 \
             48 8d 4c 24 48 e8 2b 5b f4 ff 48 8b d8 48 85 c0 74 16 48 8b 08 4c 8b 41 18 48 8d 94 24 00 05 00 00 48 8b c8 41 ff d0 90",
            "a posted command read back",
        )
        .uses(&[FACTORY_VTABLE, READ_BLOCK]),
        Point::keep(0x1580820usize, "33 d2 41 b8 04 02 00 00 48 8d 8c 24 f4 02 00 00 e8 cb 55 4b 00", "a posted command's tokenizer cleared")
            .uses(&[TOKENIZER_CLEARED]),
        Point::keep(POST_DESTROYS, "48 8b 8c 24 f8 0b 00 00 48 8b 01 ba 01 00 00 00 ff 10", "a posted command destroyed").uses(&[FREE]),
        Point::keep(SESSION_CHECKS, "48 8b 06 48 8b ce ff 50 40", "a queued command checked").uses(&[CHECK]),
        Point::keep(TOKENIZER, "48 89 5c 24 08 57 48 83 ec 20 48 89 51 08 48 8d 05 bb 27 6b 00 48 89 01 41 0f b6 d8", "make a tokenizer"),
        Point::keep(0x8ded9usize, "48 8d 05 78 1a d3 01", "the text tokenizer").uses(&[TEXT_TOKENIZER_VTABLE]),
        Point::method(TEXT_TOKENIZER_VTABLE, 0, TEXT_TOKENIZER_DESTROY, "text tokenizer destructor"),
        Point::keep(READER, "48 89 5c 24 10 4c 89 44 24 18 48 89 4c 24 08 55 56 57 48 83 ec 20 49 8b f9 49 8b f0", "make a reader"),
        Point::keep(END_READER, "40 53 48 83 ec 20 80 b9 80 06 00 00 00 48 8d 05 6c 5e 6b 00", "end a reader"),
        Point::keep(READER_DELETE, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 8c 02 00 00 f6 c3 01 74 0d ba a8 06 00 00", "a reader's size")
            .uses(&[READER_SIZE]),
        Point::keep(READ_TOKEN, "48 89 5c 24 10 57 48 81 ec 30 02 00 00 48 8b 59 28 48 8b f9", "read a token"),
        Point::keep(TOKEN_STORED, "48 8d 57 30", "a token stored in its reader").uses(&[READER_TOKEN]),
        Point::keep(MAKE, "48 89 5c 24 08 55 56 57 48 8d 6c 24 c0 48 81 ec 40 01 00 00 41 8b f8", "make a command"),
    ]
}

/// Posts the command `text` writes, such as `buildunit={ country="FRA"
/// province=183 unit="western_medieval_infantry" }`, to the game's session,
/// as the game's interface posts its own; it runs at the game's next frame,
/// for whichever country the text names. A key left out takes the game's
/// default. Refused, the command not posted: [`Status::InvalidArgument`]
/// when the text names no command of the game's or a key given did not come
/// back from the game's reader with the value given; [`Status::Rejected`]
/// when the command's own check refuses it, which gives no reason, or the
/// session takes no commands now.
pub fn post(text: &str) -> GResult<()> {
    let given = Written::parse(text).map_err(|e| Fail { status: Status::InvalidArgument, message: e })?;
    let m = game()?;
    let ctx = context(m)?;
    // The session before the command: once the command is made, only a call
    // into the game can fail before it is posted or destroyed. The game owns
    // it once its session took it; until then, it is the framework's to
    // free. A command a call into the game failed on is left alone.
    let session = command_session(m, &ctx)?;
    let Some(command) = read(m, &game_text::from_str(text))? else {
        return fail(Status::InvalidArgument, format!("the game has no command {:?}", given.kind));
    };
    let refusal = match refused(m, command, &given)? {
        Some(refusal) => Some(refusal),
        None if !super::post(m, session, command)? => Some(Fail { status: Status::Rejected, message: "the game takes no commands now".into() }),
        None => None,
    };
    match refusal {
        Some(refusal) => {
            m.call(m.vslot(command, DESTROY)?, &[command, FREE])?;
            Err(refusal)
        }
        None => Ok(()),
    }
}

/// Posts dev.toml's test commands (`[test-switches]` `post`), at the first
/// frame of a game, and logs what came of each.
#[cfg(feature = "dev-test-switches")]
pub fn post_test_commands() {
    for text in &crate::dev::options().test_switches.post {
        match post(text) {
            Ok(()) => log!("test command posted: {text}"),
            Err(e) => log!("test command not posted ({:?}: {}): {text}", e.status, e.message),
        }
    }
}

/// Why the command read from `given` must not be posted: a key that did not
/// come back as given, or its check; None when it may be.
fn refused(m: &Memory, command: usize, given: &Written) -> GResult<Option<Fail>> {
    let token = m.call(m.vslot(command, TYPE)?, &[command])? as u32;
    let again = Written::parse(&game_text::to_utf8(&written(m, command, token)?))?;
    if let Err(e) = given.came_back(&again) {
        return Ok(Some(Fail { status: Status::InvalidArgument, message: e }));
    }
    if m.call(m.vslot(command, CHECK)?, &[command])? as u8 == 0 {
        return Ok(Some(Fail { status: Status::Rejected, message: format!("the game's {} command refuses it, and gives no reason", given.kind) }));
    }
    Ok(None)
}

/// A command the game executes, heard just before it runs.
pub struct Heard {
    m: Memory,
    command: usize,
    token: u32,
    /// Its type, by the game's name.
    pub kind: String,
    /// Its ID, which the game numbers its commands by.
    pub id: u32,
    text: OnceCell<Option<String>>,
}

impl Heard {
    /// Its written form, on one line; written the first time it is asked for,
    /// so that a listener that wants only some types costs the others
    /// nothing. None when the command could not be written.
    pub fn text(&self) -> Option<&str> {
        self.text
            .get_or_init(|| {
                if WRITING_BROKEN.load(Ordering::Relaxed) {
                    return None;
                }
                match written(&self.m, self.command, self.token) {
                    Ok(text) => Some(one_line(&game_text::to_utf8(&text))),
                    Err(e) => {
                        WRITING_BROKEN.store(true, Ordering::Relaxed);
                        log!("commands: {} #{} could not be written ({e}); no more are", self.kind, self.id);
                        None
                    }
                }
            })
            .as_deref()
    }
}

static LISTENERS: Mutex<Vec<fn(&Heard)>> = Mutex::new(Vec::new());
static LISTENING: AtomicBool = AtomicBool::new(false);
static WRITING_BROKEN: AtomicBool = AtomicBool::new(false);

/// Calls `heard` with every command the game executes, the player's, the
/// AI's and the game's own, on the main thread just before it runs. A
/// command's sender does not tell the player's commands from the AI's.
pub fn listen(heard: fn(&Heard)) {
    LISTENERS.lock().unwrap_or_else(|e| e.into_inner()).push(heard);
    LISTENING.store(true, Ordering::Relaxed);
}

extern "C" fn executing(frame: *mut HookFrame) {
    if !LISTENING.load(Ordering::Relaxed) {
        return;
    }
    crate::guarded("commands", || {
        let m = crate::hooked_memory();
        let Ok(command) = m.ptr(unsafe { (*frame).args[0] } + LAMBDA_COMMAND) else { return };
        let Ok(token) = m.vslot(command, TYPE).and_then(|f| m.call(f, &[command])) else { return };
        let token = token as u32;
        let kind = type_name(&m, token).unwrap_or_else(|| format!("token {token}"));
        let heard = Heard { m, command, token, kind, id: m.u32(command + ID).unwrap_or(0), text: OnceCell::new() };
        let listeners = LISTENERS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for listener in listeners {
            listener(&heard);
        }
    })
}

/// The name of type token `token`.
fn type_name(m: &Memory, token: u32) -> Option<String> {
    if m.u8(m.at(TOKENS_READY)).ok()? == 0 || token as i32 > m.i32(m.at(TOKENS_LAST)).ok()? {
        return None;
    }
    let names = m.ptr(m.at(TOKEN_NAMES)).ok()?;
    let bytes = game_string::read(m, names + token as usize * game_string::OBJECT).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
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

/// A memory stream over the byte vector `bytes` (begin, end, capacity), as
/// the local handler's post makes it, writing to it or reading from it.
fn memory_stream(m: &Memory, bytes: &mut [usize; 3], writing: bool) -> Object<{ stream::SIZE }> {
    let mut s = Object::new();
    s.set(0, &m.at(STREAM_VTABLE).to_le_bytes());
    s.set(stream::LINE, &1u32.to_le_bytes());
    s.set(stream::MODE, &0u32.to_le_bytes());
    s.set(stream::WRITING, &(writing as u32).to_le_bytes());
    // An empty name, its text held inline.
    s.set(stream::NAME + game_string::CAPACITY, &(game_string::INLINE as u64).to_le_bytes());
    s.set(stream::BYTES, &(bytes.as_mut_ptr() as usize).to_le_bytes());
    s
}

/// The command at `command`, of type token `token`, as the game writes it in
/// text, in game text.
fn written(m: &Memory, command: usize, token: u32) -> crate::Result<Vec<u8>> {
    let mut bytes = [0usize; 3];
    let mut stream = memory_stream(m, &mut bytes, true);
    let mut writer = Object::<{ writer::SIZE }>::new();
    writer.set(0, &m.at(WRITER_VTABLE).to_le_bytes());
    writer.set(writer::STREAM, &stream.at().to_le_bytes());
    writer.0[writer::BINARY] = 0;
    let writer = writer.at();
    let result = m.call(m.at(WRITE_KEY), &[writer, token as usize]).and_then(|_| m.call(m.vslot(command, WRITE_BLOCK)?, &[command, writer])).map(|_| ());
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
        m.call(m.at(DELETE), &[block, size])?;
    }
    result
}

/// The command `text` (game text) holds, made by the game's factory and read
/// by its text reader; None when the factory made none, the text naming no
/// command of the game's. The command is the caller's to post or destroy.
fn read(m: &Memory, text: &[u8]) -> GResult<Option<usize>> {
    let start = text.as_ptr() as usize;
    let mut bytes = [start, start + text.len(), start + text.len()];
    let mut stream = memory_stream(m, &mut bytes, false);
    let mut tokenizer = Object::<TOKENIZER_SIZE>::new();
    let mut reader = Object::<READER_SIZE>::new();
    // No strings and an empty name for the reader: the post hands it its
    // handler's and the receiver's, which reading text does without.
    let mut strings = [0usize; 3];
    let mut name = Object::<{ game_string::OBJECT }>::new();
    name.set(game_string::CAPACITY, &(game_string::INLINE as u64).to_le_bytes());
    let mut factory = m.at(FACTORY_VTABLE).to_le_bytes();
    let (tokenizer, reader) = (tokenizer.at(), reader.at());
    m.call(m.at(TOKENIZER), &[tokenizer, stream.at(), 0])?;
    unsafe { *(tokenizer as *mut usize) = m.at(TEXT_TOKENIZER_VTABLE) };
    let made = m.call(m.at(READER), &[reader, tokenizer, strings.as_mut_ptr() as usize, name.at(), 0]).and_then(|_| {
        let made = m.call(m.at(READ_TOKEN), &[reader]).and_then(|_| {
            let token = m.u32(reader + READER_TOKEN)?;
            let command = m.call(m.at(MAKE), &[factory.as_mut_ptr() as usize, reader, token as usize])?;
            if command != 0 {
                if let Err(e) = m.call(m.vslot(command, READ_BLOCK)?, &[command, reader]) {
                    m.call(m.vslot(command, DESTROY)?, &[command, FREE])?;
                    return Err(e);
                }
            }
            Ok(command)
        });
        m.call(m.at(END_READER), &[reader])?;
        made
    });
    m.call(m.at(TEXT_TOKENIZER_DESTROY), &[tokenizer, 0])?;
    let command = made?;
    Ok((command != 0).then_some(command))
}

/// `text` on one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A value in the written form: a word or a quoted text, or a block of
/// entries.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Word(String),
    Block(Vec<Entry>),
}

/// An entry of a block: `key=value`, or a value alone, as in a list.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    key: Option<String>,
    value: Value,
}

/// A command in its written form: `kind={ … }`.
#[derive(Debug, PartialEq)]
struct Written {
    kind: String,
    entries: Vec<Entry>,
}

impl Written {
    fn parse(text: &str) -> crate::Result<Self> {
        let tokens = tokens(text);
        let mut at = 0;
        let entries = block(&tokens, &mut at)?;
        if at < tokens.len() {
            return Err(format!("an unmatched }} in {text:?}"));
        }
        match <[Entry; 1]>::try_from(entries) {
            Ok([Entry { key: Some(kind), value: Value::Block(entries) }]) => Ok(Self { kind, entries }),
            _ => Err(format!("{text:?} is not one command: its type, = and a block")),
        }
    }

    /// Whether every key of this, the text given, came back in `again`, the
    /// command read from it written again, with the value given.
    fn came_back(&self, again: &Written) -> crate::Result<()> {
        if again.kind != self.kind {
            return Err(format!("the game read {} as {}", self.kind, again.kind));
        }
        came_back(&self.entries, &again.entries, &self.kind)
    }
}

/// Whether each of `given` is in `again`: the n-th entry of a key as the n-th
/// of that key, values alone in their order, words as the same text or the
/// same number, blocks alike. `path` names the block, for the refusal.
fn came_back(given: &[Entry], again: &[Entry], path: &str) -> crate::Result<()> {
    let alone = |entries: &[Entry]| entries.iter().filter(|e| e.key.is_none()).map(|e| e.value.clone()).collect::<Vec<_>>();
    let (given_alone, again_alone) = (alone(given), alone(again));
    if given_alone.len() > again_alone.len() {
        return Err(format!("{path} came back with {} values, not {}", again_alone.len(), given_alone.len()));
    }
    for (i, (g, a)) in given_alone.iter().zip(&again_alone).enumerate() {
        same(g, a, &format!("{path}'s value {}", i + 1))?;
    }
    for (i, entry) in given.iter().enumerate() {
        let Some(key) = &entry.key else { continue };
        let nth = given[..i].iter().filter(|e| e.key.as_ref() == Some(key)).count();
        let path = format!("{path}.{key}");
        match again.iter().filter(|e| e.key.as_ref() == Some(key)).nth(nth) {
            Some(back) => same(&entry.value, &back.value, &path)?,
            None => return Err(format!("{path} did not come back: the game does not read it there")),
        }
    }
    Ok(())
}

fn same(given: &Value, again: &Value, path: &str) -> crate::Result<()> {
    match (given, again) {
        (Value::Word(g), Value::Word(a)) => {
            let number = |w: &str| w.parse::<f64>().ok();
            match g == a || number(g).is_some_and(|g| number(a) == Some(g)) {
                true => Ok(()),
                false => Err(format!("the game read {path}={g} as {a}")),
            }
        }
        (Value::Block(g), Value::Block(a)) => came_back(g, a, path),
        (Value::Word(g), Value::Block(_)) => Err(format!("the game read {path}={g} as a block")),
        (Value::Block(_), Value::Word(a)) => Err(format!("the game read the block {path} as {a}")),
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    Equals,
    Open,
    Close,
}

fn tokens(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '=' => tokens.push(Token::Equals),
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '"' => tokens.push(Token::Word(chars.by_ref().take_while(|&c| c != '"').collect())),
            c => {
                let mut word = c.to_string();
                while let Some(c) = chars.next_if(|c| !c.is_whitespace() && !matches!(c, '=' | '{' | '}' | '"')) {
                    word.push(c);
                }
                tokens.push(Token::Word(word));
            }
        }
    }
    tokens
}

/// The entries from `at` to the end of their block or of the tokens.
fn block(tokens: &[Token], at: &mut usize) -> crate::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    loop {
        match tokens.get(*at) {
            None | Some(Token::Close) => return Ok(entries),
            Some(Token::Equals) => return Err("an = with no key before it".into()),
            Some(Token::Word(key)) if tokens.get(*at + 1) == Some(&Token::Equals) => {
                *at += 2;
                let value = value(tokens, at).map_err(|e| format!("{key}= {e}"))?;
                entries.push(Entry { key: Some(key.clone()), value });
            }
            Some(_) => entries.push(Entry { key: None, value: value(tokens, at)? }),
        }
    }
}

/// The value at `at`: a word, or a block to its `}`.
fn value(tokens: &[Token], at: &mut usize) -> crate::Result<Value> {
    match tokens.get(*at) {
        Some(Token::Word(word)) => {
            *at += 1;
            Ok(Value::Word(word.clone()))
        }
        Some(Token::Open) => {
            *at += 1;
            let entries = block(tokens, at)?;
            if tokens.get(*at) != Some(&Token::Close) {
                return Err("has a { with no } after it".into());
            }
            *at += 1;
            Ok(Value::Block(entries))
        }
        _ => Err("has no value after it".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECRUIT: &str = r#"buildunit={ country="FRA" province=183 unit="western_medieval_infantry" }"#;
    const WRITTEN: &str = r#"buildunit={ tickstamp=65535 istargetasynchronous=no identity=0 sender=-1 country="FRA" province=183 unit="western_medieval_infantry" category=0 trade_mission=0 node=0 }"#;

    #[test]
    fn a_command_is_its_type_and_a_block() {
        let written = Written::parse(RECRUIT).unwrap();
        assert_eq!(written.kind, "buildunit");
        assert_eq!(written.entries[1], Entry { key: Some("province".into()), value: Value::Word("183".into()) });
        assert!(Written::parse("buildunit").is_err());
        assert!(Written::parse("a={ } b={ }").is_err());
        assert!(Written::parse("a={ b=1").is_err());
        assert!(Written::parse("a={ b=1 } }").is_err());
        assert!(Written::parse("a={ b= }").is_err());
    }

    #[test]
    fn keys_left_out_take_the_game_s_defaults() {
        Written::parse(RECRUIT).unwrap().came_back(&Written::parse(WRITTEN).unwrap()).unwrap();
    }

    #[test]
    fn a_value_the_reader_could_not_read_is_refused() {
        let given = Written::parse(r#"buildunit={ country="FRA" province=abc }"#).unwrap();
        let again = Written::parse(&WRITTEN.replace("province=183", "province=-1")).unwrap();
        assert_eq!(given.came_back(&again).unwrap_err(), "the game read buildunit.province=abc as -1");
    }

    #[test]
    fn a_key_the_reader_passed_over_is_refused() {
        let given = Written::parse(r#"buildunit={ country="FRA" provinec=183 }"#).unwrap();
        let err = given.came_back(&Written::parse(WRITTEN).unwrap()).unwrap_err();
        assert!(err.starts_with("buildunit.provinec did not come back"), "{err}");
    }

    #[test]
    fn numbers_compare_as_numbers_and_quotes_do_not_count() {
        let given = Written::parse(r#"spend_money={ country=FRA amount=10 }"#).unwrap();
        let again = Written::parse(r#"spend_money={ country="FRA" amount=10.000 }"#).unwrap();
        given.came_back(&again).unwrap();
        let other = Written::parse(r#"spend_money={ country="FRA" amount=10.500 }"#).unwrap();
        assert!(given.came_back(&other).is_err());
    }

    #[test]
    fn blocks_lists_and_repeated_keys_come_back_in_order() {
        let given = Written::parse("c={ action={ actor=FRA recipient=SCO } allies={ 1 2 } ally=ENG ally=CAS }").unwrap();
        let again = Written::parse("c={ x=0 action={ recipient=SCO actor=FRA y=1 } allies={ 1 2 } ally=ENG ally=CAS }").unwrap();
        given.came_back(&again).unwrap();
        let swapped = Written::parse("c={ action={ actor=FRA recipient=SCO } allies={ 2 1 } ally=ENG ally=CAS }").unwrap();
        assert!(given.came_back(&swapped).is_err());
        let one = Written::parse("c={ action={ actor=FRA recipient=SCO } allies={ 1 2 } ally=ENG }").unwrap();
        assert!(given.came_back(&one).unwrap_err().contains("ally did not come back"));
        let flat = Written::parse("c={ action=FRA allies={ 1 2 } ally=ENG ally=CAS }").unwrap();
        assert!(given.came_back(&flat).is_err());
    }

    #[test]
    fn another_type_is_refused() {
        let given = Written::parse("a={ }").unwrap();
        assert!(given.came_back(&Written::parse("b={ }").unwrap()).is_err());
    }
}
