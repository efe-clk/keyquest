//! Progress storage contract (blueprint 10) and an in-memory implementation
//! for tests. The SQLite implementation lives in `keyquest-data`.

use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::finger::Finger;
use crate::layout::Layout;
use crate::metrics::{KeyStat, Summary};

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct StoreError(#[from] Box<dyn std::error::Error + Send + Sync>);

impl StoreError {
    pub fn new(e: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> StoreError {
        StoreError(e.into())
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

/// A finished session as it is saved.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionResult {
    pub lesson_id: String,
    pub layout: String,
    /// Unix time in seconds.
    pub started_at: i64,
    /// Whether the lesson's targets were met.
    pub completed: bool,
    pub summary: Summary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LessonProgress {
    pub lesson_id: String,
    pub best_wpm: f64,
    pub best_acc: f64,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub id: i64,
    pub lesson_id: String,
    pub started_at: i64,
    pub duration_ms: u64,
    pub chars_total: u32,
    pub errors: u32,
    pub wpm_net: f64,
    pub accuracy: f64,
}

pub trait ProgressStore {
    /// Saves the session, its key statistics and the lesson's best scores
    /// atomically.
    fn save_session(&mut self, s: &SessionResult) -> StoreResult<()>;

    /// Key statistics summed over all sessions on a layout, sorted by
    /// character.
    fn key_stats(&self, layout: &str) -> StoreResult<Vec<KeyStat>>;

    fn lesson_progress(&self, lesson_id: &str) -> StoreResult<Option<LessonProgress>>;

    /// Sessions of the last `days` days on a layout, oldest first.
    fn history(&self, layout: &str, days: u32) -> StoreResult<Vec<SessionSummary>>;

    /// Deletes all progress.
    fn reset(&mut self) -> StoreResult<()>;

    /// Keys with the highest (smoothed) error rate, ignoring keys typed
    /// fewer than five times.
    fn weakest_keys(&self, layout: &str, limit: usize) -> StoreResult<Vec<KeyStat>> {
        Ok(rank_weakest(self.key_stats(layout)?, limit))
    }
}

pub(crate) const MIN_ATTEMPTS: u32 = 5;

pub fn rank_weakest(mut stats: Vec<KeyStat>, limit: usize) -> Vec<KeyStat> {
    stats.retain(|s| s.attempts() >= MIN_ATTEMPTS && s.misses > 0);
    stats.sort_by(|a, b| {
        b.weakness()
            .total_cmp(&a.weakness())
            .then(b.misses.cmp(&a.misses))
            .then(a.ch.cmp(&b.ch))
    });
    stats.truncate(limit);
    stats
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct FingerStat {
    pub finger: Finger,
    pub hits: u32,
    pub misses: u32,
}

impl FingerStat {
    pub fn error_rate(&self) -> f64 {
        let n = self.hits + self.misses;
        if n == 0 {
            0.0
        } else {
            self.misses as f64 / n as f64
        }
    }
}

/// Finger statistics derived from key statistics at run time (they are not
/// stored). Characters the layout cannot type are skipped. Sorted by error
/// rate, worst first.
pub fn finger_stats(layout: &Layout, keys: &[KeyStat]) -> Vec<FingerStat> {
    let mut by_finger: HashMap<Finger, FingerStat> = HashMap::new();
    for k in keys {
        let Some(stroke) = layout.stroke_for(k.ch) else {
            continue;
        };
        let e = by_finger.entry(stroke.finger).or_insert(FingerStat {
            finger: stroke.finger,
            hits: 0,
            misses: 0,
        });
        e.hits += k.hits;
        e.misses += k.misses;
    }
    let mut v: Vec<_> = by_finger.into_values().collect();
    v.sort_by(|a, b| {
        b.error_rate()
            .total_cmp(&a.error_rate())
            .then(a.finger.cmp(&b.finger))
    });
    v
}

/// Keeps everything in memory; for tests and for running without a disk.
#[derive(Debug, Default)]
pub struct MemoryProgressStore {
    sessions: Vec<(SessionSummary, String, Vec<KeyStat>)>,
    lessons: BTreeMap<String, LessonProgress>,
}

impl MemoryProgressStore {
    pub fn new() -> MemoryProgressStore {
        MemoryProgressStore::default()
    }
}

impl ProgressStore for MemoryProgressStore {
    fn save_session(&mut self, s: &SessionResult) -> StoreResult<()> {
        let summary = SessionSummary {
            id: self.sessions.len() as i64 + 1,
            lesson_id: s.lesson_id.clone(),
            started_at: s.started_at,
            duration_ms: s.summary.duration.as_millis() as u64,
            chars_total: s.summary.chars_total,
            errors: s.summary.errors,
            wpm_net: s.summary.wpm_net,
            accuracy: s.summary.accuracy,
        };
        self.sessions
            .push((summary, s.layout.clone(), s.summary.keys.clone()));
        let p = self
            .lessons
            .entry(s.lesson_id.clone())
            .or_insert(LessonProgress {
                lesson_id: s.lesson_id.clone(),
                best_wpm: 0.0,
                best_acc: 0.0,
                completed: false,
            });
        p.best_wpm = p.best_wpm.max(s.summary.wpm_net);
        p.best_acc = p.best_acc.max(s.summary.accuracy);
        p.completed |= s.completed;
        Ok(())
    }

    fn key_stats(&self, layout: &str) -> StoreResult<Vec<KeyStat>> {
        // (hits, misses, latency sum, latency weight)
        let mut acc: BTreeMap<char, (u32, u32, u64, u64)> = BTreeMap::new();
        for (_, l, keys) in &self.sessions {
            if l != layout {
                continue;
            }
            for k in keys {
                let e = acc.entry(k.ch).or_default();
                e.0 += k.hits;
                e.1 += k.misses;
                if let Some(ms) = k.avg_ms {
                    e.2 += ms as u64 * k.attempts() as u64;
                    e.3 += k.attempts() as u64;
                }
            }
        }
        Ok(acc
            .into_iter()
            .map(|(ch, (hits, misses, sum, n))| KeyStat {
                ch,
                hits,
                misses,
                avg_ms: (n > 0).then(|| (sum as f64 / n as f64).round() as u32),
            })
            .collect())
    }

    fn lesson_progress(&self, lesson_id: &str) -> StoreResult<Option<LessonProgress>> {
        Ok(self.lessons.get(lesson_id).cloned())
    }

    fn history(&self, layout: &str, days: u32) -> StoreResult<Vec<SessionSummary>> {
        let since = now_unix() - days as i64 * 86_400;
        let mut v: Vec<_> = self
            .sessions
            .iter()
            .filter(|(s, l, _)| l == layout && s.started_at >= since)
            .map(|(s, _, _)| s.clone())
            .collect();
        v.sort_by_key(|s| (s.started_at, s.id));
        Ok(v)
    }

    fn reset(&mut self) -> StoreResult<()> {
        *self = MemoryProgressStore::default();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutDef, LayoutOptions};
    use crate::metrics::Metrics;
    use std::time::Duration;

    pub(crate) fn result(
        lesson: &str,
        layout: &str,
        completed: bool,
        keys: &[(char, bool)],
    ) -> SessionResult {
        let mut m = Metrics::new();
        for (c, ok) in keys {
            m.record(*c, *ok, Some(Duration::from_millis(200)));
        }
        SessionResult {
            lesson_id: lesson.into(),
            layout: layout.into(),
            started_at: now_unix(),
            completed,
            summary: m.summary(Duration::from_secs(30), 0),
        }
    }

    #[test]
    fn memory_store_round_trip() {
        let mut st = MemoryProgressStore::new();
        let mut keys = vec![('a', true); 10];
        keys.extend([
            ('k', false),
            ('k', false),
            ('k', true),
            ('k', true),
            ('k', true),
        ]);
        st.save_session(&result("l1", "tr-q", false, &keys))
            .unwrap();
        st.save_session(&result("l1", "tr-q", true, &[('a', false)]))
            .unwrap();
        st.save_session(&result("l1", "us", true, &[('z', false); 9]))
            .unwrap();

        let p = st.lesson_progress("l1").unwrap().unwrap();
        assert!(p.completed);
        let weak = st.weakest_keys("tr-q", 10).unwrap();
        assert_eq!(weak.iter().map(|k| k.ch).collect::<String>(), "ka");
        assert_eq!(weak[1].attempts(), 11);
        assert_eq!(st.history("tr-q", 7).unwrap().len(), 2);
        st.reset().unwrap();
        assert!(st.lesson_progress("l1").unwrap().is_none());
    }

    #[test]
    fn finger_stats_from_keys() {
        let def = LayoutDef::from_toml(
            r#"
id = "t"
name = "T"
[[rows]]
keys = [
  { code = "KEY_A", base = "a", shift = "A", finger = "L4" },
  { code = "KEY_J", base = "j", finger = "R1" },
]
[modifiers]
shift_left = "L4"
shift_right = "R4"
altgr = "R0"
space = "R0"
"#,
        )
        .unwrap();
        let layout = Layout::from_def(def, &LayoutOptions::default()).unwrap();
        let stat = |ch, hits, misses| KeyStat {
            ch,
            hits,
            misses,
            avg_ms: None,
        };
        let fs = finger_stats(
            &layout,
            &[
                stat('a', 5, 1),
                stat('A', 2, 2),
                stat('j', 10, 0),
                stat('?', 0, 9),
            ],
        );
        assert_eq!(fs.len(), 2);
        assert_eq!(fs[0].finger.to_string(), "L4");
        assert_eq!((fs[0].hits, fs[0].misses), (7, 3));
    }
}

/// A lesson together with whether it is open and its saved progress.
#[derive(Debug, Clone)]
pub struct LessonStatus<'a> {
    pub lesson: &'a crate::lesson::Lesson,
    pub unlocked: bool,
    pub progress: Option<LessonProgress>,
}

impl LessonStatus<'_> {
    pub fn completed(&self) -> bool {
        self.progress.as_ref().is_some_and(|p| p.completed)
    }
}

/// Status of each lesson, in the given order (see [`crate::lesson::unlocked`]).
pub fn lesson_statuses<'a>(
    lessons: &[&'a crate::lesson::Lesson],
    store: &dyn ProgressStore,
) -> StoreResult<Vec<LessonStatus<'a>>> {
    let progress = lessons
        .iter()
        .map(|l| store.lesson_progress(&l.id))
        .collect::<StoreResult<Vec<_>>>()?;
    let unlocked = crate::lesson::unlocked(lessons, |id| {
        lessons
            .iter()
            .zip(&progress)
            .any(|(l, p)| l.id == id && p.as_ref().is_some_and(|p| p.completed))
    });
    Ok(lessons
        .iter()
        .zip(progress)
        .zip(unlocked)
        .map(|((lesson, progress), unlocked)| LessonStatus {
            lesson,
            unlocked,
            progress,
        })
        .collect())
}

/// The first open lesson not yet completed; the last open one if all are.
pub fn next_lesson<'a>(statuses: &[LessonStatus<'a>]) -> Option<&'a crate::lesson::Lesson> {
    let open = || statuses.iter().filter(|s| s.unlocked);
    open()
        .find(|s| !s.completed())
        .or_else(|| open().next_back())
        .map(|s| s.lesson)
}

/// Lessons that are open, for building the weak-key practice.
pub fn open_lessons<'a>(statuses: &[LessonStatus<'a>]) -> Vec<&'a crate::lesson::Lesson> {
    statuses
        .iter()
        .filter(|s| s.unlocked)
        .map(|s| s.lesson)
        .collect()
}

#[cfg(test)]
mod status_tests {
    use super::*;
    use crate::lesson::Lesson;

    fn lesson(id: &str) -> Lesson {
        Lesson {
            format: 1,
            id: id.into(),
            layout: "t".into(),
            title: id.into(),
            keys: vec!['a', ' '],
            words: vec![],
            target_wpm: 10.0,
            target_accuracy: 0.9,
            length: None,
        }
    }

    #[test]
    fn statuses_and_next() {
        let (a, b, c) = (lesson("a"), lesson("b"), lesson("c"));
        let all = [&a, &b, &c];
        let mut store = MemoryProgressStore::new();
        let st = lesson_statuses(&all, &store).unwrap();
        assert_eq!(next_lesson(&st).unwrap().id, "a");
        assert_eq!(open_lessons(&st).len(), 1);

        let summary = crate::metrics::Metrics::new().summary(std::time::Duration::ZERO, 0);
        let done = |id: &str| SessionResult {
            lesson_id: id.into(),
            layout: "t".into(),
            started_at: 0,
            completed: true,
            summary: summary.clone(),
        };
        store.save_session(&done("a")).unwrap();
        let st = lesson_statuses(&all, &store).unwrap();
        assert!(st[0].completed() && st[1].unlocked && !st[2].unlocked);
        assert_eq!(next_lesson(&st).unwrap().id, "b");
        store.save_session(&done("b")).unwrap();
        store.save_session(&done("c")).unwrap();
        let st = lesson_statuses(&all, &store).unwrap();
        assert_eq!(next_lesson(&st).unwrap().id, "c");
    }
}
