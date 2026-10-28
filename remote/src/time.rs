//! Time: the date, pausing and resuming, and the game speed.
use crate::Date;
use serde::{Deserialize, Serialize};

group! {
    /// Time: the date, pausing and resuming, and the speed.
    "time": TimeRequest, TimeAnswer;

    /// The date, whether the game is paused, and its speed.
    game Get "" {} -> Time, "the date, whether the game is paused, and the speed from 1 (slowest) to 5 (fastest)";
    /// Pauses the game, as the player would; nothing when it is paused
    /// already.
    game Pause "pause" {} -> (), "nothing";
    /// Resumes the game, as the player would: also when a window holds it
    /// paused, such as a game event's when the game pauses on events, which
    /// stays open. Nothing when it runs.
    game Resume "resume" {} -> (), "nothing";
    /// Sets the game speed, as the player would; the game stays paused or
    /// running as it was.
    game Speed "speed" {
        /// From 1 (slowest) to 5 (fastest).
        speed: u32,
    } -> (), "nothing";
}

/// The game's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Time {
    pub date: Date,
    pub paused: bool,
    /// From 1 (slowest) to 5 (fastest).
    pub speed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{request_from_json, Request};
    use serde_json::json;

    #[test]
    fn the_clock_in_json() {
        let request = request_from_json(r#"{"capability": "time.speed", "speed": 5}"#).unwrap();
        assert_eq!(request, Request::Time(TimeRequest::Speed { speed: 5 }));
        let time = Time { date: Date::from_ymd(1444, 11, 12).unwrap(), paused: true, speed: 3 };
        assert_eq!(serde_json::to_value(time).unwrap(), json!({"date": "1444.11.12", "paused": true, "speed": 3}));
    }
}
