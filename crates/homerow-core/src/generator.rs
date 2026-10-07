//! Practice text generation (blueprint 7.4).
//!
//! With a word list, only words made of the lesson's keys are used;
//! otherwise short letter groups are generated from the keys. In weak-key
//! mode each character gets a weight from its error rate, and both words and
//! letters are picked in proportion to it.

use std::collections::{BTreeSet, HashMap};

use crate::layout::Layout;
use crate::lesson::Lesson;
use crate::metrics::KeyStat;
use crate::rng::Rng;

/// Weakness per character, `0.0` (never missed) to `1.0`.
pub type KeyWeights = HashMap<char, f64>;

/// How much a fully weak key outweighs a key that is never missed.
const WEAK_BOOST: f64 = 12.0;

/// Weights from aggregated key statistics. Keys typed fewer than
/// `min_attempts` times are left out so the weights are not noise.
pub fn weights_from_stats(stats: &[KeyStat], min_attempts: u32) -> KeyWeights {
    stats
        .iter()
        .filter(|s| s.ch != ' ' && s.attempts() >= min_attempts)
        .map(|s| (s.ch, s.misses as f64 / s.attempts() as f64))
        .collect()
}

/// A lesson covering the keys and words of all given lessons, used for the
/// weak-key practice mode.
pub fn combined_lesson(layout_id: &str, lessons: &[&Lesson]) -> Lesson {
    let keys: BTreeSet<char> = lessons
        .iter()
        .flat_map(|l| l.keys.iter().copied())
        .collect();
    let words: BTreeSet<&String> = lessons.iter().flat_map(|l| &l.words).collect();
    Lesson {
        format: 1,
        id: format!("{layout_id}/zayif-tuslar"),
        layout: layout_id.to_owned(),
        title: "Zayıf tuş alıştırması".into(),
        keys: keys.into_iter().collect(),
        words: words.into_iter().cloned().collect(),
        target_wpm: 0.0,
        target_accuracy: 0.0,
        length: None,
    }
}

#[derive(Debug, Clone)]
pub struct LessonGenerator {
    rng: Rng,
}

impl LessonGenerator {
    pub fn new(seed: u64) -> LessonGenerator {
        LessonGenerator {
            rng: Rng::new(seed),
        }
    }

    pub fn from_time() -> LessonGenerator {
        LessonGenerator {
            rng: Rng::from_time(),
        }
    }

    /// Generates roughly `length` characters of words separated by single
    /// spaces, using only characters the lesson allows and the layout can
    /// type. Returns an empty string if no such character exists.
    pub fn generate(
        &mut self,
        lesson: &Lesson,
        layout: &Layout,
        weights: Option<&KeyWeights>,
        length: usize,
    ) -> String {
        let weight = |c: char| 1.0 + WEAK_BOOST * weights.and_then(|w| w.get(&c)).unwrap_or(&0.0);

        let words: Vec<&str> = lesson
            .usable_words()
            .filter(|w| w.chars().all(|c| c != ' ' && layout.contains(c)))
            .collect();
        let word_weights: Vec<f64> = words
            .iter()
            .map(|w| {
                let n = w.chars().count() as f64;
                // Mean weight, squared so weak words stand out.
                (w.chars().map(weight).sum::<f64>() / n).powi(2)
            })
            .collect();

        let mut letters: Vec<char> = lesson
            .keys
            .iter()
            .copied()
            .filter(|c| *c != ' ' && layout.contains(*c))
            .collect();
        letters.dedup();
        let letter_weights: Vec<f64> = letters.iter().map(|c| weight(*c)).collect();

        let mut out = String::new();
        let mut chars = 0;
        let length = length.max(1);
        while chars < length {
            let word = match self.rng.pick_weighted(&word_weights) {
                Some(i) => words[i].to_owned(),
                None => match self.letter_group(&letters, &letter_weights) {
                    Some(g) => g,
                    None => break,
                },
            };
            if !out.is_empty() {
                out.push(' ');
                chars += 1;
            }
            chars += word.chars().count();
            out.push_str(&word);
        }
        out
    }

    fn letter_group(&mut self, letters: &[char], weights: &[f64]) -> Option<String> {
        let len = self.rng.range(2, 5);
        let mut group = String::new();
        let mut prev = None;
        for _ in 0..len {
            let mut c = letters[self.rng.pick_weighted(weights)?];
            // Avoid runs of the same letter when there is a choice.
            if Some(c) == prev && letters.len() > 1 {
                c = letters[self.rng.pick_weighted(weights)?];
            }
            group.push(c);
            prev = Some(c);
        }
        Some(group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutDef, LayoutOptions};

    fn layout() -> Layout {
        let def = LayoutDef::from_toml(
            r#"
id = "t"
name = "T"
[[rows]]
keys = [
  { code = "KEY_A", base = "a", finger = "L4" },
  { code = "KEY_S", base = "s", finger = "L3" },
  { code = "KEY_D", base = "d", finger = "L2" },
  { code = "KEY_F", base = "f", finger = "L1" },
  { code = "KEY_J", base = "j", finger = "R1" },
  { code = "KEY_K", base = "k", finger = "R2" },
  { code = "KEY_L", base = "l", finger = "R3" },
]
[modifiers]
shift_left = "L4"
shift_right = "R4"
altgr = "R0"
space = "R0"
"#,
        )
        .unwrap();
        Layout::from_def(def, &LayoutOptions::default()).unwrap()
    }

    fn lesson(words: &[&str]) -> Lesson {
        Lesson {
            format: 1,
            id: "t/1".into(),
            layout: "t".into(),
            title: "T".into(),
            keys: "asdfjkl ".chars().collect(),
            words: words.iter().map(|w| w.to_string()).collect(),
            target_wpm: 10.0,
            target_accuracy: 0.9,
            length: None,
        }
    }

    fn count(text: &str, c: char) -> usize {
        text.chars().filter(|x| *x == c).count()
    }

    #[test]
    fn uses_only_allowed_words() {
        let l = lesson(&["sal", "dal", "kitap", "fal"]);
        let text = LessonGenerator::new(1).generate(&l, &layout(), None, 200);
        assert!(text.chars().count() >= 200);
        for w in text.split(' ') {
            assert!(["sal", "dal", "fal"].contains(&w), "{w}");
        }
    }

    #[test]
    fn letter_groups_without_words() {
        let l = lesson(&[]);
        let text = LessonGenerator::new(2).generate(&l, &layout(), None, 100);
        assert!(!text.contains("  ") && !text.starts_with(' ') && !text.ends_with(' '));
        assert!(text.chars().all(|c| l.allows(c)));
        for w in text.split(' ') {
            assert!((2..=5).contains(&w.chars().count()));
        }
    }

    #[test]
    fn deterministic_with_seed() {
        let l = lesson(&[]);
        let a = LessonGenerator::new(3).generate(&l, &layout(), None, 80);
        let b = LessonGenerator::new(3).generate(&l, &layout(), None, 80);
        assert_eq!(a, b);
    }

    #[test]
    fn weak_keys_appear_more_often() {
        let weights: KeyWeights = [('k', 0.6)].into_iter().collect();
        for words in [&[][..], &["sal", "kal", "dal", "fal", "kas", "las"][..]] {
            let l = lesson(words);
            let plain = LessonGenerator::new(4).generate(&l, &layout(), None, 3000);
            let weak = LessonGenerator::new(4).generate(&l, &layout(), Some(&weights), 3000);
            let (p, w) = (count(&plain, 'k'), count(&weak, 'k'));
            assert!(w > p * 2, "k: {p} -> {w} (words: {words:?})");
        }
    }

    #[test]
    fn weights_and_combined_lesson() {
        let stats = [
            KeyStat {
                ch: 'a',
                hits: 9,
                misses: 1,
                avg_ms: None,
            },
            KeyStat {
                ch: 'b',
                hits: 1,
                misses: 1,
                avg_ms: None,
            },
            KeyStat {
                ch: ' ',
                hits: 0,
                misses: 10,
                avg_ms: None,
            },
        ];
        let w = weights_from_stats(&stats, 5);
        assert_eq!(w.len(), 1);
        assert!((w[&'a'] - 0.1).abs() < 1e-9);

        let mut b = lesson(&["dal"]);
        b.keys.push('ş');
        let combined = combined_lesson("t", &[&lesson(&["sal"]), &b]);
        assert!(combined.allows('ş') && combined.allows(' '));
        assert_eq!(combined.words, ["dal", "sal"]);
        combined.validate().unwrap();
    }
}
