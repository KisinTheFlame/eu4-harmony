//! Time.
use crate::{Checked, Date, Refusal, Status};

/// The game speeds, slowest first.
pub const SPEEDS: std::ops::RangeInclusive<u32> = 1..=5;

/// Refuses a speed the game does not have.
pub fn speed(speed: u32) -> Checked {
    if !SPEEDS.contains(&speed) {
        return Err(Refusal::new(Status::InvalidArgument, format!("the speed is from {} to {}, not {speed}", SPEEDS.start(), SPEEDS.end())));
    }
    Ok(())
}

/// The refusal of time running once the game is over, at its end date
/// `end`: past it, unless the player chose to play on, the game stays paused
/// as it does for the player.
pub fn over(end: Date) -> Refusal {
    Refusal::new(
        Status::Rejected,
        format!("the game is over: it is past its end date, {end}, and stays paused, as for the player, until the end-of-game summary is closed to play on"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_speeds_are_the_game_s_five() {
        assert!(SPEEDS.clone().all(|s| speed(s).is_ok()));
        for wrong in [0, 6] {
            let refusal = speed(wrong).unwrap_err();
            assert_eq!((refusal.status, refusal.message), (Status::InvalidArgument, format!("the speed is from 1 to 5, not {wrong}")));
        }
    }

    #[test]
    fn the_game_is_over_at_its_end_date() {
        let refusal = over(Date::from_ymd(1821, 1, 2).unwrap());
        assert_eq!(refusal.status, Status::Rejected);
        assert!(refusal.message.starts_with("the game is over: it is past its end date, 1821.1.2, "), "{}", refusal.message);
    }
}
