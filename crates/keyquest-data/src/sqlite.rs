//! SQLite progress store (KR-3). Schema changes are numbered migrations
//! tracked with `PRAGMA user_version`; an existing database is copied to
//! `progress.db.bak` before any migration runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use keyquest_core::store::{EXPORT_FORMAT, now_unix};
use keyquest_core::{
    ExportedKey, ExportedSession, KeyStat, LessonProgress, ProgressExport, ProgressStore,
    SessionResult, SessionSummary, StoreError, StoreResult,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{DataError, Result, io_err};

const MIGRATIONS: &[&str] = &[include_str!("../migrations/001_init.sql")];

#[derive(Debug)]
pub struct SqliteProgressStore {
    conn: Connection,
}

impl SqliteProgressStore {
    pub fn open(path: &Path) -> Result<SqliteProgressStore> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        Self::init(conn, Some(path))
    }

    pub fn open_in_memory() -> Result<SqliteProgressStore> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(mut conn: Connection, path: Option<&Path>) -> Result<SqliteProgressStore> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn, path, MIGRATIONS)?;
        Ok(SqliteProgressStore { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
}

pub fn backup_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".bak");
    PathBuf::from(p)
}

fn migrate(conn: &mut Connection, path: Option<&Path>, migrations: &[&str]) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let latest = migrations.len() as i64;
    if version > latest {
        return Err(DataError::SchemaTooNew {
            found: version,
            supported: latest,
        });
    }
    if version == latest {
        return Ok(());
    }
    if let (Some(path), true) = (path, version > 0) {
        let bak = backup_path(path);
        if bak.exists() {
            std::fs::remove_file(&bak).map_err(io_err(&bak))?;
        }
        conn.execute("VACUUM INTO ?1", [bak.to_string_lossy()])?;
    }
    for (i, sql) in migrations.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

fn store_err(e: rusqlite::Error) -> StoreError {
    StoreError::new(e)
}

fn ch_from_sql(s: String) -> Option<char> {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

impl ProgressStore for SqliteProgressStore {
    fn save_session(&mut self, s: &SessionResult) -> StoreResult<()> {
        let tx = self.conn.transaction().map_err(store_err)?;
        let sum = &s.summary;
        tx.execute(
            "INSERT INTO sessions
                 (lesson_id, layout, started_at, duration_ms, chars_total, errors, wpm_net, accuracy)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                s.lesson_id,
                s.layout,
                s.started_at,
                sum.duration.as_millis() as i64,
                sum.chars_total,
                sum.errors,
                sum.wpm_net,
                sum.accuracy,
            ],
        )
        .map_err(store_err)?;
        let session_id = tx.last_insert_rowid();
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO key_stats (session_id, ch, hits, misses, avg_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(store_err)?;
            for k in &sum.keys {
                stmt.execute(params![
                    session_id,
                    k.ch.to_string(),
                    k.hits,
                    k.misses,
                    k.avg_ms
                ])
                .map_err(store_err)?;
            }
        }
        tx.execute(
            "INSERT INTO lesson_progress (lesson_id, best_wpm, best_acc, completed)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (lesson_id) DO UPDATE SET
                 best_wpm  = max(best_wpm, excluded.best_wpm),
                 best_acc  = max(best_acc, excluded.best_acc),
                 completed = max(completed, excluded.completed)",
            params![s.lesson_id, sum.wpm_net, sum.accuracy, s.completed],
        )
        .map_err(store_err)?;
        tx.commit().map_err(store_err)
    }

    fn key_stats(&self, layout: &str) -> StoreResult<Vec<KeyStat>> {
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT k.ch, SUM(k.hits), SUM(k.misses),
                        CAST(ROUND(SUM(k.avg_ms * (k.hits + k.misses)) * 1.0
                             / NULLIF(SUM(CASE WHEN k.avg_ms IS NOT NULL
                                               THEN k.hits + k.misses END), 0)) AS INTEGER)
                 FROM key_stats k JOIN sessions s ON s.id = k.session_id
                 WHERE s.layout = ?1
                 GROUP BY k.ch
                 ORDER BY k.ch",
            )
            .map_err(store_err)?;
        let rows = stmt
            .query_map([layout], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, Option<u32>>(3)?,
                ))
            })
            .map_err(store_err)?;
        let mut out = Vec::new();
        for row in rows {
            let (ch, hits, misses, avg_ms) = row.map_err(store_err)?;
            if let Some(ch) = ch_from_sql(ch) {
                out.push(KeyStat {
                    ch,
                    hits,
                    misses,
                    avg_ms,
                });
            }
        }
        // SQLite orders by bytes; keep the same order as the other stores.
        out.sort_by_key(|k| k.ch);
        Ok(out)
    }

    fn lesson_progress(&self, lesson_id: &str) -> StoreResult<Option<LessonProgress>> {
        self.conn
            .query_row(
                "SELECT lesson_id, best_wpm, best_acc, completed
                 FROM lesson_progress WHERE lesson_id = ?1",
                [lesson_id],
                |r| {
                    Ok(LessonProgress {
                        lesson_id: r.get(0)?,
                        best_wpm: r.get(1)?,
                        best_acc: r.get(2)?,
                        completed: r.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(store_err)
    }

    fn history(&self, layout: &str, days: u32) -> StoreResult<Vec<SessionSummary>> {
        let since = now_unix() - days as i64 * 86_400;
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT id, lesson_id, started_at, duration_ms, chars_total, errors, wpm_net, accuracy
                 FROM sessions
                 WHERE layout = ?1 AND started_at >= ?2
                 ORDER BY started_at, id",
            )
            .map_err(store_err)?;
        let rows = stmt
            .query_map(params![layout, since], |r| {
                Ok(SessionSummary {
                    id: r.get(0)?,
                    lesson_id: r.get(1)?,
                    started_at: r.get(2)?,
                    duration_ms: r.get::<_, i64>(3)?.max(0) as u64,
                    chars_total: r.get(4)?,
                    errors: r.get(5)?,
                    wpm_net: r.get(6)?,
                    accuracy: r.get(7)?,
                })
            })
            .map_err(store_err)?;
        rows.collect::<Result<_, _>>().map_err(store_err)
    }

    fn reset(&mut self) -> StoreResult<()> {
        self.conn
            .execute_batch(
                "BEGIN;
                 DELETE FROM key_stats;
                 DELETE FROM sessions;
                 DELETE FROM lesson_progress;
                 COMMIT;",
            )
            .map_err(store_err)
    }

    fn export_all(&self) -> StoreResult<ProgressExport> {
        let mut sessions = Vec::new();
        let mut ids = Vec::new();
        {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT id, lesson_id, layout, started_at, duration_ms, chars_total,
                            errors, wpm_net, accuracy
                     FROM sessions ORDER BY id",
                )
                .map_err(store_err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        ExportedSession {
                            lesson_id: r.get(1)?,
                            layout: r.get(2)?,
                            started_at: r.get(3)?,
                            duration_ms: r.get::<_, i64>(4)?.max(0) as u64,
                            chars_total: r.get(5)?,
                            errors: r.get(6)?,
                            wpm_net: r.get(7)?,
                            accuracy: r.get(8)?,
                            keys: Vec::new(),
                        },
                    ))
                })
                .map_err(store_err)?;
            for row in rows {
                let (id, s) = row.map_err(store_err)?;
                ids.push(id);
                sessions.push(s);
            }
        }
        {
            let mut stmt = self
                .conn
                .prepare("SELECT session_id, ch, hits, misses, avg_ms FROM key_stats ORDER BY session_id, ch")
                .map_err(store_err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u32>(2)?,
                        r.get::<_, u32>(3)?,
                        r.get::<_, Option<u32>>(4)?,
                    ))
                })
                .map_err(store_err)?;
            for row in rows {
                let (session_id, ch, hits, misses, avg_ms) = row.map_err(store_err)?;
                let (Ok(i), Some(ch)) = (ids.binary_search(&session_id), ch_from_sql(ch)) else {
                    continue;
                };
                sessions[i].keys.push(ExportedKey {
                    ch,
                    hits,
                    misses,
                    avg_ms,
                });
            }
        }
        let mut stmt = self
            .conn
            .prepare("SELECT lesson_id, best_wpm, best_acc, completed FROM lesson_progress ORDER BY lesson_id")
            .map_err(store_err)?;
        let lessons = stmt
            .query_map([], |r| {
                Ok(LessonProgress {
                    lesson_id: r.get(0)?,
                    best_wpm: r.get(1)?,
                    best_acc: r.get(2)?,
                    completed: r.get(3)?,
                })
            })
            .map_err(store_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store_err)?;
        Ok(ProgressExport {
            format: EXPORT_FORMAT,
            sessions,
            lessons,
        })
    }

    fn import_all(&mut self, data: &ProgressExport) -> StoreResult<()> {
        data.validate().map_err(StoreError::new)?;
        let tx = self.conn.transaction().map_err(store_err)?;
        tx.execute_batch(
            "DELETE FROM key_stats; DELETE FROM sessions; DELETE FROM lesson_progress;",
        )
        .map_err(store_err)?;
        {
            let mut session = tx
                .prepare(
                    "INSERT INTO sessions
                         (lesson_id, layout, started_at, duration_ms, chars_total, errors, wpm_net, accuracy)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .map_err(store_err)?;
            let mut key = tx
                .prepare(
                    "INSERT INTO key_stats (session_id, ch, hits, misses, avg_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(store_err)?;
            for s in &data.sessions {
                let id = session
                    .insert(params![
                        s.lesson_id,
                        s.layout,
                        s.started_at,
                        s.duration_ms as i64,
                        s.chars_total,
                        s.errors,
                        s.wpm_net,
                        s.accuracy,
                    ])
                    .map_err(store_err)?;
                for k in &s.keys {
                    key.execute(params![id, k.ch.to_string(), k.hits, k.misses, k.avg_ms])
                        .map_err(store_err)?;
                }
            }
            let mut lesson = tx
                .prepare(
                    "INSERT INTO lesson_progress (lesson_id, best_wpm, best_acc, completed)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(store_err)?;
            for l in &data.lessons {
                lesson
                    .execute(params![l.lesson_id, l.best_wpm, l.best_acc, l.completed])
                    .map_err(store_err)?;
            }
        }
        tx.commit().map_err(store_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyquest_core::Metrics;

    fn result(lesson: &str, completed: bool, keys: &[(char, bool, u64)]) -> SessionResult {
        let mut m = Metrics::new();
        for (c, ok, ms) in keys {
            m.record(*c, *ok, Some(Duration::from_millis(*ms)));
        }
        SessionResult {
            lesson_id: lesson.into(),
            layout: "tr-q".into(),
            started_at: now_unix(),
            completed,
            summary: m.summary(Duration::from_secs(20), 1),
        }
    }

    #[test]
    fn save_and_query() {
        let mut st = SqliteProgressStore::open_in_memory().unwrap();
        let mut keys = vec![('ş', true, 100); 4];
        keys.push(('ş', false, 300));
        keys.extend([('a', true, 100); 6]);
        st.save_session(&result("l1", false, &keys)).unwrap();
        st.save_session(&result("l1", true, &[('ş', false, 200), ('a', true, 100)]))
            .unwrap();

        let p = st.lesson_progress("l1").unwrap().unwrap();
        assert!(p.completed);
        assert!(st.lesson_progress("l2").unwrap().is_none());

        let stats = st.key_stats("tr-q").unwrap();
        assert_eq!(stats.len(), 2);
        let s = &stats[1];
        assert_eq!((s.ch, s.hits, s.misses), ('ş', 4, 2));
        // Session 1: 4 hits at 100 ms + 1 miss at 300 ms → avg 140 over 5;
        // session 2: avg 200 over 1 → weighted (140*5 + 200) / 6 = 150.
        assert_eq!(s.avg_ms, Some(150));
        assert!(st.key_stats("us").unwrap().is_empty());

        let weak = st.weakest_keys("tr-q", 5).unwrap();
        assert_eq!(weak[0].ch, 'ş');

        let h = st.history("tr-q", 1).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].chars_total, 11);

        let exported = st.export_all().unwrap();
        assert_eq!(exported.sessions.len(), 2);
        assert_eq!(exported.sessions[0].keys.len(), 2);
        let mut copy = SqliteProgressStore::open_in_memory().unwrap();
        copy.save_session(&result("other", true, &[('x', true, 1)]))
            .unwrap();
        copy.import_all(&exported).unwrap();
        assert_eq!(copy.export_all().unwrap(), exported);
        assert!(copy.lesson_progress("other").unwrap().is_none());
        assert_eq!(copy.key_stats("tr-q").unwrap(), stats);

        // A failed import rolls back and keeps the old data.
        let mut bad = exported.clone();
        bad.sessions.push(bad.sessions[0].clone());
        let duplicate = bad.sessions[1].keys[0].clone();
        bad.sessions[1].keys.push(duplicate);
        assert!(copy.import_all(&bad).is_err());
        assert_eq!(copy.export_all().unwrap(), exported);

        st.reset().unwrap();
        assert!(st.history("tr-q", 1).unwrap().is_empty());
    }

    #[test]
    fn migrates_backs_up_and_refuses_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/progress.db");
        {
            let mut st = SqliteProgressStore::open(&path).unwrap();
            assert_eq!(st.schema_version().unwrap(), MIGRATIONS.len() as i64);
            st.save_session(&result("l1", true, &[('a', true, 100)]))
                .unwrap();
        }
        // Reopening keeps data and does not need a backup.
        {
            let st = SqliteProgressStore::open(&path).unwrap();
            assert!(st.lesson_progress("l1").unwrap().is_some());
            assert!(!backup_path(&path).exists());
        }
        // A new migration backs up the old database first.
        let next = [MIGRATIONS[0], "ALTER TABLE sessions ADD COLUMN note TEXT;"];
        let mut conn = Connection::open(&path).unwrap();
        migrate(&mut conn, Some(&path), &next).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
        drop(conn);
        let bak = Connection::open(backup_path(&path)).unwrap();
        let (v, n): (i64, i64) = bak
            .query_row(
                "SELECT (SELECT user_version FROM pragma_user_version), count(*) FROM sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((v, n), (1, 1));

        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(matches!(
            SqliteProgressStore::open(&path),
            Err(DataError::SchemaTooNew {
                found: 99,
                supported: 1
            })
        ));
    }
}
