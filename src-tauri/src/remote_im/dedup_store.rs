//! Cross-restart message deduplication backed by SQLite.
//!
//! Uses `INSERT OR IGNORE` on a composite primary key `(channel, message_id)`
//! to atomically detect duplicates. Records older than [`DEDUP_TTL_SECS`] are
//! periodically purged to keep the table bounded.
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const DEDUP_TTL_SECS: u64 = 7 * 24 * 3600;
const DEDUP_CLEANUP_INTERVAL: u64 = 1024;

pub struct DedupStore {
    conn: Arc<Mutex<Connection>>,
    insert_count: AtomicU64,
}

impl DedupStore {
    pub fn open_default() -> Self {
        let path = crate::paths::app_data_root()
            .join("remote")
            .join("dedup.sqlite");
        Self::open(path)
    }

    pub fn open(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path).expect("open dedup db");
        Self::init(&conn);
        Self {
            conn: Arc::new(Mutex::new(conn)),
            insert_count: AtomicU64::new(0),
        }
    }

    #[allow(dead_code)] // used via Engine::new_ephemeral (test path)
    pub fn ephemeral() -> Self {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        Self::init(&conn);
        Self {
            conn: Arc::new(Mutex::new(conn)),
            insert_count: AtomicU64::new(0),
        }
    }

    fn init(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS seen_messages (
                channel    TEXT NOT NULL,
                message_id TEXT NOT NULL,
                seen_ts    INTEGER NOT NULL,
                PRIMARY KEY (channel, message_id)
            );
            CREATE INDEX IF NOT EXISTS idx_seen_ts ON seen_messages(seen_ts);",
        )
        .expect("init dedup schema");
    }

    /// Returns `true` if this is a new message (pass through),
    /// `false` if it is a duplicate (drop).
    pub fn check_and_mark(&self, channel: &str, message_id: &str) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let changed = {
            let conn = self.conn.lock();
            conn.execute(
                "INSERT OR IGNORE INTO seen_messages (channel, message_id, seen_ts) VALUES (?1, ?2, ?3)",
                rusqlite::params![channel, message_id, now],
            )
            .expect("dedup insert")
        };
        let is_new = changed == 1;
        if is_new {
            let n = self.insert_count.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(DEDUP_CLEANUP_INTERVAL) {
                self.cleanup_locked(now);
            }
        }
        is_new
    }

    fn cleanup_locked(&self, now: u64) {
        let cutoff = now.saturating_sub(DEDUP_TTL_SECS);
        let conn = self.conn.lock();
        let removed = conn
            .execute(
                "DELETE FROM seen_messages WHERE seen_ts < ?1",
                rusqlite::params![cutoff],
            )
            .unwrap_or(0);
        if removed > 0 {
            tracing::debug!(target: "remote_im::dedup", removed, "ttl cleanup ran");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_message_passes() {
        let store = DedupStore::ephemeral();
        assert!(store.check_and_mark("telegram", "m1"));
    }

    #[test]
    fn test_duplicate_dropped() {
        let store = DedupStore::ephemeral();
        assert!(store.check_and_mark("telegram", "m1"));
        assert!(!store.check_and_mark("telegram", "m1"));
    }

    #[test]
    fn test_different_channel_no_collision() {
        let store = DedupStore::ephemeral();
        assert!(store.check_and_mark("telegram", "m1"));
        assert!(store.check_and_mark("discord", "m1"));
    }

    #[test]
    fn test_ephemeral_no_file() {
        // ephemeral uses :memory:; just exercise it without panic or file creation.
        let store = DedupStore::ephemeral();
        assert!(store.check_and_mark("slack", "m1"));
    }

    #[test]
    fn test_ttl_cleanup() {
        let store = DedupStore::ephemeral();
        // Insert a record with an old timestamp manually.
        {
            let conn = store.conn.lock();
            conn.execute(
                "INSERT INTO seen_messages (channel, message_id, seen_ts) VALUES (?1, ?2, ?3)",
                rusqlite::params!["telegram", "old", 1_u64],
            )
            .unwrap();
        }
        // Trigger cleanup by forcing insert_count to just-before-threshold.
        store
            .insert_count
            .store(DEDUP_CLEANUP_INTERVAL - 1, Ordering::Relaxed);
        // This insert triggers cleanup (old record deleted), then inserts new.
        assert!(store.check_and_mark("telegram", "fresh"));
        // The "old" record should be gone now: re-marking it should succeed.
        assert!(store.check_and_mark("telegram", "old"));
    }

    #[test]
    fn test_persistence_across_reopen() {
        let path = std::env::temp_dir().join(format!(
            "omp-dedup-test-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let store = DedupStore::open(path.clone());
            assert!(store.check_and_mark("telegram", "persist-me"));
        }
        // Reopen same path — the record must still be known.
        let store2 = DedupStore::open(path.clone());
        assert!(!store2.check_and_mark("telegram", "persist-me"));
        let _ = std::fs::remove_file(&path);
    }
}
