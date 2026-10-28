//! The time capabilities driven through the engine, a fake clock behind them.
use super::*;
use crate::remote::{events::KEPT, fake::Fake, Engine, Place, Reply};
use eu4_harmony_remote::{response_json, ArmyId, Request};
use serde_json::{json, Value};

const PLENTY: Duration = Duration::from_secs(10);
/// 1444.11.11, where the fake clock starts.
const START: i32 = 0x035D_7710;

fn quiet(_: &str) {}

fn answer(reply: &Reply) -> Value {
    reply.wait(Duration::ZERO).map_or(Value::Null, |r| response_json(&r))
}

fn ok(result: Value) -> Value {
    json!({"ok": true, "result": result})
}

fn time(request: TimeRequest) -> Request {
    Request::Time(request)
}

fn advance(days: u32, speed: Option<u32>) -> Request {
    time(TimeRequest::Advance { days, speed })
}

fn day_event(date: i32) -> Event {
    Event::Day { date: Date(date) }
}

/// The date `days` days after the start, as JSON writes it.
fn date(days: i32) -> String {
    Date(START + days * DAY).to_string()
}

/// A day passes in the game and is heard, and the engine runs at the frame.
fn next_day(engine: &Engine, game: &mut Fake) {
    game.clock.time.date += DAY;
    engine.record([day_event(game.clock.time.date)]);
    engine.run(game, &Place::Game, PLENTY);
}

/// Where in the calls the game was asked `call`.
fn position(game: &Fake, call: &str) -> usize {
    game.calls.iter().position(|c| c == call).unwrap_or_else(|| panic!("{call} not in {:#?}", game.calls))
}

#[test]
fn the_clock_is_read_and_set_as_the_player_would() {
    let engine = Engine::new(quiet);
    let mut game = Fake::default();
    let read = |engine: &Engine, game: &mut Fake| {
        let reply = engine.submit(time(TimeRequest::Get {}));
        engine.run(game, &Place::Game, PLENTY);
        answer(&reply)
    };
    assert_eq!(read(&engine, &mut game), ok(json!({"date": "1444.11.11", "paused": true, "speed": 2})));
    let replies: Vec<Reply> = [TimeRequest::Resume {}, TimeRequest::Speed { speed: 5 }, TimeRequest::Speed { speed: 6 }].into_iter().map(|r| engine.submit(time(r))).collect();
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&replies[0]), ok(Value::Null));
    assert_eq!(answer(&replies[1]), ok(Value::Null));
    assert_eq!(answer(&replies[2])["error"]["code"], "invalid_argument");
    assert_eq!(read(&engine, &mut game), ok(json!({"date": "1444.11.11", "paused": false, "speed": 5})));
    let pause = engine.submit(time(TimeRequest::Pause {}));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&pause), ok(Value::Null));
    assert_eq!(read(&engine, &mut game)["result"]["paused"], true);
}

#[test]
fn an_advance_runs_to_its_date_and_answers_with_the_events_on_the_way() {
    let engine = Engine::new(quiet);
    let mut game = Fake::default();
    engine.record([Event::ArmyOrdered { army: ArmyId(7) }]);
    let reply = engine.submit(advance(3, Some(4)));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(answer(&reply).is_null());
    // The speed is set and the stop held before time runs.
    assert!(position(&game, "speed 4") < position(&game, &format!("stop at {}", START + 3 * DAY)));
    assert!(position(&game, &format!("stop at {}", START + 3 * DAY)) < position(&game, "resume"));
    assert_eq!(*game.clock.stops.lock().unwrap(), [START + 3 * DAY]);
    assert_eq!((game.clock.time.paused(), game.clock.time.speed), (false, 4));

    next_day(&engine, &mut game);
    next_day(&engine, &mut game);
    assert!(answer(&reply).is_null());
    // On the third day the layer's stop pauses the game, and the day is heard.
    next_day(&engine, &mut game);
    assert_eq!(
        answer(&reply),
        ok(json!({
            "stopped": "date",
            "reason": "the date was reached",
            "date": date(3),
            "events": [
                {"number": 2, "event": "day", "date": date(1)},
                {"number": 3, "event": "day", "date": date(2)},
                {"number": 4, "event": "day", "date": date(3)},
            ],
            "last": 4,
            "missed": false,
        }))
    );
    assert!(game.clock.time.paused());
    assert!(game.clock.stops.lock().unwrap().is_empty(), "the stop is let go once answered");
}

#[test]
fn an_advance_waits_for_its_last_day_s_event_a_while() {
    let engine = Engine::new(quiet);
    let mut game = Fake::default();
    let heard = engine.submit(advance(1, None));
    engine.run(&mut game, &Place::Game, PLENTY);
    // The date comes before visibility reflects it, and its event after.
    game.clock.time.date += DAY;
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(answer(&heard).is_null());
    engine.record([day_event(game.clock.time.date)]);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&heard)["result"]["stopped"], "date");
    assert_eq!(answer(&heard), Value::Null, "answered once");

    // Without its event, it answers after a while all the same.
    let unheard = engine.submit(advance(1, None));
    engine.run(&mut game, &Place::Game, PLENTY);
    game.clock.time.date += DAY;
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(answer(&unheard).is_null());
    std::thread::sleep(DAY_EVENT_WAIT + Duration::from_millis(20));
    engine.run(&mut game, &Place::Game, PLENTY);
    let got = answer(&unheard);
    assert_eq!((&got["result"]["stopped"], &got["result"]["date"], &got["result"]["events"]), (&json!("date"), &json!(date(2)), &json!([])));
}

#[test]
fn an_advance_stops_early_and_says_why() {
    type Change = fn(&mut Fake);
    let cases: [(Change, &str, &str, bool); 4] = [
        (|g| g.clock.time.pause = Pause::Player, "paused", "by the player", true),
        (|g| g.clock.time.pause = Pause::Window, "paused", "a window paused the game", true),
        (|g| g.clock.game_events = 1, "game_event", "a game event waits", true),
        (|g| g.clock.time.session = 3, "session", "another game", false),
    ];
    for (change, stopped, says, paused) in cases {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let reply = engine.submit(advance(30, None));
        engine.run(&mut game, &Place::Game, PLENTY);
        next_day(&engine, &mut game);
        change(&mut game);
        engine.run(&mut game, &Place::Game, PLENTY);
        let got = answer(&reply);
        assert_eq!(got["result"]["stopped"], stopped, "{got}");
        assert!(got["result"]["reason"].as_str().unwrap().contains(says), "{got}");
        assert_eq!((&got["result"]["date"], &got["result"]["events"][0]["date"]), (&json!(date(1)), &json!(date(1))), "{got}");
        assert_eq!(game.clock.time.paused(), paused, "{stopped}: the game is left paused");
        assert!(game.clock.stops.lock().unwrap().is_empty(), "{stopped}: the stop is let go");
    }
}

#[test]
fn an_advance_does_not_run_past_a_game_event_waiting_already() {
    let engine = Engine::new(quiet);
    let mut game = Fake::default();
    game.clock.time.pause = Pause::Running;
    game.clock.game_events = 1;
    let reply = engine.submit(advance(30, Some(5)));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(
        answer(&reply),
        ok(json!({"stopped": "game_event", "reason": "a game event waits for the player's answer", "date": "1444.11.11", "events": [], "last": 0, "missed": false}))
    );
    assert!(game.clock.time.paused());
    assert!(!game.calls.iter().any(|c| c == "resume" || c.starts_with("speed") || c.starts_with("stop at")), "{:#?}", game.calls);
}

#[test]
fn an_advance_given_up_is_called_off() {
    let engine = Engine::new(quiet);
    let mut game = Fake::default();
    let reply = engine.submit(advance(30, None));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(*game.clock.stops.lock().unwrap(), [START + 30 * DAY]);
    drop(reply);
    engine.run(&mut game, &Place::Game, PLENTY);
    // The layer stops time at the end of the day under way.
    assert_eq!(*game.clock.stops.lock().unwrap(), [i32::MIN]);
}

#[test]
fn an_advance_answers_before_the_process_ends() {
    static ENGINE: Engine = Engine::new(quiet);
    let mut game = Fake::default();
    let reply = ENGINE.submit(advance(30, None));
    ENGINE.run(&mut game, &Place::Game, PLENTY);
    next_day(&ENGINE, &mut game);
    let events = ENGINE.submit(Request::Events(eu4_harmony_remote::events::EventsRequest::Read { since: Some(1), wait_ms: Some(60_000) }));
    ENGINE.run(&mut game, &Place::Game, PLENTY);
    let client = std::thread::spawn(move || reply.wait(PLENTY).map(|r| response_json(&r)));
    let started = Instant::now();
    ENGINE.ending(PLENTY);
    // It returns once the answer is taken, not when its time is up.
    assert!(started.elapsed() < Duration::from_secs(5));
    let got = client.join().unwrap().unwrap();
    assert_eq!(got["result"]["stopped"], "main_menu");
    assert_eq!(got["result"]["date"], date(1));
    assert_eq!(got["result"]["events"][0]["date"], date(1));
    // What has no answer to give then goes on waiting.
    assert!(answer(&events).is_null());
}

#[test]
fn bad_days_and_speeds_are_refused_before_anything_is_asked() {
    let engine = Engine::new(quiet);
    let mut game = Fake::default();
    let replies: Vec<Reply> = [advance(0, None), advance(MOST_DAYS + 1, None), advance(30, Some(0)), advance(30, Some(6))].into_iter().map(|r| engine.submit(r)).collect();
    engine.run(&mut game, &Place::Game, PLENTY);
    for reply in &replies {
        assert_eq!(answer(reply)["error"]["code"], "invalid_argument");
    }
    assert!(game.calls.is_empty(), "{:#?}", game.calls);
}

#[test]
fn an_advance_past_more_events_than_are_kept_says_some_were_missed() {
    let engine = Engine::new(quiet);
    let mut game = Fake::default();
    let reply = engine.submit(advance(1, None));
    engine.run(&mut game, &Place::Game, PLENTY);
    engine.record((0..KEPT as u64).map(|army| Event::ArmyOrdered { army: ArmyId(army) }));
    next_day(&engine, &mut game);
    let got = answer(&reply);
    assert_eq!((&got["result"]["stopped"], &got["result"]["missed"], &got["result"]["last"]), (&json!("date"), &json!(true), &json!(KEPT + 1)));
    assert_eq!(got["result"]["events"].as_array().unwrap().len(), KEPT);
}
