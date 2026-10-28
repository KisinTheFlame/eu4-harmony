//! The client against pipes of the tests' own, served by this test program,
//! which stands in for the game.
use super::*;
use std::{
    io::{BufRead, BufReader},
    sync::mpsc,
    thread::{self, JoinHandle},
};
use windows_sys::Win32::{
    Foundation::{LocalFree, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE},
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::PIPE_ACCESS_DUPLEX,
    System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT},
};

pub(crate) const PLENTY: Duration = Duration::from_secs(10);

pub(crate) fn pipe_name(test: &str) -> String {
    format!(r"\\.\pipe\eu4-harmony-cli-test-{}-{test}", std::process::id())
}

/// The executable of this test program, the "game" serving the tests' pipes.
pub(crate) fn this_program() -> String {
    std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned()
}

/// A new instance of the pipe `name`, with the access list `sddl` or the
/// default one.
fn instance(name: &str, sddl: Option<&str>) -> File {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let mut descriptor = std::ptr::null_mut();
    if let Some(sddl) = sddl {
        let sddl: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
        let made = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, std::ptr::null_mut()) };
        assert_ne!(made, 0, "{}", io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: descriptor, bInheritHandle: 0 };
    let security = if sddl.is_some() { &attributes as *const _ } else { std::ptr::null() };
    let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT;
    let handle = unsafe { CreateNamedPipeW(wide.as_ptr(), PIPE_ACCESS_DUPLEX, mode, PIPE_UNLIMITED_INSTANCES, 65536, 65536, 0, security) };
    let error = io::Error::last_os_error();
    if !descriptor.is_null() {
        unsafe { LocalFree(descriptor) };
    }
    assert_ne!(handle, INVALID_HANDLE_VALUE, "cannot make {name}: {error}");
    unsafe { File::from_raw_handle(handle) }
}

/// Serves one client on the pipe `name`, made before this returns: waits for
/// it, reads its request line (empty when it sent none) and hands both to
/// `then`.
pub(crate) fn serve_one(name: &str, then: impl FnOnce(&File, String) + Send + 'static) -> JoinHandle<()> {
    let pipe = instance(name, None);
    thread::spawn(move || {
        // Connected now, or before this waited for it, or come and gone
        // before that.
        let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) } != 0;
        let error = io::Error::last_os_error().raw_os_error().map(|code| code as u32);
        assert!(connected || matches!(error, Some(ERROR_PIPE_CONNECTED | ERROR_NO_DATA)), "{error:?}");
        let mut line = String::new();
        let _ = BufReader::new(&pipe).read_line(&mut line);
        then(&pipe, line);
    })
}

/// Writes `text` to the client.
pub(crate) fn answer(pipe: &File, text: &str) {
    { pipe }.write_all(text.as_bytes()).unwrap();
}

/// Waits until the client has closed the pipe; how long that took.
fn client_gone(pipe: &File) -> Duration {
    let started = Instant::now();
    while started.elapsed() < PLENTY {
        let mut available = 0;
        if unsafe { PeekNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut available, std::ptr::null_mut()) } == 0 {
            return started.elapsed();
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("the client kept the pipe open");
}

fn connect_to(name: &str) -> Client {
    connect(name, &this_program(), Instant::now() + PLENTY).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn sends_a_request_and_reads_its_answer() {
    let name = pipe_name("answers");
    let served = serve_one(&name, |pipe, line| {
        assert_eq!(line, "{\"capability\":\"session\"}\n");
        answer(pipe, "{\"ok\":true,\"result\":1}\r\n");
    });
    let mut client = connect_to(&name);
    assert_eq!(client.ask("{\"capability\":\"session\"}", None).unwrap(), "{\"ok\":true,\"result\":1}");
    served.join().unwrap();
}

#[test]
fn reads_an_answer_that_comes_in_pieces() {
    let name = pipe_name("pieces");
    let long = "x".repeat(300_000);
    let whole = format!("{{\"ok\":true,\"result\":\"{long}\"}}");
    let sent = whole.clone();
    let served = serve_one(&name, move |pipe, _| {
        let (first, rest) = sent.split_at(100_000);
        answer(pipe, first);
        thread::sleep(Duration::from_millis(50));
        answer(pipe, rest);
        thread::sleep(Duration::from_millis(50));
        answer(pipe, "\n");
    });
    let mut client = connect_to(&name);
    assert_eq!(client.ask("{\"capability\":\"province.list\"}", Some(Instant::now() + PLENTY)).unwrap(), whole);
    served.join().unwrap();
}

#[test]
fn waits_for_the_pipe_to_open() {
    let name = pipe_name("late");
    let late = {
        let name = name.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            serve_one(&name, |pipe, _| answer(pipe, "{\"ok\":true,\"result\":null}\n")).join().unwrap();
        })
    };
    let started = Instant::now();
    let mut client = connect_to(&name);
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert_eq!(client.ask("{\"capability\":\"text.language\"}", None).unwrap(), "{\"ok\":true,\"result\":null}");
    late.join().unwrap();
}

#[test]
fn gives_up_on_a_pipe_that_does_not_open_in_time() {
    let started = Instant::now();
    let failure = connect(&pipe_name("never"), &this_program(), started + Duration::from_millis(200)).err().unwrap();
    assert!(matches!(failure, Failure::NotOpen), "{failure:?}");
    assert!((Duration::from_millis(200)..Duration::from_secs(5)).contains(&started.elapsed()));
}

#[test]
fn tells_the_process_serving_the_pipe() {
    let name = pipe_name("serving");
    let served = serve_one(&name, |_, _| {});
    let pipe = File::options().read(true).write(true).open(&name).unwrap();
    let found = serving(&pipe).unwrap();
    assert_eq!(found.process, std::process::id());
    assert!(found.same_user);
    assert_eq!(found.image.file_name().unwrap().to_string_lossy(), this_program());
    drop(pipe);
    served.join().unwrap();
}

#[test]
fn sends_nothing_to_a_server_that_is_not_the_game() {
    let name = pipe_name("not-the-game");
    let (sent, received) = mpsc::channel();
    let served = serve_one(&name, move |_, line| sent.send(line).unwrap());
    let failure = connect(&name, GAME, Instant::now() + PLENTY).err().unwrap();
    let Failure::NotTheGame(why) = &failure else { panic!("{failure:?}") };
    assert!(why.contains(&this_program()) && why.contains("not by the game (eu4.exe)"), "{why}");
    assert!(failure.to_string().starts_with("nothing was sent: "));
    assert_eq!(received.recv_timeout(PLENTY).unwrap(), "", "a request was sent");
    served.join().unwrap();
}

#[test]
fn only_the_game_run_by_this_user_serves_the_pipe() {
    let process = |image: &str, same_user| Serving { process: 4321, image: PathBuf::from(image), same_user };
    assert!(check(&process(r"E:\Steam\Europa Universalis IV\eu4.exe", true), GAME).is_ok());
    assert!(check(&process(r"E:\Steam\Europa Universalis IV\EU4.EXE", true), GAME).is_ok());
    let other_user = check(&process(r"E:\Steam\Europa Universalis IV\eu4.exe", false), GAME).unwrap_err().to_string();
    assert!(other_user.contains("process 4321") && other_user.contains("runs as another Windows user"), "{other_user}");
    let other_program = check(&process(r"C:\Temp\eu4.exe.bak", true), GAME).unwrap_err().to_string();
    assert!(other_program.contains(r"C:\Temp\eu4.exe.bak") && other_program.contains("not by the game"), "{other_program}");
}

#[test]
fn a_pipe_closed_to_this_user_is_refused_at_once() {
    let name = pipe_name("closed");
    // A protected access list with no entry lets nobody in.
    let _pipe = instance(&name, Some("D:P"));
    let started = Instant::now();
    let failure = connect(&name, &this_program(), started + PLENTY).err().unwrap();
    assert!(matches!(failure, Failure::Denied), "{failure:?}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_pipe_that_breaks_ends_the_wait_at_once() {
    let name = pipe_name("breaks");
    let served = serve_one(&name, |_, _| thread::sleep(Duration::from_millis(100)));
    let mut client = connect_to(&name);
    let started = Instant::now();
    let failure = client.ask("{\"capability\":\"events\",\"wait_ms\":60000}", None).unwrap_err();
    assert!(matches!(failure, Failure::Broken), "{failure:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    served.join().unwrap();

    // Gone before the request was written.
    let name = pipe_name("gone");
    let (connected, wait) = mpsc::channel::<()>();
    let served = {
        let pipe = instance(&name, None);
        thread::spawn(move || {
            unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) };
            wait.recv().unwrap();
        })
    };
    let mut client = connect_to(&name);
    connected.send(()).unwrap();
    served.join().unwrap();
    let failure = client.ask("{\"capability\":\"session\"}", None).unwrap_err();
    assert!(matches!(failure, Failure::Broken), "{failure:?}");
}

#[test]
fn a_request_without_an_answer_in_time_is_given_up() {
    let name = pipe_name("slow");
    let (gone, told) = mpsc::channel();
    let served = serve_one(&name, move |pipe, _| gone.send(client_gone(pipe)).unwrap());
    let mut client = connect_to(&name);
    let started = Instant::now();
    let failure = client.ask("{\"capability\":\"events\",\"wait_ms\":60000}", Some(started + Duration::from_millis(200))).unwrap_err();
    assert!(matches!(failure, Failure::TimedOut), "{failure:?}");
    assert!(started.elapsed() >= Duration::from_millis(200));
    // Closing the pipe is what gives the request up.
    drop(client);
    assert!(told.recv_timeout(PLENTY).unwrap() < PLENTY);
    served.join().unwrap();
}
