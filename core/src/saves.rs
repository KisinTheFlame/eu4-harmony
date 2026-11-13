//! Extension save data (ADR extension-save-data-in-the-save), kept in the save
//! itself: the game's writer writes it as one more top-level entry of the game
//! state, `eu4_harmony`, after the world's own entries, before the AI a plain
//! save holds and before the checksum, and when a save is loaded the framework
//! reads that entry before the world's handler of top-level entries sees it.
//! How the game writes and reads saves is in docs/research/saves.md.
//!
//! The save loader is observed as well, which tells the sessions
//! (`crate::sessions`) when a save is being read and whether it was.
pub mod file;

use crate::called_by_game;
use crate::{
    hooks::Call,
    memory::Memory,
    points::{GameFunction, Point},
    Result,
};
use eu4_harmony_remote::binary as base64;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{atomic::AtomicUsize, Mutex},
};

/// In the world's write of itself into a save (`0x74ff30(world, writer)`), the
/// writer in `rsi`: where the game state has been written, and the AI follows
/// in a plain save (`AI`) or not in the `gamestate` of a zipped one (`NO_AI`).
/// A plain save's top-level entries are read only up to the AI.
const BEFORE_AI: usize = 0x751c87;
const AI: usize = 0x751c8c;
const NO_AI: usize = 0x751c98;
/// The game's writer: one token as given; `{` and `}`, each with a line
/// break; a quoted string, with a line break; a line break; a line's indent.
/// Line breaks and indents are written in text saves only.
pub(crate) const WRITE_TOKEN: GameFunction<2> = GameFunction::at(0x1708870);
const WRITE_OPEN: GameFunction<1> = GameFunction::at(0x1708750);
pub(crate) const WRITE_CLOSE: GameFunction<1> = GameFunction::at(0x17087e0);
const WRITE_QUOTED: GameFunction<4> = GameFunction::at(0x1707d90);
const WRITE_LINE_BREAK: GameFunction<1> = GameFunction::at(0x17084a0);
const WRITE_INDENT: GameFunction<1> = GameFunction::at(0x1708560);
/// In the world's handler of top-level entries (`0x751cd0`), the branch for an
/// entry it has no case for begins by calling the content registry, then asks
/// it about the entry's key; the handler returns at `ENTRY_HANDLED`.
const UNKNOWN_ENTRY: usize = 0x7552bb;
const UNKNOWN_ENTRY_NEXT: usize = 0x7552c0;
const UNKNOWN_ENTRY_KEY: usize = 0x7552c3;
const CONTENT_REGISTRY: usize = 0x7e2d40;
const ENTRY_HANDLED: usize = 0x752d61;
/// The game's own way of skipping a block, which reads the reader's fields
/// below as the framework does.
const SKIP_BLOCK: usize = 0x170a4e6;
const LOAD: usize = 0x5cdc60;

/// The writer's depth of blocks: 0 inside the world's own entries.
pub(crate) const WRITER_DEPTH: usize = 0x8;
/// The reader of a save: its tokenizer, the key of the entry it stands in,
/// and the type of the value after the key's `=`.
const READER_TOKENIZER: usize = 0x28;
const READER_KEY_TEXT: usize = 0x34;
const READER_VALUE_TYPE: usize = 0x440;
/// The tokenizer: the method that reads the next token, returning 0 at the
/// end; the token's type, followed by its text.
const TOKENIZER_NEXT: usize = 0x8;
const TOKEN_TYPE: usize = 0x14;
const TOKEN_TEXT: usize = TOKEN_TYPE + 4;

/// Token types, the same for what the writer takes and what the tokenizer
/// reads: `=`, `{`, `}`, a number, and a string with no id of its own.
pub(crate) const EQUALS: u32 = 1;
const OPEN: u32 = 3;
const CLOSE: u32 = 4;
pub(crate) const NUMBER: u32 = 0xc;
const STRING: u32 = 0xf;
/// The text a token holds, its NUL included.
const TOKEN_TEXT_LEN: usize = 512;

const KEY: &str = "eu4_harmony";
const VERSION: &str = "1";
/// The game's reader returns an empty string for one of 512 bytes or more.
const CHUNK: usize = 500;

struct LoadFrame {
    world: usize,
    success: usize,
    read: Option<Result<BTreeMap<String, Value>>>,
}

/// Authoritative lifecycle of extension save data across nested save reads,
/// pre-session ironman setup saves, session start (`on_session`), mid-session
/// main-thread or background saves, and in-place extension reloads.
#[derive(Default)]
struct SaveData {
    extensions: BTreeSet<String>,
    frames: Vec<LoadFrame>,
    /// Data from a completed world-replacing load not yet handed to `on_session`.
    pending_session: Option<BTreeMap<String, Value>>,
    /// Data written into a save if written right now.
    current: BTreeMap<String, Value>,
    /// True once the active session's initial data has been handed to
    /// `on_session`, making live `save_data` handlers authoritative on the main
    /// thread. False between `load_finished` and `begin_session`, protecting
    /// loaded data during an ironman setup save before the first safe frame.
    delivered: bool,
}

impl SaveData {
    fn keep_for(&mut self, ids: impl IntoIterator<Item = String>) {
        self.extensions = ids.into_iter().collect();
    }

    fn load_began(&mut self, world: usize, success: usize) {
        self.frames.push(LoadFrame { world, success, read: None });
    }

    fn entry_read(&mut self, result: Result<BTreeMap<String, Value>>) -> bool {
        let Some(top) = self.frames.last_mut() else { return false };
        top.read = Some(result);
        true
    }

    fn load_finished(&mut self, check: impl FnOnce(usize, usize) -> Result<bool>) -> Result<Option<usize>> {
        let Some(frame) = self.frames.pop() else { return Ok(None) };
        let replaced_world = check(frame.world, frame.success)?;
        let raw = frame.read.unwrap_or_else(|| Ok(BTreeMap::new()));
        let filtered = raw.map(|mut data| {
            data.retain(|id, _| {
                let kept = self.extensions.contains(id);
                if !kept {
                    log!("saves: dropped the data of {id:?}, which is not loaded");
                }
                kept
            });
            data
        });
        if !replaced_world {
            return filtered.map(|_| None);
        }
        self.delivered = false;
        match filtered {
            Ok(data) => {
                let count = data.len();
                self.current = data.clone();
                self.pending_session = Some(data);
                Ok(Some(count))
            }
            Err(e) => {
                self.current.clear();
                self.pending_session = Some(BTreeMap::new());
                Err(e)
            }
        }
    }

    /// Marks the session's initial data as handed to `on_session`; returns
    /// whether the session began from a loaded save.
    fn begin_session(&mut self) -> bool {
        self.delivered = true;
        match self.pending_session.take() {
            Some(loaded) => {
                self.current = loaded;
                true
            }
            None => {
                self.current.clear();
                false
            }
        }
    }

    fn get(&self, id: &str) -> Option<Value> {
        self.current.get(id).cloned()
    }

    fn clear_session(&mut self) {
        self.current.clear();
        self.delivered = false;
    }

    fn reloaded(&mut self, id: &str, carried: Option<Value>) {
        match carried {
            Some(v) if self.extensions.contains(id) => {
                self.current.insert(id.to_string(), v);
            }
            _ => {
                self.current.remove(id);
            }
        }
    }

    fn should_poll_live(&self, on_main_thread: bool) -> bool {
        on_main_thread && self.delivered
    }

    fn apply_polled(&mut self, polled: Vec<(String, Option<Value>)>) -> BTreeMap<String, Value> {
        for (id, value) in polled {
            match value {
                Some(v) if self.extensions.contains(&id) => {
                    self.current.insert(id, v);
                }
                _ => {
                    self.current.remove(&id);
                }
            }
        }
        self.current.clone()
    }
}

static SAVE_DATA: Mutex<SaveData> =
    Mutex::new(SaveData { extensions: BTreeSet::new(), frames: Vec::new(), pending_session: None, current: BTreeMap::new(), delivered: false });

fn with_data<T>(f: impl FnOnce(&mut SaveData) -> T) -> T {
    f(&mut SAVE_DATA.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Says which extensions loaded, once they have.
pub fn keep_for(ids: impl IntoIterator<Item = String>) {
    with_data(|d| d.keep_for(ids));
}

/// Prepares the extensions' save data for a session that is starting now and
/// returns whether that session came from a loaded save.
pub fn begin_session() -> bool {
    with_data(SaveData::begin_session)
}

pub fn get(id: &str) -> Option<Value> {
    with_data(|d| d.get(id))
}

/// Synchronizes `id`'s current save data after an in-place extension reload.
pub fn reloaded(id: &str, carried: Option<Value>) {
    with_data(|d| d.reloaded(id, carried));
}

/// Clears the previous session's save-data authority after its exit save (if
/// any) and `on_session_ended` have finished, or on a world reset, so a new
/// ironman game's pre-session setup save never inherits an earlier session's
/// state.
pub fn clear_session() {
    with_data(SaveData::clear_session);
}

/// One step of writing the entry, as the game's writer takes it.
#[derive(Debug, PartialEq, Eq)]
enum Write {
    Token(u32, &'static str),
    Open,
    Close,
    Quoted(String),
    LineBreak,
    Indent,
}

/// The entry for `data`, which must not be empty:
///
/// ```text
/// eu4_harmony={
///     version=1
///     data={
///         "<the data as JSON, in base64, at most 500 bytes a string>"
///     }
/// }
/// ```
fn entry(data: &BTreeMap<String, Value>) -> Vec<Write> {
    let json = serde_json::to_vec(data).expect("JSON values serialize");
    let mut writes = vec![
        Write::Token(STRING, KEY),
        Write::Token(EQUALS, "="),
        Write::Open,
        Write::Indent,
        Write::Token(STRING, "version"),
        Write::Token(EQUALS, "="),
        Write::Token(NUMBER, VERSION),
        Write::LineBreak,
        Write::Indent,
        Write::Token(STRING, "data"),
        Write::Token(EQUALS, "="),
        Write::Open,
    ];
    for chunk in base64::encode(&json).as_bytes().chunks(CHUNK) {
        writes.push(Write::Indent);
        writes.push(Write::Quoted(String::from_utf8(chunk.to_vec()).expect("base64 is ASCII")));
    }
    writes.extend([Write::Close, Write::Close]);
    writes
}

/// A token of the entry's block, as the tokenizer reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Equals,
    Open,
    Close,
    Text(String),
}

/// The data in the tokens of the entry's block, from after its `{` to before
/// its `}`.
fn decode(tokens: &[Token]) -> Result<BTreeMap<String, Value>> {
    let rest = match tokens {
        [Token::Text(key), Token::Equals, Token::Text(version), rest @ ..] if key == "version" => {
            if version != VERSION {
                return Err(format!("the data is in format version {version}, which this framework does not read"));
            }
            rest
        }
        _ => return Err("the data has no format version".into()),
    };
    let [Token::Text(key), Token::Equals, Token::Open, chunks @ .., Token::Close] = rest else { return Err("no data after the format version".into()) };
    if key != "data" {
        return Err(format!("{key:?} where the data should be"));
    }
    let mut text = String::new();
    for chunk in chunks {
        let Token::Text(chunk) = chunk else { return Err("the data holds more than strings".into()) };
        text.push_str(chunk);
    }
    let json = base64::decode(&text).ok_or("the data is not base64")?;
    serde_json::from_slice(&json).map_err(|e| format!("the data is not JSON of extension ids: {e}"))
}

/// A token as the game's writer takes it: its type, then its text up to a
/// NUL; the byte after the text stays 0, as the game's own callers leave it.
#[repr(C)]
pub(crate) struct GameToken {
    kind: u32,
    text: [u8; TOKEN_TEXT_LEN],
    rest: u32,
}

impl GameToken {
    pub(crate) fn new(kind: u32, text: &str) -> Self {
        let mut token = Self { kind, text: [0; TOKEN_TEXT_LEN], rest: 0 };
        token.text[..text.len()].copy_from_slice(text.as_bytes());
        token
    }

    /// A token of text the game keeps, its NUL included.
    pub(crate) fn from_bytes(kind: u32, text: &[u8]) -> Self {
        let mut token = Self { kind, text: [0; TOKEN_TEXT_LEN], rest: 0 };
        let len = text.len().min(TOKEN_TEXT_LEN);
        token.text[..len].copy_from_slice(&text[..len]);
        token
    }
}

/// Writes the entry with the game's writer, after the world's game state and
/// before the checksum is taken of it.
fn write_entry(m: Memory, writer: usize) -> Result<()> {
    // Each extension's data is what its save_data returns now once the session
    // has delivered `on_session` (ADR save-data-is-asked-for-by-save-data).
    // Before `on_session` (an ironman setup save on load) or off the main
    // thread, the authoritative `current` map is written without polling Lua.
    let on_main = crate::thread_id() == crate::main_thread();
    if !on_main {
        log!("saves: the world is written off the main thread; the data from each extension's last save_data is written");
    }
    let data = if with_data(|d| d.should_poll_live(on_main)) {
        let polled = crate::lua::collect_save_data();
        with_data(|d| d.apply_polled(polled))
    } else {
        with_data(|d| d.current.clone())
    };
    if data.is_empty() {
        return Ok(());
    }
    let depth = m.i32(writer + WRITER_DEPTH)?;
    if depth != 0 {
        return Err(format!("the writer is {depth} blocks deep after the world; no data written"));
    }
    let writes = entry(&data);
    for write in &writes {
        match write {
            Write::Token(kind, text) => WRITE_TOKEN.call(&m, [writer, &GameToken::new(*kind, text) as *const _ as usize])?,
            Write::Open => WRITE_OPEN.call(&m, [writer])?,
            Write::Close => WRITE_CLOSE.call(&m, [writer])?,
            Write::Quoted(text) => {
                let text: Vec<u8> = text.bytes().chain([0]).collect();
                WRITE_QUOTED.call(&m, [writer, text.as_ptr() as usize, 0, 0])?
            }
            Write::LineBreak => WRITE_LINE_BREAK.call(&m, [writer])?,
            Write::Indent => WRITE_INDENT.call(&m, [writer])?,
        };
    }
    let chunks = writes.iter().filter(|w| matches!(w, Write::Quoted(_))).count();
    log!("saves: wrote the data of {} extension(s), {chunks} string(s)", data.len());
    Ok(())
}

#[called_by_game]
extern "C" fn game_state_written(writer: usize) {
    crate::guarded("parallel save", || crate::saving::parallel::join(writer));
    crate::guarded("save data writer", || {
        if let Err(e) = write_entry(crate::hooked_memory(), writer) {
            log!("saves: {e}");
        }
    });
}

crate::resume!(AI_RESUME: WRITE_AI = AI, SKIP_AI = NO_AI);

/// Replaces `test r15b, r15b; je <no AI>` where the world has written its game
/// state: the entry is written, then the AI or not, as the game chose.
#[unsafe(naked)]
extern "C" fn before_ai_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, rsi", call written),
        "test r15b, r15b",
        "je 2f",
        "jmp qword ptr [rip + {ai}]",
        "2:",
        "jmp qword ptr [rip + {no_ai}]",
        written = sym game_state_written,
        ai = sym WRITE_AI,
        no_ai = sym SKIP_AI,
    );
}

/// Whether the reader stands in the framework's entry: its key, and a block.
fn our_entry(m: Memory, reader: usize) -> Result<bool> {
    let key = m.bytes(reader + READER_KEY_TEXT, KEY.len() + 1)?;
    if key[..KEY.len()] != *KEY.as_bytes() || key[KEY.len()] != 0 {
        return Ok(false);
    }
    if m.u32(reader + READER_VALUE_TYPE)? != OPEN {
        return Err(format!("the {KEY} entry is not a block"));
    }
    Ok(true)
}

/// The tokens of the block the reader stands in, up to its `}`, which is read
/// too, as the game's own skipping of a block does.
fn block_tokens(m: Memory, reader: usize) -> Result<Vec<Token>> {
    let tokenizer = m.ptr(reader + READER_TOKENIZER)?;
    let next = m.vslot(tokenizer, TOKENIZER_NEXT)?;
    let mut depth = 0usize;
    let mut tokens = Vec::new();
    loop {
        if m.call(next, &[tokenizer])? as u32 == 0 {
            return Err(format!("the save ends inside the {KEY} entry"));
        }
        let token = match m.u32(tokenizer + TOKEN_TYPE)? {
            EQUALS => Token::Equals,
            OPEN => {
                depth += 1;
                Token::Open
            }
            CLOSE if depth == 0 => return Ok(tokens),
            CLOSE => {
                depth -= 1;
                Token::Close
            }
            _ => {
                let text = m.bytes(tokenizer + TOKEN_TEXT, TOKEN_TEXT_LEN)?;
                let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
                Token::Text(String::from_utf8_lossy(&text[..end]).into_owned())
            }
        };
        tokens.push(token);
    }
}

/// Reads the framework's entry, when the reader (`rsi`) stands in it, and
/// says whether it did; the game then goes on as if it had read it itself.
#[called_by_game]
extern "C" fn take_entry(reader: usize) -> bool {
    let m = crate::hooked_memory();
    let read = match our_entry(m, reader) {
        Ok(false) => return false,
        Ok(true) => block_tokens(m, reader).and_then(|tokens| decode(&tokens)),
        // The game skips the entry, as one it does not know.
        Err(e) if !crate::sessions::loading() => {
            log!("saves: {e}");
            return false;
        }
        Err(e) => {
            let _ = with_data(|d| d.entry_read(Err(e)));
            return false;
        }
    };
    if !with_data(|d| d.entry_read(read)) {
        log!("saves: read a {KEY} entry outside a load, and left it");
    }
    true
}

crate::resume!(ENTRY_RESUME: ENTRY_UNKNOWN = UNKNOWN_ENTRY_NEXT, ENTRY_DONE = ENTRY_HANDLED);
static REGISTRY: AtomicUsize = AtomicUsize::new(0);

/// Replaces `call <content registry>` at the start of the branch for an entry
/// the world's handler has no case for, the reader in `rsi`. The framework's
/// entry is read and the handler returns; for any other the registry is
/// called as the game called it, returning to the game's next instruction.
#[unsafe(naked)]
extern "C" fn entry_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, rsi", call take, "cmp byte ptr [rsp + 0x48], 0"),
        "jne 2f",
        "push qword ptr [rip + {unknown}]",
        "jmp qword ptr [rip + {registry}]",
        "2:",
        "jmp qword ptr [rip + {handled}]",
        take = sym take_entry,
        unknown = sym ENTRY_UNKNOWN,
        registry = sym REGISTRY,
        handled = sym ENTRY_DONE,
    );
}

fn world_state(m: &Memory) -> Result<(usize, u8)> {
    use crate::objects::campaign::{globals, world};
    let at = m.ptr(m.at(globals::WORLD))?;
    if at == 0 {
        return Err("no world".into());
    }
    Ok((at, m.u8(at + world::DAILY_UPDATE)?))
}

fn load_before(call: &mut Call) {
    // A save compressed in the background is in its place before any is read.
    crate::saving::background::wait();
    crate::sessions::load_began();
    with_data(|d| d.load_began(call.arg(2), call.arg(1)));
}

fn load_after(call: &mut Call) {
    struct Exit;
    impl Drop for Exit {
        fn drop(&mut self) {
            crate::sessions::load_ended();
        }
    }
    let _exit = Exit;
    let outcome = with_data(|d| {
        d.load_finished(|expected_world, success_ptr| {
            if call.faulted() {
                return Err("the loader unwound".to_string());
            }
            let m = call.memory();
            let (world, daily) = world_state(&m)?;
            let read = call.result() == Some(success_ptr) && m.u8(success_ptr)? == 1 && world == expected_world && daily == 0;
            crate::sessions::load_read(read);
            Ok(read)
        })
    });
    match outcome {
        Ok(Some(count)) => log!("saves: loaded a save with data for {count} extension(s)"),
        Ok(None) => {}
        Err(e) => log!("saves: loaded a save whose data cannot be read, or failed to finish: {e}"),
    }
}

/// Where the data is written and read, and the loader.
pub fn points() -> Vec<Point> {
    vec![
        Point::jump(BEFORE_AI, "45 84 ff 74 0c", "world has written its game state", before_ai_stub as *const () as usize, &AI_RESUME),
        Point::keep(AI, "48 8b d6 49 8b ce e8 f9 dd ff ff", "world writes the AI with the writer it was given"),
        Point::keep(NO_AI, "48 8d 4c 24 50 e8 3e 27 94 ff", "world write ends"),
        Point::keep(WRITE_TOKEN, "40 55 53 56 57 41 54 41 56 41 57 48 8d 6c 24 d9", "save writer writes a token"),
        Point::keep(WRITE_OPEN, "40 53 48 81 ec 30 02 00 00 83 79 08 00 48 8b d9", "save writer opens a block").uses(&[WRITER_DEPTH]),
        Point::keep(WRITE_CLOSE, "40 53 48 81 ec 30 02 00 00 ff 49 08 48 8b d9", "save writer closes a block").uses(&[WRITER_DEPTH]),
        Point::keep(WRITE_QUOTED, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57", "save writer writes a quoted string"),
        Point::keep(WRITE_LINE_BREAK, "40 53 48 83 ec 20 48 8b d9 b9 08 00 00 00 65 48", "save writer breaks a line"),
        Point::keep(WRITE_INDENT, "48 89 5c 24 08 48 89 74 24 10 57 48 81 ec 30 02", "save writer indents a line"),
        Point::jump(UNKNOWN_ENTRY, "e8 80 da 08 00", "world reads an entry it has no case for", entry_stub as *const () as usize, &ENTRY_RESUME),
        Point::keep(UNKNOWN_ENTRY_KEY, "48 8d 56 34", "world reads an unknown entry's key").uses(&[READER_KEY_TEXT]),
        Point::keep(CONTENT_REGISTRY, "48 89 5c 24 10 57 48 83 ec 20 48 8b 05 4f 8f c4", "content registry").recorded(&REGISTRY),
        Point::keep(ENTRY_HANDLED, "48 8b 9c 24 90 0b 00 00 48 81 c4 50 0b 00 00", "world has read an entry"),
        Point::keep(
            SKIP_BLOCK,
            "83 b9 40 04 00 00 03 48 8b f9 75 50 48 8b 49 28 48 89 5c 24 30 33 db 48 8b 01 ff 50 08 85 c0 74 2e 66 0f 1f 84 00 00 00 00 00 48 8b 4f 28 8b 41 14 83 f8 03",
            "save reader skips a block",
        )
        .uses(&[READER_VALUE_TYPE, READER_TOKENIZER, TOKENIZER_NEXT, TOKEN_TYPE]),
        Point::observe(LOAD, "48 89 5c 24 18 55 56 57 48 8b ec 48 81 ec 80 00", "save loader", 4, Some(load_before), Some(load_after)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The tokens the game's tokenizer reads from what the writer wrote,
    /// after the entry's `{`, up to before its `}`.
    fn read_back(writes: &[Write]) -> Vec<Token> {
        let [Write::Token(STRING, KEY), Write::Token(EQUALS, "="), Write::Open, inner @ .., Write::Close] = writes else { panic!("{writes:?}") };
        inner
            .iter()
            .filter_map(|w| match w {
                Write::Token(EQUALS, _) => Some(Token::Equals),
                Write::Token(_, text) => Some(Token::Text(text.to_string())),
                Write::Quoted(text) => Some(Token::Text(text.clone())),
                Write::Open => Some(Token::Open),
                Write::Close => Some(Token::Close),
                Write::LineBreak | Write::Indent => None,
            })
            .collect()
    }

    fn some_data() -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("regional-combat".to_string(), json!({"armies": [{"army": 12345678901234i64, "center": 151, "radius": 3}]})),
            ("other".to_string(), json!("quotes \" and \\ and 中文 and a long tail ".repeat(40))),
        ])
    }

    #[test]
    fn what_is_written_reads_back_as_it_was() {
        let data = some_data();
        let writes = entry(&data);
        assert!(writes.iter().all(|w| !matches!(w, Write::Quoted(s) if s.len() > CHUNK || s.is_empty())));
        assert!(writes.iter().filter(|w| matches!(w, Write::Quoted(_))).count() > 1, "the data fills more than one string");
        assert_eq!(decode(&read_back(&writes)).unwrap(), data);
    }

    #[test]
    fn data_that_cannot_be_read_says_why() {
        let good = read_back(&entry(&some_data()));
        let newer: Vec<Token> = good.iter().map(|t| if *t == Token::Text(VERSION.into()) { Token::Text("2".into()) } else { t.clone() }).collect();
        assert!(decode(&newer).unwrap_err().contains("format version 2"));
        assert!(decode(&[]).unwrap_err().contains("no format version"));
        let last_chunk = good.len() - 2;
        let mut broken = good.clone();
        broken[last_chunk] = Token::Text("not*base64".into());
        assert!(decode(&broken).unwrap_err().contains("not base64"));
        let mut nested = good.clone();
        nested.splice(last_chunk..last_chunk, [Token::Open, Token::Close]);
        assert!(decode(&nested).unwrap_err().contains("more than strings"));
        let mut not_ids = good.clone();
        not_ids.splice(6.., [Token::Text(base64::encode(b"[1, 2]")), Token::Close]);
        assert!(decode(&not_ids).unwrap_err().contains("not JSON of extension ids"));
    }

    #[test]
    fn the_tokens_written_fit_the_game_s() {
        for write in entry(&some_data()) {
            if let Write::Token(_, text) = write {
                assert!(text.len() < TOKEN_TEXT_LEN);
            }
        }
        assert_eq!(std::mem::size_of::<GameToken>(), 0x208);
    }

    #[test]
    fn lifecycle_preserves_loaded_data_across_ironman_setup_saves_nested_loads_and_reloads() {
        let mut state = SaveData::default();
        state.keep_for(["regional-combat".into(), "tools".into()]);

        // Outer load parses extension data; a nested non-replacing load inside it must not wipe outer `read`.
        state.load_began(0x1000, 0x2000);
        assert!(state.entry_read(Ok(BTreeMap::from([("regional-combat".into(), json!({"armies": [7]})), ("unloaded".into(), json!(99)),]))));
        state.load_began(0x3000, 0x4000);
        assert_eq!(state.load_finished(|_, _| Ok(false)).unwrap(), None);
        assert_eq!(state.load_finished(|_, _| Ok(true)).unwrap(), Some(1));

        // Ironman setup save runs on the main thread BEFORE `begin_session` / `on_session`:
        // it must not poll stale/uninitialized Lua handlers or clobber loaded data.
        assert!(!state.should_poll_live(true));
        assert_eq!(state.get("regional-combat"), Some(json!({"armies": [7]})));
        assert_eq!(state.get("unloaded"), None);

        // First safe frame delivers `on_session`; from now on main-thread saves poll live handlers.
        assert!(state.begin_session());
        assert!(state.should_poll_live(true));
        assert!(!state.should_poll_live(false));
        assert_eq!(state.get("regional-combat"), Some(json!({"armies": [7]})));

        // Mid-session save updates `current`; reload carries new data or clears removed handlers.
        let saved = state.apply_polled(vec![("regional-combat".into(), Some(json!({"armies": [8]}))), ("tools".into(), Some(json!(1)))]);
        assert_eq!(saved["regional-combat"], json!({"armies": [8]}));
        state.reloaded("tools", Some(json!(2)));
        assert_eq!(state.get("tools"), Some(json!(2)));
        state.reloaded("tools", None);
        assert_eq!(state.get("tools"), None);

        // Ending the session (after any ironman exit save) clears authority so a
        // subsequent new ironman game's pre-session setup save writes nothing from the old game.
        state.clear_session();
        assert!(!state.should_poll_live(true) && state.current.is_empty());

        // Starting a fresh game without a save keeps `current` empty and enables live polling.
        assert!(!state.begin_session());
        assert!(state.should_poll_live(true) && state.current.is_empty());

        // Corrupted data on a world-replacing load clears `current`; on a failed load it leaves `current` intact.
        state.reloaded("regional-combat", Some(json!(42)));
        state.load_began(0x1000, 0x2000);
        state.entry_read(Err("bad base64".into()));
        assert!(state.load_finished(|_, _| Ok(false)).is_err());
        assert_eq!(state.get("regional-combat"), Some(json!(42)));
        state.load_began(0x1000, 0x2000);
        state.entry_read(Err("bad base64".into()));
        assert!(state.load_finished(|_, _| Ok(true)).is_err());
        assert_eq!(state.get("regional-combat"), None);
    }
}
