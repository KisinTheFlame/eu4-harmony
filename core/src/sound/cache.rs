//! The loudness measured of each track heard, kept between games in
//! `eu4-harmony/music-loudness.txt` under the user directory: one line a
//! track, its fingerprint in hex, its integrated loudness and the seconds
//! measured. A longer hearing replaces a shorter one.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What was measured of a track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measured {
    pub loudness: f64,
    pub seconds: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cache {
    tracks: HashMap<u64, Measured>,
}

/// A hearing replaces what is kept when it is this many seconds longer, so
/// that a track is written a few times while it plays, not every chunk.
const LONGER_BY: f64 = 20.0;

impl Cache {
    pub fn parse(text: &str) -> Self {
        let tracks = text
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                let fingerprint = u64::from_str_radix(words.next()?, 16).ok()?;
                let loudness: f64 = words.next()?.parse().ok()?;
                let seconds: f64 = words.next()?.parse().ok()?;
                (loudness.is_finite() && seconds.is_finite()).then_some((fingerprint, Measured { loudness, seconds }))
            })
            .collect();
        Self { tracks }
    }

    pub fn render(&self) -> String {
        let mut lines: Vec<_> = self.tracks.iter().map(|(f, m)| format!("{f:016x} {:.2} {:.1}\n", m.loudness, m.seconds)).collect();
        lines.sort();
        lines.concat()
    }

    pub fn get(&self, fingerprint: u64) -> Option<Measured> {
        self.tracks.get(&fingerprint).copied()
    }

    /// Keeps `measured` for the track if it is the first or a longer hearing;
    /// whether it was kept.
    pub fn offer(&mut self, fingerprint: u64, measured: Measured) -> bool {
        let longer = self.tracks.get(&fingerprint).is_none_or(|kept| measured.seconds >= kept.seconds + LONGER_BY);
        if longer && measured.seconds >= LONGER_BY {
            self.tracks.insert(fingerprint, measured);
        }
        longer && measured.seconds >= LONGER_BY
    }
}

pub fn path(user_dir: &Path) -> PathBuf {
    user_dir.join("eu4-harmony").join("music-loudness.txt")
}

pub fn read(path: &Path) -> Cache {
    std::fs::read_to_string(path).map(|text| Cache::parse(&text)).unwrap_or_default()
}

/// Writes the cache beside its place first, so that a game ended while it
/// writes leaves the old file whole.
pub fn write(path: &Path, cache: &Cache) -> std::io::Result<()> {
    let temporary = path.with_extension("txt.tmp");
    std::fs::write(&temporary, cache.render())?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_skips_what_it_cannot_read() {
        let mut cache = Cache::default();
        assert!(cache.offer(0xabc, Measured { loudness: -21.6, seconds: 160.3 }));
        assert!(cache.offer(0x1, Measured { loudness: -9.9, seconds: 95.0 }));
        let text = cache.render();
        assert_eq!(text, "0000000000000001 -9.90 95.0\n0000000000000abc -21.60 160.3\n");
        assert_eq!(Cache::parse(&format!("{text}nonsense\nffff NaN 3\n12 -3\n")), cache);
    }

    #[test]
    fn keeps_a_hearing_only_when_it_is_long_enough_and_longer() {
        let mut cache = Cache::default();
        assert!(!cache.offer(7, Measured { loudness: -12.0, seconds: 10.0 }));
        assert_eq!(cache.get(7), None);
        assert!(cache.offer(7, Measured { loudness: -12.0, seconds: 25.0 }));
        assert!(!cache.offer(7, Measured { loudness: -14.0, seconds: 40.0 }));
        assert!(cache.offer(7, Measured { loudness: -14.0, seconds: 45.0 }));
        assert_eq!(cache.get(7), Some(Measured { loudness: -14.0, seconds: 45.0 }));
    }
}
