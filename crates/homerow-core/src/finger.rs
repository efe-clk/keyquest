//! Hands and fingers. Encoded as `L`/`R` + `0` thumb, `1` index, `2` middle,
//! `3` ring, `4` pinky (e.g. `L4` is the left pinky).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hand {
    Left,
    Right,
}

impl Hand {
    pub fn opposite(self) -> Hand {
        match self {
            Hand::Left => Hand::Right,
            Hand::Right => Hand::Left,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Hand::Left => "sol",
            Hand::Right => "sağ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Digit {
    Thumb = 0,
    Index = 1,
    Middle = 2,
    Ring = 3,
    Pinky = 4,
}

impl Digit {
    pub const ALL: [Digit; 5] = [
        Digit::Thumb,
        Digit::Index,
        Digit::Middle,
        Digit::Ring,
        Digit::Pinky,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Digit::Thumb => "başparmak",
            Digit::Index => "işaret",
            Digit::Middle => "orta",
            Digit::Ring => "yüzük",
            Digit::Pinky => "serçe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Finger {
    pub hand: Hand,
    pub digit: Digit,
}

impl Finger {
    pub const fn new(hand: Hand, digit: Digit) -> Finger {
        Finger { hand, digit }
    }

    /// All ten fingers, left pinky to right pinky (the order they sit on the
    /// keyboard).
    pub fn all() -> [Finger; 10] {
        use Digit::*;
        use Hand::*;
        [
            Finger::new(Left, Pinky),
            Finger::new(Left, Ring),
            Finger::new(Left, Middle),
            Finger::new(Left, Index),
            Finger::new(Left, Thumb),
            Finger::new(Right, Thumb),
            Finger::new(Right, Index),
            Finger::new(Right, Middle),
            Finger::new(Right, Ring),
            Finger::new(Right, Pinky),
        ]
    }

    /// Human readable Turkish name, e.g. "sol serçe".
    pub fn label(&self) -> String {
        format!("{} {}", self.hand.label(), self.digit.label())
    }
}

impl fmt::Display for Finger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hand = match self.hand {
            Hand::Left => 'L',
            Hand::Right => 'R',
        };
        write!(f, "{hand}{}", self.digit as u8)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("geçersiz parmak kodu {0:?} (beklenen: L0-L4 veya R0-R4)")]
pub struct ParseFingerError(String);

impl FromStr for Finger {
    type Err = ParseFingerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseFingerError(s.to_owned());
        let mut chars = s.chars();
        let hand = match chars.next() {
            Some('L') => Hand::Left,
            Some('R') => Hand::Right,
            _ => return Err(err()),
        };
        let digit = match (chars.next(), chars.next()) {
            (Some(d), None) => d.to_digit(10).ok_or_else(err)?,
            _ => return Err(err()),
        };
        let digit = *Digit::ALL.get(digit as usize).ok_or_else(err)?;
        Ok(Finger::new(hand, digit))
    }
}

impl TryFrom<String> for Finger {
    type Error = ParseFingerError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Finger> for String {
    fn from(f: Finger) -> String {
        f.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        for f in Finger::all() {
            assert_eq!(f.to_string().parse::<Finger>().unwrap(), f);
        }
        assert_eq!(
            "L4".parse::<Finger>().unwrap(),
            Finger::new(Hand::Left, Digit::Pinky)
        );
        assert_eq!("R0".parse::<Finger>().unwrap().label(), "sağ başparmak");
    }

    #[test]
    fn rejects_bad_codes() {
        for bad in ["", "L", "L5", "X1", "L12", "l1"] {
            assert!(bad.parse::<Finger>().is_err(), "{bad}");
        }
    }
}
