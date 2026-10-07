//! Keyboard layouts and the "which key, which finger, which modifier"
//! resolution for a character (KR-6).

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::finger::{Digit, Finger, Hand};

pub const SHIFT_LEFT: &str = "KEY_LEFTSHIFT";
pub const SHIFT_RIGHT: &str = "KEY_RIGHTSHIFT";
pub const ALTGR: &str = "KEY_RIGHTALT";
pub const SPACE: &str = "KEY_SPACE";

/// Highest data file format version this build understands.
pub const FORMAT_VERSION: u32 = 1;

/// Physical key identifier, using Linux evdev names (`KEY_A`, `KEY_SEMICOLON`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyId(pub String);

impl KeyId {
    pub fn new(s: impl Into<String>) -> KeyId {
        KeyId(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("TOML okunamadı: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("desteklenmeyen format sürümü {0} (en fazla {FORMAT_VERSION})")]
    UnsupportedFormat(u32),
    #[error("geçersiz düzen: {0}")]
    Invalid(String),
}

pub(crate) fn default_format() -> u32 {
    1
}

/// Layout file as written in TOML (see `docs/blueprint.md`, 8.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutDef {
    #[serde(default = "default_format")]
    pub format: u32,
    pub id: String,
    pub name: String,
    pub rows: Vec<RowDef>,
    pub modifiers: ModifierDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowDef {
    pub keys: Vec<KeyDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyDef {
    pub code: String,
    pub base: Option<char>,
    pub shift: Option<char>,
    pub altgr: Option<char>,
    pub finger: Finger,
    #[serde(default)]
    pub home: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierDef {
    pub shift_left: Finger,
    pub shift_right: Finger,
    pub altgr: Finger,
    pub space: Finger,
}

impl LayoutDef {
    pub fn from_toml(s: &str) -> Result<LayoutDef, LayoutError> {
        let def: LayoutDef = toml::from_str(s)?;
        def.validate()?;
        Ok(def)
    }

    pub fn validate(&self) -> Result<(), LayoutError> {
        if self.format == 0 || self.format > FORMAT_VERSION {
            return Err(LayoutError::UnsupportedFormat(self.format));
        }
        if !crate::lesson::is_valid_id(&self.id) {
            return Err(LayoutError::Invalid(format!("geçersiz id {:?}", self.id)));
        }
        if self.name.trim().is_empty() {
            return Err(LayoutError::Invalid("ad boş".into()));
        }
        let mut codes = HashSet::new();
        let mut any = false;
        for key in self.rows.iter().flat_map(|r| &r.keys) {
            any = true;
            if !key.code.starts_with("KEY_") {
                return Err(LayoutError::Invalid(format!(
                    "tuş kodu KEY_ ile başlamalı: {:?}",
                    key.code
                )));
            }
            if !codes.insert(key.code.as_str()) {
                return Err(LayoutError::Invalid(format!(
                    "tekrarlanan tuş: {}",
                    key.code
                )));
            }
            for ch in [key.base, key.shift, key.altgr].into_iter().flatten() {
                if ch.is_control() || ch == ' ' {
                    return Err(LayoutError::Invalid(format!(
                        "{}: yazdırılamayan karakter {ch:?}",
                        key.code
                    )));
                }
            }
        }
        if !any {
            return Err(LayoutError::Invalid("hiç tuş yok".into()));
        }
        Ok(())
    }
}

/// A key on the virtual keyboard.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub id: KeyId,
    pub base: Option<char>,
    pub shift: Option<char>,
    pub altgr: Option<char>,
    pub finger: Finger,
    pub home: bool,
}

/// How to type one character: the key, the finger that presses it and,
/// if needed, the modifier key and the finger holding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: KeyId,
    pub finger: Finger,
    pub modifier: Option<(KeyId, Finger)>,
}

#[derive(Debug, Clone, Default)]
pub struct LayoutOptions {
    /// Which thumb presses the space bar; `None` keeps the layout default.
    pub space_thumb: Option<Hand>,
}

#[derive(Debug, Clone)]
pub struct Layout {
    id: String,
    name: String,
    rows: Vec<Vec<Key>>,
    strokes: HashMap<char, KeyStroke>,
    fingers: HashMap<KeyId, Finger>,
}

impl Layout {
    /// Builds the layout and its `char -> KeyStroke` table once.
    ///
    /// Rules: shifted characters use the Shift key of the opposite hand
    /// (left-hand key → right Shift, right-hand key → left Shift); AltGr
    /// characters use the AltGr finger (right thumb by default); space uses
    /// the configured thumb. When a character appears on several keys, the
    /// unmodified one wins.
    pub fn from_def(def: LayoutDef, opts: &LayoutOptions) -> Result<Layout, LayoutError> {
        def.validate()?;
        let mods = def.modifiers;
        let space_finger = match opts.space_thumb {
            Some(hand) => Finger::new(hand, Digit::Thumb),
            None => mods.space,
        };

        let rows: Vec<Vec<Key>> = def
            .rows
            .into_iter()
            .map(|row| {
                row.keys
                    .into_iter()
                    .map(|k| Key {
                        id: KeyId(k.code),
                        base: k.base,
                        shift: k.shift,
                        altgr: k.altgr,
                        finger: k.finger,
                        home: k.home,
                    })
                    .collect()
            })
            .collect();

        let mut fingers: HashMap<KeyId, Finger> = rows
            .iter()
            .flatten()
            .map(|k| (k.id.clone(), k.finger))
            .collect();
        fingers.insert(KeyId::new(SHIFT_LEFT), mods.shift_left);
        fingers.insert(KeyId::new(SHIFT_RIGHT), mods.shift_right);
        fingers.insert(KeyId::new(ALTGR), mods.altgr);
        fingers.insert(KeyId::new(SPACE), space_finger);

        let mut strokes = HashMap::new();
        strokes.insert(
            ' ',
            KeyStroke {
                key: KeyId::new(SPACE),
                finger: space_finger,
                modifier: None,
            },
        );
        let keys = || rows.iter().flatten();
        for key in keys() {
            if let Some(ch) = key.base {
                strokes.entry(ch).or_insert_with(|| KeyStroke {
                    key: key.id.clone(),
                    finger: key.finger,
                    modifier: None,
                });
            }
        }
        for key in keys() {
            if let Some(ch) = key.shift {
                let shift = match key.finger.hand {
                    Hand::Left => (KeyId::new(SHIFT_RIGHT), mods.shift_right),
                    Hand::Right => (KeyId::new(SHIFT_LEFT), mods.shift_left),
                };
                strokes.entry(ch).or_insert_with(|| KeyStroke {
                    key: key.id.clone(),
                    finger: key.finger,
                    modifier: Some(shift),
                });
            }
        }
        for key in keys() {
            if let Some(ch) = key.altgr {
                strokes.entry(ch).or_insert_with(|| KeyStroke {
                    key: key.id.clone(),
                    finger: key.finger,
                    modifier: Some((KeyId::new(ALTGR), mods.altgr)),
                });
            }
        }

        Ok(Layout {
            id: def.id,
            name: def.name,
            rows,
            strokes,
            fingers,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rows of keys, top to bottom, for drawing the virtual keyboard.
    pub fn rows(&self) -> &[Vec<Key>] {
        &self.rows
    }

    pub fn keys(&self) -> impl Iterator<Item = &Key> {
        self.rows.iter().flatten()
    }

    pub fn stroke_for(&self, ch: char) -> Option<&KeyStroke> {
        self.strokes.get(&ch)
    }

    pub fn contains(&self, ch: char) -> bool {
        self.strokes.contains_key(&ch)
    }

    /// Finger responsible for a key, including Shift, AltGr and space.
    pub fn finger_of(&self, key: &KeyId) -> Option<Finger> {
        self.fingers.get(key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
id = "test"
name = "Test"

[[rows]]
keys = [
  { code = "KEY_A", base = "a", shift = "A", altgr = "æ", finger = "L4" },
  { code = "KEY_F", base = "f", shift = "F", finger = "L1", home = true },
  { code = "KEY_J", base = "j", shift = "J", finger = "R1", home = true },
  { code = "KEY_SEMICOLON", base = "ş", shift = "Ş", finger = "R4" },
  { code = "KEY_Q", base = "q", shift = "Q", altgr = "@", finger = "L4" },
  { code = "KEY_X", base = "a", finger = "L3" },
]

[modifiers]
shift_left  = "L4"
shift_right = "R4"
altgr       = "R0"
space       = "R0"
"#;

    fn layout(opts: &LayoutOptions) -> Layout {
        Layout::from_def(LayoutDef::from_toml(SAMPLE).unwrap(), opts).unwrap()
    }

    fn finger(s: &str) -> Finger {
        s.parse().unwrap()
    }

    #[test]
    fn base_character_has_no_modifier() {
        let l = layout(&LayoutOptions::default());
        let s = l.stroke_for('f').unwrap();
        assert_eq!(s.key.as_str(), "KEY_F");
        assert_eq!(s.finger, finger("L1"));
        assert_eq!(s.modifier, None);
    }

    #[test]
    fn shift_uses_opposite_hand() {
        let l = layout(&LayoutOptions::default());
        let left = l.stroke_for('F').unwrap();
        assert_eq!(left.modifier, Some((KeyId::new(SHIFT_RIGHT), finger("R4"))));
        let right = l.stroke_for('Ş').unwrap();
        assert_eq!(right.key.as_str(), "KEY_SEMICOLON");
        assert_eq!(right.finger, finger("R4"));
        assert_eq!(right.modifier, Some((KeyId::new(SHIFT_LEFT), finger("L4"))));
    }

    #[test]
    fn altgr_uses_right_thumb() {
        let l = layout(&LayoutOptions::default());
        let s = l.stroke_for('@').unwrap();
        assert_eq!(s.key.as_str(), "KEY_Q");
        assert_eq!(s.modifier, Some((KeyId::new(ALTGR), finger("R0"))));
    }

    #[test]
    fn space_follows_setting() {
        let l = layout(&LayoutOptions::default());
        assert_eq!(l.stroke_for(' ').unwrap().finger, finger("R0"));
        let l = layout(&LayoutOptions {
            space_thumb: Some(Hand::Left),
        });
        assert_eq!(l.stroke_for(' ').unwrap().finger, finger("L0"));
        assert_eq!(l.finger_of(&KeyId::new(SPACE)), Some(finger("L0")));
    }

    #[test]
    fn first_unmodified_key_wins() {
        let l = layout(&LayoutOptions::default());
        assert_eq!(l.stroke_for('a').unwrap().key.as_str(), "KEY_A");
        assert!(l.stroke_for('z').is_none());
    }

    #[test]
    fn rejects_duplicate_codes_and_bad_format() {
        let dup = SAMPLE.replace("KEY_X", "KEY_A");
        assert!(matches!(
            LayoutDef::from_toml(&dup),
            Err(LayoutError::Invalid(_))
        ));
        let future = format!("format = 9\n{SAMPLE}");
        assert!(matches!(
            LayoutDef::from_toml(&future),
            Err(LayoutError::UnsupportedFormat(9))
        ));
        let multi = SAMPLE.replace(r#"base = "j""#, r#"base = "jj""#);
        assert!(matches!(
            LayoutDef::from_toml(&multi),
            Err(LayoutError::Parse(_))
        ));
    }
}

/// Detects a system keyboard layout that differs from the selected one (R-1).
///
/// Feed it each wrong keystroke. A mistake counts as evidence for another
/// layout when that layout produces the typed character on the physical key
/// that the selected layout uses for the expected one — what happens when the
/// user presses the right key but the desktop maps it differently.
#[derive(Debug)]
pub struct LayoutMismatch {
    evidence: HashMap<String, u32>,
    threshold: u32,
}

impl LayoutMismatch {
    pub fn new(threshold: u32) -> LayoutMismatch {
        LayoutMismatch {
            evidence: HashMap::new(),
            threshold: threshold.max(1),
        }
    }

    /// Returns the id of the likely system layout once it has been seen
    /// `threshold` times.
    pub fn record<'a>(
        &mut self,
        selected: &Layout,
        others: &'a [Layout],
        expected: char,
        typed: char,
    ) -> Option<&'a str> {
        let expected_key = &selected.stroke_for(expected)?.key;
        for other in others.iter().filter(|o| o.id() != selected.id()) {
            let produces = other.keys().any(|k| {
                &k.id == expected_key && [k.base, k.shift, k.altgr].contains(&Some(typed))
            });
            if produces {
                let n = self.evidence.entry(other.id().to_owned()).or_default();
                *n += 1;
                if *n >= self.threshold {
                    return Some(other.id());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod mismatch_tests {
    use super::*;

    fn layout(id: &str, a: char) -> Layout {
        let def = LayoutDef::from_toml(&format!(
            r#"
id = "{id}"
name = "{id}"
[[rows]]
keys = [{{ code = "KEY_A", base = "{a}", finger = "L4" }}, {{ code = "KEY_B", base = "b", finger = "L1" }}]
[modifiers]
shift_left = "L4"
shift_right = "R4"
altgr = "R0"
space = "R0"
"#
        ))
        .unwrap();
        Layout::from_def(def, &LayoutOptions::default()).unwrap()
    }

    #[test]
    fn detects_after_threshold() {
        let selected = layout("q", 'a');
        let others = [layout("q", 'a'), layout("f", 'u')];
        let mut m = LayoutMismatch::new(2);
        assert_eq!(m.record(&selected, &others, 'a', 'x'), None);
        assert_eq!(m.record(&selected, &others, 'a', 'u'), None);
        assert_eq!(m.record(&selected, &others, 'b', 'u'), None);
        assert_eq!(m.record(&selected, &others, 'a', 'u'), Some("f"));
    }
}
