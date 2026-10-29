//! Time: the date, pausing and resuming, the game speed, and letting time run
//! for a number of days.
use crate::{events::Events, Date};
use serde::{Deserialize, Serialize};

group! {
    /// Time: the date, pausing and resuming, the speed, and advancing time.
    "time": TimeRequest, TimeAnswer;

    /// The date, whether the game is paused, and its speed.
    game Get "" {} -> Time, "the date, whether the game is paused, and the speed from 1 (slowest) to 5 (fastest)";
    /// Pauses the game with the player's own pause, as the player would,
    /// also when a window holds it paused already, such as an event window:
    /// the game then stays paused once the window closes, until it is
    /// resumed. Nothing when the player's pause holds it already.
    game Pause "pause" {} -> (), "nothing";
    /// Resumes the game, as the player would: also when a window holds it
    /// paused, such as an event window when the game pauses on events, which
    /// stays open. Nothing when it runs.
    game Resume "resume" {} -> (), "nothing";
    /// Sets the game speed, as the player would; the game stays paused or
    /// running as it was.
    game Speed "speed" {
        /// From 1 (slowest) to 5 (fastest).
        speed: u32,
    } -> (), "nothing";
    /// Lets time run until the date some days from now, then pauses the game
    /// and answers with the events on the way. It answers early, with the
    /// reason and the date, when the game is paused by someone else or by a
    /// window, when a question waits for the player's answer (also one
    /// waiting already: then time does not run at all), when another game
    /// is loaded or started, and when the game returns to the main menu
    /// (eu4.exe starts again, and the answer comes just before the old
    /// process goes). Stopped at the date, paused or for a question, it
    /// leaves the game paused as time.pause does, so that it stays paused
    /// until it is resumed, also once a window that holds it paused closes.
    /// The answer comes at a frame of the game's, however long the days take
    /// at the speed; a client that gives the request up calls the advance
    /// off, and time stops at the end of the day under way.
    game Advance "advance" {
        /// How many days, from 1 to 36500.
        days: u32,
        /// The speed to run at, from 1 (slowest) to 5 (fastest); the game
        /// stays at it after the advance, as after time.speed. Absent to
        /// keep the game's.
        speed: Option<u32>,
    } -> Advanced, "why it stopped (date, paused, question, session or main_menu) and in words, the date then, and as events answers them (read from the last event before time ran): the process, the events on the way, the number of the last event, and whether some were missed";
}

/// The game's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Time {
    pub date: Date,
    pub paused: bool,
    /// From 1 (slowest) to 5 (fastest).
    pub speed: u32,
}

/// Why an advance stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stop {
    /// The date was reached; the game is paused.
    Date,
    /// The game was paused before the date: by the player, an extension or
    /// another remote client, or by a window that pauses it.
    Paused,
    /// A question (one of the game's events, question.list) waits for the
    /// player's answer; the game is paused.
    Question,
    /// Another game was loaded or started.
    Session,
    /// The game is returning to the main menu: eu4.exe starts again.
    MainMenu,
}

/// Where an advance stopped, and what happened on the way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advanced {
    pub stopped: Stop,
    /// Why, in words.
    pub reason: String,
    /// The date when it stopped.
    pub date: Date,
    /// The events after the last one before time ran: `process`, `events`,
    /// `last` and `missed`, as the events capability answers them.
    #[serde(flatten)]
    pub events: Events,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{events::Numbered, request_from_json, response_json, Answer, Request};
    use serde_json::json;

    #[test]
    fn an_advance_in_json() {
        let request = request_from_json(r#"{"capability": "time.advance", "days": 30, "speed": 5}"#).unwrap();
        assert_eq!(request, Request::Time(TimeRequest::Advance { days: 30, speed: Some(5) }));
        assert_eq!(serde_json::to_value(Request::Time(TimeRequest::Advance { days: 30, speed: None })).unwrap(), json!({"capability": "time.advance", "days": 30}));
        let date = Date::from_ymd(1444, 11, 12).unwrap();
        let advanced = Advanced {
            stopped: Stop::Question,
            reason: "a question waits for the player's answer".into(),
            date,
            events: Events { process: 1_761_700_000_000, events: vec![Numbered { number: 4, event: crate::events::Event::Day { date } }], last: 4, missed: false },
        };
        assert_eq!(
            response_json(&Ok(Answer::Time(TimeAnswer::Advance(advanced)))),
            json!({"ok": true, "result": {
                "stopped": "question", "reason": "a question waits for the player's answer", "date": "1444.11.12", "process": 1_761_700_000_000u64,
                "events": [{"number": 4, "event": "day", "date": "1444.11.12"}], "last": 4, "missed": false,
            }})
        );
        let time = Time { date, paused: true, speed: 3 };
        assert_eq!(serde_json::to_value(time).unwrap(), json!({"date": "1444.11.12", "paused": true, "speed": 3}));
    }
}
