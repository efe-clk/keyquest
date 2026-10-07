//! One practice session: target text, cursor, typed characters, errors and
//! time (blueprint 9.2). Timestamps are supplied by the caller as a monotonic
//! `Duration` (e.g. `Instant::elapsed()`), which keeps tests deterministic.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::metrics::{Metrics, Summary};

/// What happens when a wrong character is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorMode {
    /// The cursor waits until the right character is typed.
    #[default]
    StopOnError,
    /// The cursor moves on; the error can be fixed with backspace.
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Ready,
    Typing,
    Paused,
    Finished,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharStatus {
    Pending,
    Correct,
    /// Holds the character that was typed instead.
    Wrong(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputResult {
    /// The session is not accepting input (paused, finished, cancelled).
    Ignored,
    Correct,
    Wrong {
        expected: char,
    },
}

#[derive(Debug, Clone)]
pub struct Session {
    target: Vec<char>,
    statuses: Vec<CharStatus>,
    cursor: usize,
    mode: ErrorMode,
    state: SessionState,
    metrics: Metrics,
    started_at: Option<Duration>,
    ended_at: Option<Duration>,
    paused_at: Option<Duration>,
    paused_total: Duration,
    last_key_at: Option<Duration>,
}

impl Session {
    pub fn new(text: &str, mode: ErrorMode) -> Session {
        let target: Vec<char> = text.chars().collect();
        Session {
            statuses: vec![CharStatus::Pending; target.len()],
            state: if target.is_empty() {
                SessionState::Finished
            } else {
                SessionState::Ready
            },
            target,
            cursor: 0,
            mode,
            metrics: Metrics::new(),
            started_at: None,
            ended_at: None,
            paused_at: None,
            paused_total: Duration::ZERO,
            last_key_at: None,
        }
    }

    pub fn input(&mut self, ch: char, now: Duration) -> InputResult {
        match self.state {
            SessionState::Ready => {
                self.state = SessionState::Typing;
                self.started_at = Some(now);
            }
            SessionState::Typing => {}
            _ => return InputResult::Ignored,
        }
        let expected = self.target[self.cursor];
        let latency = self.last_key_at.map(|t| now.saturating_sub(t));
        self.last_key_at = Some(now);

        let correct = ch == expected;
        self.metrics.record(expected, correct, latency);
        if correct {
            self.statuses[self.cursor] = CharStatus::Correct;
            self.cursor += 1;
        } else {
            self.statuses[self.cursor] = CharStatus::Wrong(ch);
            match self.mode {
                ErrorMode::Continue => self.cursor += 1,
                ErrorMode::StopOnError => self.metrics.reject_last(),
            }
        }
        if self.cursor == self.target.len() {
            self.state = SessionState::Finished;
            self.ended_at = Some(now);
        }
        if correct {
            InputResult::Correct
        } else {
            InputResult::Wrong { expected }
        }
    }

    /// Moves back one character (continue mode) or clears the error marker
    /// at the cursor (stop mode). Returns whether anything changed.
    pub fn backspace(&mut self) -> bool {
        if self.state != SessionState::Typing {
            return false;
        }
        match self.mode {
            ErrorMode::Continue if self.cursor > 0 => {
                self.cursor -= 1;
                self.statuses[self.cursor] = CharStatus::Pending;
                true
            }
            ErrorMode::StopOnError
                if matches!(self.statuses[self.cursor], CharStatus::Wrong(_)) =>
            {
                self.statuses[self.cursor] = CharStatus::Pending;
                true
            }
            _ => false,
        }
    }

    pub fn pause(&mut self, now: Duration) {
        if self.state == SessionState::Typing {
            self.state = SessionState::Paused;
            self.paused_at = Some(now);
        }
    }

    pub fn resume(&mut self, now: Duration) {
        if self.state == SessionState::Paused {
            let since = self.paused_at.take().unwrap_or(now);
            self.paused_total += now.saturating_sub(since);
            self.state = SessionState::Typing;
            // The pause would otherwise count as reach time for the next key.
            self.last_key_at = None;
        }
    }

    pub fn cancel(&mut self) {
        if self.state != SessionState::Finished {
            self.state = SessionState::Cancelled;
        }
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn is_finished(&self) -> bool {
        self.state == SessionState::Finished
    }

    pub fn mode(&self) -> ErrorMode {
        self.mode
    }

    pub fn target(&self) -> &[char] {
        &self.target
    }

    pub fn statuses(&self) -> &[CharStatus] {
        &self.statuses
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn next_char(&self) -> Option<char> {
        match self.state {
            SessionState::Finished | SessionState::Cancelled => None,
            _ => self.target.get(self.cursor).copied(),
        }
    }

    /// Active typing time: from the first keystroke, without paused time.
    pub fn elapsed(&self, now: Duration) -> Duration {
        let Some(start) = self.started_at else {
            return Duration::ZERO;
        };
        let end = self.ended_at.or(self.paused_at).unwrap_or(now);
        end.saturating_sub(start).saturating_sub(self.paused_total)
    }

    fn uncorrected_errors(&self) -> u32 {
        self.statuses
            .iter()
            .filter(|s| matches!(s, CharStatus::Wrong(_)))
            .count() as u32
    }

    /// Live figures for display while typing.
    pub fn summary_at(&self, now: Duration) -> Summary {
        self.metrics
            .summary(self.elapsed(now), self.uncorrected_errors())
    }

    /// Final figures, once the text is complete.
    pub fn result(&self) -> Option<Summary> {
        let end = self.ended_at?;
        Some(self.summary_at(end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn stop_on_error_waits_for_correct_key() {
        let mut s = Session::new("ab", ErrorMode::StopOnError);
        assert_eq!(s.state(), SessionState::Ready);
        assert_eq!(s.input('a', ms(1000)), InputResult::Correct);
        assert_eq!(s.state(), SessionState::Typing);
        assert_eq!(s.input('x', ms(1200)), InputResult::Wrong { expected: 'b' });
        assert_eq!(s.cursor(), 1);
        assert_eq!(s.statuses()[1], CharStatus::Wrong('x'));
        assert_eq!(s.next_char(), Some('b'));
        assert_eq!(s.input('b', ms(1300)), InputResult::Correct);
        assert!(s.is_finished());
        assert_eq!(s.next_char(), None);

        let r = s.result().unwrap();
        assert_eq!(r.duration, ms(300));
        assert_eq!(r.chars_total, 3);
        // Two characters entered in 0.3 s; the rejected key adds no speed.
        assert!((r.wpm_gross - (2.0 / 5.0) / (0.3 / 60.0)).abs() < 1e-6);
        assert_eq!(r.errors, 1);
        assert_eq!(r.uncorrected_errors, 0);
        assert!((r.accuracy - 2.0 / 3.0).abs() < 1e-9);
        let b = r.keys.iter().find(|k| k.ch == 'b').unwrap();
        assert_eq!((b.hits, b.misses, b.avg_ms), (1, 1, Some(150)));
    }

    #[test]
    fn continue_mode_and_backspace() {
        let mut s = Session::new("abc", ErrorMode::Continue);
        s.input('a', ms(0));
        s.input('x', ms(100));
        assert_eq!(s.cursor(), 2);
        assert!(s.backspace());
        assert_eq!(s.cursor(), 1);
        assert_eq!(s.statuses()[1], CharStatus::Pending);
        s.input('b', ms(200));
        s.input('y', ms(300));
        assert!(s.is_finished());
        let r = s.result().unwrap();
        assert_eq!(r.errors, 2);
        assert_eq!(r.uncorrected_errors, 1);
        assert!(!s.backspace(), "finished session ignores backspace");
    }

    #[test]
    fn paused_time_is_not_counted() {
        let mut s = Session::new("abc", ErrorMode::StopOnError);
        s.input('a', ms(0));
        s.pause(ms(1000));
        assert_eq!(s.input('b', ms(1500)), InputResult::Ignored);
        assert_eq!(s.elapsed(ms(9000)), ms(1000));
        s.resume(ms(5000));
        s.input('b', ms(5500));
        s.input('c', ms(6000));
        let r = s.result().unwrap();
        assert_eq!(r.duration, ms(2000));
        // The keystroke right after the pause has no reach time.
        let b = r.keys.iter().find(|k| k.ch == 'b').unwrap();
        assert_eq!(b.avg_ms, None);
    }

    #[test]
    fn unicode_and_cancel() {
        let mut s = Session::new("şİ", ErrorMode::StopOnError);
        assert_eq!(s.input('ş', ms(0)), InputResult::Correct);
        s.cancel();
        assert_eq!(s.state(), SessionState::Cancelled);
        assert_eq!(s.input('İ', ms(10)), InputResult::Ignored);
        assert!(s.result().is_none());
    }

    #[test]
    fn empty_text_is_finished() {
        let s = Session::new("", ErrorMode::Continue);
        assert!(s.is_finished());
        assert_eq!(s.next_char(), None);
    }
}
