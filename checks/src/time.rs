//! Time.
use crate::{Checked, Refusal, Status};

/// The game speeds, slowest first.
pub const SPEEDS: std::ops::RangeInclusive<u32> = 1..=5;

/// Refuses a speed the game does not have.
pub fn speed(speed: u32) -> Checked {
    if !SPEEDS.contains(&speed) {
        return Err(Refusal::new(Status::InvalidArgument, format!("the speed is from {} to {}, not {speed}", SPEEDS.start(), SPEEDS.end())));
    }
    Ok(())
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
}
