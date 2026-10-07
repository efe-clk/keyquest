//! User settings in `config.toml` (FG-10).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use homerow_core::{ErrorMode, Hand, LayoutOptions};
use serde::{Deserialize, Serialize};

use crate::{DataError, Result, io_err};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// Missing fields fall back to their defaults, so older or hand-written
/// files keep working.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub layout: String,
    pub space_thumb: Hand,
    pub error_mode: ErrorMode,
    pub theme: Theme,
    pub font_scale: f64,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            layout: "tr-q".into(),
            space_thumb: Hand::Right,
            error_mode: ErrorMode::StopOnError,
            theme: Theme::System,
            font_scale: 1.0,
        }
    }
}

impl Config {
    pub fn layout_options(&self) -> LayoutOptions {
        LayoutOptions {
            space_thumb: Some(self.space_thumb),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> ConfigStore {
        ConfigStore { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the settings; a missing file gives the defaults.
    pub fn load(&self) -> Result<Config> {
        match fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text).map_err(|source| DataError::Config {
                path: self.path.clone(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(io_err(&self.path)(e)),
        }
    }

    /// Writes to a temporary file and renames it over the old one, so a crash
    /// never leaves a half-written config.
    pub fn save(&self, config: &Config) -> Result<()> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir).map_err(io_err(dir))?;
        let text = toml::to_string_pretty(config).expect("Config always serializes");
        let tmp = self.path.with_extension("toml.tmp");
        let mut f = fs::File::create(&tmp).map_err(io_err(&tmp))?;
        f.write_all(text.as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(io_err(&tmp))?;
        fs::rename(&tmp, &self.path).map_err(io_err(&self.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_and_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("sub/config.toml"));
        assert_eq!(store.load().unwrap(), Config::default());

        let config = Config {
            layout: "tr-f".into(),
            space_thumb: Hand::Left,
            error_mode: ErrorMode::Continue,
            theme: Theme::Dark,
            font_scale: 1.25,
        };
        store.save(&config).unwrap();
        assert_eq!(store.load().unwrap(), config);

        fs::write(store.path(), "error_mode = \"continue\"\n").unwrap();
        let c = store.load().unwrap();
        assert_eq!(c.error_mode, ErrorMode::Continue);
        assert_eq!(c.layout, "tr-q");

        fs::write(store.path(), "theme = 3").unwrap();
        assert!(matches!(store.load(), Err(DataError::Config { .. })));
    }
}
