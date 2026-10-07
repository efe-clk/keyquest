//! Progress export and import as JSON (blueprint 12): a readable backup that
//! also moves progress to another computer.

use std::fs;
use std::io::Write;
use std::path::Path;

use keyquest_core::{ProgressExport, ProgressStore};

use crate::{DataError, Result, io_err};

/// Import files larger than this are rejected (years of daily practice stay
/// far below it).
pub const MAX_IMPORT_SIZE: u64 = 64 * 1024 * 1024;

/// Writes all progress to `path` as pretty-printed JSON, through a temporary
/// file so an interrupted export never leaves a truncated file behind.
pub fn export_to_file(store: &dyn ProgressStore, path: &Path) -> Result<usize> {
    let data = store.export_all().map_err(DataError::Store)?;
    let json = serde_json::to_string_pretty(&data).expect("export data always serializes");
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let mut f = fs::File::create(&tmp).map_err(io_err(&tmp))?;
    f.write_all(json.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(io_err(&tmp))?;
    fs::rename(&tmp, path).map_err(io_err(path))?;
    Ok(data.sessions.len())
}

/// Reads and validates an export file without changing anything.
pub fn read_export(path: &Path) -> Result<ProgressExport> {
    let size = fs::metadata(path).map_err(io_err(path))?.len();
    if size > MAX_IMPORT_SIZE {
        return Err(DataError::Import(format!("dosya çok büyük ({size} bayt)")));
    }
    let text = fs::read_to_string(path).map_err(io_err(path))?;
    let data: ProgressExport =
        serde_json::from_str(&text).map_err(|e| DataError::Import(e.to_string()))?;
    data.validate().map_err(DataError::Import)?;
    Ok(data)
}

/// Replaces all progress with the contents of an export file. Returns the
/// number of imported sessions. On any error the old progress is kept.
pub fn import_from_file(store: &mut dyn ProgressStore, path: &Path) -> Result<usize> {
    let data = read_export(path)?;
    store.import_all(&data).map_err(DataError::Store)?;
    Ok(data.sessions.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteProgressStore;
    use keyquest_core::store::now_unix;
    use keyquest_core::{Metrics, SessionResult};
    use std::time::Duration;

    #[test]
    fn round_trip_through_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ilerleme.json");
        let mut a = SqliteProgressStore::open_in_memory().unwrap();
        let mut m = Metrics::new();
        m.record('ş', true, Some(Duration::from_millis(120)));
        m.record('ş', false, None);
        a.save_session(&SessionResult {
            lesson_id: "tr-q/01-ana-sira".into(),
            layout: "tr-q".into(),
            started_at: now_unix(),
            completed: true,
            summary: m.summary(Duration::from_secs(10), 0),
        })
        .unwrap();
        assert_eq!(export_to_file(&a, &path).unwrap(), 1);
        assert!(fs::read_to_string(&path).unwrap().contains("\"ş\""));

        let mut b = SqliteProgressStore::open_in_memory().unwrap();
        assert_eq!(import_from_file(&mut b, &path).unwrap(), 1);
        assert_eq!(b.export_all().unwrap(), a.export_all().unwrap());

        fs::write(&path, "{\"format\": 1}").unwrap();
        assert!(matches!(read_export(&path), Err(DataError::Import(_))));
        assert!(b.lesson_progress("tr-q/01-ana-sira").unwrap().is_some());
    }
}
