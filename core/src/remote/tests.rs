//! The engine driven with typed requests, a fake game behind the capability
//! layer's traits.
use super::fake::{self, Fake, ARMY, ENEMY, FLEET};
use super::*;
use eu4_harmony_remote::{
    armies::ArmyRequest, camera::CameraRequest, capabilities, countries::CountryRequest, describe::DescribeRequest, events::Event, events::EventsRequest, provinces::ProvinceRequest,
    response_json, session::SessionRequest, texts::TextRequest, ArmyId, CountryTag, ProvinceId,
};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn remember(line: &str) {
    LOG.with(|l| l.borrow_mut().push(line.to_string()));
}

fn log() -> Vec<String> {
    LOG.with(|l| l.borrow().clone())
}

const PLENTY: Duration = Duration::from_secs(10);

/// What came back for a reply, in JSON; null when nothing came yet.
fn answer(reply: &Reply) -> Value {
    reply.wait(Duration::ZERO).map_or(Value::Null, |r| response_json(&r))
}

fn ok(result: Value) -> Value {
    json!({"ok": true, "result": result})
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

#[test]
fn every_capability_runs_at_a_game_frame() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let snapshot = json!({
        "id": ARMY.to_string(), "owner": "TUR", "province": 149, "land": true, "in_battle": false, "sieging": false, "retreating": false,
        "exiled": false, "movement_locked": false, "attached": false, "has_followers": false, "busy": false, "regiments": 12,
        "route_len": 0, "destination": null, "route_source": "none",
    });
    let fleet = json!({
        "id": FLEET.to_string(), "owner": "TUR", "province": null, "land": false, "in_battle": false, "sieging": false, "retreating": false,
        "exiled": false, "movement_locked": false, "attached": false, "has_followers": false, "busy": false, "regiments": 0,
        "route_len": 0, "destination": null, "route_source": "none",
    });
    let province_json = |id: u32, owner: &str, controller: &str| json!({"id": id, "land": true, "lake": false, "impassable": false, "visible": true, "owner": owner, "controller": controller});
    let cases: Vec<(Request, Box<dyn Fn(&Value) -> bool>)> = vec![
        (
            Request::Session(SessionRequest::Get {}),
            Box::new(|v: &Value| *v == ok(json!({"session": 2, "date": "1444.11.11", "paused": true, "visibility_current": true, "player": "TUR"}))),
        ),
        (Request::Country(CountryRequest::Hostile { country: CountryTag(*b"TUR"), other: CountryTag(*b"BYZ") }), Box::new(|v: &Value| *v == ok(json!(true)))),
        (Request::Country(CountryRequest::Hostile { country: CountryTag(*b"BYZ"), other: CountryTag(*b"TUR") }), Box::new(|v: &Value| *v == ok(json!(false)))),
        (Request::Country(CountryRequest::Hostile { country: CountryTag(*b"TUR"), other: CountryTag(*b"ZZZ") }), Box::new(error("not_found"))),
        (
            Request::Province(ProvinceRequest::List {}),
            Box::new(move |v: &Value| *v == ok(json!([province_json(149, "TUR", "TUR"), province_json(151, "BYZ", "TUR")]))),
        ),
        (
            Request::Province(ProvinceRequest::Neighbors { province: province(149) }),
            Box::new(|v: &Value| *v == ok(json!([{"province": 151, "strait": false}, {"province": 316, "strait": true}]))),
        ),
        (
            Request::Army(ArmyRequest::Get { army: army(ARMY) }),
            Box::new({
                let snapshot = snapshot.clone();
                move |v: &Value| *v == ok(snapshot.clone())
            }),
        ),
        (
            Request::Army(ArmyRequest::Get { army: army(FLEET) }),
            Box::new({
                let fleet = fleet.clone();
                move |v: &Value| *v == ok(fleet.clone())
            }),
        ),
        (Request::Army(ArmyRequest::Get { army: army(1) }), Box::new(error("not_found"))),
        (Request::Army(ArmyRequest::List {}), Box::new(move |v: &Value| *v == ok(json!([snapshot, fleet])))),
        (
            Request::Army(ArmyRequest::InProvince { province: province(151) }),
            Box::new(|v: &Value| {
                let a = &v["result"][0];
                a["id"] == ENEMY.to_string() && a["owner"] == "BYZ" && a["province"] == 151 && a["sieging"] == true && a["route_source"] == "extension"
            }),
        ),
        (Request::Army(ArmyRequest::Route { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(json!([])))),
        (Request::Army(ArmyRequest::CanEnter { army: army(ARMY), province: province(151) }), Box::new(|v: &Value| *v == ok(json!(true)))),
        (Request::Army(ArmyRequest::StraitOpen { army: army(ARMY), from: province(149), to: province(316) }), Box::new(|v: &Value| *v == ok(json!(false)))),
        (Request::Army(ArmyRequest::CanAttack { army: army(ARMY), target: army(ENEMY), province: province(151) }), Box::new(|v: &Value| *v == ok(json!(true)))),
        (Request::Army(ArmyRequest::FindRoute { army: army(ARMY), goal: province(151) }), Box::new(|v: &Value| *v == ok(json!([150, 151])))),
        (Request::Army(ArmyRequest::Move { army: army(ARMY), goal: province(151) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Army(ArmyRequest::Route { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(json!([150, 151])))),
        (
            Request::Army(ArmyRequest::Move { army: army(ARMY), goal: province(1) }),
            Box::new(|v: &Value| error("rejected")(v) && v["error"]["message"] == "no route to 1"),
        ),
        (Request::Army(ArmyRequest::Stop { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Army(ArmyRequest::Route { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(json!([])))),
        (Request::Army(ArmyRequest::Selected {}), Box::new(error("not_found"))),
        (Request::Text(TextRequest::Language {}), Box::new(|v: &Value| *v == ok(json!("simp_chinese")))),
        (
            Request::Events(EventsRequest::Read { since: None, wait_ms: None }),
            Box::new(|v: &Value| *v == ok(json!({"events": [], "last": 0, "missed": false}))),
        ),
        (
            Request::Camera(CameraRequest::Get {}),
            Box::new(|v: &Value| *v == ok(json!({"x": 3248, "y": 1376, "height": 800, "province": 149, "min_height": 50, "max_height": 3000}))),
        ),
        (Request::Camera(CameraRequest::LookAtProvince { province: province(151) }), Box::new(|v: &Value| v["result"]["province"] == 151 && v["result"]["x"] == 3270)),
        (Request::Camera(CameraRequest::LookAtProvince { province: province(1) }), Box::new(error("not_found"))),
        (Request::Camera(CameraRequest::LookAt { x: 100, y: 200 }), Box::new(|v: &Value| v["result"]["x"] == 100 && v["result"]["y"] == 200 && v["result"]["province"].is_null())),
        (Request::Camera(CameraRequest::LookAt { x: 6000, y: 200 }), Box::new(error("invalid_argument"))),
        (Request::Camera(CameraRequest::Zoom { height: 1200 }), Box::new(|v: &Value| v["result"]["height"] == 1200 && v["result"]["province"] == 149)),
        (Request::Camera(CameraRequest::Zoom { height: 10 }), Box::new(error("invalid_argument"))),
        (
            Request::Describe(DescribeRequest::All {}),
            Box::new(|v: &Value| v["result"].as_array().map(Vec::len) == Some(capabilities().len()) && v["result"][0]["capability"] == "session"),
        ),
    ];
    // Every capability is among them.
    let asked: BTreeSet<String> = cases.iter().map(|(r, _)| r.name()).collect();
    let all: BTreeSet<String> = capabilities().iter().map(|c| c.name()).collect();
    assert_eq!(asked, all);

    let replies: Vec<Reply> = cases.iter().map(|(r, _)| engine.submit(r.clone())).collect();
    assert!(replies.iter().all(|r| answer(r).is_null()), "nothing runs before a safe point");
    engine.run(&mut game, &Place::Game, PLENTY);
    for ((request, expected), reply) in cases.iter().zip(&replies) {
        let got = answer(reply);
        assert!(expected(&got), "{}: {got}", request.name());
    }
    // The layer was called with what the requests said, in their order.
    let byz = format!("{:x}", fake::byz().0);
    let tur = format!("{:x}", fake::tur().0);
    for call in [format!("hostile {tur} {byz}"), format!("move {ARMY} 151"), format!("stop {ARMY}"), format!("strait open {ARMY} 149 316")] {
        assert!(game.calls.contains(&call), "{call} not in {:#?}", game.calls);
    }
    for call in ["look at province 151", "look at 100 200", "zoom 1200"] {
        assert!(game.calls.iter().any(|c| c == call), "{call} not in {:#?}", game.calls);
    }
    let moved = game.calls.iter().position(|c| *c == format!("move {ARMY} 151")).unwrap();
    let stopped = game.calls.iter().position(|c| *c == format!("stop {ARMY}")).unwrap();
    assert!(moved < stopped);
}

#[test]
fn every_request_is_written_to_the_log() {
    let engine = Engine::new(remember);
    let reply = engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), goal: province(1) }));
    let fine = engine.submit(Request::Session(SessionRequest::Get {}));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    assert!(answer(&reply)["ok"] == false && answer(&fine)["ok"] == true);
    let lines = log();
    let line = |text: &str| lines.iter().any(|l| l.contains(text));
    assert!(line(&format!(r#"{{"capability":"army.move","army":"{ARMY}","goal":1}}: rejected: no route to 1 in "#)), "{lines:#?}");
    assert!(line(r#"{"capability":"session"}: ok in "#), "{lines:#?}");
}

#[test]
fn outside_a_game_requests_that_need_one_wait_or_are_answered() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let session = engine.submit(Request::Session(SessionRequest::Get {}));
    let language = engine.submit(Request::Text(TextRequest::Language {}));
    let stop = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));

    // While a game is being set up: what needs no game runs, the rest waits.
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert_eq!(answer(&language), ok(json!("simp_chinese")));
    assert!(answer(&session).is_null() && answer(&stop).is_null());
    assert_eq!(game.calls, ["language"]);

    // In the main menu they are answered without the game being asked.
    let menu = Place::Outside(Some(Error::new(Code::NotInGame, "no game is running")));
    let later = engine.submit(Request::Army(ArmyRequest::List {}));
    engine.run(&mut game, &menu, PLENTY);
    for reply in [&session, &stop, &later] {
        assert_eq!(answer(reply), json!({"ok": false, "error": {"code": "not_in_game", "message": "no game is running"}}));
    }
    assert_eq!(game.calls, ["language"]);
}

#[test]
fn waiting_requests_keep_their_order_and_run_at_the_game_frame() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let moved = engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), goal: province(151) }));
    let stopped = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    let route = engine.submit(Request::Army(ArmyRequest::Route { army: army(ARMY) }));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.calls, [format!("move {ARMY} 151"), format!("stop {ARMY}"), format!("route {ARMY}")]);
    assert_eq!((answer(&moved), answer(&stopped), answer(&route)), (ok(Value::Null), ok(Value::Null), ok(json!([]))));
}

#[test]
fn the_time_budget_ends_a_frame_and_the_rest_runs_at_the_next() {
    let engine = Engine::new(remember);
    let mut game = Fake { delay: Duration::from_millis(20), ..Default::default() };
    let replies: Vec<Reply> = (0..3).map(|_| engine.submit(Request::Session(SessionRequest::Get {}))).collect();
    // At least one request runs, however small the budget.
    engine.run(&mut game, &Place::Game, Duration::from_millis(1));
    assert_eq!(game.calls.len(), 1);
    engine.run(&mut game, &Place::Game, Duration::from_millis(30));
    assert_eq!(game.calls.len(), 3);
    assert!(replies.iter().all(|r| answer(r)["ok"] == true));
}

#[test]
fn a_request_given_up_before_it_runs_never_runs() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    drop(engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), goal: province(151) })));
    let kept = engine.submit(Request::Army(ArmyRequest::Route { army: army(ARMY) }));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.calls, [format!("route {ARMY}")]);
    assert_eq!(answer(&kept), ok(json!([])));
    let given_up = format!(r#"{{"capability":"army.move","army":"{ARMY}","goal":151}}: given up by its client before it ran"#);
    assert!(log().iter().any(|l| l.contains(&given_up)), "{:#?}", log());
}

#[test]
fn a_read_that_waits_is_answered_at_a_later_safe_point() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let read = |wait_ms| Request::Events(EventsRequest::Read { since: Some(0), wait_ms: Some(wait_ms) });

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
    assert_eq!(answer(&next), ok(json!({"events": [{"number": 1, "event": "army_ordered", "army": ARMY.to_string()}], "last": 1, "missed": false})));
    assert!(log().iter().any(|l| l.contains("events: ok after ")));

    // With something to read already, it answers at once; with nothing, when
    // its time is up, in the main menu too.
    let at_once = engine.submit(read(10_000));
    let short = engine.submit(Request::Events(EventsRequest::Read { since: Some(1), wait_ms: Some(1) }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert_eq!(answer(&at_once)["result"]["last"], 1);
    std::thread::sleep(Duration::from_millis(5));
    engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "no game"))), PLENTY);
    assert_eq!(answer(&short), ok(json!({"events": [], "last": 1, "missed": false})));

    let too_long = engine.submit(read(events::LONGEST_WAIT + 1));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(error("invalid_argument")(&answer(&too_long)));

    // A waiting read given up is dropped.
    let given_up = engine.submit(Request::Events(EventsRequest::Read { since: Some(1), wait_ms: Some(10_000) }));
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
                    let reply = ENGINE.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), goal: province(200 + i) }));
                    got.push(reply.wait(PLENTY).map(|r| response_json(&r)));
                }
                got
            })
        })
        .collect();
    let mut game = Fake::default();
    while !clients.iter().all(|c| c.is_finished()) {
        ENGINE.run(&mut game, &Place::Game, PLENTY);
        std::thread::sleep(Duration::from_millis(1));
    }
    for client in clients {
        assert!(client.join().unwrap().iter().all(|r| *r == Some(ok(Value::Null))));
    }
    assert_eq!(game.calls.len(), 20);
}

#[test]
fn what_a_game_frame_tells_extensions_is_kept_as_events() {
    let frame = game::Frame {
        ctx: game::Ctx { world: 0, date: 0x035D_7710, player: fake::tur(), country: 0, countries: 0 },
        session: 3,
        new_session: true,
        visibility_current: true,
        changes: vec![game::events::Change::Ordered(ARMY), game::events::Change::Defeated(ARMY)],
        new_day: true,
    };
    let engine = Engine::new(remember);
    engine.record(happened(&frame));
    let read = engine.submit(Request::Events(EventsRequest::Read { since: None, wait_ms: None }));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    assert_eq!(
        answer(&read)["result"]["events"],
        json!([
            {"number": 1, "event": "session", "session": 3},
            {"number": 2, "event": "army_ordered", "army": ARMY.to_string()},
            {"number": 3, "event": "army_defeated", "army": ARMY.to_string()},
            {"number": 4, "event": "day", "date": "1444.11.11"},
        ])
    );
    let quiet = game::Frame { new_session: false, changes: vec![], new_day: false, ..frame };
    assert!(happened(&quiet).is_empty());
}

#[test]
fn the_layer_s_failures_keep_their_meaning() {
    let cases = [
        (game::Status::NotInGame, Code::NotInGame),
        (game::Status::NotReady, Code::NotReady),
        (game::Status::NotFound, Code::NotFound),
        (game::Status::Rejected, Code::Rejected),
        (game::Status::InvalidArgument, Code::InvalidArgument),
        (game::Status::Unavailable, Code::Unavailable),
        (game::Status::WrongContext, Code::Internal),
        (game::Status::Internal, Code::Internal),
    ];
    for (status, code) in cases {
        assert_eq!(Error::from(game::Fail { status, message: "why".into() }), Error::new(code, "why"));
    }
}
