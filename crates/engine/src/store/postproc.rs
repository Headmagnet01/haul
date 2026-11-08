//! Post-processing runs, saved archive passwords and sort rules (migration 4).

use super::Store;
use crate::error::EngineError;
use crate::types::{DownloadId, DownloadState, PackageId};
use rusqlite::{OptionalExtension, params};

/// One post-processing run as stored. `steps_json` is owned by `postproc`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRow {
    pub id: i64,
    pub package_id: Option<PackageId>,
    pub download_id: Option<DownloadId>,
    pub state: String,
    pub steps_json: String,
    pub created_at: String,
    pub updated_at: String,
}

const RUN_COLS: &str = "id, package_id, download_id, state, steps_json, created_at, updated_at";

fn row_to_run(r: &rusqlite::Row) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        id: r.get(0)?,
        package_id: r.get(1)?,
        download_id: r.get(2)?,
        state: r.get(3)?,
        steps_json: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

/// (enabled, name, match_json, action_json), in order.
pub type RuleRowIn = (bool, String, String, String);

/// (id, enabled, name, match_json, action_json).
pub type RuleRowOut = (i64, bool, String, String, String);

impl Store {
    /// Back to the queue from zero: no saved path, no error, no segments.
    pub fn reset_download(&self, id: DownloadId) -> Result<(), EngineError> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        tx.execute(
            "UPDATE downloads SET state = ?2, error = NULL, target_path = NULL, finished_at = NULL WHERE id = ?1",
            params![id, DownloadState::Queued.as_str()],
        )?;
        tx.execute("DELETE FROM segments WHERE download_id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_expected_hash(&self, id: DownloadId, h: Option<&str>) -> Result<(), EngineError> {
        self.lock().execute("UPDATE downloads SET expected_hash = ?2 WHERE id = ?1", params![id, h])?;
        Ok(())
    }

    /// Adds one to the re-download count and returns the new count.
    pub fn bump_verify_retries(&self, id: DownloadId) -> Result<u32, EngineError> {
        let c = self.lock();
        c.execute("UPDATE downloads SET verify_retries = verify_retries + 1 WHERE id = ?1", [id])?;
        Ok(c.query_row("SELECT verify_retries FROM downloads WHERE id = ?1", [id], |r| r.get::<_, i64>(0))? as u32)
    }

    /// The run for this package (or lone download), made if missing. `true` when it is new.
    pub fn ensure_run(
        &self,
        package_id: Option<PackageId>,
        download_id: Option<DownloadId>,
        state: &str,
        steps_json: &str,
    ) -> Result<(i64, bool), EngineError> {
        let c = self.lock();
        let existing: Option<i64> = c
            .query_row(
                "SELECT id FROM postproc_runs WHERE package_id IS ?1 AND download_id IS ?2",
                params![package_id, download_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok((id, false));
        }
        c.execute(
            "INSERT INTO postproc_runs (package_id, download_id, state, steps_json) VALUES (?1, ?2, ?3, ?4)",
            params![package_id, download_id, state, steps_json],
        )?;
        Ok((c.last_insert_rowid(), true))
    }

    pub fn get_run(&self, id: i64) -> Result<RunRow, EngineError> {
        Ok(self.lock().query_row(&format!("SELECT {RUN_COLS} FROM postproc_runs WHERE id = ?1"), [id], row_to_run)?)
    }

    pub fn list_runs(&self) -> Result<Vec<RunRow>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare(&format!("SELECT {RUN_COLS} FROM postproc_runs ORDER BY id"))?;
        Ok(st.query_map([], row_to_run)?.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn save_run(&self, id: i64, state: &str, steps_json: &str) -> Result<(), EngineError> {
        self.lock().execute(
            "UPDATE postproc_runs SET state = ?2, steps_json = ?3, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?1",
            params![id, state, steps_json],
        )?;
        Ok(())
    }

    /// Appends an encrypted password at the end of the list.
    pub fn add_password(&self, blob: &[u8]) -> Result<i64, EngineError> {
        let c = self.lock();
        c.execute(
            "INSERT INTO passwords (position, blob) VALUES ((SELECT COALESCE(MAX(position), -1) + 1 FROM passwords), ?1)",
            [blob],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// (id, encrypted blob) in order.
    pub fn list_passwords(&self) -> Result<Vec<(i64, Vec<u8>)>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare("SELECT id, blob FROM passwords ORDER BY position, id")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_password(&self, id: i64) -> Result<(), EngineError> {
        self.lock().execute("DELETE FROM passwords WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Positions 0..n in the order of `ids`.
    pub fn order_passwords(&self, ids: &[i64]) -> Result<(), EngineError> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute("UPDATE passwords SET position = ?2 WHERE id = ?1", params![id, i as i64])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// (id, enabled, name, match_json, action_json), in order.
    pub fn get_rules_json(&self) -> Result<Vec<RuleRowOut>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare("SELECT id, enabled, name, match_json, action_json FROM rules ORDER BY position, id")?;
        Ok(st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// Replaces every rule in one transaction.
    pub fn set_rules_json(&self, rows: &[RuleRowIn]) -> Result<(), EngineError> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM rules", [])?;
        for (i, (enabled, name, m, a)) in rows.iter().enumerate() {
            tx.execute(
                "INSERT INTO rules (position, enabled, name, match_json, action_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![i as i64, enabled, name, m, a],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::segmenter::Range;
    use crate::store::{NewDownload, Store};
    use crate::types::{DownloadState, Priority, ProbeInfo};
    use std::path::PathBuf;

    fn new_dl(s: &Store, pkg: Option<i64>) -> i64 {
        s.insert_download(&NewDownload {
            package_id: pkg,
            url: "https://example.org/a.bin".into(),
            headers_json: None,
            target_dir: PathBuf::from(r"C:\dl"),
            file_name: None,
            priority: Priority::Normal,
            connections: 8,
            speed_limit: None,
        })
        .unwrap()
    }

    #[test]
    fn schema_version_is_latest() {
        assert_eq!(Store::open_in_memory().unwrap().schema_version().unwrap(), crate::store::SCHEMA_VERSION);
    }

    #[test]
    fn migrates_v3_to_v4() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("haul.db");
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch(&format!(
                "BEGIN; {} {} {} PRAGMA user_version = 3; COMMIT;",
                include_str!("schema.sql"),
                include_str!("migration_2.sql"),
                include_str!("migration_3.sql")
            ))
            .unwrap();
            c.execute(
                "INSERT INTO downloads (url, position, target_dir, file_name) VALUES ('https://x.org/a', 1, 'C:\\dl', 'a')",
                [],
            )
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.schema_version().unwrap(), crate::store::SCHEMA_VERSION);
        let d = &s.list_downloads().unwrap()[0];
        assert_eq!(d.file_name.as_deref(), Some("a"));
        assert_eq!(d.verify_retries, 0);
        assert_eq!(d.expected_hash, None);
        assert_eq!(d.server_digest, None);
    }

    #[test]
    fn old_probe_json_without_digest_parses() {
        let s = Store::open_in_memory().unwrap();
        let b = s.create_batch("paste", "x").unwrap();
        let l = s.insert_link(b, "https://x.org/a.bin").unwrap();
        s.lock()
            .execute(
                "UPDATE collector_links SET status = 'online', probe_json = ?2 WHERE id = ?1",
                rusqlite::params![
                    l,
                    r#"{"final_url":"https://x.org/a.bin","size":5,"resumable":true,"file_name":"a.bin","etag":null,"last_modified":null,"content_type":null}"#
                ],
            )
            .unwrap();
        let links = s.list_links().unwrap();
        let p = links[0].probe.as_ref().expect("probe parses");
        assert_eq!(p.size, Some(5));
        assert_eq!(p.digest, None);
    }

    #[test]
    fn set_probe_stores_digest() {
        let s = Store::open_in_memory().unwrap();
        let id = new_dl(&s, None);
        let p = ProbeInfo {
            final_url: "https://x.org/a.bin".into(),
            file_name: "a.bin".into(),
            digest: Some("sha256:ab".into()),
            ..Default::default()
        };
        s.set_probe(id, &p).unwrap();
        let d = s.get_download(id).unwrap();
        assert_eq!(d.server_digest.as_deref(), Some("sha256:ab"));
        assert_eq!(d.probe.unwrap().digest.as_deref(), Some("sha256:ab"));
    }

    #[test]
    fn reset_download_requeues_from_zero() {
        let s = Store::open_in_memory().unwrap();
        let id = new_dl(&s, None);
        s.save_segments(id, &[Range { start: 0, pos: 10, end: 100 }]).unwrap();
        s.set_completed(id, std::path::Path::new(r"C:\dl\a.bin")).unwrap();
        s.reset_download(id).unwrap();
        let d = s.get_download(id).unwrap();
        assert_eq!(d.state, DownloadState::Queued);
        assert_eq!(d.target_path, None);
        assert!(s.load_segments(id).unwrap().is_empty());
    }

    #[test]
    fn expected_hash_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        let id = new_dl(&s, None);
        s.set_expected_hash(id, Some("md5:00")).unwrap();
        assert_eq!(s.get_download(id).unwrap().expected_hash.as_deref(), Some("md5:00"));
        s.set_expected_hash(id, None).unwrap();
        assert_eq!(s.get_download(id).unwrap().expected_hash, None);
    }

    #[test]
    fn bump_verify_retries_counts() {
        let s = Store::open_in_memory().unwrap();
        let id = new_dl(&s, None);
        assert_eq!(s.bump_verify_retries(id).unwrap(), 1);
        assert_eq!(s.bump_verify_retries(id).unwrap(), 2);
        assert_eq!(s.get_download(id).unwrap().verify_retries, 2);
    }

    #[test]
    fn ensure_run_is_idempotent() {
        let s = Store::open_in_memory().unwrap();
        let pkg = s.create_package("p", "paste").unwrap();
        let (a, new_a) = s.ensure_run(Some(pkg), None, "queued", "{}").unwrap();
        let (b, new_b) = s.ensure_run(Some(pkg), None, "held", "{}").unwrap();
        assert_eq!(a, b);
        assert!(new_a);
        assert!(!new_b);
        assert_eq!(s.get_run(a).unwrap().state, "queued");
    }

    #[test]
    fn run_roundtrip_and_cascade() {
        let s = Store::open_in_memory().unwrap();
        let id = new_dl(&s, None);
        let (r, _) = s.ensure_run(None, Some(id), "queued", "{}").unwrap();
        s.save_run(r, "done", r#"{"steps":[]}"#).unwrap();
        let row = s.get_run(r).unwrap();
        assert_eq!(row.state, "done");
        assert_eq!(row.steps_json, r#"{"steps":[]}"#);
        assert_eq!(row.download_id, Some(id));
        assert_eq!(s.list_runs().unwrap().len(), 1);
        s.delete_download(id).unwrap();
        assert!(s.list_runs().unwrap().is_empty());
    }

    #[test]
    fn passwords_order() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_password(b"a").unwrap();
        let b = s.add_password(b"b").unwrap();
        let c = s.add_password(b"c").unwrap();
        let blobs = |s: &Store| s.list_passwords().unwrap().into_iter().map(|(_, b)| b).collect::<Vec<_>>();
        assert_eq!(blobs(&s), vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
        s.order_passwords(&[c, a, b]).unwrap();
        assert_eq!(blobs(&s), vec![b"c".to_vec(), b"a".to_vec(), b"b".to_vec()]);
        s.delete_password(b).unwrap();
        assert_eq!(s.list_passwords().unwrap().into_iter().map(|(i, _)| i).collect::<Vec<_>>(), vec![c, a]);
    }

    #[test]
    fn rules_replace_all() {
        let s = Store::open_in_memory().unwrap();
        s.set_rules_json(&[
            (true, "a".into(), "{}".into(), "{}".into()),
            (false, "b".into(), "{}".into(), "{}".into()),
        ])
        .unwrap();
        s.set_rules_json(&[(true, "c".into(), "{\"x\":1}".into(), "{}".into())]).unwrap();
        let rows = s.get_rules_json().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].1, rows[0].2.as_str(), rows[0].3.as_str()), (true, "c", "{\"x\":1}"));
    }
}
