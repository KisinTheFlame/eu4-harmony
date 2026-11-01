//! Extension save data (ADR 0020), kept in the save itself: the game's writer
//! writes it as one more top-level entry of the game state, `eu4_harmony`,
//! after the world's own entries, before the AI a plain save holds and before
//! the checksum, and when a save is loaded the framework reads that entry
//! before the world's handler of top-level entries sees it. How the game
//! writes and reads saves is in docs/research/saves.md.
//!
//! The save loader is observed as well, which tells the rest of the core when
//! a save is being read and whether it was.
use crate::{hooks::HookFrame, memory::Memory, points::Point, Result};
use eu4_harmony_remote::binary as base64;
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
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
const WRITE_TOKEN: usize = 0x1708870;
const WRITE_OPEN: usize = 0x1708750;
const WRITE_CLOSE: usize = 0x17087e0;
const WRITE_QUOTED: usize = 0x1707d90;
const WRITE_LINE_BREAK: usize = 0x17084a0;
const WRITE_INDENT: usize = 0x1708560;
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
const WRITER_DEPTH: usize = 0x8;
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
const EQUALS: u32 = 1;
const OPEN: u32 = 3;
const CLOSE: u32 = 4;
const NUMBER: u32 = 0xc;
const STRING: u32 = 0xf;
/// The text a token holds, its NUL included.
const TOKEN_TEXT_LEN: usize = 512;

const KEY: &str = "eu4_harmony";
const VERSION: &str = "1";
/// The game's reader returns an empty string for one of 512 bytes or more.
const CHUNK: usize = 500;

static LOAD_DEPTH: AtomicUsize = AtomicUsize::new(0);
/// The saves the game began to read since it started.
static LOADS: AtomicUsize = AtomicUsize::new(0);
/// The game read the last save it began to read into its world.
static LAST_READ: AtomicBool = AtomicBool::new(false);

/// The extensions loaded in this run, the only ones a save's data is kept for.
static EXTENSIONS: OnceLock<BTreeSet<String>> = OnceLock::new();
/// What the save being read held in its entry, once read.
static READ: Mutex<Option<Result<BTreeMap<String, Value>>>> = Mutex::new(None);
/// Data loaded with the last save, waiting for the session that loaded it.
static LOADED: Mutex<Option<BTreeMap<String, Value>>> = Mutex::new(None);
/// Current data of every extension, written with every save.
static CURRENT: Mutex<BTreeMap<String, Value>> = Mutex::new(BTreeMap::new());

pub fn load_in_progress() -> bool {
    LOAD_DEPTH.load(Ordering::SeqCst) != 0
}

/// How many saves the game began to read since it started.
pub fn loads() -> usize {
    LOADS.load(Ordering::SeqCst)
}

/// Whether the game read the last save it began to read: its loader said
/// so, for the world it read into, outside a daily update.
pub fn last_load_read() -> bool {
    LAST_READ.load(Ordering::SeqCst)
}

/// Says which extensions loaded, once they have.
pub fn keep_for(ids: impl IntoIterator<Item = String>) {
    let _ = EXTENSIONS.set(ids.into_iter().collect());
}

/// Data the last load brought along, if the session has not taken it yet.
pub fn take_loaded() -> Option<BTreeMap<String, Value>> {
    LOADED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

pub fn get(id: &str) -> Option<Value> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned()
}

pub fn set(id: &str, value: Option<Value>) {
    let mut current = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    match value {
        Some(v) => current.insert(id.to_string(), v),
        None => current.remove(id),
    };
}

/// Replaces all extensions' data, when a load replaces the world and at a
/// session start.
pub fn replace_all(data: BTreeMap<String, Value>) {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = data;
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

/// Keeps what a load brought, only for the extensions loaded in this run,
/// for its session. Once the load has replaced the world, the data is current
/// at once: the game saves an ironman game it loads before the session starts,
/// and that save must carry it.
fn loaded(read: bool, data: Result<BTreeMap<String, Value>>, extensions: &BTreeSet<String>) -> Result<()> {
    let data = data.map(|mut data| {
        data.retain(|id, _| {
            let kept = extensions.contains(id);
            if !kept {
                log!("saves: dropped the data of {id:?}, which is not loaded");
            }
            kept
        });
        data
    });
    if read {
        replace_all(data.clone().unwrap_or_default());
    }
    *LOADED.lock().unwrap_or_else(|e| e.into_inner()) = Some(data?);
    Ok(())
}

/// A token as the game's writer takes it: its type, then its text up to a
/// NUL; the byte after the text stays 0, as the game's own callers leave it.
#[repr(C)]
struct GameToken {
    kind: u32,
    text: [u8; TOKEN_TEXT_LEN],
    rest: u32,
}

impl GameToken {
    fn new(kind: u32, text: &str) -> Self {
        let mut token = Self { kind, text: [0; TOKEN_TEXT_LEN], rest: 0 };
        token.text[..text.len()].copy_from_slice(text.as_bytes());
        token
    }
}

/// Writes the entry with the game's writer, after the world's game state and
/// before the checksum is taken of it.
fn write_entry(m: Memory, writer: usize) -> Result<()> {
    let data = CURRENT.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
            Write::Token(kind, text) => m.call(m.at(WRITE_TOKEN), &[writer, &GameToken::new(*kind, text) as *const _ as usize])?,
            Write::Open => m.call(m.at(WRITE_OPEN), &[writer])?,
            Write::Close => m.call(m.at(WRITE_CLOSE), &[writer])?,
            Write::Quoted(text) => {
                let text: Vec<u8> = text.bytes().chain([0]).collect();
                m.call(m.at(WRITE_QUOTED), &[writer, text.as_ptr() as usize, 0, 0])?
            }
            Write::LineBreak => m.call(m.at(WRITE_LINE_BREAK), &[writer])?,
            Write::Indent => m.call(m.at(WRITE_INDENT), &[writer])?,
        };
    }
    let chunks = writes.iter().filter(|w| matches!(w, Write::Quoted(_))).count();
    log!("saves: wrote the data of {} extension(s), {chunks} string(s)", data.len());
    Ok(())
}

extern "C" fn game_state_written(writer: usize) {
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
extern "C" fn take_entry(reader: usize) -> bool {
    crate::guarded("save data reader", || {
        let m = crate::hooked_memory();
        let read = match our_entry(m, reader) {
            Ok(false) => return false,
            Ok(true) => block_tokens(m, reader).and_then(|tokens| decode(&tokens)),
            // The game skips the entry, as one it does not know.
            Err(e) if !load_in_progress() => {
                log!("saves: {e}");
                return false;
            }
            Err(e) => {
                *READ.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(e));
                return false;
            }
        };
        if load_in_progress() {
            *READ.lock().unwrap_or_else(|e| e.into_inner()) = Some(read);
        } else {
            log!("saves: read a {KEY} entry outside a load, and left it");
        }
        true
    })
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

/// The loads in progress on this thread: the world each reads into, and where
/// its loader reports success.
struct Load {
    world: usize,
    success: usize,
}
thread_local! { static LOADING: RefCell<Vec<Load>> = const { RefCell::new(Vec::new()) }; }

fn world_state(m: &Memory) -> Result<(usize, u8)> {
    use crate::objects::campaign::{globals, world};
    let at = m.ptr(m.at(globals::WORLD))?;
    if at == 0 {
        return Err("no world".into());
    }
    Ok((at, m.u8(at + world::DAILY_UPDATE)?))
}

extern "C" fn load_before(frame: *mut HookFrame) {
    LOAD_DEPTH.fetch_add(1, Ordering::SeqCst);
    LOADS.fetch_add(1, Ordering::SeqCst);
    LAST_READ.store(false, Ordering::SeqCst);
    *LOADED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *READ.lock().unwrap_or_else(|e| e.into_inner()) = None;
    crate::game::events::load_begin();
    crate::remote::campaign_changes();
    let h = unsafe { &*frame };
    LOADING.with(|l| l.borrow_mut().push(Load { world: h.args[2], success: h.args[1] }));
}

extern "C" fn load_after(frame: *mut HookFrame) {
    struct Exit;
    impl Drop for Exit {
        fn drop(&mut self) {
            let _ = LOAD_DEPTH.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| Some(n.saturating_sub(1)));
        }
    }
    let _exit = Exit;
    crate::guarded("save loader", || {
        let h = unsafe { &*frame };
        let Some(load) = LOADING.with(|l| l.borrow_mut().pop()) else { return };
        let outcome = (|| {
            if h.fault != 0 {
                return Err("the loader unwound".to_string());
            }
            let m = crate::hooked_memory();
            let (world, daily) = world_state(&m)?;
            let read = h.result == load.success && m.u8(load.success)? == 1 && world == load.world && daily == 0;
            LAST_READ.store(read, Ordering::SeqCst);
            let data = READ.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_else(|| Ok(BTreeMap::new()));
            if read {
                match &data {
                    Ok(d) => log!("saves: loaded a save with data for {} extension(s)", d.len()),
                    Err(e) => log!("saves: loaded a save whose data cannot be read, so no extension has any: {e}"),
                }
            }
            loaded(read, data, EXTENSIONS.get().unwrap_or(&BTreeSet::new()))
        })();
        if let Err(e) = outcome {
            log!("saves: {e}");
        }
    });
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
    fn a_load_keeps_the_data_of_loaded_extensions_and_makes_it_current() {
        let loaded_here = BTreeSet::from(["regional-combat".to_string()]);
        let data = BTreeMap::from([("regional-combat".to_string(), json!({"armies": [1]})), ("gone".to_string(), json!(1))]);
        replace_all(BTreeMap::new());
        loaded(true, Ok(data), &loaded_here).unwrap();
        assert_eq!(get("regional-combat"), Some(json!({"armies": [1]})));
        assert_eq!(get("gone"), None);
        assert_eq!(take_loaded(), Some(BTreeMap::from([("regional-combat".to_string(), json!({"armies": [1]}))])));
        // A world whose data could not be read starts with none.
        assert!(loaded(true, Err("the data is not base64".into()), &loaded_here).is_err());
        assert_eq!(get("regional-combat"), None);
        assert_eq!(take_loaded(), None);
        // A load that did not replace the world leaves its data alone.
        set("regional-combat", Some(json!(2)));
        assert!(loaded(false, Err("the save ends inside the eu4_harmony entry".into()), &loaded_here).is_err());
        assert_eq!(get("regional-combat"), Some(json!(2)));
        replace_all(BTreeMap::new());
    }
}
