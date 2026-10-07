//! Homerow data layer: embedded and user TOML files (layouts, lessons),
//! settings and the SQLite progress store.

mod config;
mod embedded;
mod paths;
mod repo;
mod sqlite;

pub use config::{Config, ConfigStore, Theme};
pub use paths::Paths;
pub use repo::{LayoutRepository, LessonRepository, MAX_FILE_SIZE, Warning};
pub use sqlite::SqliteProgressStore;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("veritabanı hatası: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("veritabanı bu sürümden yeni (şema {found}, desteklenen en fazla {supported})")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("{path}: ayarlar okunamadı: {source}")]
    Config {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("bilinmeyen klavye düzeni: {0}")]
    UnknownLayout(String),
    #[error(transparent)]
    Layout(#[from] homerow_core::LayoutError),
    #[error("ev dizini bulunamadı")]
    NoHome,
}

pub type Result<T, E = DataError> = std::result::Result<T, E>;

pub(crate) fn io_err(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> DataError {
    let path = path.into();
    move |source| DataError::Io { path, source }
}
