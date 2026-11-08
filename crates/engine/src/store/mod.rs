//! SQLite persistence: downloads, segments, packages and settings.

use crate::error::EngineError;
use crate::segmenter::Range;
use crate::types::{DownloadId, DownloadState, PackageId, Priority, ProbeInfo};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

mod clients;
mod collector;
mod postproc;
pub use clients::PairedClient;
pub use collector::{Batch, CollectorLink};
pub use postproc::{RuleRowIn, RuleRowOut, RunRow};

const SCHEMA_VERSION: i64 = 5;

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Clone, Debug)]
pub struct NewDownload {
    pub package_id: Option<PackageId>,
    pub url: String,
    pub headers_json: Option<String>,
    pub target_dir: PathBuf,
    pub file_name: Option<String>,
    pub priority: Priority,
    pub connections: u32,
    pub speed_limit: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Download {
    pub id: DownloadId,
    pub package_id: Option<PackageId>,
    pub url: String,
    pub headers_json: Option<String>,
    pub probe: Option<ProbeInfo>,
    pub state: DownloadState,
    pub priority: Priority,
    pub position: i64,
    pub connections: u32,
    pub speed_limit: Option<u64>,
    pub target_dir: PathBuf,
    pub file_name: Option<String>,
    pub target_path: Option<PathBuf>,
    pub error: Option<String>,
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub created_at: String,
    /// A hash the user pasted, as `"<algo>:<hex>"`.
    pub expected_hash: Option<String>,
    /// The server's digest from the probe, as `"<algo>:<hex>"`.
    pub server_digest: Option<String>,
    /// How many times a checksum mismatch re-downloaded this file.
    pub verify_retries: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub id: PackageId,
    pub name: String,
    pub source: String,
    pub created_at: String,
}

const DOWNLOAD_COLS: &str = "id, package_id, url, headers_blob, state, priority, position, connections, \
    speed_limit, target_dir, file_name, target_path, error, final_url, size, etag, last_modified, \
    resumable, probe_file_name, content_type, created_at, expected_hash, server_digest, verify_retries";

fn row_to_download(r: &Row) -> rusqlite::Result<Download> {
    let final_url: Option<String> = r.get(13)?;
    let probe = match final_url {
        Some(final_url) => Some(ProbeInfo {
            final_url,
            size: r.get::<_, Option<i64>>(14)?.map(|v| v as u64),
            etag: r.get(15)?,
            last_modified: r.get(16)?,
            resumable: r.get::<_, Option<bool>>(17)?.unwrap_or(false),
            file_name: r.get::<_, Option<String>>(18)?.unwrap_or_default(),
            content_type: r.get(19)?,
            digest: r.get(22)?,
        }),
        None => None,
    };
    let state: String = r.get(4)?;
    Ok(Download {
        id: r.get(0)?,
        package_id: r.get(1)?,
        url: r.get(2)?,
        headers_json: open_headers(r.get(3)?),
        probe,
        state: DownloadState::parse(&state).unwrap_or(DownloadState::Failed),
        priority: Priority::from_i64(r.get(5)?),
        position: r.get(6)?,
        connections: r.get::<_, i64>(7)? as u32,
        speed_limit: r.get::<_, Option<i64>>(8)?.map(|v| v as u64),
        target_dir: PathBuf::from(r.get::<_, String>(9)?),
        file_name: r.get(10)?,
        target_path: r.get::<_, Option<String>>(11)?.map(PathBuf::from),
        error: r.get(12)?,
        created_at: r.get(20)?,
        expected_hash: r.get(21)?,
        server_digest: r.get(22)?,
        verify_retries: r.get::<_, i64>(23)? as u32,
    })
}

/// Request headers are only ever stored encrypted for the current Windows user.
fn seal_headers(json: Option<&str>) -> Result<Option<Vec<u8>>, EngineError> {
    json.map(|j| crate::secret::protect(j.as_bytes())).transpose()
}

fn open_headers(blob: Option<Vec<u8>>) -> Option<String> {
    let blob = blob?;
    match crate::secret::unprotect(&blob) {
        Ok(plain) => String::from_utf8(plain).ok(),
        Err(e) => {
            tracing::warn!(error = %e, "stored request headers could not be decrypted; ignoring them");
            None
        }
    }
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, EngineError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Store, EngineError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store, EngineError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let migrations = [
            include_str!("schema.sql"),
            include_str!("migration_2.sql"),
            include_str!("migration_3.sql"),
            include_str!("migration_4.sql"),
            include_str!("migration_5.sql"),
        ];
        for (i, sql) in migrations.iter().enumerate().skip(version.max(0) as usize) {
            let to = i + 1;
            conn.execute_batch(&format!("BEGIN; {sql} PRAGMA user_version = {to}; COMMIT;"))?;
        }
        debug_assert_eq!(migrations.len() as i64, SCHEMA_VERSION);
        Ok(Store { conn: Arc::new(Mutex::new(conn)) })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn schema_version(&self) -> Result<i64, EngineError> {
        Ok(self.lock().pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    pub fn create_package(&self, name: &str, source: &str) -> Result<PackageId, EngineError> {
        let c = self.lock();
        c.execute("INSERT INTO packages (name, source) VALUES (?1, ?2)", params![name, source])?;
        Ok(c.last_insert_rowid())
    }

    pub fn list_packages(&self) -> Result<Vec<Package>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare("SELECT id, name, source, created_at FROM packages ORDER BY id")?;
        let rows = st
            .query_map([], |r| {
                Ok(Package { id: r.get(0)?, name: r.get(1)?, source: r.get(2)?, created_at: r.get(3)? })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn insert_download(&self, d: &NewDownload) -> Result<DownloadId, EngineError> {
        let c = self.lock();
        c.execute(
            "INSERT INTO downloads (package_id, url, headers_blob, priority, position, connections, \
             speed_limit, target_dir, file_name) \
             VALUES (?1, ?2, ?3, ?4, (SELECT COALESCE(MAX(position), 0) + 1 FROM downloads), ?5, ?6, ?7, ?8)",
            params![
                d.package_id,
                d.url,
                seal_headers(d.headers_json.as_deref())?,
                d.priority.as_i64(),
                d.connections.clamp(1, 32) as i64,
                d.speed_limit.map(|v| v as i64),
                path_str(&d.target_dir),
                d.file_name,
            ],
        )?;
        Ok(c.last_insert_rowid())
    }

    pub fn get_download(&self, id: DownloadId) -> Result<Download, EngineError> {
        Ok(self.lock().query_row(
            &format!("SELECT {DOWNLOAD_COLS} FROM downloads WHERE id = ?1"),
            [id],
            row_to_download,
        )?)
    }

    /// All downloads, ordered by priority, position, id.
    pub fn list_downloads(&self) -> Result<Vec<Download>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare(&format!("SELECT {DOWNLOAD_COLS} FROM downloads ORDER BY priority, position, id"))?;
        let rows = st.query_map([], row_to_download)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_probe(&self, id: DownloadId, p: &ProbeInfo) -> Result<(), EngineError> {
        self.lock().execute(
            "UPDATE downloads SET final_url = ?2, size = ?3, etag = ?4, last_modified = ?5, resumable = ?6, \
             probe_file_name = ?7, content_type = ?8, server_digest = ?9 WHERE id = ?1",
            params![
                id,
                p.final_url,
                p.size.map(|v| v as i64),
                p.etag,
                p.last_modified,
                p.resumable,
                p.file_name,
                p.content_type,
                p.digest
            ],
        )?;
        Ok(())
    }

    /// Fixes the file name the first time a download is probed, so resumes find the same part file.
    pub fn set_file_name(&self, id: DownloadId, name: &str) -> Result<(), EngineError> {
        self.lock().execute("UPDATE downloads SET file_name = ?2 WHERE id = ?1", params![id, name])?;
        Ok(())
    }

    /// A download that failed for good forgets its request headers.
    pub fn set_state(&self, id: DownloadId, s: DownloadState, error: Option<&str>) -> Result<(), EngineError> {
        let wipe = s == DownloadState::Failed;
        self.lock().execute(
            "UPDATE downloads SET state = ?2, error = ?3,              headers_blob = CASE WHEN ?4 THEN NULL ELSE headers_blob END WHERE id = ?1",
            params![id, s.as_str(), error, wipe],
        )?;
        Ok(())
    }

    /// Replaces a download's request headers (a fresh capture from the browser).
    pub fn refresh_headers(&self, id: DownloadId, headers_json: Option<&str>) -> Result<(), EngineError> {
        let blob = seal_headers(headers_json)?;
        self.lock().execute("UPDATE downloads SET headers_blob = ?2 WHERE id = ?1", params![id, blob])?;
        Ok(())
    }

    /// Every download of exactly this URL, newest first.
    pub fn find_by_url(&self, url: &str) -> Result<Vec<Download>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare(&format!("SELECT {DOWNLOAD_COLS} FROM downloads WHERE url = ?1 ORDER BY id DESC"))?;
        let rows = st.query_map([url], row_to_download)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Queued -> Running in one step. False if the row is in any other state or gone,
    /// so a pause or remove that lands first always wins.
    pub fn start_if_queued(&self, id: DownloadId) -> Result<bool, EngineError> {
        let n = self.lock().execute(
            "UPDATE downloads SET state = ?2, error = NULL WHERE id = ?1 AND state = ?3",
            params![id, DownloadState::Running.as_str(), DownloadState::Queued.as_str()],
        )?;
        Ok(n == 1)
    }

    pub fn set_completed(&self, id: DownloadId, path: &Path) -> Result<(), EngineError> {
        self.lock().execute(
            "UPDATE downloads SET state = ?2, error = NULL, target_path = ?3, headers_blob = NULL WHERE id = ?1",
            params![id, DownloadState::Completed.as_str(), path_str(path)],
        )?;
        Ok(())
    }

    pub fn set_order(&self, id: DownloadId, p: Priority, position: i64) -> Result<(), EngineError> {
        self.lock().execute(
            "UPDATE downloads SET priority = ?2, position = ?3 WHERE id = ?1",
            params![id, p.as_i64(), position],
        )?;
        Ok(())
    }

    /// Puts `ids` first, in that order, at positions 1..=k; every other row follows in its
    /// current (priority, position, id) order. Priorities are unchanged.
    pub fn reorder(&self, ids: &[DownloadId]) -> Result<(), EngineError> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        let rest: Vec<DownloadId> = {
            let mut st = tx.prepare("SELECT id FROM downloads ORDER BY priority, position, id")?;
            st.query_map([], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?
        };
        {
            let mut up = tx.prepare("UPDATE downloads SET position = ?2 WHERE id = ?1")?;
            let mut pos = 0i64;
            for id in ids.iter().chain(rest.iter().filter(|i| !ids.contains(i))) {
                pos += 1;
                up.execute(params![id, pos])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes the row; its segments go with it (foreign-key cascade).
    pub fn delete_download(&self, id: DownloadId) -> Result<(), EngineError> {
        self.lock().execute("DELETE FROM downloads WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Replaces all segments of `id` in one transaction.
    pub fn save_segments(&self, id: DownloadId, ranges: &[Range]) -> Result<(), EngineError> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM segments WHERE download_id = ?1", [id])?;
        {
            let mut st =
                tx.prepare("INSERT INTO segments (download_id, start, pos, \"end\") VALUES (?1, ?2, ?3, ?4)")?;
            for r in ranges {
                st.execute(params![id, r.start as i64, r.pos as i64, r.end as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_segments(&self, id: DownloadId) -> Result<Vec<Range>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare("SELECT start, pos, \"end\" FROM segments WHERE download_id = ?1 ORDER BY start")?;
        let rows = st
            .query_map([id], |r| {
                Ok(Range {
                    start: r.get::<_, i64>(0)? as u64,
                    pos: r.get::<_, i64>(1)? as u64,
                    end: r.get::<_, i64>(2)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Every download's segments in one query, each list ordered by start.
    pub fn load_all_segments(&self) -> Result<std::collections::HashMap<DownloadId, Vec<Range>>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare("SELECT download_id, start, pos, \"end\" FROM segments ORDER BY download_id, start")?;
        let mut out: std::collections::HashMap<DownloadId, Vec<Range>> = std::collections::HashMap::new();
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Range {
                    start: r.get::<_, i64>(1)? as u64,
                    pos: r.get::<_, i64>(2)? as u64,
                    end: r.get::<_, i64>(3)? as u64,
                },
            ))
        })?;
        for row in rows {
            let (id, range) = row?;
            out.entry(id).or_default().push(range);
        }
        Ok(out)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, EngineError> {
        let v: Option<String> =
            self.lock().query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?;
        Ok(v.and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub fn set_setting(&self, key: &str, v: &serde_json::Value) -> Result<(), EngineError> {
        self.lock().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, v.to_string()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DownloadState, Priority, ProbeInfo};
    use std::path::PathBuf;

    fn nd(p: Priority) -> NewDownload {
        NewDownload {
            package_id: None,
            url: "https://example.org/a.bin".into(),
            headers_json: Some("{\"Cookie\":\"x=1\"}".into()),
            target_dir: PathBuf::from(r"C:\dl"),
            file_name: Some("a.bin".into()),
            priority: p,
            connections: 8,
            speed_limit: Some(1000),
        }
    }

    #[test]
    fn insert_and_get_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        let pkg = s.create_package("pkg", "manual").unwrap();
        let mut n = nd(Priority::High);
        n.package_id = Some(pkg);
        let id = s.insert_download(&n).unwrap();
        let probe = ProbeInfo {
            final_url: "https://cdn.example.org/a.bin".into(),
            size: Some(1234),
            resumable: true,
            file_name: "a.bin".into(),
            etag: Some("\"e\"".into()),
            last_modified: Some("Tue, 01 Jan 2030 00:00:00 GMT".into()),
            content_type: Some("application/octet-stream".into()),
            digest: None,
        };
        s.set_probe(id, &probe).unwrap();
        let d = s.get_download(id).unwrap();
        assert_eq!(d.id, id);
        assert_eq!(d.package_id, Some(pkg));
        assert_eq!(d.url, n.url);
        assert_eq!(d.headers_json, n.headers_json);
        assert_eq!(d.probe, Some(probe));
        assert_eq!(d.state, DownloadState::Queued);
        assert_eq!(d.priority, Priority::High);
        assert_eq!(d.position, 1);
        assert_eq!(d.connections, 8);
        assert_eq!(d.speed_limit, Some(1000));
        assert_eq!(d.target_dir, n.target_dir);
        assert_eq!(d.file_name, n.file_name);
        assert_eq!(d.target_path, None);
        assert_eq!(d.error, None);
        s.set_completed(id, std::path::Path::new(r"C:\dl\a.bin")).unwrap();
        let d = s.get_download(id).unwrap();
        assert_eq!(d.state, DownloadState::Completed);
        assert_eq!(d.target_path, Some(PathBuf::from(r"C:\dl\a.bin")));
    }

    #[test]
    fn positions_increment() {
        let s = Store::open_in_memory().unwrap();
        let ids: Vec<_> = (0..3).map(|_| s.insert_download(&nd(Priority::Normal)).unwrap()).collect();
        let pos: Vec<_> = ids.iter().map(|i| s.get_download(*i).unwrap().position).collect();
        assert_eq!(pos, vec![1, 2, 3]);
    }

    #[test]
    fn list_orders_by_priority_then_position() {
        let s = Store::open_in_memory().unwrap();
        let a = s.insert_download(&nd(Priority::Normal)).unwrap();
        let b = s.insert_download(&nd(Priority::Low)).unwrap();
        let c = s.insert_download(&nd(Priority::High)).unwrap();
        let d = s.insert_download(&nd(Priority::Normal)).unwrap();
        let ids: Vec<_> = s.list_downloads().unwrap().into_iter().map(|x| x.id).collect();
        assert_eq!(ids, vec![c, a, d, b]);
        s.set_order(b, Priority::High, 0).unwrap();
        let ids: Vec<_> = s.list_downloads().unwrap().into_iter().map(|x| x.id).collect();
        assert_eq!(ids, vec![b, c, a, d]);
    }

    #[test]
    fn segments_replace() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert_download(&nd(Priority::Normal)).unwrap();
        let three = [
            Range { start: 0, pos: 5, end: 10 },
            Range { start: 10, pos: 10, end: 20 },
            Range { start: 20, pos: 25, end: 30 },
        ];
        s.save_segments(id, &three).unwrap();
        assert_eq!(s.load_segments(id).unwrap(), three.to_vec());
        let two = [Range { start: 0, pos: 10, end: 10 }, Range { start: 10, pos: 12, end: 30 }];
        s.save_segments(id, &two).unwrap();
        assert_eq!(s.load_segments(id).unwrap(), two.to_vec());
    }

    #[test]
    fn state_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("haul.db");
        let id = {
            let s = Store::open(&path).unwrap();
            let id = s.insert_download(&nd(Priority::Normal)).unwrap();
            s.set_state(id, DownloadState::Paused, Some("x")).unwrap();
            id
        };
        let s = Store::open(&path).unwrap();
        let d = s.get_download(id).unwrap();
        assert_eq!(d.state, DownloadState::Paused);
        assert_eq!(d.error.as_deref(), Some("x"));
    }

    #[test]
    fn start_if_queued_only_starts_queued_rows() {
        let s = Store::open_in_memory().unwrap();
        let a = s.insert_download(&nd(Priority::Normal)).unwrap();
        let b = s.insert_download(&nd(Priority::Normal)).unwrap();
        s.set_state(b, DownloadState::Paused, None).unwrap();
        assert!(s.start_if_queued(a).unwrap());
        assert_eq!(s.get_download(a).unwrap().state, DownloadState::Running);
        assert!(!s.start_if_queued(a).unwrap(), "already running");
        assert!(!s.start_if_queued(b).unwrap(), "paused must stay paused");
        assert_eq!(s.get_download(b).unwrap().state, DownloadState::Paused);
        assert!(!s.start_if_queued(9999).unwrap(), "gone");
    }

    #[test]
    fn delete_cascades_segments() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert_download(&nd(Priority::Normal)).unwrap();
        s.save_segments(id, &[Range { start: 0, pos: 0, end: 9 }]).unwrap();
        s.delete_download(id).unwrap();
        assert!(s.load_segments(id).unwrap().is_empty());
        assert!(s.get_download(id).is_err());
    }

    #[test]
    fn reorder_renumbers() {
        let s = Store::open_in_memory().unwrap();
        let ids: Vec<_> = (0..4).map(|_| s.insert_download(&nd(Priority::Normal)).unwrap()).collect();
        let (a, b, c, d) = (ids[0], ids[1], ids[2], ids[3]);
        s.reorder(&[c, a]).unwrap();
        let list = s.list_downloads().unwrap();
        assert_eq!(list.iter().map(|x| x.id).collect::<Vec<_>>(), vec![c, a, b, d]);
        assert_eq!(list.iter().map(|x| x.position).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn packages_listed() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_package("p", "paste").unwrap();
        let ps = s.list_packages().unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].id, id);
        assert_eq!(ps[0].name, "p");
        assert_eq!(ps[0].source, "paste");
        assert!(ps[0].created_at.ends_with('Z'), "{}", ps[0].created_at);
    }

    #[test]
    fn created_at_filled() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert_download(&nd(Priority::Normal)).unwrap();
        assert_eq!(s.get_download(id).unwrap().created_at.len(), 20);
    }

    #[test]
    fn all_segments_grouped_by_download() {
        let s = Store::open_in_memory().unwrap();
        let a = s.insert_download(&nd(Priority::Normal)).unwrap();
        let b = s.insert_download(&nd(Priority::Normal)).unwrap();
        s.save_segments(a, &[Range { start: 10, pos: 15, end: 20 }, Range { start: 0, pos: 5, end: 10 }]).unwrap();
        s.save_segments(b, &[Range { start: 0, pos: 1, end: 2 }]).unwrap();
        let all = s.load_all_segments().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[&a], vec![Range { start: 0, pos: 5, end: 10 }, Range { start: 10, pos: 15, end: 20 }]);
        assert_eq!(all[&b], vec![Range { start: 0, pos: 1, end: 2 }]);
    }

    #[test]
    fn settings_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.get_setting("k").unwrap(), None);
        s.set_setting("k", &serde_json::json!({"a": 1})).unwrap();
        s.set_setting("k", &serde_json::json!(2)).unwrap();
        assert_eq!(s.get_setting("k").unwrap(), Some(serde_json::json!(2)));
    }

    #[test]
    fn schema_version_is_5() {
        assert_eq!(Store::open_in_memory().unwrap().schema_version().unwrap(), 5);
    }

    #[test]
    fn headers_are_stored_encrypted() {
        let s = Store::open_in_memory().unwrap();
        let mut n = nd(Priority::Normal);
        n.headers_json = Some(r#"{"Cookie":"sid=abc"}"#.into());
        let id = s.insert_download(&n).unwrap();
        let (plain, blob): (Option<String>, Option<Vec<u8>>) = s
            .lock()
            .query_row("SELECT headers_json, headers_blob FROM downloads WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(plain, None);
        let blob = blob.expect("a blob");
        assert!(!blob.windows(7).any(|w| w == b"sid=abc"));
        assert_eq!(s.get_download(id).unwrap().headers_json.as_deref(), Some(r#"{"Cookie":"sid=abc"}"#));
    }

    #[test]
    fn headers_wiped_on_complete() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert_download(&nd(Priority::Normal)).unwrap();
        s.set_completed(id, std::path::Path::new(r"C:\dl\a.bin")).unwrap();
        assert_eq!(s.get_download(id).unwrap().headers_json, None);
    }

    #[test]
    fn headers_wiped_on_failed_kept_on_pause() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert_download(&nd(Priority::Normal)).unwrap();
        for st in [DownloadState::Paused, DownloadState::LinkExpired, DownloadState::Offline] {
            s.set_state(id, st, None).unwrap();
            assert!(s.get_download(id).unwrap().headers_json.is_some(), "{st:?} keeps headers");
        }
        s.set_state(id, DownloadState::Failed, Some("boom")).unwrap();
        assert_eq!(s.get_download(id).unwrap().headers_json, None);
    }

    #[test]
    fn refresh_headers_replaces() {
        let s = Store::open_in_memory().unwrap();
        let id = s.insert_download(&nd(Priority::Normal)).unwrap();
        s.refresh_headers(id, Some(r#"{"Cookie":"new"}"#)).unwrap();
        assert_eq!(s.get_download(id).unwrap().headers_json.as_deref(), Some(r#"{"Cookie":"new"}"#));
        s.refresh_headers(id, None).unwrap();
        assert_eq!(s.get_download(id).unwrap().headers_json, None);
    }

    #[test]
    fn find_by_url_newest_first() {
        let s = Store::open_in_memory().unwrap();
        let a = s.insert_download(&nd(Priority::Normal)).unwrap();
        let b = s.insert_download(&nd(Priority::Normal)).unwrap();
        let mut other = nd(Priority::Normal);
        other.url = "https://example.org/other.bin".into();
        s.insert_download(&other).unwrap();
        let ids: Vec<_> = s.find_by_url("https://example.org/a.bin").unwrap().iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![b, a]);
    }
}
