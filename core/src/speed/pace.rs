//! The pace at the fastest speed (docs/plans/game-speed.md).
//!
//! Between the daily update's phases the game draws a whole frame once a day
//! has run longer than an interval, the float at `0x245b070` times the
//! speed's factor in a table at `0x1c33e18` (0.05 s at speed 5). Such a frame
//! executes no commands and draws no random numbers (GLOSSARY: Frame inside a
//! day). With "faster time" the framework sets speed 5's factor so large that
//! no frame comes inside a day; the other speeds' factors stay the game's.
//! Only that instruction reads the table.
use super::SpeedUp;
use crate::points::Point;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const SPEED_UP: SpeedUp = SpeedUp { name: "fastest-pace", group: "frames", points, checkable: false };

/// The settings' choice for the fastest speed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pace {
    /// Frames inside a day as the game draws them.
    #[default]
    Smooth,
    /// No frame inside a day at the fastest speed.
    Faster,
}

impl Pace {
    /// Every pace, in the order the settings window's choice bar shows them.
    pub const ALL: [Pace; 2] = [Pace::Smooth, Pace::Faster];

    /// As the settings file keeps it.
    pub fn name(self) -> &'static str {
        match self {
            Pace::Smooth => "smooth",
            Pace::Faster => "faster",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// The key of its label in the settings window's choice bar.
    pub fn label(self) -> &'static str {
        match self {
            Pace::Smooth => "EU4HARMONY_SETTINGS_PACE_SMOOTH",
            Pace::Faster => "EU4HARMONY_SETTINGS_PACE_FASTER",
        }
    }

    /// Speed 5's factor: the game's, or more seconds than any day takes.
    fn factor(self) -> f32 {
        match self {
            Pace::Smooth => GAME_FACTOR,
            Pace::Faster => 1.0e6,
        }
    }
}

/// Speed 5's factor as the game has it.
const GAME_FACTOR: f32 = 0.05;

/// Where speed 5's factor is, once startup has checked it; 0 when the pace is
/// left to the game.
static FASTEST_FACTOR: AtomicUsize = AtomicUsize::new(0);

/// The table of factors, one f32 for each speed from 1 to 5.
const FACTORS: usize = 0x1c33e18;
/// The interval's float, which the game takes from its settings.
const INTERVAL: usize = 0x245b070;

fn points() -> Vec<Point> {
    vec![
        // movss xmm6, [interval]; mulss xmm6, [r13 + speed*4 + factors]:
        // the interval of frames inside a day, by speed.
        Point::keep(0x769f59, "f3 0f 10 35 0f 11 cf 01 f3 41 0f 59 b4 85 18 3e c3 01", "the interval of frames inside a day").uses(&[INTERVAL, FACTORS]),
        // 0.05, speed 5's factor.
        Point::keep(FACTORS + 4 * 4, "cd cc 4c 3d", "the fastest speed's factor for frames inside a day").recorded(&FASTEST_FACTOR),
    ]
}

/// Sets the pace at the fastest speed, from now on: at startup, and when the
/// settings window closes.
pub fn set_pace(pace: Pace) {
    let at = FASTEST_FACTOR.load(Ordering::Acquire);
    let Some(m) = crate::memory().filter(|_| at != 0) else { return };
    match m.write_read_only(at, &pace.factor().to_le_bytes()) {
        Ok(()) => log!("speed: at the fastest speed, {}", pace.name()),
        Err(e) => log!("speed: the pace at the fastest speed stays the game's: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paces_are_named_as_the_settings_file_keeps_them() {
        for pace in [Pace::Smooth, Pace::Faster] {
            assert_eq!(Pace::from_name(pace.name()), Some(pace));
        }
        assert_eq!(Pace::from_name("fast"), None);
        assert_eq!(Pace::default(), Pace::Smooth);
    }

    #[test]
    fn smooth_is_the_games_factor_and_faster_outlasts_any_day() {
        assert_eq!(Pace::Smooth.factor().to_le_bytes(), [0xcd, 0xcc, 0x4c, 0x3d]);
        assert!(Pace::Faster.factor() > 3600.0);
    }
}
