//! The time capabilities driven through the engine, a fake clock behind them.
use super::*;
use crate::remote::{fake::Fake, Engine, Place, Reply};
use eu4_harmony_remote::{response_json, Request};
use serde_json::{json, Value};
use std::time::Duration;

const PLENTY: Duration = Duration::from_secs(10);

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
