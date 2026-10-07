//! KeyQuest core: practice sessions, metrics, keyboard layouts and lesson
//! generation.
//!
//! This crate knows nothing about GTK, files or databases. Time is always
//! passed in by the caller, so everything here is deterministic and testable.

pub mod finger;
pub mod generator;
pub mod layout;
pub mod lesson;
pub mod metrics;
pub mod rng;
pub mod session;
pub mod store;

pub use finger::{Digit, Finger, Hand};
pub use generator::{KeyWeights, LessonGenerator};
pub use layout::{
    Key, KeyId, KeyStroke, Layout, LayoutDef, LayoutError, LayoutMismatch, LayoutOptions,
};
pub use lesson::{Lesson, LessonError};
pub use metrics::{KeyStat, Metrics, Summary};
pub use session::{CharStatus, ErrorMode, InputResult, Session, SessionState};
pub use store::{
    FingerStat, LessonProgress, LessonStatus, MemoryProgressStore, ProgressStore, SessionResult,
    SessionSummary, StoreError, StoreResult,
};
