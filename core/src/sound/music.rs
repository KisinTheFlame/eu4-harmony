//! What the framework does to a playing track before it is mixed: the
//! loudness normalization (GLOSSARY) gives every track the same integrated
//! loudness, the music leveling (GLOSSARY) brings quiet and loud passages
//! closer to it, and a limiter keeps the raised samples within full scale.
//! Nothing here knows the game: a [`Track`] is fed the track's frames as the
//! game mixes them, with the gain the game gives them.
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
/// The seconds of a first hearing measured before its integrated loudness
/// is used to lower it; until then the track plays as it is.
const FIRST_ESTIMATE: f64 = 3.0;
/// The seconds measured before a first hearing is raised: a quiet opening is
/// no measure of a track (the main theme is -35.6 LUFS over its first 20 s,
/// -13.7 over a minute and -11.4 over 240 s).
const FIRST_RAISE: f64 = 60.0;
/// How fast the gains follow, in dB a second: a track's known loudness is
/// taken up within a second, an estimate and leveling more slowly.
const KNOWN_SLEW: f64 = 12.0;
const ESTIMATE_SLEW: f64 = 2.0;
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
    meter: Meter,
    /// The track's integrated loudness from an earlier hearing, if known, and
    /// the seconds it is over.
    known: Option<(f64, f64)>,
    normalization: f64,
    leveling: f64,
    /// The limiter's current factor, 1 for none.
    limiter: f64,
}

impl Default for Track {
    fn default() -> Self {
        Self { meter: Meter::default(), known: None, normalization: 0.0, leveling: 0.0, limiter: 1.0 }
    }
}

impl Track {
    /// Gives the track's loudness from an earlier hearing over `seconds`.
    pub fn know(&mut self, loudness: f64, seconds: f64) {
        self.known = Some((loudness, seconds));
    }

    /// What this hearing measured: its integrated loudness and the seconds
    /// it is over.
    pub fn measured(&self) -> Option<(f64, f64)> {
        self.meter.integrated().map(|loudness| (loudness, self.meter.seconds()))
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
        // An earlier hearing counts until this one is longer.
        let known = self.known.filter(|&(_, seconds)| seconds >= self.meter.seconds());
        let (wanted, slew) = match (choices.normalize, known) {
            (false, _) => (0.0, KNOWN_SLEW),
            (true, Some((known, _))) => ((TARGET - known).clamp(-RANGE, RANGE), KNOWN_SLEW),
            (true, None) => match self.meter.integrated() {
                Some(estimate) if self.meter.seconds() >= FIRST_ESTIMATE => {
                    let wanted = (TARGET - estimate).clamp(-RANGE, RANGE);
                    (if self.meter.seconds() < FIRST_RAISE { wanted.min(0.0) } else { wanted }, ESTIMATE_SLEW)
                }
                _ => (self.normalization, ESTIMATE_SLEW),
            },
        };
        self.normalization = toward(self.normalization, wanted, slew * seconds);
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

/// Recognizes a track by its decoded samples, so that what was measured of
/// it is found again in a later game whatever its file is called: a hash of
/// its first second after any silence at its start.
#[derive(Clone, Debug, Default)]
pub struct Fingerprint {
    hash: u64,
    frames: usize,
}

/// A sample quieter than this, about -60 dBFS, is silence before the music starts.
const QUIET: i16 = 32;
const FINGERPRINT_FRAMES: usize = RATE as usize;

impl Fingerprint {
    /// Takes the track's next frames; the fingerprint once it has enough.
    pub fn add(&mut self, frames: &[i16]) -> Option<u64> {
        for frame in frames.chunks_exact(2) {
            if self.frames >= FINGERPRINT_FRAMES {
                break;
            }
            if self.frames == 0 && frame.iter().all(|s| s.unsigned_abs() < QUIET as u16) {
                continue;
            }
            if self.frames == 0 {
                self.hash = 0xcbf2_9ce4_8422_2325;
            }
            for byte in frame.iter().flat_map(|s| s.to_le_bytes()) {
                self.hash = (self.hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
            }
            self.frames += 1;
        }
        self.done()
    }

    pub fn done(&self) -> Option<u64> {
        (self.frames >= FINGERPRINT_FRAMES).then_some(self.hash)
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
    fn a_known_quiet_track_is_raised_to_the_target_and_held_below_full_scale() {
        let mut track = Track::default();
        // A tone's loudness is its level in dBFS, so -23 LUFS: raised 10.2 dB.
        track.know(-23.0, 200.0);
        let out = play(&mut track, -23.0, 2.0, Choices { normalize: true, level: false });
        let raised = 20.0 * peak(&out).log10() - -23.0;
        assert!((raised - 10.2).abs() < 0.1, "{raised}");
        let mut loud = Track::default();
        loud.know(-24.0, 200.0);
        let out = play(&mut loud, -3.0, 2.0, Choices { normalize: true, level: false });
        assert!(peak(&out) <= CEILING + 1e-3, "{}", peak(&out));
    }

    #[test]
    fn a_first_hearing_is_lowered_soon_and_raised_only_after_a_minute() {
        let choices = Choices { normalize: true, level: false };
        let change = |out: &[i16], dbfs: f64| 20.0 * peak(out).log10() - dbfs;
        let mut loud = Track::default();
        // Played as it is until 3 s are measured.
        let out = play(&mut loud, -3.0, 2.5, choices);
        assert!(change(&out, -3.0).abs() < 0.05);
        // Then lowered by 2 dB a second toward -9.8 dB.
        let out = play(&mut loud, -3.0, 3.0, choices);
        assert!((-6.5..-3.0).contains(&change(&out, -3.0)), "{}", change(&out, -3.0));
        let out = play(&mut loud, -3.0, 5.0, choices);
        assert!((change(&out, -3.0) - -9.8).abs() < 0.1, "{}", change(&out, -3.0));
        // A quiet one is not raised before a minute is measured, then toward 7.2 dB.
        let mut quiet = Track::default();
        let out = play(&mut quiet, -20.0, 55.0, choices);
        assert!(change(&out, -20.0).abs() < 0.05, "{}", change(&out, -20.0));
        let out = play(&mut quiet, -20.0, 10.0, choices);
        assert!((change(&out, -20.0) - 7.2).abs() < 0.1, "{}", change(&out, -20.0));
        let (loudness, seconds) = quiet.measured().unwrap();
        assert!((loudness - -20.0).abs() < 0.1 && seconds > 60.0);
    }

    #[test]
    fn an_earlier_hearing_counts_until_this_one_is_longer() {
        let choices = Choices { normalize: true, level: false };
        let mut track = Track::default();
        // Kept from a minute of a quiet opening: raised the most at first.
        track.know(-26.0, 60.0);
        let out = play(&mut track, -20.0, 30.0, choices);
        assert!((20.0 * peak(&out).log10() - -20.0 - 12.0).abs() < 0.1);
        // Past a minute, this hearing's -20 LUFS: 7.2 dB.
        let out = play(&mut track, -20.0, 40.0, choices);
        assert!((20.0 * peak(&out).log10() - -20.0 - 7.2).abs() < 0.1);
    }

    #[test]
    fn leveling_raises_a_quiet_passage_by_two_thirds_of_its_distance() {
        let mut track = Track::default();
        track.know(TARGET, 200.0);
        let out = play(&mut track, -30.8, 12.0, Choices { normalize: true, level: true });
        // 18 dB below the target, raised by 12, the most.
        let raised = 20.0 * peak(&out).log10() - -30.8;
        assert!((raised - 12.0).abs() < 0.1, "{raised}");
        let out = play(&mut track, -18.8, 12.0, Choices { normalize: true, level: true });
        let raised = 20.0 * peak(&out).log10() - -18.8;
        assert!((raised - 4.0).abs() < 0.1, "{raised}");
    }

    #[test]
    fn the_fingerprint_starts_after_silence_and_needs_a_second() {
        let silence = vec![0i16; 2048];
        let music: Vec<i16> = (0..50).flat_map(|n| tone(-20.0, n * 1024)).collect();
        let mut late = Fingerprint::default();
        assert_eq!(late.add(&silence), None);
        assert_eq!(late.add(&music[..40000]), None);
        let late = late.add(&music[40000..]).unwrap();
        let mut early = Fingerprint::default();
        assert_eq!(early.add(&music), Some(late));
        let mut other = Fingerprint::default();
        let quieter: Vec<i16> = (0..50).flat_map(|n| tone(-21.0, n * 1024)).collect();
        assert_ne!(other.add(&quieter), Some(late));
    }
}
