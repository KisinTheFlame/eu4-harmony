//! A track's integrated loudness, from its whole file before it is heard:
//! the Ogg Vorbis file decoded (with `lewton`, a pure Rust decoder) and
//! measured as the game plays it, a mono file on both channels and a file of
//! more than two on its first two.
use super::loudness::Meter;
use std::io::Cursor;

/// The integrated loudness of the Ogg Vorbis file `data`; None for a file of
/// silence.
pub fn loudness(data: &[u8]) -> Result<Option<f64>, String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(data)).map_err(|e| format!("not Ogg Vorbis: {e}"))?;
    let channels = usize::from(reader.ident_hdr.audio_channels);
    if channels == 0 {
        return Err("no channels".into());
    }
    let mut meter = Meter::at(f64::from(reader.ident_hdr.audio_sample_rate));
    while let Some(samples) = reader.read_dec_packet_itl().map_err(|e| format!("cannot be decoded: {e}"))? {
        for frame in samples.chunks_exact(channels) {
            let left = f64::from(frame[0]) / 32768.0;
            let right = frame.get(1).map_or(left, |&s| f64::from(s) / 32768.0);
            meter.add(left, right);
        }
    }
    Ok(meter.integrated())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_not_ogg_vorbis_is_refused() {
        assert!(loudness(b"RIFF....WAVEfmt ").is_err());
        assert!(loudness(&[]).is_err());
    }

    /// The game's main theme, measured by ffmpeg's `ebur128` at -10.9 LUFS.
    #[test]
    #[ignore = "needs the game's files, EU4_GAME_DIR"]
    fn the_main_theme_is_what_ffmpeg_measures() {
        let dir = std::env::var("EU4_GAME_DIR").expect("EU4_GAME_DIR");
        let data = std::fs::read(std::path::Path::new(&dir).join("music").join("maintheme.ogg")).unwrap();
        let started = std::time::Instant::now();
        let loudness = loudness(&data).unwrap().unwrap();
        let took = started.elapsed();
        assert!((loudness - -10.9).abs() < 0.15, "{loudness}");
        assert!(took.as_secs_f64() < 2.0, "{took:?}");
    }
}
