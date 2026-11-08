//! Browser extensions paired with Haul. Only a hash of each one's key is stored.

use super::Store;
use crate::error::EngineError;
use rusqlite::{OptionalExtension, Row, params};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedClient {
    pub id: i64,
    pub browser: String,
    /// The extension's `chrome-extension://<id>` origin, bound at pairing.
    pub origin: String,
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub created_at: String,
    pub last_seen: Option<String>,
}

const COLS: &str = "id, browser, origin, created_at, last_seen";

fn row(r: &Row) -> rusqlite::Result<PairedClient> {
    Ok(PairedClient {
        id: r.get(0)?,
        browser: r.get(1)?,
        origin: r.get(2)?,
        created_at: r.get(3)?,
        last_seen: r.get(4)?,
    })
}

impl Store {
    pub fn add_paired_client(&self, browser: &str, origin: &str, token_hash: &str) -> Result<i64, EngineError> {
        let c = self.lock();
        c.execute(
            "INSERT INTO paired_clients (browser, origin, token_hash) VALUES (?1, ?2, ?3)",
            params![browser, origin, token_hash],
        )?;
        Ok(c.last_insert_rowid())
    }

    pub fn paired_client_by_hash(&self, token_hash: &str) -> Result<Option<PairedClient>, EngineError> {
        Ok(self
            .lock()
            .query_row(&format!("SELECT {COLS} FROM paired_clients WHERE token_hash = ?1"), [token_hash], row)
            .optional()?)
    }

    pub fn touch_paired_client(&self, id: i64) -> Result<(), EngineError> {
        self.lock().execute(
            "UPDATE paired_clients SET last_seen = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    /// Oldest first.
    pub fn list_paired_clients(&self) -> Result<Vec<PairedClient>, EngineError> {
        let c = self.lock();
        let mut st = c.prepare(&format!("SELECT {COLS} FROM paired_clients ORDER BY id"))?;
        let rows = st.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn remove_paired_client(&self, id: i64) -> Result<(), EngineError> {
        self.lock().execute("DELETE FROM paired_clients WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::store::Store;

    #[test]
    fn add_then_find_by_hash() {
        let s = Store::open_in_memory().unwrap();
        let id = s.add_paired_client("Microsoft Edge", "chrome-extension://abc", "h1").unwrap();
        let c = s.paired_client_by_hash("h1").unwrap().expect("found");
        assert_eq!(c.id, id);
        assert_eq!(c.browser, "Microsoft Edge");
        assert_eq!(c.origin, "chrome-extension://abc");
        assert_eq!(c.last_seen, None);
        assert!(!c.created_at.is_empty());
    }

    #[test]
    fn unknown_hash_is_none() {
        let s = Store::open_in_memory().unwrap();
        s.add_paired_client("Google Chrome", "chrome-extension://abc", "h1").unwrap();
        assert!(s.paired_client_by_hash("nope").unwrap().is_none());
    }

    #[test]
    fn touch_sets_last_seen() {
        let s = Store::open_in_memory().unwrap();
        let id = s.add_paired_client("Google Chrome", "chrome-extension://abc", "h1").unwrap();
        s.touch_paired_client(id).unwrap();
        assert!(s.paired_client_by_hash("h1").unwrap().unwrap().last_seen.is_some());
    }

    #[test]
    fn remove_forgets_client() {
        let s = Store::open_in_memory().unwrap();
        let id = s.add_paired_client("Google Chrome", "chrome-extension://abc", "h1").unwrap();
        s.remove_paired_client(id).unwrap();
        assert!(s.paired_client_by_hash("h1").unwrap().is_none());
        assert!(s.list_paired_clients().unwrap().is_empty());
    }

    #[test]
    fn list_is_oldest_first() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_paired_client("A", "chrome-extension://a", "ha").unwrap();
        let b = s.add_paired_client("B", "chrome-extension://b", "hb").unwrap();
        let ids: Vec<_> = s.list_paired_clients().unwrap().iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![a, b]);
    }
}
