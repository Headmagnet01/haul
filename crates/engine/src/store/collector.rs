//! Collector persistence: batches of incoming links, their probe results, and the Queue
//! package each started collector package became.

use super::Store;
use crate::error::EngineError;
use crate::types::{LinkStatus, PackageId, ProbeInfo};
use rusqlite::{OptionalExtension, params};

/// One intake: everything from one paste, one drop or one clipboard change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub id: i64,
    pub source: String,
    /// The fallback package name, e.g. "Pasted list · 5 Nov 18:40".
    pub label: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollectorLink {
    pub id: i64,
    pub batch_id: i64,
    pub url: String,
    pub status: LinkStatus,
    pub probe: Option<ProbeInfo>,
    pub message: Option<String>,
    pub created_at: String,
}

impl Store {
    pub fn create_batch(&self, source: &str, label: &str) -> Result<i64, EngineError> {
        let c = self.lock();
        c.execute("INSERT INTO collector_batches (source, label) VALUES (?1, ?2)", params![source, label])?;
        Ok(c.last_insert_rowid())
    }

    /// Adds a link in the `checking` state.
    pub fn insert_link(&self, batch_id: i64, url: &str) -> Result<i64, EngineError> {
        let c = self.lock();
        c.execute("INSERT INTO collector_links (batch_id, url) VALUES (?1, ?2)", params![batch_id, url])?;
        Ok(c.last_insert_rowid())
    }

    /// Adds links to a batch in one transaction, all `checking`; returns their ids in order.
    pub fn insert_links(&self, batch_id: i64, urls: &[String]) -> Result<Vec<i64>, EngineError> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        let mut ids = Vec::with_capacity(urls.len());
        {
            let mut st = tx.prepare("INSERT INTO collector_links (batch_id, url) VALUES (?1, ?2)")?;
            for u in urls {
                st.execute(params![batch_id, u])?;
                ids.push(tx.last_insert_rowid());
            }
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Stores a link's request headers sealed for the current Windows user (`None` clears them).
    pub fn set_link_headers(&self, id: i64, headers_json: Option<&str>) -> Result<(), EngineError> {
        let blob = super::seal_headers(headers_json)?;
        self.lock().execute("UPDATE collector_links SET headers_blob = ?2 WHERE id = ?1", params![id, blob])?;
        Ok(())
    }

    /// A link's request headers; a missing link, no headers or a blob that can't be unsealed is `None`.
    /// True while the link is still in the Collector.
    pub fn link_exists(&self, id: i64) -> Result<bool, EngineError> {
        let n: i64 = self.lock().query_row("SELECT COUNT(*) FROM collector_links WHERE id = ?1", [id], |r| r.get(0))?;
        Ok(n > 0)
    }

    pub fn link_headers(&self, id: i64) -> Result<Option<String>, EngineError> {
        let blob: Option<Vec<u8>> = self
            .lock()
            .query_row("SELECT headers_blob FROM collector_links WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .flatten();
        Ok(super::open_headers(blob))
    }

    /// The id of a waiting link with exactly this URL.
    pub fn find_link_by_url(&self, url: &str) -> Result<Option<i64>, EngineError> {
        Ok(self
            .lock()
            .query_row("SELECT id FROM collector_links WHERE url = ?1 ORDER BY id LIMIT 1", [url], |r| r.get(0))
            .optional()?)
    }

    pub fn list_batches(&self) -> Result<Vec<Batch>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare("SELECT id, source, label, created_at FROM collector_batches ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(Batch { id: r.get(0)?, source: r.get(1)?, label: r.get(2)?, created_at: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn list_links(&self) -> Result<Vec<CollectorLink>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare(
            "SELECT id, batch_id, url, status, probe_json, message, created_at FROM collector_links ORDER BY id",
        )?;
        let rows = st.query_map([], |r| {
            let status: String = r.get(3)?;
            let probe: Option<String> = r.get(4)?;
            Ok(CollectorLink {
                id: r.get(0)?,
                batch_id: r.get(1)?,
                url: r.get(2)?,
                status: LinkStatus::parse(&status).unwrap_or(LinkStatus::Error),
                probe: probe.and_then(|j| serde_json::from_str(&j).ok()),
                message: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stores a check result. A link that has since been removed or started is left alone.
    pub fn set_link_probe(
        &self,
        id: i64,
        status: LinkStatus,
        probe: Option<&ProbeInfo>,
        message: Option<&str>,
    ) -> Result<(), EngineError> {
        let json = probe
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| EngineError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        self.lock().execute(
            "UPDATE collector_links SET status = ?2, probe_json = ?3, message = ?4 WHERE id = ?1",
            params![id, status.as_str(), json, message],
        )?;
        Ok(())
    }

    /// Deletes links, then any batch left with none.
    pub fn delete_links(&self, ids: &[i64]) -> Result<(), EngineError> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        {
            let mut st = tx.prepare("DELETE FROM collector_links WHERE id = ?1")?;
            for id in ids {
                st.execute([id])?;
            }
        }
        tx.execute(
            "DELETE FROM collector_batches WHERE id NOT IN (SELECT DISTINCT batch_id FROM collector_links)",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn group_package(&self, batch_id: i64, group_key: &str) -> Result<Option<PackageId>, EngineError> {
        Ok(self
            .lock()
            .query_row(
                "SELECT package_id FROM collector_groups WHERE batch_id = ?1 AND group_key = ?2",
                params![batch_id, group_key],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The Queue package an archive set became, while that package still has downloads.
    pub fn archive_package(&self, group_key: &str) -> Result<Option<PackageId>, EngineError> {
        Ok(self
            .lock()
            .query_row(
                "SELECT a.package_id FROM collector_archives a WHERE a.group_key = ?1 \
                 AND EXISTS (SELECT 1 FROM downloads d WHERE d.package_id = a.package_id)",
                params![group_key],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_archive_package(&self, group_key: &str, package_id: PackageId) -> Result<(), EngineError> {
        self.lock().execute(
            "INSERT INTO collector_archives (group_key, package_id) VALUES (?1, ?2) \
             ON CONFLICT(group_key) DO UPDATE SET package_id = excluded.package_id",
            params![group_key, package_id],
        )?;
        Ok(())
    }

    pub fn set_group_package(&self, batch_id: i64, group_key: &str, package_id: PackageId) -> Result<(), EngineError> {
        self.lock().execute(
            "INSERT INTO collector_groups (batch_id, group_key, package_id) VALUES (?1, ?2, ?3) \
             ON CONFLICT(batch_id, group_key) DO UPDATE SET package_id = excluded.package_id",
            params![batch_id, group_key, package_id],
        )?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use crate::store::{NewDownload, Store};
    use crate::types::{LinkStatus, Priority, ProbeInfo};
    use std::path::PathBuf;

    fn probe() -> ProbeInfo {
        ProbeInfo {
            final_url: "https://cdn.example.org/a.bin".into(),
            size: Some(1234),
            resumable: true,
            file_name: "a.bin".into(),
            etag: None,
            last_modified: None,
            content_type: Some("application/octet-stream".into()),
            digest: None,
        }
    }

    #[test]
    fn migrates_v1_to_latest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("haul.db");
        // A database that only ever saw migration 1, holding a download.
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch(&format!("BEGIN; {} PRAGMA user_version = 1; COMMIT;", include_str!("schema.sql")))
                .unwrap();
            c.execute(
                "INSERT INTO downloads (url, position, target_dir) VALUES ('https://example.org/a.bin', 1, 'C:/dl')",
                [],
            )
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.schema_version().unwrap(), crate::store::SCHEMA_VERSION);
        assert_eq!(s.list_downloads().unwrap()[0].url, "https://example.org/a.bin");
        s.create_batch("paste", "x").unwrap();
    }

    #[test]
    fn link_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        let b = s.create_batch("paste", "Pasted list · 5 Nov 18:40").unwrap();
        let id = s.insert_link(b, "https://example.org/a.bin").unwrap();
        let batches = s.list_batches().unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].id, b);
        assert_eq!(batches[0].source, "paste");
        assert_eq!(batches[0].label, "Pasted list · 5 Nov 18:40");
        let l = &s.list_links().unwrap()[0];
        assert_eq!((l.id, l.batch_id, l.url.as_str()), (id, b, "https://example.org/a.bin"));
        assert_eq!(l.status, LinkStatus::Checking);
        assert_eq!(l.probe, None);
        s.set_link_probe(id, LinkStatus::Online, Some(&probe()), None).unwrap();
        let l = &s.list_links().unwrap()[0];
        assert_eq!(l.status, LinkStatus::Online);
        assert_eq!(l.probe, Some(probe()));
        s.set_link_probe(id, LinkStatus::Offline, None, Some("404")).unwrap();
        let l = &s.list_links().unwrap()[0];
        assert_eq!((l.status, l.probe.clone(), l.message.as_deref()), (LinkStatus::Offline, None, Some("404")));
    }

    #[test]
    fn set_probe_on_missing_link_is_noop() {
        let s = Store::open_in_memory().unwrap();
        s.set_link_probe(999, LinkStatus::Online, None, None).unwrap();
    }

    #[test]
    fn delete_links_drops_empty_batch() {
        let s = Store::open_in_memory().unwrap();
        let b = s.create_batch("paste", "x").unwrap();
        let l1 = s.insert_link(b, "https://a.org/1").unwrap();
        let l2 = s.insert_link(b, "https://a.org/2").unwrap();
        s.delete_links(&[l1]).unwrap();
        assert_eq!(s.list_batches().unwrap().len(), 1);
        assert_eq!(s.list_links().unwrap().len(), 1);
        s.delete_links(&[l2]).unwrap();
        assert!(s.list_batches().unwrap().is_empty());
        assert!(s.list_links().unwrap().is_empty());
    }

    #[test]
    fn group_package_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        let b = s.create_batch("paste", "x").unwrap();
        let pkg = s.create_package("m", "paste").unwrap();
        assert_eq!(s.group_package(b, "rest").unwrap(), None);
        s.set_group_package(b, "rest", pkg).unwrap();
        assert_eq!(s.group_package(b, "rest").unwrap(), Some(pkg));
        assert_eq!(s.group_package(b, "7z:m").unwrap(), None);
    }

    #[test]
    fn archive_package_needs_live_downloads() {
        let s = Store::open_in_memory().unwrap();
        let pkg = s.create_package("m", "paste").unwrap();
        assert_eq!(s.archive_package("rar:m").unwrap(), None);
        s.set_archive_package("rar:m", pkg).unwrap();
        assert_eq!(s.archive_package("rar:m").unwrap(), None, "a package with no downloads is not reused");
        s.insert_download(&NewDownload {
            package_id: Some(pkg),
            url: "https://a.org/m.part1.rar".into(),
            headers_json: None,
            target_dir: PathBuf::from(r"C:\dl"),
            file_name: None,
            priority: Priority::Normal,
            connections: 8,
            speed_limit: None,
        })
        .unwrap();
        assert_eq!(s.archive_package("rar:m").unwrap(), Some(pkg));
    }

    #[test]
    fn link_status_strings_roundtrip() {
        for s in [LinkStatus::Checking, LinkStatus::Online, LinkStatus::Offline, LinkStatus::Expired, LinkStatus::Error]
        {
            assert_eq!(LinkStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(LinkStatus::parse("nope"), None);
    }
}
