//! Lesson definitions (TOML, see `docs/blueprint.md`, 8.1) and unlocking.

use serde::{Deserialize, Serialize};

use crate::layout::{FORMAT_VERSION, Layout, default_format};
use crate::metrics::Summary;

#[derive(Debug, thiserror::Error)]
pub enum LessonError {
    #[error("TOML okunamadı: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("desteklenmeyen format sürümü {0} (en fazla {FORMAT_VERSION})")]
    UnsupportedFormat(u32),
    #[error("geçersiz ders: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    #[serde(default = "default_format")]
    pub format: u32,
    pub id: String,
    pub layout: String,
    pub title: String,
    /// Characters allowed in this lesson, including space.
    pub keys: Vec<char>,
    /// Optional word list; only words made of `keys` are used.
    #[serde(default)]
    pub words: Vec<String>,
    pub target_wpm: f64,
    pub target_accuracy: f64,
    /// Length of the generated practice text, in characters.
    #[serde(default)]
    pub length: Option<usize>,
}

pub const DEFAULT_LENGTH: usize = 120;

/// Ids are lowercase ASCII words separated by `-`, `_` or `/`.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'/')
        })
        && !id.starts_with('/')
        && !id.ends_with('/')
}

impl Lesson {
    pub fn from_toml(s: &str) -> Result<Lesson, LessonError> {
        let lesson: Lesson = toml::from_str(s)?;
        lesson.validate()?;
        Ok(lesson)
    }

    pub fn validate(&self) -> Result<(), LessonError> {
        let invalid = |m: String| Err(LessonError::Invalid(m));
        if self.format == 0 || self.format > FORMAT_VERSION {
            return Err(LessonError::UnsupportedFormat(self.format));
        }
        if !is_valid_id(&self.id) {
            return invalid(format!("geçersiz id {:?}", self.id));
        }
        if !is_valid_id(&self.layout) {
            return invalid(format!("geçersiz düzen id {:?}", self.layout));
        }
        if self.title.trim().is_empty() {
            return invalid("başlık boş".into());
        }
        if !self.keys.iter().any(|c| *c != ' ') {
            return invalid("tuş listesi boş".into());
        }
        if let Some(c) = self.keys.iter().find(|c| c.is_control()) {
            return invalid(format!("yazdırılamayan karakter {c:?}"));
        }
        if !(0.0..=300.0).contains(&self.target_wpm) {
            return invalid(format!("target_wpm aralık dışında: {}", self.target_wpm));
        }
        if !(0.0..=1.0).contains(&self.target_accuracy) {
            return invalid(format!(
                "target_accuracy 0 ile 1 arasında olmalı: {}",
                self.target_accuracy
            ));
        }
        if self.length == Some(0) {
            return invalid("length 0 olamaz".into());
        }
        Ok(())
    }

    pub fn length(&self) -> usize {
        self.length.unwrap_or(DEFAULT_LENGTH)
    }

    pub fn allows(&self, ch: char) -> bool {
        self.keys.contains(&ch)
    }

    /// Words that use only this lesson's keys.
    pub fn usable_words(&self) -> impl Iterator<Item = &str> {
        self.words
            .iter()
            .map(String::as_str)
            .filter(|w| !w.is_empty() && w.chars().all(|c| self.allows(c)))
    }

    /// Whether a finished session meets the lesson's targets.
    pub fn passed(&self, summary: &Summary) -> bool {
        summary.wpm_net >= self.target_wpm && summary.accuracy >= self.target_accuracy
    }

    /// Problems found when checking this lesson against a layout: keys the
    /// layout cannot type, and words that use characters outside `keys`.
    pub fn check_against(&self, layout: &Layout) -> Vec<String> {
        let mut problems = Vec::new();
        if self.layout != layout.id() {
            problems.push(format!(
                "ders {:?} düzeni için, {:?} verildi",
                self.layout,
                layout.id()
            ));
        }
        for c in &self.keys {
            if !layout.contains(*c) {
                problems.push(format!("{c:?} düzende yok"));
            }
        }
        for w in &self.words {
            if let Some(c) = w.chars().find(|c| !self.allows(*c)) {
                problems.push(format!("{w:?} kelimesindeki {c:?} dersin tuşlarında yok"));
            }
        }
        problems
    }
}

/// Lessons unlock in order: the first is always open, every later one once
/// the previous lesson has been completed.
pub fn unlocked(lessons: &[&Lesson], is_completed: impl Fn(&str) -> bool) -> Vec<bool> {
    let mut open = true;
    lessons
        .iter()
        .map(|l| {
            let this = open;
            open = this && is_completed(&l.id);
            this
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const SAMPLE: &str = r#"
id = "tr-q/01-ana-sira"
layout = "tr-q"
title = "Ana sıra: a s d f j k l ş"
keys = ["a", "s", "d", "f", "j", "k", "l", "ş", " "]
words = ["aşk", "sal", "kas", "lal", "dal", "kitap"]
target_wpm = 15
target_accuracy = 0.95
"#;

    #[test]
    fn parses_blueprint_example() {
        let l = Lesson::from_toml(SAMPLE).unwrap();
        assert_eq!(l.keys.len(), 9);
        assert!(l.allows('ş'));
        assert_eq!(l.length(), DEFAULT_LENGTH);
        let words: Vec<_> = l.usable_words().collect();
        assert_eq!(words, ["aşk", "sal", "kas", "lal", "dal"]);
    }

    #[test]
    fn validation() {
        assert!(Lesson::from_toml(&SAMPLE.replace("0.95", "1.5")).is_err());
        assert!(Lesson::from_toml(&SAMPLE.replace("tr-q/01", "TR/01")).is_err());
        assert!(Lesson::from_toml(&format!("extra = 1\n{SAMPLE}")).is_err());
    }

    #[test]
    fn passing_and_unlocking() {
        let l = Lesson::from_toml(SAMPLE).unwrap();
        let mut s = crate::metrics::Metrics::new().summary(Duration::from_secs(60), 0);
        s.wpm_net = 16.0;
        s.accuracy = 0.96;
        assert!(l.passed(&s));
        s.accuracy = 0.9;
        assert!(!l.passed(&s));

        let mut b = l.clone();
        b.id = "b".into();
        let mut c = l.clone();
        c.id = "c".into();
        let all = [&l, &b, &c];
        assert_eq!(unlocked(&all, |_| false), [true, false, false]);
        assert_eq!(unlocked(&all, |id| id == l.id), [true, true, false]);
        assert_eq!(unlocked(&all, |id| id == "b"), [true, false, false]);
    }
}
