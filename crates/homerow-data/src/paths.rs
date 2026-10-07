use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::{DataError, Result};

const APP: &str = "homerow";

/// Where Homerow keeps its files (XDG, blueprint 8.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    /// `~/.config/homerow/config.toml` and `~/.local/share/homerow/`
    /// (honouring `XDG_CONFIG_HOME` and `XDG_DATA_HOME`).
    pub fn from_env() -> Result<Paths> {
        let dirs = BaseDirs::new().ok_or(DataError::NoHome)?;
        Ok(Paths {
            config_file: dirs.config_dir().join(APP).join("config.toml"),
            data_dir: dirs.data_dir().join(APP),
        })
    }

    /// Everything under one directory; for tests and portable use.
    pub fn in_dir(root: &Path) -> Paths {
        Paths {
            config_file: root.join("config.toml"),
            data_dir: root.to_owned(),
        }
    }

    pub fn progress_db(&self) -> PathBuf {
        self.data_dir.join("progress.db")
    }

    pub fn user_layouts(&self) -> PathBuf {
        self.data_dir.join("layouts")
    }

    pub fn user_lessons(&self) -> PathBuf {
        self.data_dir.join("lessons")
    }
}
