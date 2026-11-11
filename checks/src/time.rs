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

/// The most days one wait or advance may ask for: a hundred years.
pub const MOST_DAYS: u32 = 36_500;

/// Refuses a number of days to wait for that is not from 1 to
/// [`MOST_DAYS`].
pub fn days(days: u32) -> Checked {
    if !(1..=MOST_DAYS).contains(&days) {
        return Err(Refusal::new(Status::InvalidArgument, format!("days is from 1 to {MOST_DAYS}, not {days}")));
    }
    Ok(())
}

/// The date `days` after `from`, or the refusal of a wait past the
/// calendar's end.
pub fn days_after(from: Date, days: u32) -> Result<Date, Refusal> {
    self::days(days)?;
    from.days_later(days).ok_or_else(|| Refusal::new(Status::InvalidArgument, "the date would run past the game's last"))
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
    fn a_wait_is_for_one_day_to_a_hundred_years() {
        let from = Date::from_ymd(1444, 11, 11).unwrap();
        assert_eq!(days_after(from, 30).unwrap(), Date::from_ymd(1444, 12, 11).unwrap());
        for wrong in [0, MOST_DAYS + 1] {
            let refusal = days_after(from, wrong).unwrap_err();
            assert_eq!((refusal.status, refusal.message), (Status::InvalidArgument, format!("days is from 1 to 36500, not {wrong}")));
        }
    }

    #[test]
    fn the_game_is_over_at_its_end_date() {
        let refusal = over(Date::from_ymd(1821, 1, 2).unwrap());
        assert_eq!(refusal.status, Status::Rejected);
        assert!(refusal.message.starts_with("the game is over: it is past its end date, 1821.1.2, "), "{}", refusal.message);
    }
}
