//! Remote control's transport (ADR 0016): the named pipe
//! `\\.\pipe\eu4-harmony`, open to the Windows user running the game and
//! nobody else (ADR 0017), with one JSON request per line in and one JSON
//! response per line out (`request_from_json`, `response_json`).
//!
//! Every remote client has a pipe instance and a thread of its own, so
//! several are served at once. A client's requests are handled one after
//! another: the next line is read once the answer to the one before is
//! written and the client has read it. A client so gets its answers in the
//! order it sent its requests, and none of its requests overtakes an earlier
//! one, such as one that waits for a game. A client that wants to wait for
//! events while it does other things opens the pipe a second time.
//!
//! A client gives a request up by closing the pipe before the answer comes,
//! even with requests it wrote after it still unread: the transport drops the
//! request's reply, so the engine never runs it if it has not yet. The pipe
//! lasts as long as the process. When the game ends, or
//! returns to the main menu, which starts `eu4.exe` again
//! (docs/research/safe-points.md), every client finds its pipe broken; the
//! new process opens the pipe once the old one is gone. Returning, the
//! engine first answers every request (`remote::Engine::end`), and the
//! process waits, up to 2 s, until the transport has written every answer
//! and its client has read it, however large.
//!
//! The transport only reads and writes: it hands the engine typed requests and
//! never touches game state.
use crate::remote::Reply;
use eu4_harmony_remote::{request_from_json, response_json, Code, Error, Request, Response};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read as _, Write as _},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use windows_sys::{
    Wdk::Storage::FileSystem::{FilePipeLocalInformation, NtQueryInformationFile, FILE_PIPE_CONNECTED_STATE, FILE_PIPE_LOCAL_INFORMATION},
    Win32::{
        Foundation::{LocalFree, ERROR_ACCESS_DENIED, ERROR_NO_DATA, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE},
        Security::{
            Authorization::{ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
            GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
        },
        Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX},
        System::{
            Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT},
            Threading::{GetCurrentProcess, OpenProcessToken},
            IO::IO_STATUS_BLOCK,
        },
    },
};

/// The pipe remote clients open.
pub const NAME: &str = eu4_harmony_remote::PIPE;
/// The longest request line read, in bytes, without its line break.
pub const LONGEST_LINE: usize = 1 << 20;
/// How often a client waiting for its answer is checked for being still
/// there.
const CHECK: Duration = Duration::from_millis(50);
/// How long to wait before trying again to open the pipe.
const RETRY: Duration = Duration::from_secs(1);
/// The pipe's buffers, each way, in bytes.
const BUFFER: u32 = 64 * 1024;

/// Opens the pipe for the game's remote control, from now on, on threads of
/// its own.
pub fn start() {
    if let Err(e) = serve(NAME, crate::remote::submit, |line| log!("{line}")) {
        log!("remote pipe: cannot open {NAME}: {e}");
    }
}

/// Serves the pipe `name`, handing its requests to `submit` and writing what
/// happens to `log`: a thread waits for clients, and each client gets a
/// thread of its own.
pub fn serve(name: &str, submit: fn(Request) -> Reply, log: fn(&str)) -> io::Result<()> {
    let pipe = Arc::new(Pipe::new(name, submit, log)?);
    std::thread::Builder::new().name("remote pipe".into()).spawn(move || guarded("remote control's pipe", || listen(&pipe)))?;
    Ok(())
}

/// Runs a thread's work. In the game a panic there is a framework bug, which
/// ends the game (`crate::guarded`); a test only fails.
fn guarded(what: &str, f: impl FnOnce()) {
    #[cfg(not(test))]
    crate::guarded(what, f);
    #[cfg(test)]
    {
        let _ = what;
        f();
    }
}

/// A pipe being served.
struct Pipe {
    name: String,
    /// The name as Windows takes it, NUL-terminated.
    wide: Vec<u16>,
    security: Security,
    submit: fn(Request) -> Reply,
    log: fn(&str),
    /// How many clients came so far.
    clients: AtomicU64,
}

impl Pipe {
    fn new(name: &str, submit: fn(Request) -> Reply, log: fn(&str)) -> io::Result<Self> {
        let wide = name.encode_utf16().chain([0]).collect();
        Ok(Self { name: name.into(), wide, security: Security::current_user()?, submit, log, clients: AtomicU64::new(0) })
    }

    fn log(&self, line: &str) {
        (self.log)(&format!("remote pipe: {line}"));
    }

    /// A new instance of the pipe, for the next client. The `first` one fails
    /// while any process has the pipe open, this one included.
    fn instance(&self, first: bool) -> io::Result<File> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.security.descriptor,
            bInheritHandle: 0,
        };
        let open = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        let handle = unsafe { CreateNamedPipeW(self.wide.as_ptr(), open, mode, PIPE_UNLIMITED_INSTANCES, BUFFER, BUFFER, 0, &attributes) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    /// The next instance, trying again until there is one. Made while the
    /// process still has one ([`accept`]), it joins the process's own pipe.
    fn next_instance(&self) -> File {
        loop {
            match self.instance(false) {
                Ok(instance) => return instance,
                Err(e) => {
                    self.log(&format!("cannot make room for the next client: {e}; trying again"));
                    std::thread::sleep(RETRY);
                }
            }
        }
    }
}

/// Waits for clients, one at a time, and hands each to a thread of its own.
fn listen(pipe: &Arc<Pipe>) {
    // Another process may have the pipe: the game that returned to the main
    // menu, which ends a few seconds after this one starts, or a second game.
    let mut told = false;
    let mut instance = loop {
        match pipe.instance(true) {
            Ok(instance) => break instance,
            Err(e) => {
                if !told {
                    let why = if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) { "another process has it".to_string() } else { e.to_string() };
                    pipe.log(&format!("cannot open {} yet ({why}); trying again every second", pipe.name));
                    told = true;
                }
                std::thread::sleep(RETRY);
            }
        }
    };
    pipe.log(&format!("open at {}, to {} only", pipe.name, pipe.security.user));
    loop {
        instance = accept(pipe, instance);
    }
}

/// Waits for a client on `instance` and hands it to a thread of its own;
/// returns the instance for the client after it.
///
/// That one is made before `instance` is handed on or closed, so the process
/// has an instance of the pipe all along. Were it to have none for a moment,
/// as after a client that closed the pipe at once, the name would lapse:
/// another process could make a pipe of that name, and the next instance
/// would join it, under that process's access list.
fn accept(pipe: &Arc<Pipe>, instance: File) -> File {
    let connected = unsafe { ConnectNamedPipe(instance.as_raw_handle(), std::ptr::null_mut()) } != 0;
    let error = io::Error::last_os_error();
    let next = pipe.next_instance();
    if connected || error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
        let number = pipe.clients.fetch_add(1, Ordering::Relaxed) + 1;
        let client = Arc::clone(pipe);
        let spawned = std::thread::Builder::new()
            .name(format!("remote pipe client {number}"))
            .spawn(move || guarded("remote control's pipe", || serve_client(&client, &instance, number)));
        if let Err(e) = spawned {
            pipe.log(&format!("client {number} cannot be served: {e}"));
        }
    } else if error.raw_os_error() == Some(ERROR_NO_DATA as i32) {
        // The client came and went before it was waited for.
        drop(instance);
    } else {
        pipe.log(&format!("cannot wait for a client: {error}; trying again"));
        drop(instance);
        std::thread::sleep(RETRY);
    }
    next
}

/// Answers a client's requests, one line at a time, until it closes the
/// pipe.
fn serve_client(pipe: &Pipe, instance: &File, number: u64) {
    let mut reader = BufReader::with_capacity(BUFFER as usize, instance);
    let mut answered = 0u64;
    let end = loop {
        let line = match read_line(&mut reader) {
            Ok(Some(line)) => line,
            Ok(None) => break "closed the pipe".to_string(),
            Err(e) => break format!("cannot be read from ({e})"),
        };
        let request = match line {
            Line::Text(text) if text.iter().all(u8::is_ascii_whitespace) => continue,
            Line::Text(text) => std::str::from_utf8(&text).map_err(|_| Error::new(Code::BadRequest, "a request is UTF-8 text")).and_then(request_from_json),
            Line::TooLong => Err(Error::new(Code::BadRequest, format!("a request is one line of at most {LONGEST_LINE} bytes"))),
        };
        // The reply is kept until the answer is written and read: the
        // process ending as the game starts again waits for that.
        let (response, reply) = match request {
            Ok(request) => {
                let reply = (pipe.submit)(request);
                match answer(&reply, instance) {
                    Some(response) => (response, Some(reply)),
                    None => break "closed the pipe before its answer came, which gave the request up".to_string(),
                }
            }
            Err(error) => {
                pipe.log(&format!("client {number}: {error}"));
                (Err(error), None)
            }
        };
        let mut text = response_json(&response).to_string();
        text.push('\n');
        if let Err(e) = write(instance, text.as_bytes()) {
            break format!("went before its answer was written ({e})");
        }
        drop(reply);
        answered += 1;
    };
    pipe.log(&format!("client {number} {end}, after {answered} answer(s)"));
}

/// A line a client sent.
enum Line {
    /// Without its line break.
    Text(Vec<u8>),
    /// Longer than [`LONGEST_LINE`]; skipped to its end.
    TooLong,
}

/// The next line; None once the client closed the pipe, a line it left
/// unfinished included.
fn read_line(reader: &mut impl BufRead) -> io::Result<Option<Line>> {
    // The longest line, its line break written as CR LF.
    const MOST: usize = LONGEST_LINE + 2;
    let mut line = Vec::new();
    let read = reader.by_ref().take(MOST as u64).read_until(b'\n', &mut line)?;
    if line.last() == Some(&b'\n') {
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        return Ok(Some(if line.len() <= LONGEST_LINE { Line::Text(line) } else { Line::TooLong }));
    }
    if read < MOST {
        return Ok(None);
    }
    // Too long: what is left of it is skipped.
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(None);
        }
        match buffer.iter().position(|&b| b == b'\n') {
            Some(end) => {
                reader.consume(end + 1);
                return Ok(Some(Line::TooLong));
            }
            None => {
                let all = buffer.len();
                reader.consume(all);
            }
        }
    }
}

/// Writes `bytes` to the client and waits until it has read them all
/// (`FlushFileBuffers`): an answer larger than the pipe's buffer is read as
/// it is written, and the process may end right after.
fn write(mut instance: &File, bytes: &[u8]) -> io::Result<()> {
    instance.write_all(bytes)?;
    instance.sync_all()
}

/// Waits for the answer while the client is there; None once it is gone,
/// when the caller drops the reply and so gives the request up.
fn answer(reply: &Reply, instance: &File) -> Option<Response> {
    loop {
        if let Some(response) = reply.wait(CHECK) {
            return Some(response);
        }
        if !open(instance) {
            return None;
        }
    }
}

/// Whether the client still has the pipe open, as the pipe's state tells:
/// connected, or closing once the client closed it. PeekNamedPipe cannot
/// tell: it goes on succeeding after the client closed the pipe while bytes
/// it wrote are still unread, such as its next request.
fn open(instance: &File) -> bool {
    let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
    let mut pipe = FILE_PIPE_LOCAL_INFORMATION::default();
    let size = size_of::<FILE_PIPE_LOCAL_INFORMATION>() as u32;
    let queried = unsafe { NtQueryInformationFile(instance.as_raw_handle(), &mut status, (&raw mut pipe).cast(), size, FilePipeLocalInformation) };
    queried >= 0 && pipe.NamedPipeState == FILE_PIPE_CONNECTED_STATE
}

/// A security descriptor that lets the Windows user the process runs as, and
/// nobody else, use the pipe.
struct Security {
    descriptor: PSECURITY_DESCRIPTOR,
    /// The user's SID, such as `S-1-5-21-…`.
    user: String,
}

// The descriptor is only read once made.
unsafe impl Send for Security {}
unsafe impl Sync for Security {}

impl Security {
    fn current_user() -> io::Result<Self> {
        let user = user_sid()?;
        // A protected DACL with one entry: all access for the user.
        let sddl: Vec<u16> = format!("D:P(A;;GA;;;{user})").encode_utf16().chain([0]).collect();
        let mut descriptor = std::ptr::null_mut();
        if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, std::ptr::null_mut()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { descriptor, user })
    }
}

impl Drop for Security {
    fn drop(&mut self) {
        unsafe { LocalFree(self.descriptor) };
    }
}

/// The SID of the user the process runs as, as text.
fn user_sid() -> io::Result<String> {
    let mut raw = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut size = 0;
    unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, std::ptr::null_mut(), 0, &mut size) };
    // u64s keep TOKEN_USER aligned.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    if unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, buffer.as_mut_ptr().cast(), size, &mut size) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let length = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
    let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) });
    unsafe { LocalFree(text.cast()) };
    Ok(sid)
}

#[cfg(test)]
mod tests;
