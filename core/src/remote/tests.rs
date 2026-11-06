//! The engine driven with typed requests through the catalogue's seam, a
//! scripted catalogue behind it: what the engine itself does with requests,
//! whatever they ask. What a group of the catalogue does is tested with the
//! group (`crate::catalogue`).
use super::*;
use crate::catalogue::{
    events,
    fake::{self, answer, ok, ARMY, FLEET, PLENTY},
    process, Outcome, Poll,
};
use eu4_harmony_remote::{
    armies::{ArmyAnswer, ArmyRequest},
    capabilities,
    console::{ConsoleAnswer, ConsoleRequest},
    events::{Event, EventsRequest},
    response_json,
    session::SessionRequest,
    texts::{TextAnswer, TextRequest},
    Answer, ArmyId, CountryTag, ProvinceId,
};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn remember(line: &str) {
    LOG.with(|l| l.borrow_mut().push(line.to_string()));
}

fn log() -> Vec<String> {
    LOG.with(|l| l.borrow().clone())
}

fn error(code: &str) -> impl Fn(&Value) -> bool + '_ {
    move |v: &Value| v["ok"] == false && v["error"]["code"] == code
}

fn army(id: u64) -> ArmyId {
    ArmyId(id)
}

fn province(id: u32) -> ProvinceId {
    ProvinceId(id)
}

/// The catalogue as a test scripts it: each request answers what the test
/// gave for its capability, `null` otherwise; remote control's events are
/// read by the catalogue's own group, which needs no game.
#[derive(Default)]
struct Scripted {
    /// The capabilities run, in order.
    ran: Vec<String>,
    /// What a capability answers, by its full name.
    answers: HashMap<String, Response>,
    /// How long each request takes.
    delay: Duration,
    /// What running a request does besides.
    does: Option<Box<dyn FnMut() + Send>>,
}

impl Scripted {
    fn answering(answers: impl IntoIterator<Item = (&'static str, Response)>) -> Self {
        Self { answers: answers.into_iter().map(|(name, response)| (name.to_string(), response)).collect(), ..Self::default() }
    }
}

/// An answer of `null`.
fn nothing() -> Response {
    Ok(Answer::Army(ArmyAnswer::Stop(())))
}

impl Catalogue for Scripted {
    fn run(&mut self, request: Request, events: Option<&events::Buffer>, now: Instant) -> Outcome {
        std::thread::sleep(self.delay);
        let name = request.name();
        self.ran.push(name.clone());
        if let Some(does) = &mut self.does {
            does();
        }
        match request {
            Request::Events(r) => events::run(r, events.expect("the engine keeps events"), now),
            _ => Outcome::Done(self.answers.get(&name).cloned().unwrap_or_else(nothing)),
        }
    }

    fn at(&mut self, events: Option<&events::Buffer>, now: Instant, place: &Place, polls: &mut dyn FnMut(&mut Poll<'_>)) {
        polls(&mut Poll::new(events, now, place, None));
    }

    fn handled(&mut self, _: game::input::Taken) -> bool {
        true
    }
}

/// A request for `capability`, every argument it needs an example of its kind.
fn example(capability: &eu4_harmony_remote::Capability) -> Request {
    let mut request = serde_json::Map::new();
    request.insert("capability".into(), capability.name().into());
    for arg in capability.args.iter().filter(|a| !a.optional) {
        request.insert(arg.name.into(), arg.kind.json(arg.kind.example()).unwrap());
    }
    eu4_harmony_remote::request_from_json(&Value::Object(request).to_string()).unwrap()
}

#[test]
fn every_capability_runs_where_its_kind_says() {
    let not_in_game = Error::new(Code::NotInGame, "the main menu is open");
    let all: Vec<Request> = capabilities().iter().map(example).collect();
    assert!(all.len() > 250, "only {} capabilities", all.len());
    let ran = |game: &Scripted, request: &Request| game.ran.contains(&request.name());

    // In the main menu what needs a game is answered so, without running;
    // the rest runs.
    let engine = Engine::new(fake::quiet);
    let mut game = Scripted::default();
    let replies: Vec<Reply> = all.iter().map(|r| engine.submit(r.clone())).collect();
    engine.run(&mut game, &Place::Outside(Some(not_in_game.clone())), PLENTY);
    for (request, reply) in all.iter().zip(&replies) {
        let got = answer(reply);
        if request.in_game() {
            assert!(!ran(&game, request) && error("not_in_game")(&got), "{}: {got}", request.name());
        } else {
            assert!(ran(&game, request) && !got.is_null(), "{} did not run in the main menu", request.name());
        }
    }

    // Where it is not known whether a game goes on, what needs one, and
    // what does one thing in the main menu and another in a game, waits for
    // a game's frame, and runs there.
    let engine = Engine::new(fake::quiet);
    let mut game = Scripted::default();
    let replies: Vec<Reply> = all.iter().map(|r| engine.submit(r.clone())).collect();
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    for request in &all {
        assert_eq!(ran(&game, request), !request.needs_place(), "{}", request.name());
    }
    engine.run(&mut game, &Place::Game, PLENTY);
    for (request, reply) in all.iter().zip(&replies) {
        assert!(ran(&game, request) && !answer(reply).is_null(), "{} did not run at a game's frame", request.name());
    }

    // At a game's frame the player cannot play, what needs a country the
    // player plays is answered why, and what needs only the game runs.
    let engine = Engine::new(fake::quiet);
    let mut game = Scripted::default();
    let replies: Vec<Reply> = all.iter().map(|r| engine.submit(r.clone())).collect();
    engine.run(&mut game, &Place::Unplayable(Error::new(Code::NotInGame, "no controllable player")), PLENTY);
    for (request, reply) in all.iter().zip(&replies) {
        assert_eq!(ran(&game, request), !request.in_game() || request.observing(), "{}", request.name());
        assert!(!answer(reply).is_null(), "{} was not answered", request.name());
    }
}

#[test]
fn every_request_is_written_to_the_log() {
    let engine = Engine::new(remember);
    let mut game = Scripted::answering([("army.move", Err(Error::new(Code::Rejected, "no route to 1 in 3 days")))]);
    let reply = engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(1), transports: None }));
    let fine = engine.submit(Request::Session(SessionRequest::Get {}));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(answer(&reply)["ok"] == false && answer(&fine)["ok"] == true);
    let lines = log();
    let line = |text: &str| lines.iter().any(|l| l.contains(text));
    assert!(line(&format!(r#"{{"capability":"army.move","army":"{ARMY}","province":1}}: rejected: no route to 1 in "#)), "{lines:#?}");
    assert!(line(r#"{"capability":"session.get"}: ok in "#), "{lines:#?}");
}

#[test]
fn outside_a_game_requests_that_need_one_wait_or_are_answered() {
    let engine = Engine::new(remember);
    let mut game = Scripted::answering([("text.language", Ok(Answer::Text(TextAnswer::Language("simp_chinese".into()))))]);
    let session = engine.submit(Request::Session(SessionRequest::Get {}));
    let language = engine.submit(Request::Text(TextRequest::Language {}));
    let stop = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));

    // While a game is being set up: what needs no game runs, the rest waits.
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert_eq!(answer(&language), ok(json!("simp_chinese")));
    assert!(answer(&session).is_null() && answer(&stop).is_null());
    assert_eq!(game.ran, ["text.language"]);

    // In the main menu they are answered without the game being asked.
    let menu = Place::Outside(Some(Error::new(Code::NotInGame, "no game is running")));
    let later = engine.submit(Request::Army(ArmyRequest::List {}));
    engine.run(&mut game, &menu, PLENTY);
    for reply in [&session, &stop, &later] {
        assert_eq!(answer(reply), json!({"ok": false, "error": {"code": "not_in_game", "message": "no game is running"}}));
    }
    assert_eq!(game.ran, ["text.language"]);
}

#[test]
fn waiting_requests_keep_their_order_and_run_at_the_game_frame() {
    let engine = Engine::new(remember);
    let mut game = Scripted::default();
    let moved = engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(151), transports: None }));
    let stopped = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    let read = engine.submit(Request::Army(ArmyRequest::Get { army: army(ARMY) }));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.ran, ["army.move", "army.stop", "army.get"]);
    assert!([&moved, &stopped, &read].iter().all(|r| answer(r) == ok(Value::Null)));
}

#[test]
fn the_time_budget_ends_a_frame_and_the_rest_runs_at_the_next() {
    let engine = Engine::new(remember);
    let mut game = Scripted { delay: Duration::from_millis(20), ..Default::default() };
    let replies: Vec<Reply> = (0..3).map(|_| engine.submit(Request::Session(SessionRequest::Get {}))).collect();
    // At least one request runs, however small the budget, and once it is
    // spent the rest wait for the next frame. (Each takes 20 ms at least,
    // however loaded the machine, so the budget is spent after the first.)
    engine.run(&mut game, &Place::Game, Duration::from_millis(1));
    assert_eq!(game.ran.len(), 1);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.ran.len(), 3);
    assert!(replies.iter().all(|r| answer(r)["ok"] == true));
}

#[test]
fn a_request_given_up_before_it_runs_never_runs() {
    let engine = Engine::new(remember);
    let mut game = Scripted::default();
    drop(engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(151), transports: None })));
    let kept = engine.submit(Request::Army(ArmyRequest::Get { army: army(ARMY) }));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.ran, ["army.get"]);
    assert_eq!(answer(&kept), ok(Value::Null));
    let given_up = format!(r#"{{"capability":"army.move","army":"{ARMY}","province":151}}: given up by its client before it ran"#);
    assert!(log().iter().any(|l| l.contains(&given_up)), "{:#?}", log());
}

#[test]
fn a_read_that_waits_is_answered_at_a_later_safe_point() {
    let engine = Engine::new(remember);
    let mut game = Scripted::default();
    let read = |wait_ms| Request::Events(EventsRequest::Read { since: Some(0), wait_ms: Some(wait_ms), process: None, kinds: None, countries: None });

    let next = engine.submit(read(10_000));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert!(answer(&next).is_null());
    // Requests behind it are not held up.
    let language = engine.submit(Request::Text(TextRequest::Language {}));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(answer(&next).is_null());
    assert_eq!(answer(&language)["ok"], true);

    engine.record([Event::ArmyOrdered { army: army(ARMY) }]);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(
        answer(&next),
        ok(json!({"process": process(), "events": [{"number": 1, "event": "army_ordered", "army": ARMY.to_string()}], "last": 1, "missed": false}))
    );
    assert!(log().iter().any(|l| l.contains("events: ok after ")));

    // With something to read already, it answers at once; with nothing, when
    // its time is up, in the main menu too.
    let at_once = engine.submit(read(10_000));
    let short = engine.submit(Request::Events(EventsRequest::Read { since: Some(1), wait_ms: Some(1), process: None, kinds: None, countries: None }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert_eq!(answer(&at_once)["result"]["last"], 1);
    std::thread::sleep(Duration::from_millis(5));
    engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "no game"))), PLENTY);
    assert_eq!(answer(&short), ok(json!({"process": process(), "events": [], "last": 1, "missed": false})));

    let too_long = engine.submit(read(events::LONGEST_WAIT + 1));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(error("invalid_argument")(&answer(&too_long)));

    // A waiting read given up is dropped.
    let given_up = engine.submit(Request::Events(EventsRequest::Read { since: Some(1), wait_ms: Some(10_000), process: None, kinds: None, countries: None }));
    engine.run(&mut game, &Place::Game, PLENTY);
    drop(given_up);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(log().iter().any(|l| l.contains("events: given up by its client while it waited")));
}

#[test]
fn requests_from_other_threads_are_answered_on_the_one_that_runs_the_engine() {
    static ENGINE: Engine = Engine::new(remember);
    let clients: Vec<_> = (0..4)
        .map(|i| {
            std::thread::spawn(move || {
                let mut got = Vec::new();
                for _ in 0..5 {
                    let reply = ENGINE.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(200 + i), transports: None }));
                    got.push(reply.wait(PLENTY).map(|r| response_json(&r)));
                }
                got
            })
        })
        .collect();
    let mut game = Scripted::default();
    while !clients.iter().all(|c| c.is_finished()) {
        ENGINE.run(&mut game, &Place::Game, PLENTY);
        std::thread::sleep(Duration::from_millis(1));
    }
    for client in clients {
        assert!(client.join().unwrap().iter().all(|r| *r == Some(ok(Value::Null))));
    }
    assert_eq!(game.ran.len(), 20);
}

#[test]
fn what_a_game_frame_tells_extensions_is_kept_as_events() {
    let frame = game::Frame {
        ctx: game::Ctx { world: 0, date: 0x035D_7710, player: fake::tur(), country: 0, countries: 0 },
        session: 3,
        new_session: true,
        visibility_current: true,
        // A battle the player neither sees nor takes part in, which remote
        // control, reading the player's view, does not hear.
        changes: vec![
            game::events::Change::Ordered(ARMY),
            game::events::Change::Defeated(ARMY),
            game::events::Change::Battle(game::events::Battle {
                province: 151,
                naval: false,
                attackers: vec![fake::wal()],
                defenders: vec![fake::byz()],
                attacker_won: true,
                attacker_losses: 100,
                defender_losses: 900,
                seen: false,
            }),
        ],
        new_day: true,
        new_month: true,
        new_year: false,
        wars: vec![
            game::wars::Happened::WarDeclared { war: "Ottoman-Byzantine War".into(), attacker: fake::tur(), defender: fake::byz() },
            game::wars::Happened::OwnerChanged { province: 151, from: Some(fake::byz()), to: None },
        ],
    };
    let engine = Engine::new(remember);
    engine.record(happened(&frame));
    let read = engine.submit(Request::Events(EventsRequest::Read { since: None, wait_ms: None, process: None, kinds: None, countries: None }));
    engine.run(&mut Scripted::default(), &Place::Game, PLENTY);
    assert_eq!(
        answer(&read)["result"]["events"],
        json!([
            {"number": 1, "event": "session", "session": 3},
            {"number": 2, "event": "army_ordered", "army": ARMY.to_string()},
            {"number": 3, "event": "army_defeated", "army": ARMY.to_string()},
            {"number": 4, "event": "war_declared", "war": "Ottoman-Byzantine War", "attacker": "TUR", "defender": "BYZ"},
            {"number": 5, "event": "province_owner_changed", "province": 151, "from": "BYZ", "to": null},
            {"number": 6, "event": "day", "date": "1444.11.11"},
            {"number": 7, "event": "month", "date": "1444.11.11"},
        ])
    );
    // Only those naming a country asked for, and those naming none.
    let read = engine.submit(Request::Events(EventsRequest::Read {
        since: None,
        wait_ms: None,
        process: None,
        kinds: None,
        countries: Some(eu4_harmony_remote::CountryList(vec![CountryTag(*b"TUR")])),
    }));
    engine.run(&mut Scripted::default(), &Place::Game, PLENTY);
    let numbers: Vec<u64> = answer(&read)["result"]["events"].as_array().unwrap().iter().map(|e| e["number"].as_u64().unwrap()).collect();
    assert_eq!(numbers, [1, 2, 3, 4, 6, 7]);
    let quiet = game::Frame { new_session: false, changes: vec![], new_day: false, new_month: false, wars: vec![], ..frame };
    assert!(happened(&quiet).is_empty());
}

#[test]
fn a_request_sent_in_one_session_never_runs_in_the_next() {
    let engine = Engine::new(remember);
    let mut game = Scripted::default();
    engine.enter(2);
    let old = engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(151), transports: None }));
    let reads = engine.submit(Request::Text(TextRequest::Language {}));
    // A save begins to load; what is sent while it loads is for the next.
    engine.leave();
    let new = engine.submit(Request::Session(SessionRequest::Get {}));
    engine.enter(3);
    engine.run(&mut game, &Place::Game, PLENTY);
    let old = answer(&old);
    assert!(error("session_changed")(&old), "{old}");
    assert_eq!(old["error"]["message"], "it was sent in session 2, which ended before it ran; this is session 3");
    assert_eq!(answer(&reads)["ok"], true, "what needs no game has no session");
    assert_eq!(answer(&new)["ok"], true);
    assert_eq!(game.ran, ["text.language", "session.get"]);
    // In the same session it runs, also after waiting outside a game frame.
    let same = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&same), ok(Value::Null));
}

#[test]
fn a_request_sent_once_its_session_ended_belongs_to_the_next() {
    use eu4_harmony_remote::events::SessionEnd;
    let engine = Engine::new(remember);
    let mut game = Scripted::default();
    engine.enter(2);
    let old = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    // An in-process return: the session ends, and no save is read after it.
    engine.session_ended(SessionEnd::Return);
    // Sent while a new game is set up, it waits for that game.
    let new = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert!(answer(&old).is_null() && answer(&new).is_null());
    engine.enter(3);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(error("session_changed")(&answer(&old)), "{}", answer(&old));
    assert_eq!(answer(&new), ok(Value::Null));
    assert_eq!(game.ran, ["army.stop"]);
}

#[test]
fn events_say_which_process_numbered_them() {
    let engine = Engine::new(remember);
    engine.record([Event::ArmyOrdered { army: army(ARMY) }, Event::ArmyOrdered { army: army(FLEET) }]);
    let read = |since, from| {
        engine.submit(Request::Events(EventsRequest::Read { since: Some(since), wait_ms: Some(10_000), process: Some(from), kinds: None, countries: None }))
    };
    let (here, other) = (read(1, process()), read(1, process() - 1));
    engine.run(&mut Scripted::default(), &Place::Game, PLENTY);
    // From this process, the events after since; from another, every event
    // kept, at once, with missed: since meant something else there.
    let (here, other) = (answer(&here), answer(&other));
    assert_eq!(here["result"]["events"].as_array().map(Vec::len), Some(1));
    assert_eq!(here["result"]["missed"], false);
    assert_eq!(other["result"]["events"].as_array().map(Vec::len), Some(2));
    assert_eq!(other["result"]["missed"], true);
    assert_eq!(other["result"]["process"], process());
}

#[test]
fn what_happens_inside_a_request_is_kept_as_events() {
    // A console command that loads a save ends the session inside the
    // request that runs it, and remote control keeps the end there.
    use eu4_harmony_remote::events::SessionEnd;
    static ENGINE: Engine = Engine::new(fake::quiet);
    let (send, got) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let ended = || ENGINE.record([Event::SessionEnded { reason: SessionEnd::Load }]);
        let mut game = Scripted::answering([("console.run", Ok(Answer::Console(ConsoleAnswer::Run("Gold changed by 100".into()))))]);
        game.does = Some(Box::new(ended));
        let run = ENGINE.submit(Request::Console(ConsoleRequest::Run { command: "cash 100".into() }));
        ENGINE.run(&mut game, &Place::Game, PLENTY);
        game.does = None;
        let read = ENGINE.submit(Request::Events(EventsRequest::Read { since: Some(0), wait_ms: None, process: None, kinds: None, countries: None }));
        ENGINE.run(&mut game, &Place::Game, PLENTY);
        let _ = send.send((answer(&run), answer(&read)));
    });
    let (run, read) = got.recv_timeout(Duration::from_secs(10)).expect("the engine held itself up keeping an event from inside a request");
    assert_eq!(run, ok(json!("Gold changed by 100")));
    assert_eq!(read["result"]["events"], json!([{"number": 1, "event": "session_ended", "reason": "load"}]), "{read}");
}
