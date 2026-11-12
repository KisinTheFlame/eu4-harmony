//! The command layer (ADR one-command-layer-in-written-form,
//! ADR commands-run-at-once, docs/plans/commands.md): posts one of the game's
//! commands from its written form, through the game's own reader, check and
//! post, as the local handler reads back every command it posts, and has the
//! game execute it at once, as the session's update executes a command at the
//! next frame; and hears every command the game executes, in the same form
//! (docs/research/commands.md). Its vocabulary, the commands' type names and
//! keys, is the game's and stays in the core: capabilities write their
//! command's text with [`write`] and post it here.
//!
//! The game's reader takes almost any text and says nothing of what it could
//! not read, so [`Commands::post`] writes the command it read out again and refuses it
//! unless every key given came back with the value given.
use super::{command_session, context, game, world_context, Fail, GResult};
use crate::{
    game_string,
    hooks::Call,
    memory::Memory,
    objects::session::{self, fields as session_fields},
    points::{GameFunction, Point},
    unicode::game_text,
};
use std::cell::OnceCell;
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
const WRITE_KEY: GameFunction<2> = GameFunction::at(0x1709280);
/// The token table: whether it is set up, its last index, and its names, one
/// `std::string` each.
pub(crate) const TOKENS_READY: usize = 0x2357b30;
pub(crate) const TOKENS_LAST: usize = 0x2357b28;
pub(crate) const TOKEN_NAMES: usize = 0x242b888;
/// The memory stream's and the writer's vtables, as the local handler's post
/// makes them; the stream's `+0x18` sets its mode and whether it writes.
pub(crate) const STREAM_VTABLE: usize = 0x1dc0358;
pub(crate) const WRITER_VTABLE: usize = 0x1dbf7d8;
const STREAM_MODE: usize = 0x171ed90;
/// Reading, as the local handler's post reads back what it wrote: a text
/// tokenizer (its base `(tokenizer, stream, owns)`, then its own vtable, whose
/// `+0` destroys it, flag 0 leaving the stream), a reader `(reader,
/// tokenizer, strings, name, owns)` with its destructor, the reader's first
/// token, read into `+0x30`, and the factory `(factory, reader, token)`, an
/// object of its own, which makes an empty command of the token's type.
const TOKENIZER: GameFunction<3> = GameFunction::at(0x170d1d0);
const TEXT_TOKENIZER_VTABLE: usize = 0x1dbf958;
const TEXT_TOKENIZER_DESTROY: GameFunction<2> = GameFunction::at(0x170ccb0);
const READER: GameFunction<5> = GameFunction::at(0x17096f0);
const END_READER: GameFunction<1> = GameFunction::at(0x1709950);
const READ_TOKEN: GameFunction<1> = GameFunction::at(0x170ada0);
const READER_TOKEN: usize = 0x30;
const MAKE: GameFunction<3> = GameFunction::at(0x14c6500);
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
/// Where the session's update reads its state, and its two branches that run
/// the queued commands: the layer executes a command only in a state in
/// which the update would.
const UPDATE_STATE: usize = 0x14ca572;
const UPDATE_RUNS_QUEUE: usize = 0x14ca5de;
const UPDATE_RUNS_QUEUE_RUNNING: usize = 0x14ca651;
/// Where the session's post stamps a command with the sender and the tick,
/// and where its execution runs one command that passed its check.
const SESSION_STAMPS: usize = 0x14ca318;
const SESSION_EXECUTES: usize = 0x14caed8;
/// A command's sender (i32) and tick (u16), its flag that it runs on one
/// machine only (u8), and its slots called before and after it executes,
/// `(command)`.
const SENDER: usize = 0x38;
const TICK: usize = 0x48;
const ONE_MACHINE: usize = 0x4a;
const BEFORE: usize = 0x68;
const AFTER: usize = 0x70;
/// The highest tick a command carries.
const TICK_LIMIT: u32 = 0x7fff;
/// The flag that forbids the random generator, set to a command's
/// [`ONE_MACHINE`] while it executes.
const RANDOM_FORBIDDEN: usize = 0x1fd10c4;
/// The function object the execution runs a command through, a
/// `std::function` of 0x40 bytes: its vtable, whose `+0x10` is [`EXECUTE`]
/// and `+0x20` destroys it (`(object, 0)` for one in place), the command at
/// [`LAMBDA_COMMAND`], and the object's own address at `+0x38`; and `(object)`,
/// which runs it.
const LAMBDA_VTABLE: usize = 0x1d8c8b0;
const LAMBDA_DESTROY: usize = 0x20;
const LAMBDA_SELF: usize = 0x38;
const LAMBDA_SIZE: usize = 0x40;
const RUN_LAMBDA: GameFunction<1> = GameFunction::at(0x14d4e70);

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
        Point::method(TEXT_TOKENIZER_VTABLE, 0, TEXT_TOKENIZER_DESTROY.rva(), "text tokenizer destructor"),
        Point::keep(READER, "48 89 5c 24 10 4c 89 44 24 18 48 89 4c 24 08 55 56 57 48 83 ec 20 49 8b f9 49 8b f0", "make a reader"),
        Point::keep(END_READER, "40 53 48 83 ec 20 80 b9 80 06 00 00 00 48 8d 05 6c 5e 6b 00", "end a reader"),
        Point::keep(READER_DELETE, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 8c 02 00 00 f6 c3 01 74 0d ba a8 06 00 00", "a reader's size")
            .uses(&[READER_SIZE]),
        Point::keep(READ_TOKEN, "48 89 5c 24 10 57 48 81 ec 30 02 00 00 48 8b 59 28 48 8b f9", "read a token"),
        Point::keep(TOKEN_STORED, "48 8d 57 30", "a token stored in its reader").uses(&[READER_TOKEN]),
        Point::keep(MAKE, "48 89 5c 24 08 55 56 57 48 8d 6c 24 c0 48 81 ec 40 01 00 00 41 8b f8", "make a command"),
        Point::keep(UPDATE_STATE, "8b 47 48 83 f8 07", "session state").uses(&[session_fields::STATE]),
        Point::keep(
            UPDATE_RUNS_QUEUE,
            "83 f8 0f 77 43 b9 14 c0 00 00 0f a3 c1 73 39 c6 87 14 08 00 00 01 80 bf d4 07 00 00 00 75 1d 80 bf 41 08 00 00 00 75 14 \
             48 8b 4f 58 48 8b 01 33 d2 ff 50 38 48 8b cf e8 56 02 00 00",
            "the session's update runs its queue",
        )
        .uses(&[session::RUNS_QUEUE as usize, session_fields::EXECUTING, session_fields::HELD]),
        Point::keep(
            UPDATE_RUNS_QUEUE_RUNNING,
            "80 bf d4 07 00 00 00 75 62 80 bf 41 08 00 00 00 75 59 83 7f 48 06 0f 85 c2 01 00 00 80 bf 40 08 00 00 00",
            "the running session's update runs its queue",
        )
        .uses(&[session_fields::EXECUTING, session_fields::HELD, session_fields::STATE, session::RUNNING as usize, session_fields::STARTING]),
        Point::keep(SESSION_STAMPS, "8b 87 8c 00 00 00 b9 ff 7f 00 00 89 43 38 48 8b d3 8b 47 68 3b c1 66 0f 4c c8 66 89 4b 48", "a posted command stamped")
            .uses(&[session_fields::SENDER, TICK_LIMIT as usize, SENDER, session_fields::TICK, TICK]),
        Point::keep(
            SESSION_EXECUTES,
            "8b 4f 50 8d 41 fc a9 fc ff ff ff 75 0e 83 f9 05 74 09 48 8b 06 48 8b ce ff 50 68 4c 8d b7 d4 07 00 00 4c 89 75 90 \
             45 0f b6 3e 44 88 7d 98 41 c6 06 01 c6 45 99 01 0f b6 46 4a 0f b6 1d ab 61 b0 00 88 9d e0 01 00 00 88 05 9f 61 b0 00 \
             48 8d 05 84 19 8c 00 48 89 44 24 50 48 89 74 24 58 48 8d 44 24 50 48 89 45 88 48 8d 4c 24 50 e8 27 9f 00 00 90 \
             48 8b 4d 88 48 85 c9 74 12 48 8b 01 48 8d 54 24 50 48 3b ca 0f 95 c2 ff 50 20 90 88 1d 59 61 b0 00 45 88 3e \
             8b 4f 50 8d 41 fc a9 fc ff ff ff 75 20 83 f9 05 74 1b 48 8b 06 48 8b ce ff 50 70",
            "a command executed",
        )
        .uses(&[session_fields::MODE, BEFORE, session_fields::EXECUTING, ONE_MACHINE, RANDOM_FORBIDDEN, LAMBDA_VTABLE, LAMBDA_DESTROY, AFTER]),
        Point::method(LAMBDA_VTABLE, 0x10, EXECUTE, "command execution"),
        Point::keep(RUN_LAMBDA, "48 89 4c 24 08 48 83 ec 38 e8 52 ff ff ff 0f b6 c0 85 c0 74", "run a function object"),
    ]
}

/// Why [`Commands::post`] did not post a command.
#[derive(Debug)]
pub enum Refused {
    /// The game could not be read or called, as the failure says.
    Game(Fail),
    /// The text names no command of the game's, or the game read it
    /// otherwise than it is written: how.
    Unread(String),
    /// The command's own check refuses it; it gives no reason.
    Check,
    /// The game's session executes no commands now.
    NotTaken,
}

impl From<Fail> for Refused {
    fn from(fail: Fail) -> Self {
        Self::Game(fail)
    }
}

impl From<String> for Refused {
    fn from(message: String) -> Self {
        Self::Game(message.into())
    }
}

/// Where a capability's commands go: the game's session, or in a test a
/// record kept on the test's thread ([`record`]). The session a capability
/// reads carries it (`Session::commands`), so that a capability posts
/// through its session alone and a test of it, over a session the test
/// builds, sees what it posted. A capability carries out a panel's action
/// through `super::action`, which words its refusals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Commands {
    Game,
    #[cfg(test)]
    Recorded,
}

impl Commands {
    /// Has the game execute the command `text` writes, such as
    /// `buildunit={ country="FRA" province=183 unit="western_medieval_infantry" }`,
    /// at once (ADR commands-run-at-once), for whichever country the text
    /// names: as the session's update executes a command the game's interface
    /// posted, but now and alone. When this returns, the game has carried it
    /// out. A key left out takes the game's default. Only on the main thread,
    /// at a safe point.
    pub fn post(self, text: &str) -> Result<(), Refused> {
        self.post_with(text, |execute| execute())
    }

    /// [`Commands::post`], with `around` called to execute the command.
    pub fn post_with(self, text: &str, around: impl FnOnce(&dyn Fn() -> GResult<()>) -> GResult<()>) -> Result<(), Refused> {
        match self {
            Self::Game => post_with(text, around),
            #[cfg(test)]
            Self::Recorded => record::post(text, around),
        }
    }
}

/// What a test's capabilities posted through [`Commands::Recorded`], kept on
/// the test's own thread, and the refusals the test has them answered with.
#[cfg(test)]
pub mod record {
    use super::{GResult, Refused};
    use std::{cell::RefCell, collections::VecDeque};

    #[derive(Default)]
    struct Record {
        posted: Vec<String>,
        refusals: VecDeque<Refused>,
    }

    thread_local! {
        static RECORD: RefCell<Record> = RefCell::default();
    }

    /// Records `text` as posted, `around` called as the game's execution
    /// is, unless a refusal waits: then answers with it, and records
    /// nothing.
    pub(super) fn post(text: &str, around: impl FnOnce(&dyn Fn() -> GResult<()>) -> GResult<()>) -> Result<(), Refused> {
        if let Some(refused) = RECORD.with(|r| r.borrow_mut().refusals.pop_front()) {
            return Err(refused);
        }
        around(&|| Ok(()))?;
        RECORD.with(|r| r.borrow_mut().posted.push(text.to_string()));
        Ok(())
    }

    /// Has the next post answered with `refused`.
    pub fn refuse_next(refused: Refused) {
        RECORD.with(|r| r.borrow_mut().refusals.push_back(refused));
    }

    /// The texts posted since the last call, in order.
    pub fn posted() -> Vec<String> {
        RECORD.with(|r| std::mem::take(&mut r.borrow_mut().posted))
    }
}

/// [`Commands::post_with`] for the game.
fn post_with(text: &str, around: impl FnOnce(&dyn Fn() -> GResult<()>) -> GResult<()>) -> Result<(), Refused> {
    let given = Written::parse(text).map_err(Refused::Unread)?;
    let mut operation = crate::log::Operation::begin(&given.kind);
    let m = game()?;
    // The game's own commands, such as the speed's, go also while the player
    // observes.
    let world = world_context(m)?;
    // Once the command is made, only a call into the game can fail before it
    // is destroyed. A command a call into the game failed on is left alone.
    let session = command_session(m, &world)?;
    if crate::thread_id() != crate::main_thread() {
        return Err(Refused::NotTaken);
    }
    if !executes(m, session)? {
        let flag = |field| m.u8(session + field).unwrap_or(0xff);
        log!(
            "commands: the session executes no commands now (state {}, executing {}, held {}, starting {})",
            m.u32(session + session_fields::STATE).unwrap_or(u32::MAX),
            flag(session_fields::EXECUTING),
            flag(session_fields::HELD),
            flag(session_fields::STARTING)
        );
        return Err(Refused::NotTaken);
    }
    let Some(command) = read(m, &game_text::from_str(text))? else {
        return Err(Refused::Unread(format!("the game has no command {:?}", given.kind)));
    };
    if let Some(refusal) = refused(m, command, &given)? {
        m.call(m.vslot(command, DESTROY)?, &[command, FREE])?;
        return Err(refusal);
    }
    stamp(m, session, command)?;
    let executed = EXECUTED.load(Ordering::Relaxed);
    around(&|| execute(m, session, command))?;
    m.call(m.vslot(command, DESTROY)?, &[command, FREE])?;
    if EXECUTED.load(Ordering::Relaxed) == executed {
        return Err("the game did not execute the command".to_string().into());
    }
    operation.succeeded();
    Ok(())
}

/// Has the game read the command `text` writes, as [`Commands::post`] reads it, and
/// gives it to `look` unexecuted, then destroys it: for a capability that
/// asks a command's checks before it posts it, or without posting it.
/// [`Refused::Unread`] for a text the game does not read as written;
/// [`Refused::Check`] is never answered.
pub fn with_command<T>(written: &str, look: impl FnOnce(&Memory, usize) -> GResult<T>) -> Result<T, Refused> {
    let given = Written::parse(written).map_err(Refused::Unread)?;
    let m = game()?;
    let Some(command) = read(m, &game_text::from_str(written))? else {
        return Err(Refused::Unread(format!("the game has no command {:?}", given.kind)));
    };
    let looked = match Written::parse(&text(m, command)?).map(|again| given.came_back(&again)) {
        Ok(Ok(())) => look(m, command).map_err(Refused::Game),
        Ok(Err(how)) | Err(how) => Err(Refused::Unread(how)),
    };
    m.call(m.vslot(command, DESTROY)?, &[command, FREE])?;
    looked
}

/// Whether the command `text` writes would pass its own check now, as
/// [`Commands::post`] asks it before executing it, without executing it: for a panel's
/// read of what its buttons would do. Refused only when the game reads the
/// text otherwise than it is written. Only on the main thread, at a safe
/// point.
pub fn passes(text: &str) -> Result<bool, Refused> {
    let given = Written::parse(text).map_err(Refused::Unread)?;
    let m = game()?;
    context(m)?;
    let Some(command) = read(m, &game_text::from_str(text))? else {
        return Err(Refused::Unread(format!("the game has no command {:?}", given.kind)));
    };
    let refusal = refused(m, command, &given)?;
    m.call(m.vslot(command, DESTROY)?, &[command, FREE])?;
    match refusal {
        None => Ok(true),
        Some(Refused::Check) => Ok(false),
        Some(refusal) => Err(refusal),
    }
}

/// Whether the session's update would execute commands now: in a state in
/// which it runs its queue, with no command executing and none held back.
fn executes(m: &Memory, session: usize) -> GResult<bool> {
    let state = m.u32(session + session_fields::STATE)?;
    let runs = (state < 32 && session::RUNS_QUEUE >> state & 1 == 1) || (state == session::RUNNING && m.u8(session + session_fields::STARTING)? == 0);
    Ok(runs && m.u8(session + session_fields::EXECUTING)? == 0 && m.u8(session + session_fields::HELD)? == 0)
}

/// Stamps the command with the session's sender and tick, as the session's
/// post does.
fn stamp(m: &Memory, session: usize, command: usize) -> GResult<()> {
    let sender = m.i32(session + session_fields::SENDER)?;
    let tick = m.i32(session + session_fields::TICK)?;
    let tick = if tick < TICK_LIMIT as i32 { tick as u16 } else { TICK_LIMIT as u16 };
    m.write(command + SENDER, &sender.to_le_bytes())?;
    m.write(command + TICK, &tick.to_le_bytes())?;
    Ok(())
}

/// Executes the command as the session's execution does one that passed its
/// check: the steps before and after it in the modes that call them, the
/// session's flag that a command executes and the random generator's, set
/// while it runs through the function object the game logs it from and the
/// layer hears it at. Not through the session's queue, so that what the
/// game posted before it, such as the next day, runs at the next frame as it
/// would have.
fn execute(m: &Memory, session: usize, command: usize) -> GResult<()> {
    let mode = m.u32(session + session_fields::MODE)?;
    let around = (4..=7).contains(&mode) && mode != 5;
    if around {
        m.call(m.vslot(command, BEFORE)?, &[command])?;
    }
    let executing = m.u8(session + session_fields::EXECUTING)?;
    let random = m.u8(m.at(RANDOM_FORBIDDEN))?;
    m.write(session + session_fields::EXECUTING, &[1])?;
    m.write(m.at(RANDOM_FORBIDDEN), &[m.u8(command + ONE_MACHINE)?])?;
    let mut lambda = Object::<LAMBDA_SIZE>::new();
    let at = lambda.at();
    lambda.set(0, &m.at(LAMBDA_VTABLE).to_le_bytes());
    lambda.set(LAMBDA_COMMAND, &command.to_le_bytes());
    lambda.set(LAMBDA_SELF, &at.to_le_bytes());
    let ran = RUN_LAMBDA.call(m, [at]).and_then(|_| m.call(m.vslot(at, LAMBDA_DESTROY)?, &[at, 0]));
    m.write(m.at(RANDOM_FORBIDDEN), &[random])?;
    m.write(session + session_fields::EXECUTING, &[executing])?;
    ran?;
    if around {
        m.call(m.vslot(command, AFTER)?, &[command])?;
    }
    Ok(())
}

/// The command at `command`, one the game made, in its written form on one
/// line, as the game writes it: for a capability that has the game make its
/// command, to [`Commands::post`] it as text.
pub fn text(m: &Memory, command: usize) -> GResult<String> {
    let token = m.call(m.vslot(command, TYPE)?, &[command])? as u32;
    Ok(one_line(&game_text::to_utf8(&written(m, command, token)?)))
}

/// Posts dev.toml's test commands (`[test-switches]` `post`), at the first
/// frame of a game, and logs what came of each.
#[cfg(feature = "dev-test-switches")]
pub fn post_test_commands() {
    for text in &crate::dev::options().test_switches.post {
        match Commands::Game.post(text) {
            Ok(()) => log!("test command carried out: {text}"),
            Err(refused) => log!("test command not posted ({refused:?}): {text}"),
        }
    }
}

/// Why the command read from `given` must not be posted: a key that did not
/// come back as given, or its check; None when it may be.
fn refused(m: &Memory, command: usize, given: &Written) -> GResult<Option<Refused>> {
    let again = Written::parse(&text(m, command)?)?;
    if let Err(how) = given.came_back(&again) {
        return Ok(Some(Refused::Unread(how)));
    }
    if m.call(m.vslot(command, CHECK)?, &[command])? as u8 == 0 {
        return Ok(Some(Refused::Check));
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
/// How many commands the game executed, for [`post_with`] to tell that its
/// command ran.
static EXECUTED: AtomicU64 = AtomicU64::new(0);
static WRITING_BROKEN: AtomicBool = AtomicBool::new(false);

/// Calls `heard` with every command the game executes, the player's, the
/// AI's and the game's own, on the main thread just before it runs. A
/// command's sender does not tell the player's commands from the AI's.
pub fn listen(heard: fn(&Heard)) {
    LISTENERS.lock().unwrap_or_else(|e| e.into_inner()).push(heard);
    LISTENING.store(true, Ordering::Relaxed);
}

fn executing(call: &mut Call) {
    EXECUTED.fetch_add(1, Ordering::Relaxed);
    if !LISTENING.load(Ordering::Relaxed) {
        return;
    }
    let m = call.memory();
    let Ok(command) = m.ptr(call.arg(0) + LAMBDA_COMMAND) else { return };
    let Ok(token) = m.vslot(command, TYPE).and_then(|f| m.call(f, &[command])) else { return };
    let token = token as u32;
    let kind = type_name(&m, token).unwrap_or_else(|| format!("token {token}"));
    let heard = Heard { m, command, token, kind, id: m.u32(command + ID).unwrap_or(0), text: OnceCell::new() };
    let listeners = LISTENERS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for listener in listeners {
        listener(&heard);
    }
}

/// The name of token `token`: a command's type, or the key of an object
/// written by its token, such as a culture.
pub fn type_name(m: &Memory, token: u32) -> Option<String> {
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
    let result = WRITE_KEY.call(m, [writer, token as usize]).and_then(|_| m.call(m.vslot(command, WRITE_BLOCK)?, &[command, writer])).map(|_| ());
    // The game grew the vector through the stream.
    let bytes = unsafe { std::ptr::read_volatile(&bytes) };
    let result = result.and_then(|_| m.bytes(bytes[0], bytes[1] - bytes[0]));
    crate::game_heap::free_vector(m, bytes)?;
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
    TOKENIZER.call(m, [tokenizer, stream.at(), 0])?;
    unsafe { *(tokenizer as *mut usize) = m.at(TEXT_TOKENIZER_VTABLE) };
    let made = READER.call(m, [reader, tokenizer, strings.as_mut_ptr() as usize, name.at(), 0]).and_then(|_| {
        let made = READ_TOKEN.call(m, [reader]).and_then(|_| {
            let token = m.u32(reader + READER_TOKEN)?;
            let command = MAKE.call(m, [factory.as_mut_ptr() as usize, reader, token as usize])?;
            if command != 0 {
                if let Err(e) = m.call(m.vslot(command, READ_BLOCK)?, &[command, reader]) {
                    m.call(m.vslot(command, DESTROY)?, &[command, FREE])?;
                    return Err(e);
                }
            }
            Ok(command)
        });
        END_READER.call(m, [reader])?;
        made
    });
    TEXT_TOKENIZER_DESTROY.call(m, [tokenizer, 0])?;
    let command = made?;
    Ok((command != 0).then_some(command))
}

/// `text` on one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------- the written form ----------

/// A block of a command's written form, as a capability writes it: its
/// entries in the order given, each one of the game's keys with its value,
/// or a value alone, as in a list. [`write`] makes a command of it, in the
/// same form the layer reads back ([`Written`]).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Block(Vec<Entry>);

impl Block {
    pub fn new() -> Self {
        Self::default()
    }

    /// `key=value`: a number, `yes` or `no`, or anything else the game
    /// writes unquoted; it holds no space, `=`, brace or quote.
    pub fn word(self, key: &str, value: impl Display) -> Self {
        let value = value.to_string();
        debug_assert!(!value.is_empty() && !value.contains(|c: char| c.is_whitespace() || matches!(c, '=' | '{' | '}' | '"')), "{value:?} is not a word");
        self.entry(Some(key), Value::Word(value))
    }

    /// `key="text"`, such as a content key; the game's reader takes the text
    /// up to the next quote, so it holds none.
    pub fn quoted(self, key: &str, text: impl Display) -> Self {
        let text = text.to_string();
        debug_assert!(!text.contains('"'), "{text:?} holds a quote");
        self.entry(Some(key), Value::Quoted(text))
    }

    /// `key="FRA"`: a country by its tag's letters.
    pub fn tag(self, key: &str, letters: [u8; 3]) -> Self {
        self.quoted(key, String::from_utf8_lossy(&letters))
    }

    /// `key={ id=… type=… }`: an ID as the game writes a unit's or a
    /// leader's, its number in the high 32 bits and its type in the low.
    pub fn id(self, key: &str, id: u64) -> Self {
        self.block(key, Block::new().word("id", (id >> 32) as i32).word("type", id as u32 as i32))
    }

    /// `key={ … }`.
    pub fn block(self, key: &str, block: Block) -> Self {
        self.entry(Some(key), Value::Block(block.0))
    }

    /// A value alone, as in a list: the `1` and `2` of `{ 1 2 }`.
    pub fn value(self, value: impl Display) -> Self {
        let value = value.to_string();
        debug_assert!(!value.is_empty() && !value.contains(|c: char| c.is_whitespace() || matches!(c, '=' | '{' | '}' | '"')), "{value:?} is not a word");
        self.entry(None, Value::Word(value))
    }

    fn entry(mut self, key: Option<&str>, value: Value) -> Self {
        self.0.push(Entry { key: key.map(str::to_string), value });
        self
    }
}

/// The command of the type `kind` with `block`, in its written form, as
/// [`Commands::post`] takes it: `kind={ key=value … }`.
pub fn write(kind: &str, block: Block) -> String {
    Written { kind: kind.to_string(), entries: block.0 }.to_string()
}

/// A value in the written form: a word, a quoted text, or a block of
/// entries. A word and a quoted text of the same characters are one value
/// to the game's reader; they are told apart only to be written as given.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Word(String),
    Quoted(String),
    Block(Vec<Entry>),
}

impl Value {
    /// Its characters, when it is a word or a quoted text.
    fn word(&self) -> Option<&str> {
        match self {
            Value::Word(w) | Value::Quoted(w) => Some(w),
            Value::Block(_) => None,
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Word(w) => f.write_str(w),
            Value::Quoted(t) => write!(f, "\"{t}\""),
            Value::Block(entries) => braced(entries, f),
        }
    }
}

/// `{ entry entry }`, and `{ }` for none.
fn braced(entries: &[Entry], f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("{ ")?;
    for entry in entries {
        write!(f, "{entry} ")?;
    }
    f.write_str("}")
}

/// An entry of a block: `key=value`, or a value alone, as in a list.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    key: Option<String>,
    value: Value,
}

impl Display for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.key {
            Some(key) => write!(f, "{key}={}", self.value),
            None => write!(f, "{}", self.value),
        }
    }
}

/// A command in its written form: `kind={ … }`.
#[derive(Debug, PartialEq)]
struct Written {
    kind: String,
    entries: Vec<Entry>,
}

impl Display for Written {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}=", self.kind)?;
        braced(&self.entries, f)
    }
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
    if let (Value::Block(g), Value::Block(a)) = (given, again) {
        return came_back(g, a, path);
    }
    match (given.word(), again.word()) {
        (Some(g), Some(a)) => {
            let number = |w: &str| w.parse::<f64>().ok();
            match g == a || number(g).is_some_and(|g| number(a) == Some(g)) {
                true => Ok(()),
                false => Err(format!("the game read {path}={g} as {a}")),
            }
        }
        (Some(g), _) => Err(format!("the game read {path}={g} as a block")),
        (_, a) => Err(format!("the game read the block {path} as {}", a.unwrap_or_default())),
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    Quoted(String),
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
            '"' => tokens.push(Token::Quoted(chars.by_ref().take_while(|&c| c != '"').collect())),
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
            Some(Token::Word(key) | Token::Quoted(key)) if tokens.get(*at + 1) == Some(&Token::Equals) => {
                *at += 2;
                let value = value(tokens, at).map_err(|e| format!("{key}= {e}"))?;
                entries.push(Entry { key: Some(key.clone()), value });
            }
            Some(_) => entries.push(Entry { key: None, value: value(tokens, at)? }),
        }
    }
}

/// The value at `at`: a word, a quoted text, or a block to its `}`.
fn value(tokens: &[Token], at: &mut usize) -> crate::Result<Value> {
    match tokens.get(*at) {
        Some(Token::Word(word)) => {
            *at += 1;
            Ok(Value::Word(word.clone()))
        }
        Some(Token::Quoted(text)) => {
            *at += 1;
            Ok(Value::Quoted(text.clone()))
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
    fn a_command_is_written_as_the_layer_reads_it_back() {
        let block = Block::new()
            .tag("country", *b"FRA")
            .word("province", 183)
            .quoted("unit", "western_medieval_infantry")
            .block("members", Block::new().word("dummy", 2).value("yes").value("no"))
            .id("leader", 12 << 32 | 49)
            .block("empty", Block::new());
        let text = write("buildunit", block.clone());
        assert_eq!(
            text,
            r#"buildunit={ country="FRA" province=183 unit="western_medieval_infantry" members={ dummy=2 yes no } leader={ id=12 type=49 } empty={ } }"#
        );
        assert_eq!(Written::parse(&text).unwrap(), Written { kind: "buildunit".into(), entries: block.0 });
        Written::parse(&text).unwrap().came_back(&Written::parse(&text).unwrap()).unwrap();
    }

    #[test]
    fn a_command_of_no_keys_and_a_negative_id() {
        assert_eq!(write("increase_stability", Block::new()), "increase_stability={ }");
        assert_eq!(write("u", Block::new().id("unit", u64::MAX)), "u={ unit={ id=-1 type=-1 } }");
    }

    #[test]
    fn a_quoted_text_comes_back_as_the_same_word() {
        let given = Written::parse(r#"c={ country="FRA" }"#).unwrap();
        given.came_back(&Written::parse("c={ country=FRA }").unwrap()).unwrap();
        assert!(given.came_back(&Written::parse("c={ country={ FRA } }").unwrap()).unwrap_err().ends_with("as a block"));
    }

    #[test]
    fn another_type_is_refused() {
        let given = Written::parse("a={ }").unwrap();
        assert!(given.came_back(&Written::parse("b={ }").unwrap()).is_err());
    }
}
