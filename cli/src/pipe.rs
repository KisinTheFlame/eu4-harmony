//! The remote client's side of remote control's named pipe (ADR 0016): opens
//! the pipe, makes sure the game run by this Windows user serves it, writes
//! one request line and reads the answer line.
//!
//! Any process may make a pipe of the game's name before the game does, and
//! would then receive the requests. So the client opens the pipe at the
//! identification level only, which keeps the process serving it from acting
//! as this user, and sends nothing until it has checked that process: run by
//! this Windows user, from the game's executable.
//!
//! When the game returns to the main menu it starts again: the old process's
//! pipe breaks, and the new one opens the pipe a few seconds later
//! (docs/remote-control.md, F3). So the client waits for the pipe to appear,
//! and a pipe that breaks is an error at once, never a wait.
use std::{
    fmt,
    fs::File,
    io::{self, Read, Write},
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::PathBuf,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_NOT_CONNECTED, HANDLE},
    Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
    Storage::FileSystem::SECURITY_IDENTIFICATION,
    System::{
        Pipes::{GetNamedPipeServerProcessId, PeekNamedPipe},
        Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

/// The pipe the game serves.
pub const NAME: &str = r"\\.\pipe\eu4-harmony";
/// The game's executable, the only one trusted to serve the pipe.
pub const GAME: &str = "eu4.exe";
/// How often to look for the pipe while it is not there.
const LOOK: Duration = Duration::from_millis(50);
/// How often to look for the answer while none of it has come.
const POLL: Duration = Duration::from_millis(5);
/// The most read from the pipe at once.
const CHUNK: usize = 64 * 1024;

/// Why no answer came.
#[derive(Debug)]
pub enum Failure {
    /// No process opened the pipe in time.
    NotOpen,
    /// A process has the pipe, but it is not open to this Windows user.
    Denied,
    /// The pipe is served by something other than the game run by this
    /// Windows user; nothing was sent.
    NotTheGame(String),
    /// Who serves the pipe cannot be told; nothing was sent.
    Unknown(String),
    /// The pipe broke before the whole answer came.
    Broken,
    /// No answer came in time; closing the pipe gave the request up.
    TimedOut,
    /// Reading or writing the pipe failed otherwise.
    Io(io::Error),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NotOpen => write!(f, "no game with EU4 Harmony opened {NAME} in time: the game is not running, is still starting, or is starting again after returning to the main menu"),
            Self::Denied => write!(f, "{NAME} is there but not open to this Windows user: a game run by another user, or another program, has it"),
            Self::NotTheGame(why) => write!(f, "nothing was sent: {why}"),
            Self::Unknown(why) => write!(f, "nothing was sent: who serves {NAME} cannot be told ({why})"),
            Self::Broken => write!(f, "the pipe broke before the answer came: the game ended, or returned to the main menu, which starts it again; the request may or may not have run"),
            Self::TimedOut => write!(f, "no answer in time; the request was given up, so it never runs if it had not yet"),
            Self::Io(e) => write!(f, "the pipe: {e}"),
        }
    }
}

/// A failure from a read or write on the pipe: a pipe that broke, or else
/// the error as it is.
fn io_failure(e: io::Error) -> Failure {
    match e.raw_os_error().map(|code| code as u32) {
        Some(ERROR_BROKEN_PIPE | ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED) => Failure::Broken,
        _ => Failure::Io(e),
    }
}

/// The process serving a pipe.
#[derive(Debug)]
pub struct Serving {
    pub process: u32,
    /// Its executable.
    pub image: PathBuf,
    /// It runs as the Windows user this process runs as.
    pub same_user: bool,
}

/// A pipe open to the game.
pub struct Client {
    pipe: File,
}

/// Opens the pipe `name`, waiting until `deadline` for a process to open it,
/// once the process serving it is a `game`'s run by this Windows user.
pub fn connect(name: &str, game: &str, deadline: Instant) -> Result<Client, Failure> {
    let pipe = loop {
        match File::options().read(true).write(true).security_qos_flags(SECURITY_IDENTIFICATION).open(name) {
            Ok(pipe) => break pipe,
            Err(e) => match e.raw_os_error().map(|code| code as u32) {
                // Not there yet, or every instance taken for a moment.
                Some(ERROR_FILE_NOT_FOUND | ERROR_PIPE_BUSY) if Instant::now() < deadline => std::thread::sleep(LOOK),
                Some(ERROR_FILE_NOT_FOUND | ERROR_PIPE_BUSY) => return Err(Failure::NotOpen),
                Some(ERROR_ACCESS_DENIED) => return Err(Failure::Denied),
                _ => return Err(Failure::Io(e)),
            },
        }
    };
    check(&serving(&pipe)?, game)?;
    Ok(Client { pipe })
}

/// Whether the process `serving` is the `game` run by this Windows user.
pub fn check(serving: &Serving, game: &str) -> Result<(), Failure> {
    let image = serving.image.display();
    if !serving.same_user {
        return Err(Failure::NotTheGame(format!("{NAME} is served by {image} (process {}), which runs as another Windows user", serving.process)));
    }
    if !serving.image.file_name().is_some_and(|name| name.eq_ignore_ascii_case(game)) {
        return Err(Failure::NotTheGame(format!("{NAME} is served by {image} (process {}), not by the game ({game})", serving.process)));
    }
    Ok(())
}

/// The process serving `pipe`.
pub fn serving(pipe: &File) -> Result<Serving, Failure> {
    let mut process = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut process) } == 0 {
        return Err(Failure::Unknown(format!("its process cannot be read: {}", io::Error::last_os_error())));
    }
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process) };
    if raw.is_null() {
        return Err(Failure::Unknown(format!("process {process} cannot be opened: {}", io::Error::last_os_error())));
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
    let image = image(&handle).map_err(|e| Failure::Unknown(format!("the executable of process {process} cannot be read: {e}")))?;
    let theirs = ProcessUser::of(handle.as_raw_handle()).map_err(|e| Failure::Unknown(format!("the user {} (process {process}) runs as cannot be read: {e}", image.display())))?;
    let ours = ProcessUser::of(unsafe { GetCurrentProcess() }).map_err(|e| Failure::Unknown(format!("this process's user cannot be read: {e}")))?;
    Ok(Serving { process, image, same_user: theirs.is(&ours) })
}

/// The executable of `process`.
fn image(process: &OwnedHandle) -> io::Result<PathBuf> {
    let mut path = vec![0u16; 32 * 1024];
    let mut length = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process.as_raw_handle(), 0, path.as_mut_ptr(), &mut length) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&path[..length as usize])))
}

/// The user a process runs as, from its token.
struct ProcessUser {
    /// A `TOKEN_USER` and the SID after it; u64s keep it aligned.
    buffer: Vec<u64>,
}

impl ProcessUser {
    fn of(process: HANDLE) -> io::Result<Self> {
        let mut raw = std::ptr::null_mut();
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut size = 0;
        unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, std::ptr::null_mut(), 0, &mut size) };
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        if unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, buffer.as_mut_ptr().cast(), size, &mut size) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { buffer })
    }

    /// Whether both are the same user.
    fn is(&self, other: &Self) -> bool {
        let sid = |user: &Self| unsafe { (*user.buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        unsafe { EqualSid(sid(self), sid(other)) != 0 }
    }
}

impl Client {
    /// Sends `request`, one line of JSON, and waits for the answer line until
    /// `deadline`, or as long as it takes without one. The pipe breaking ends
    /// the wait at once.
    pub fn ask(&mut self, request: &str, deadline: Option<Instant>) -> Result<String, Failure> {
        let mut line = request.trim_end().as_bytes().to_vec();
        line.push(b'\n');
        self.pipe.write_all(&line).map_err(io_failure)?;
        self.read_line(deadline)
    }

    /// The next line, without its line break. The pipe is looked at until
    /// bytes are there, so that the wait can end at the deadline.
    fn read_line(&mut self, deadline: Option<Instant>) -> Result<String, Failure> {
        let mut line = Vec::new();
        let mut chunk = vec![0u8; CHUNK];
        loop {
            let mut available = 0u32;
            let peeked = unsafe { PeekNamedPipe(self.pipe.as_raw_handle(), std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut available, std::ptr::null_mut()) };
            if peeked == 0 {
                return Err(io_failure(io::Error::last_os_error()));
            }
            if available == 0 {
                if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    return Err(Failure::TimedOut);
                }
                std::thread::sleep(POLL);
                continue;
            }
            let wanted = (available as usize).min(CHUNK);
            let read = self.pipe.read(&mut chunk[..wanted]).map_err(io_failure)?;
            if read == 0 {
                return Err(Failure::Broken);
            }
            line.extend_from_slice(&chunk[..read]);
            if let Some(end) = line.iter().position(|&b| b == b'\n') {
                line.truncate(end);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return String::from_utf8(line).map_err(|_| Failure::Io(io::Error::new(io::ErrorKind::InvalidData, "the answer is not UTF-8")));
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
