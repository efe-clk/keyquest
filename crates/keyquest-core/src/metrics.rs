//! Speed and accuracy measurement.
//!
//! - gross WPM = (characters entered / 5) / minutes, where a keystroke the
//!   session rejected (a wrong key in stop-on-error mode) enters nothing
//! - net WPM   = gross WPM − uncorrected errors / minutes (never below 0)
//! - accuracy  = correct keystrokes / all keystrokes

use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default)]
struct KeyAcc {
    hits: u32,
    misses: u32,
    latency_ms_sum: u64,
    latency_n: u32,
}

/// Per-character totals. `avg_ms` is the average time to reach the key
/// from the previous keystroke, when known.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyStat {
    pub ch: char,
    pub hits: u32,
    pub misses: u32,
    pub avg_ms: Option<u32>,
}

impl KeyStat {
    pub fn attempts(&self) -> u32 {
        self.hits + self.misses
    }

    /// Error rate with add-one smoothing, so a single miss on a rarely typed
    /// key does not dominate the ranking.
    pub fn weakness(&self) -> f64 {
        (self.misses as f64 + 1.0) / (self.attempts() as f64 + 2.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub duration: Duration,
    /// Keystrokes that produced a character (backspace excluded).
    pub chars_total: u32,
    /// Wrong keystrokes, whether corrected later or not.
    pub errors: u32,
    pub uncorrected_errors: u32,
    pub wpm_gross: f64,
    pub wpm_net: f64,
    pub accuracy: f64,
    /// Sorted by character.
    pub keys: Vec<KeyStat>,
}

impl Summary {
    /// Keys with at least one miss, most missed first.
    pub fn missed_keys(&self) -> Vec<&KeyStat> {
        let mut v: Vec<_> = self.keys.iter().filter(|k| k.misses > 0).collect();
        v.sort_by(|a, b| b.misses.cmp(&a.misses).then(a.ch.cmp(&b.ch)));
        v
    }
}

#[derive(Debug, Clone, Default)]
pub struct Metrics {
    keys: BTreeMap<char, KeyAcc>,
    keystrokes: u32,
    rejected: u32,
    correct: u32,
}

impl Metrics {
    pub fn new() -> Metrics {
        Metrics::default()
    }

    /// Records one keystroke against the character that was expected.
    pub fn record(&mut self, expected: char, correct: bool, latency: Option<Duration>) {
        let acc = self.keys.entry(expected).or_default();
        self.keystrokes += 1;
        if correct {
            self.correct += 1;
            acc.hits += 1;
        } else {
            acc.misses += 1;
        }
        if let Some(l) = latency {
            acc.latency_ms_sum += l.as_millis() as u64;
            acc.latency_n += 1;
        }
    }

    /// Marks the last keystroke as not entered into the text, so it does not
    /// count towards speed (it still counts against accuracy).
    pub fn reject_last(&mut self) {
        self.rejected = (self.rejected + 1).min(self.keystrokes);
    }

    pub fn keystrokes(&self) -> u32 {
        self.keystrokes
    }

    pub fn summary(&self, elapsed: Duration, uncorrected_errors: u32) -> Summary {
        let minutes = elapsed.as_secs_f64() / 60.0;
        let (gross, net) = if minutes > 0.0 {
            let entered = self.keystrokes - self.rejected;
            let gross = (entered as f64 / 5.0) / minutes;
            (
                gross,
                (gross - uncorrected_errors as f64 / minutes).max(0.0),
            )
        } else {
            (0.0, 0.0)
        };
        let accuracy = if self.keystrokes > 0 {
            self.correct as f64 / self.keystrokes as f64
        } else {
            1.0
        };
        let keys = self
            .keys
            .iter()
            .map(|(&ch, a)| KeyStat {
                ch,
                hits: a.hits,
                misses: a.misses,
                avg_ms: (a.latency_n > 0).then(|| (a.latency_ms_sum / a.latency_n as u64) as u32),
            })
            .collect();
        Summary {
            duration: elapsed,
            chars_total: self.keystrokes,
            errors: self.keystrokes - self.correct,
            uncorrected_errors,
            wpm_gross: gross,
            wpm_net: net,
            accuracy,
            keys,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpm_and_accuracy() {
        let mut m = Metrics::new();
        // 50 keystrokes in one minute, 5 of them wrong and 2 left uncorrected.
        for i in 0..50 {
            m.record('a', i % 10 != 0, Some(Duration::from_millis(100)));
        }
        let s = m.summary(Duration::from_secs(60), 2);
        assert_eq!(s.chars_total, 50);
        assert_eq!(s.errors, 5);
        assert!((s.wpm_gross - 10.0).abs() < 1e-9);
        assert!((s.wpm_net - 8.0).abs() < 1e-9);
        assert!((s.accuracy - 0.9).abs() < 1e-9);
        assert_eq!(
            s.keys,
            vec![KeyStat {
                ch: 'a',
                hits: 45,
                misses: 5,
                avg_ms: Some(100)
            }]
        );
    }

    #[test]
    fn empty_and_zero_duration() {
        let s = Metrics::new().summary(Duration::ZERO, 0);
        assert_eq!(s.wpm_gross, 0.0);
        assert_eq!(s.wpm_net, 0.0);
        assert_eq!(s.accuracy, 1.0);
    }

    #[test]
    fn rejected_keystrokes_do_not_count_as_speed() {
        let mut m = Metrics::new();
        for _ in 0..50 {
            m.record('a', true, None);
        }
        for _ in 0..25 {
            m.record('b', false, None);
            m.reject_last();
        }
        let s = m.summary(Duration::from_secs(60), 0);
        assert!((s.wpm_gross - 10.0).abs() < 1e-9);
        assert_eq!(s.chars_total, 75);
        assert!((s.accuracy - 50.0 / 75.0).abs() < 1e-9);
    }

    #[test]
    fn net_wpm_never_negative() {
        let mut m = Metrics::new();
        m.record('a', false, None);
        let s = m.summary(Duration::from_secs(1), 100);
        assert_eq!(s.wpm_net, 0.0);
    }
}
