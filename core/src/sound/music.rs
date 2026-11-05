//! What the framework does to a playing track before it is mixed: the
//! loudness normalization (GLOSSARY) brings the track, by its integrated
//! loudness measured from its whole file, to the same loudness as every
//! other; the music leveling (GLOSSARY) brings quiet and loud passages closer
//! to it as they play; and a limiter keeps the raised samples within full
//! scale. Nothing here knows the game: a [`Track`] is fed the track's frames
//! as the game mixes them, with the gain the game gives them.
use super::loudness::{amplitude, Meter, RATE};

/// The loudness every track is brought to: the median of the vanilla tracks'
/// integrated loudness (docs/prototype-log.md, 2026-11-05), so that music as
/// a whole is about as loud as in vanilla.
pub const TARGET: f64 = -12.8;
/// The most a track is raised or lowered, by normalization and by leveling each.
const RANGE: f64 = 12.0;
/// How far leveling brings the short-term loudness to the target: of the
/// distance, all but a third.
const LEVELING_RATIO: f64 = 3.0;
/// Below this short-term loudness a passage is silence, which leveling does
/// not raise.
const SILENCE: f64 = -60.0;
/// How fast the gains follow, in dB a second: a track's loudness is taken up
/// within a second of its analysis, leveling more slowly.
const NORMALIZATION_SLEW: f64 = 12.0;
const LEVELING_SLEW: f64 = 3.0;
/// The limiter's ceiling, as a fraction of full scale, and how much of the
/// way back to no reduction it goes in a frame (about 100 ms to recover).
const CEILING: f64 = 0.95;
const RELEASE: f64 = 1.0 / (0.1 * RATE);

/// What the player turned on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Choices {
    pub normalize: bool,
    pub level: bool,
}

/// One track as it plays.
#[derive(Clone, Debug)]
pub struct Track {
    /// What has played, for leveling's short-term loudness.
    meter: Meter,
    /// The integrated loudness of the track's whole file, once analyzed.
    loudness: Option<f64>,
    normalization: f64,
    leveling: f64,
    /// The limiter's current factor, 1 for none.
    limiter: f64,
}

impl Default for Track {
    fn default() -> Self {
        Self { meter: Meter::default(), loudness: None, normalization: 0.0, leveling: 0.0, limiter: 1.0 }
    }
}

impl Track {
    /// Gives the integrated loudness of the track's whole file; until then
    /// it plays as it is.
    pub fn know(&mut self, loudness: f64) {
        self.loudness = Some(loudness);
    }

    pub fn known(&self) -> bool {
        self.loudness.is_some()
    }

    /// Mixes `source`, interleaved stereo frames, into `mixed` at the game's
    /// `gain`, the way SDL mixes: added, and held within the samples' range.
    pub fn mix(&mut self, source: &[i16], mixed: &mut [i16], gain: f64, choices: Choices) {
        let frames = source.len().min(mixed.len()) / 2;
        for frame in source[..frames * 2].chunks_exact(2) {
            self.meter.add(f64::from(frame[0]) / 32768.0, f64::from(frame[1]) / 32768.0);
        }
        let seconds = frames as f64 / RATE;
        let (from_normalization, from_leveling) = (self.normalization, self.leveling);
        self.follow(choices, seconds);
        let start = gain * amplitude(from_normalization + from_leveling);
        let end = gain * amplitude(self.normalization + self.leveling);
        for i in 0..frames {
            // The gain moves across the chunk, so a change makes no step.
            let g = start + (end - start) * (i as f64 + 1.0) / frames as f64;
            let (l, r) = (f64::from(source[2 * i]) * g, f64::from(source[2 * i + 1]) * g);
            let peak = l.abs().max(r.abs()) / 32768.0;
            self.limiter += (1.0 - self.limiter) * RELEASE;
            if peak * self.limiter > CEILING {
                self.limiter = CEILING / peak;
            }
            for (out, x) in mixed[2 * i..2 * i + 2].iter_mut().zip([l, r]) {
                *out = (f64::from(*out) + (x * self.limiter).round()).clamp(-32768.0, 32767.0) as i16;
            }
        }
    }

    /// Moves the gains toward what the track wants now, over `seconds`.
    fn follow(&mut self, choices: Choices, seconds: f64) {
        let wanted = match (choices.normalize, self.loudness) {
            (true, Some(loudness)) => (TARGET - loudness).clamp(-RANGE, RANGE),
            _ => 0.0,
        };
        self.normalization = toward(self.normalization, wanted, NORMALIZATION_SLEW * seconds);
        let wanted = match (choices.level, self.meter.short_term()) {
            (true, Some(short_term)) if short_term > SILENCE => {
                let heard = short_term + self.normalization;
                ((TARGET - heard) * (1.0 - 1.0 / LEVELING_RATIO)).clamp(-RANGE, RANGE)
            }
            (true, _) => self.leveling,
            (false, _) => 0.0,
        };
        self.leveling = toward(self.leveling, wanted, LEVELING_SLEW * seconds);
    }
}

fn toward(from: f64, to: f64, by: f64) -> f64 {
    if to > from {
        (from + by).min(to)
    } else {
        (from - by).max(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk of the game's size of a 1 kHz tone at `dbfs`, from frame `at`.
    fn tone(dbfs: f64, at: usize) -> Vec<i16> {
        let a = amplitude(dbfs) * 32767.0;
        (at..at + 1024)
            .flat_map(|i| {
                let x = (a * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / RATE).sin()).round() as i16;
                [x, x]
            })
            .collect()
    }

    /// Plays `seconds` of a tone through `track`, the last chunk's output.
    fn play(track: &mut Track, dbfs: f64, seconds: f64, choices: Choices) -> Vec<i16> {
        let mut out = Vec::new();
        for n in 0..(seconds * RATE / 1024.0) as usize {
            out = vec![0; 2048];
            track.mix(&tone(dbfs, n * 1024), &mut out, 1.0, choices);
        }
        out
    }

    fn peak(samples: &[i16]) -> f64 {
        samples.iter().map(|s| f64::from(s.unsigned_abs()) / 32768.0).fold(0.0, f64::max)
    }

    fn change(out: &[i16], dbfs: f64) -> f64 {
        20.0 * peak(out).log10() - dbfs
    }

    #[test]
    fn nothing_turned_on_mixes_as_sdl_does() {
        let mut track = Track::default();
        let source = tone(-6.0, 0);
        let mut mixed = vec![30000; 2048];
        track.mix(&source, &mut mixed, 0.5, Choices::default());
        for (m, s) in mixed.iter().zip(&source) {
            assert_eq!(*m, (30000.0 + (f64::from(*s) * 0.5).round()).min(32767.0) as i16);
        }
    }

    #[test]
    fn a_quiet_track_is_raised_to_the_target_and_a_loud_one_held_below_full_scale() {
        let choices = Choices { normalize: true, level: false };
        let mut track = Track::default();
        // A tone's loudness is its level in dBFS, so -23 LUFS: raised 10.2 dB.
        track.know(-23.0);
        let out = play(&mut track, -23.0, 2.0, choices);
        assert!((change(&out, -23.0) - 10.2).abs() < 0.1, "{}", change(&out, -23.0));
        let mut loud = Track::default();
        loud.know(-24.0);
        let out = play(&mut loud, -3.0, 2.0, choices);
        assert!(peak(&out) <= CEILING + 1e-3, "{}", peak(&out));
    }

    #[test]
    fn a_track_plays_as_it_is_until_its_analysis_is_in() {
        let choices = Choices { normalize: true, level: false };
        let mut track = Track::default();
        let out = play(&mut track, -20.0, 1.0, choices);
        assert!(change(&out, -20.0).abs() < 0.05);
        // Then 7.2 dB, reached within a second.
        track.know(-20.0);
        let out = play(&mut track, -20.0, 1.0, choices);
        assert!((change(&out, -20.0) - 7.2).abs() < 0.1, "{}", change(&out, -20.0));
        // Normalization turned off goes back to the track as it is.
        let out = play(&mut track, -20.0, 1.0, Choices::default());
        assert!(change(&out, -20.0).abs() < 0.05);
    }

    #[test]
    fn leveling_raises_a_quiet_passage_by_two_thirds_of_its_distance() {
        let choices = Choices { normalize: true, level: true };
        let mut track = Track::default();
        track.know(TARGET);
        let out = play(&mut track, -30.8, 12.0, choices);
        // 18 dB below the target, raised by 12, the most.
        assert!((change(&out, -30.8) - 12.0).abs() < 0.1, "{}", change(&out, -30.8));
        let out = play(&mut track, -18.8, 12.0, choices);
        assert!((change(&out, -18.8) - 4.0).abs() < 0.1, "{}", change(&out, -18.8));
    }
}
