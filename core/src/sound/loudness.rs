//! Loudness as ITU-R BS.1770 measures it and EBU R 128 gates it, for the
//! game's sound: 44.1 kHz, two channels. The signal is K-weighted (a high
//! shelf for the head, then a high-pass), its mean square taken over blocks
//! of 400 ms that start every 100 ms, and a block's loudness is
//! `-0.691 + 10 log10(L + R)` in LUFS. The integrated loudness leaves out
//! blocks below -70 LUFS, then blocks more than 10 LU below the loudness of
//! those left; the short-term loudness is that of the last 3 s, ungated.

/// The game's output rate, which it asks SDL for and lets no driver change.
pub const RATE: f64 = 44100.0;
/// Frames in the 100 ms step between blocks.
const STEP: usize = 4410;
/// Steps in a block (400 ms) and in the short-term window (3 s).
const BLOCK_STEPS: usize = 4;
const SHORT_TERM_STEPS: usize = 30;
const ABSOLUTE_GATE: f64 = -70.0;
const RELATIVE_GATE: f64 = -10.0;
/// The histogram the integrated loudness is gated over: bins of 0.1 LU from
/// the absolute gate up. A block's loudness is within a bin of where it was,
/// so the gates are applied to within 0.1 LU.
const BIN: f64 = 0.1;
const BINS: usize = 800;

/// A biquad filter's coefficients, normalized so that `a0` is 1.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Coefficients {
    b: [f64; 3],
    a: [f64; 2],
}

/// The two stages of the K-weighting at `rate`, designed from the analog
/// prototype BS.1770 defines at 48 kHz, so that they give its coefficients
/// there.
fn k_weighting(rate: f64) -> [Coefficients; 2] {
    let shelf = {
        let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let vh = 10f64.powf(gain_db / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        Coefficients {
            b: [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        }
    };
    let high_pass = {
        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        Coefficients { b: [1.0, -2.0, 1.0], a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0] }
    };
    [shelf, high_pass]
}

/// One channel's two filter stages, in direct form II transposed.
#[derive(Clone, Debug)]
struct Channel {
    state: [[f64; 2]; 2],
}

impl Channel {
    fn filter(&mut self, stages: &[Coefficients; 2], mut x: f64) -> f64 {
        for (c, s) in stages.iter().zip(self.state.iter_mut()) {
            let y = c.b[0] * x + s[0];
            s[0] = c.b[1] * x - c.a[0] * y + s[1];
            s[1] = c.b[2] * x - c.a[1] * y;
            x = y;
        }
        x
    }
}

/// The loudness of the energy `mean_square`, the sum of the channels' mean
/// squares.
pub fn lufs(mean_square: f64) -> f64 {
    -0.691 + 10.0 * mean_square.max(1e-20).log10()
}

/// Measures a stream of stereo frames as they come.
#[derive(Clone, Debug)]
pub struct Meter {
    stages: [Coefficients; 2],
    channels: [Channel; 2],
    /// The weighted squares summed so far in the current step, and its frames.
    sum: f64,
    frames: usize,
    /// The mean square of each of the last steps, the newest last.
    steps: std::collections::VecDeque<f64>,
    /// The blocks above the absolute gate: in each bin, their number and the
    /// sum of their mean squares.
    counts: Vec<u32>,
    energies: Vec<f64>,
    blocks: u64,
}

impl Default for Meter {
    fn default() -> Self {
        Self {
            stages: k_weighting(RATE),
            channels: [Channel { state: [[0.0; 2]; 2] }, Channel { state: [[0.0; 2]; 2] }],
            sum: 0.0,
            frames: 0,
            steps: std::collections::VecDeque::with_capacity(SHORT_TERM_STEPS + 1),
            counts: vec![0; BINS],
            energies: vec![0.0; BINS],
            blocks: 0,
        }
    }
}

impl Meter {
    /// Adds one frame, its samples as fractions of full scale.
    pub fn add(&mut self, left: f64, right: f64) {
        let l = self.channels[0].filter(&self.stages, left);
        let r = self.channels[1].filter(&self.stages, right);
        self.sum += l * l + r * r;
        self.frames += 1;
        if self.frames == STEP {
            self.end_step();
        }
    }

    fn end_step(&mut self) {
        self.steps.push_back(self.sum / STEP as f64);
        if self.steps.len() > SHORT_TERM_STEPS {
            self.steps.pop_front();
        }
        self.sum = 0.0;
        self.frames = 0;
        if self.steps.len() >= BLOCK_STEPS {
            let block = self.steps.iter().rev().take(BLOCK_STEPS).sum::<f64>() / BLOCK_STEPS as f64;
            let loudness = lufs(block);
            if loudness > ABSOLUTE_GATE {
                let bin = (((loudness - ABSOLUTE_GATE) / BIN) as usize).min(BINS - 1);
                self.counts[bin] += 1;
                self.energies[bin] += block;
            }
            self.blocks += 1;
        }
    }

    /// The seconds of blocks measured, gated or not.
    pub fn seconds(&self) -> f64 {
        self.blocks as f64 * STEP as f64 / RATE
    }

    /// The integrated loudness of what was added, if any block was above the
    /// absolute gate.
    pub fn integrated(&self) -> Option<f64> {
        let above = |from: usize| {
            let (count, energy) = (from..BINS).fold((0u64, 0.0), |(c, e), bin| (c + u64::from(self.counts[bin]), e + self.energies[bin]));
            (count > 0).then(|| energy / count as f64)
        };
        let relative = lufs(above(0)?) + RELATIVE_GATE;
        let from = (((relative - ABSOLUTE_GATE) / BIN).max(0.0) as usize).min(BINS - 1);
        above(from).or(above(0)).map(lufs)
    }

    /// The loudness of the last 3 s, or of what there is when less was added;
    /// None before the first 100 ms.
    pub fn short_term(&self) -> Option<f64> {
        (!self.steps.is_empty()).then(|| lufs(self.steps.iter().sum::<f64>() / self.steps.len() as f64))
    }
}

/// The amplitude factor of `db` decibels.
pub fn amplitude(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, within: f64) -> bool {
        (a - b).abs() <= within
    }

    #[test]
    fn the_k_weighting_at_48_khz_is_the_standards() {
        let [shelf, high_pass] = k_weighting(48000.0);
        let expected = [1.53512485958697, -2.69169618940638, 1.19839281085285, -1.69065929318241, 0.73248077421585];
        let got = [shelf.b[0], shelf.b[1], shelf.b[2], shelf.a[0], shelf.a[1]];
        assert!(got.iter().zip(expected).all(|(g, e)| close(*g, e, 1e-10)), "{got:?}");
        assert_eq!(high_pass.b, [1.0, -2.0, 1.0]);
        assert!(close(high_pass.a[0], -1.99004745483398, 1e-10) && close(high_pass.a[1], 0.99007225036621, 1e-10), "{high_pass:?}");
    }

    fn sine(meter: &mut Meter, dbfs: f64, seconds: f64) {
        let amplitude = amplitude(dbfs);
        for i in 0..(seconds * RATE) as usize {
            let x = amplitude * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / RATE).sin();
            meter.add(x, x);
        }
    }

    #[test]
    fn a_stereo_tone_at_minus_23_dbfs_is_minus_23_lufs() {
        // EBU Tech 3341, case 1: 1 kHz in both channels at -23 dBFS.
        let mut meter = Meter::default();
        sine(&mut meter, -23.0, 20.0);
        let integrated = meter.integrated().unwrap();
        assert!(close(integrated, -23.0, 0.1), "{integrated}");
        assert!(close(meter.short_term().unwrap(), -23.0, 0.1));
        assert!(close(meter.seconds(), 19.7, 0.05), "{}", meter.seconds());
    }

    #[test]
    fn the_gates_leave_out_silence_and_quiet_passages() {
        // EBU Tech 3341, case 3 in spirit: a quiet passage more than 10 LU
        // below the rest does not count.
        let mut meter = Meter::default();
        for _ in 0..(5.0 * RATE) as usize {
            meter.add(0.0, 0.0);
        }
        sine(&mut meter, -46.0, 10.0);
        sine(&mut meter, -23.0, 10.0);
        sine(&mut meter, -46.0, 10.0);
        let integrated = meter.integrated().unwrap();
        assert!(close(integrated, -23.0, 0.2), "{integrated}");
        // The short term is the last 3 s, the quiet passage.
        assert!(close(meter.short_term().unwrap(), -46.0, 0.1));
    }

    #[test]
    fn silence_has_no_integrated_loudness() {
        let mut meter = Meter::default();
        for _ in 0..RATE as usize {
            meter.add(0.0, 0.0);
        }
        assert_eq!(meter.integrated(), None);
    }
}
