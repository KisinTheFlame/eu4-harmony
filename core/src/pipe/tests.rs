//! Test clients on pipes of the tests' own, an engine behind each, run every
//! millisecond by a fake game on a thread of its own.
use super::*;
use crate::remote::{
    fake::{Fake, ARMY},
    Engine, Place,
};
use eu4_harmony_remote::{events::Event, ArmyId};
use serde_json::{json, Value};
use std::{
    sync::{atomic::AtomicBool, Mutex},
    thread::JoinHandle,
    time::Instant,
};
use windows_sys::Win32::Security::{
    Authorization::{ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT},
    DACL_SECURITY_INFORMATION,
};

const PLENTY: Duration = Duration::from_secs(10);

fn pipe_name(test: &str) -> String {
    format!(r"\\.\pipe\eu4-harmony-test-{}-{test}", std::process::id())
}

/// A test's pipe and the game behind it.
struct Test {
    name: String,
    engine: &'static Engine,
    log: &'static Mutex<Vec<String>>,
    place: Arc<Mutex<Place>>,
    stop: Arc<AtomicBool>,
    game: Option<JoinHandle<Fake>>,
}

/// Serves a pipe for the test named `$test`, from an engine with a log of its
/// own, the game at `$place`.
macro_rules! test_pipe {
    ($test:literal, $place:expr) => {{
        static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
        static ENGINE: Engine = Engine::new(|line| LOG.lock().unwrap().push(line.to_string()));
        Test::start($test, &ENGINE, &LOG, |request| ENGINE.submit(request), |line| LOG.lock().unwrap().push(line.to_string()), $place)
    }};
}

impl Test {
    fn start(test: &str, engine: &'static Engine, log: &'static Mutex<Vec<String>>, submit: fn(Request) -> Reply, write: fn(&str), place: Place) -> Self {
        let name = pipe_name(test);
        serve(&name, submit, write).unwrap();
        let place = Arc::new(Mutex::new(place));
        let stop = Arc::new(AtomicBool::new(false));
        let game = {
            let (place, stop) = (place.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut game = Fake::default();
                while !stop.load(Ordering::Relaxed) {
                    let at = place.lock().unwrap().clone();
                    engine.run(&mut game, &at, PLENTY);
                    std::thread::sleep(Duration::from_millis(1));
                }
                game
            })
        };
        Self { name, engine, log, place, stop, game: Some(game) }
    }

    /// A client on the pipe, once the pipe is there.
    fn connect(&self) -> Client {
        let until = Instant::now() + PLENTY;
        loop {
            match std::fs::OpenOptions::new().read(true).write(true).open(&self.name) {
                Ok(pipe) => return Client { pipe: BufReader::new(pipe) },
                // Not open yet, or every instance taken.
                Err(e) if Instant::now() < until && matches!(e.raw_os_error(), Some(2 | 231)) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => panic!("cannot open {}: {e}", self.name),
            }
        }
    }

    fn set(&self, place: Place) {
        *self.place.lock().unwrap() = place;
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    /// Whether a log line has `text` in it, within a while.
    fn logged(&self, text: &str) -> bool {
        let until = Instant::now() + PLENTY;
        while Instant::now() < until {
            if self.log().iter().any(|l| l.contains(text)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Stops the game, which tells what it was asked.
    fn stop(mut self) -> Fake {
        self.stop.store(true, Ordering::Relaxed);
        self.game.take().unwrap().join().unwrap()
    }
}

struct Client {
    pipe: BufReader<File>,
}

impl Client {
    fn send(&mut self, bytes: impl AsRef<[u8]>) {
        self.pipe.get_ref().write_all(bytes.as_ref()).unwrap();
    }

    fn ask(&mut self, request: &str) -> Value {
        self.send(format!("{request}\n"));
        self.answer(PLENTY).expect("an answer")
    }

    /// The next answer, if one comes `within` that long.
    fn answer(&mut self, within: Duration) -> Option<Value> {
        let until = Instant::now() + within;
        while !self.pipe.buffer().contains(&b'\n') && available(self.pipe.get_ref()) == 0 {
            if Instant::now() >= until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut line = String::new();
        self.pipe.read_line(&mut line).unwrap();
        assert!(line.ends_with('\n'), "an answer is a line: {line:?}");
        Some(serde_json::from_str(&line).unwrap())
    }
}

/// How many bytes wait to be read from a pipe.
fn available(pipe: &File) -> u32 {
    let mut bytes = 0;
    let null = std::ptr::null_mut();
    unsafe { PeekNamedPipe(pipe.as_raw_handle(), null, 0, null.cast(), &mut bytes, null.cast()) };
    bytes
}

fn bad_request(answer: &Value, saying: &str) -> bool {
    answer["ok"] == false && answer["error"]["code"] == "bad_request" && answer["error"]["message"].as_str().is_some_and(|m| m.contains(saying))
}

#[test]
fn a_client_reads_the_session_and_errors_come_as_json() {
    let test = test_pipe!("session", Place::Game);
    let mut client = test.connect();
    assert_eq!(
        client.ask(r#"{"capability": "session"}"#),
        json!({"ok": true, "result": {"session": 2, "date": "1444.11.11", "paused": true, "visibility_current": true, "player": "TUR"}})
    );
    let missing = client.ask(r#"{"capability": "army.get", "army": "1"}"#);
    assert_eq!(missing, json!({"ok": false, "error": {"code": "not_found", "message": "no own army 1"}}), "{missing}");
    // A line may end in CR LF, and a blank line is not answered.
    client.send("\r\n  \n{\"capability\": \"text.language\"}\r\n");
    assert_eq!(client.answer(PLENTY), Some(json!({"ok": true, "result": "simp_chinese"})));
    assert_eq!(client.answer(Duration::from_millis(50)), None);
    drop(client);
    assert!(test.logged("remote pipe: client 1 closed the pipe, after 3 answer(s)"), "{:#?}", test.log());
    test.stop();
}

#[test]
fn a_line_that_is_no_request_is_answered_bad_request_and_the_client_goes_on() {
    let test = test_pipe!("bad", Place::Game);
    let mut client = test.connect();
    let cases = [
        ("not json", "line 1"),
        (r#"{"capability": "army.fly"}"#, "no capability army.fly"),
        (r#"{"capability": "army.get"}"#, "missing field `army`"),
        (r#"{"capability": "army.get", "army": 7}"#, "an army ID"),
    ];
    for (line, saying) in cases {
        let answer = client.ask(line);
        assert!(bad_request(&answer, saying), "{line}: {answer}");
    }
    client.send(b"{\"capability\": \"session\xff\"}\n");
    let answer = client.answer(PLENTY).unwrap();
    assert!(bad_request(&answer, "UTF-8"), "{answer}");
    // A line too long is skipped to its end.
    client.send("x".repeat(LONGEST_LINE + 10) + "\n");
    let answer = client.answer(PLENTY).unwrap();
    assert!(bad_request(&answer, "at most 1048576 bytes"), "{answer}");
    assert_eq!(client.ask(r#"{"capability": "session"}"#)["ok"], true);
    assert!(test.log().iter().any(|l| l == "remote pipe: client 1: bad_request: no capability army.fly"), "{:#?}", test.log());
    test.stop();
}

#[test]
fn a_client_gets_its_answers_in_the_order_it_sent_its_requests() {
    // While a game is being set up, what needs a game waits for one, and
    // what needs none runs at once, unless an earlier request of its client
    // still waits.
    let test = test_pipe!("order", Place::Outside(None));
    let mut client = test.connect();
    client.send("{\"capability\": \"session\"}\n{\"capability\": \"text.language\"}\n");
    assert_eq!(client.answer(Duration::from_millis(300)), None);
    test.set(Place::Game);
    assert_eq!(client.answer(PLENTY).unwrap()["result"]["player"], "TUR");
    assert_eq!(client.answer(PLENTY).unwrap()["result"], "simp_chinese");
    assert_eq!(test.stop().calls, ["session", "language"]);
}

#[test]
fn several_clients_are_served_at_once() {
    let test = test_pipe!("several", Place::Game);
    let mut waiting = test.connect();
    waiting.send("{\"capability\": \"events\", \"wait_ms\": 10000}\n");
    let clients: Vec<_> = (0..4)
        .map(|i| {
            let mut client = test.connect();
            std::thread::spawn(move || (0..5).map(|_| client.ask(&format!(r#"{{"capability": "army.move", "army": "{ARMY}", "goal": {}}}"#, 200 + i))).collect::<Vec<_>>())
        })
        .collect();
    for client in clients {
        assert!(client.join().unwrap().iter().all(|a| *a == json!({"ok": true, "result": null})));
    }
    assert_eq!(waiting.answer(Duration::ZERO), None);
    test.engine.record([Event::ArmyOrdered { army: ArmyId(ARMY) }]);
    assert_eq!(waiting.answer(PLENTY).unwrap()["result"]["events"][0]["event"], "army_ordered");
    assert_eq!(test.stop().calls.iter().filter(|c| c.starts_with("move")).count(), 20);
}

#[test]
fn a_client_that_closes_the_pipe_gives_its_request_up() {
    let test = test_pipe!("give-up", Place::Outside(None));
    let mut client = test.connect();
    client.send(format!("{{\"capability\": \"army.move\", \"army\": \"{ARMY}\", \"goal\": 151}}\n"));
    std::thread::sleep(Duration::from_millis(200));
    drop(client);
    assert!(test.logged("remote pipe: client 1 closed the pipe before its answer came, which gave the request up, after 0 answer(s)"), "{:#?}", test.log());
    test.set(Place::Game);
    assert!(test.logged(&format!(r#"{{"capability":"army.move","army":"{ARMY}","goal":151}}: given up by its client before it ran"#)), "{:#?}", test.log());

    // A request that already waits is given up too.
    let mut reader = test.connect();
    reader.send("{\"capability\": \"events\", \"wait_ms\": 10000}\n");
    assert!(test.logged(r#"{"capability":"events","wait_ms":10000}: answers later"#), "{:#?}", test.log());
    drop(reader);
    assert!(test.logged("events: given up by its client while it waited"), "{:#?}", test.log());
    assert!(test.stop().calls.iter().all(|c| !c.starts_with("move")));
}

/// The access list of a pipe, written in SDDL.
fn dacl(pipe: &File) -> String {
    let (mut acl, mut descriptor, mut text) = (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
    let null = std::ptr::null_mut();
    let error = unsafe { GetSecurityInfo(pipe.as_raw_handle(), SE_KERNEL_OBJECT, DACL_SECURITY_INFORMATION, null, null, &mut acl, null.cast(), &mut descriptor) };
    assert_eq!(error, 0);
    assert_ne!(unsafe { ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, DACL_SECURITY_INFORMATION, &mut text, null.cast()) }, 0);
    let length = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
    let sddl = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) });
    unsafe {
        LocalFree(text.cast());
        LocalFree(descriptor);
    }
    sddl
}

#[test]
fn the_pipe_is_open_to_its_user_only_and_to_one_process_at_a_time() {
    let name = pipe_name("security");
    let pipe = Pipe::new(&name, |_| unreachable!(), |_| {}).unwrap();
    let first = pipe.instance(true).unwrap();
    assert_eq!(dacl(&first), format!("D:P(A;;FA;;;{})", user_sid().unwrap()));
    // More clients get more instances, but no other process gets the name
    // while this one has it.
    let more = pipe.instance(false).unwrap();
    let other = Pipe::new(&name, |_| unreachable!(), |_| {}).unwrap();
    assert!(other.instance(true).is_err());
    drop((first, more));
    assert!(other.instance(true).is_ok());
}

#[test]
fn a_game_started_again_opens_the_pipe_once_the_one_before_is_gone() {
    let before = Pipe::new(&pipe_name("again"), |_| unreachable!(), |_| {}).unwrap().instance(true).unwrap();
    let test = test_pipe!("again", Place::Game);
    assert!(test.logged("yet (another process has it); trying again every second"), "{:#?}", test.log());
    drop(before);
    assert!(test.logged("remote pipe: open at"), "{:#?}", test.log());
    assert_eq!(test.connect().ask(r#"{"capability": "session"}"#)["ok"], true);
    test.stop();
}
