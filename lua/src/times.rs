//! What an extension's stretches took: how many, in all, and how they are
//! spread, kept as a histogram whose buckets are 1 µs wide up to 100 µs and
//! 2% wide above, so a quantile is within 2% of the stretch it names.
use std::{collections::BTreeMap, time::Duration};

/// Stretches slower than this are counted apart, and logged.
pub const SLOW: Duration = Duration::from_millis(5);

/// Up to this many microseconds a bucket is one microsecond wide.
const FINE: u64 = 100;
/// Above [`FINE`], each bucket starts this much above the one before.
const STEP: f64 = 1.02;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Times {
    pub count: u64,
    pub total: Duration,
    pub longest: Duration,
    /// Over [`SLOW`].
    pub slow: u64,
    buckets: BTreeMap<u32, u64>,
}

fn bucket(micros: u64) -> u32 {
    if micros < FINE {
        micros as u32
    } else {
        FINE as u32 + ((micros as f64 / FINE as f64).ln() / STEP.ln()) as u32
    }
}

/// Where a bucket starts.
fn start(bucket: u32) -> Duration {
    let micros = if u64::from(bucket) < FINE { f64::from(bucket) } else { FINE as f64 * STEP.powi((bucket - FINE as u32) as i32) };
    Duration::from_nanos((micros * 1000.0) as u64)
}

impl Times {
    pub fn add(&mut self, elapsed: Duration) {
        self.count += 1;
        self.total += elapsed;
        self.longest = self.longest.max(elapsed);
        if elapsed > SLOW {
            self.slow += 1;
        }
        *self.buckets.entry(bucket(elapsed.as_micros() as u64)).or_default() += 1;
    }

    pub fn mean(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total / self.count as u32
        }
    }

    /// The stretch `q` of the way through them by time (0.5 the median), as
    /// the start of its bucket.
    pub fn quantile(&self, q: f64) -> Duration {
        let rank = ((q * self.count as f64).ceil() as u64).max(1);
        let mut seen = 0;
        for (&b, &n) in &self.buckets {
            seen += n;
            if seen >= rank {
                return start(b);
            }
        }
        Duration::ZERO
    }

    /// One line, as the report and the log write it.
    pub fn summary(&self) -> String {
        let ms = |d: Duration| format!("{:.3}", d.as_secs_f64() * 1000.0);
        format!(
            "{} stretch(es), {} ms in total; mean {} ms, median {} ms, p95 {} ms, p99 {} ms, longest {} ms; {} over {} ms",
            self.count,
            self.total.as_millis(),
            ms(self.mean()),
            ms(self.quantile(0.5)),
            ms(self.quantile(0.95)),
            ms(self.quantile(0.99)),
            ms(self.longest),
            self.slow,
            SLOW.as_millis()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_come_within_two_percent() {
        let mut t = Times::default();
        for micros in 1..=10_000u64 {
            t.add(Duration::from_micros(micros));
        }
        assert_eq!(t.count, 10_000);
        assert_eq!(t.longest, Duration::from_millis(10));
        assert_eq!(t.slow, 5_000);
        for (q, want) in [(0.5, 5_000.0), (0.95, 9_500.0), (0.99, 9_900.0)] {
            let got = t.quantile(q).as_secs_f64() * 1e6;
            assert!(got <= want && got >= want / STEP, "{q}: {got} for {want}");
        }
        assert_eq!(t.quantile(0.001), Duration::from_micros(10));
        assert_eq!(Times::default().quantile(0.5), Duration::ZERO);
    }
}
