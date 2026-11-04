//! ZDM SQLite Persistence
//! Developed by Ramy Badraan
//!
//! Stores download rows, per-segment resume offsets and user settings.
//! All writes are short and synchronous: SQLite in WAL mode with a busy
//! timeout so a burst of progress updates can never deadlock the UI.

use crate::engine::types::{DownloadItem, DownloadStatus, SegmentProgress, Settings};
use rusqlite::{params, Connection, OptionalExtension, Result};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

/// Columns added to a table after the first release shipped.
///
/// `CREATE TABLE IF NOT EXISTS` silently does nothing when the table already
/// exists, so a database created by an older build keeps its original column
/// set forever. The first query that names a newer column then fails — which is
/// how an upgraded install died at startup with
/// `no such column: queue_position`. Every later addition belongs in this list.
const ADDED_COLUMNS: &[(&str, &str, &str)] = &[(
    "downloads",
    "queue_position",
    "INTEGER NOT NULL DEFAULT 0",
)];

/// Bumped whenever `ADDED_COLUMNS` grows, so a future migration can branch on it.
const SCHEMA_VERSION: i64 = 1;

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        // WAL keeps readers (the UI) from blocking writers (the engine).
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                filename TEXT NOT NULL,
                save_path TEXT NOT NULL,
                total_size INTEGER NOT NULL,
                downloaded_size INTEGER NOT NULL,
                status TEXT NOT NULL,
                category TEXT NOT NULL,
                supports_range INTEGER NOT NULL,
                etag TEXT,
                referer TEXT,
                connections INTEGER NOT NULL DEFAULT 8,
                created_at INTEGER NOT NULL,
                queue_position INTEGER NOT NULL DEFAULT 0,
                finished_at INTEGER,
                error_message TEXT
            );

            CREATE TABLE IF NOT EXISTS segments (
                download_id TEXT NOT NULL,
                segment_index INTEGER NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                downloaded_bytes INTEGER NOT NULL,
                is_finished INTEGER NOT NULL,
                PRIMARY KEY (download_id, segment_index)
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;

        // Must run before the indexes below: an index over a column that an
        // older database does not have yet aborts the whole batch.
        Self::migrate(&conn)?;

        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
            CREATE INDEX IF NOT EXISTS idx_downloads_queue ON downloads(queue_position);
            CREATE INDEX IF NOT EXISTS idx_segments_download ON segments(download_id);
            ",
        )?;

        let _ = conn.pragma_update(None, "user_version", SCHEMA_VERSION);
        Ok(())
    }

    /// Bring a database created by an older build up to the current schema.
    ///
    /// Adding a column is additive and safe to repeat: each column is checked
    /// individually, so a half-migrated file simply finishes on the next run.
    fn migrate(conn: &Connection) -> Result<()> {
        let mut added_queue_position = false;

        for (table, column, definition) in ADDED_COLUMNS {
            if Self::has_column(conn, table, column)? {
                continue;
            }
            conn.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition};"
            ))?;
            if *table == "downloads" && *column == "queue_position" {
                added_queue_position = true;
            }
        }

        // Rows from an older build all share the default position, so "move up"
        // would be a no-op and the queue order arbitrary. Give them dense
        // positions in creation order, once.
        if added_queue_position {
            Self::backfill_queue_positions(conn)?;
        }

        Ok(())
    }

    fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn backfill_queue_positions(conn: &Connection) -> Result<()> {
        let ids: Vec<String> = {
            let mut stmt =
                conn.prepare("SELECT id FROM downloads ORDER BY created_at ASC, rowid ASC")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>>>()?
        };

        for (position, id) in ids.iter().enumerate() {
            conn.execute(
                "UPDATE downloads SET queue_position = ?1 WHERE id = ?2",
                params![position as i64, id],
            )?;
        }
        Ok(())
    }

    pub fn insert_download(&self, item: &DownloadItem) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT OR REPLACE INTO downloads (
                id, url, filename, save_path, total_size, downloaded_size,
                status, category, supports_range, etag, referer, connections,
                created_at, finished_at, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                item.id,
                item.url,
                item.filename,
                item.save_path,
                item.total_size as i64,
                item.downloaded_size as i64,
                item.status.as_str(),
                item.category,
                if item.supports_range { 1 } else { 0 },
                item.etag,
                item.referer,
                item.connections as i64,
                item.created_at,
                item.finished_at,
                item.error_message,
            ],
        )?;

        tx.execute("DELETE FROM segments WHERE download_id = ?1", params![item.id])?;
        for seg in &item.segments {
            tx.execute(
                "INSERT INTO segments (
                    download_id, segment_index, start_byte, end_byte, downloaded_bytes, is_finished
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    item.id,
                    seg.index as i64,
                    seg.start_byte as i64,
                    seg.end_byte as i64,
                    seg.downloaded_bytes as i64,
                    if seg.is_finished { 1 } else { 0 }
                ],
            )?;
        }

        tx.commit()
    }

    /// Progress + status in a single statement so a pause can never be
    /// overwritten by a stale "downloading" tick.
    /// Record a total size discovered after the row was created.
    ///
    /// A torrent's size is unknown until its metadata resolves, so the row starts
    /// at zero and is corrected here.
    pub fn update_total_size(&self, id: &str, total: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET total_size = ?1 WHERE id = ?2",
            params![total as i64, id],
        )?;
        Ok(())
    }

    pub fn update_download_url(&self, id: &str, new_url: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET url = ?1 WHERE id = ?2",
            params![new_url, id],
        )?;
        Ok(())
    }

    pub fn reset_download_progress(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET downloaded_size = 0, status = 'downloading', error_message = NULL WHERE id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM segments WHERE download_id = ?1", params![id])?;
        Ok(())
    }


    pub fn update_progress(&self, id: &str, downloaded: u64, status: &DownloadStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET downloaded_size = ?1, status = ?2 WHERE id = ?3",
            params![downloaded as i64, status.as_str(), id],
        )?;
        Ok(())
    }

    /// Persist each segment's offset so a restart resumes exactly where it
    /// stopped instead of re-downloading (or corrupting) what is on disk.
    pub fn save_segments(&self, id: &str, segments: &[SegmentProgress]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for seg in segments {
            tx.execute(
                "INSERT INTO segments (
                    download_id, segment_index, start_byte, end_byte, downloaded_bytes, is_finished
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(download_id, segment_index) DO UPDATE SET
                    start_byte = excluded.start_byte,
                    end_byte = excluded.end_byte,
                    downloaded_bytes = excluded.downloaded_bytes,
                    is_finished = excluded.is_finished",
                params![
                    id,
                    seg.index as i64,
                    seg.start_byte as i64,
                    seg.end_byte as i64,
                    seg.downloaded_bytes as i64,
                    if seg.is_finished { 1 } else { 0 }
                ],
            )?;
        }
        tx.commit()
    }

    pub fn mark_completed(&self, id: &str, finished_at: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = 'completed', downloaded_size = total_size, finished_at = ?1, error_message = NULL WHERE id = ?2",
            params![finished_at, id],
        )?;
        Ok(())
    }

    pub fn mark_completed_with_size(&self, id: &str, final_size: u64, finished_at: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = 'completed', downloaded_size = ?1, total_size = ?1, finished_at = ?2, error_message = NULL WHERE id = ?3",
            params![final_size as i64, finished_at, id],
        )?;
        Ok(())
    }

    pub fn set_status(&self, id: &str, status: DownloadStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = ?1 WHERE id = ?2",
            params![status.as_str(), id],
        )?;
        Ok(())
    }

    pub fn set_error(&self, id: &str, message: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = 'error', error_message = ?1 WHERE id = ?2",
            params![message, id],
        )?;
        Ok(())
    }

    pub fn delete_download(&self, id: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM segments WHERE download_id = ?1", params![id])?;
        tx.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
        tx.commit()
    }

    pub fn get_all_downloads(&self) -> Result<Vec<DownloadItem>> {
        // The connection mutex is not re-entrant: collect the rows inside a
        // scope so the guard is released before per-download segment lookups.
        // (Holding it across the second lock deadlocked the whole UI thread.)
        let mut items = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, url, filename, save_path, total_size, downloaded_size,
                        status, category, supports_range, etag, referer, connections,
                        created_at, finished_at, error_message
                 FROM downloads ORDER BY created_at DESC",
            )?;

            let rows = stmt.query_map([], |row| {
                let status_str: String = row.get(6)?;
                let total_size: i64 = row.get(4)?;
                let downloaded: i64 = row.get(5)?;
                let percent = if total_size > 0 {
                    ((downloaded as f64 / total_size as f64) * 100.0) as f32
                } else {
                    0.0
                };
                Ok(DownloadItem {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    filename: row.get(2)?,
                    save_path: row.get(3)?,
                    total_size: total_size.max(0) as u64,
                    downloaded_size: downloaded.max(0) as u64,
                    status: DownloadStatus::parse(&status_str),
                    category: row.get(7)?,
                    speed_bps: 0,
                    eta_seconds: 0,
                    progress_percent: percent.clamp(0.0, 100.0),
                    connections: row.get::<_, i64>(11).unwrap_or(1).max(1) as usize,
                    segments: Vec::new(),
                    supports_range: row.get::<_, i64>(8).unwrap_or(0) != 0,
                    etag: row.get(9)?,
                    referer: row.get(10)?,
                    created_at: row.get(12)?,
                    finished_at: row.get(13)?,
                    error_message: row.get(14)?,
                })
            })?;

            rows.collect::<Result<Vec<_>>>()?
        };

        for item in items.iter_mut() {
            item.segments = self.get_segments(&item.id).unwrap_or_default();
        }
        Ok(items)
    }

    pub fn get_download(&self, id: &str) -> Result<Option<DownloadItem>> {
        Ok(self.get_all_downloads()?.into_iter().find(|d| d.id == id))
    }

    fn get_segments(&self, download_id: &str) -> Result<Vec<SegmentProgress>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT segment_index, start_byte, end_byte, downloaded_bytes, is_finished
             FROM segments WHERE download_id = ?1 ORDER BY segment_index ASC",
        )?;
        let rows = stmt.query_map(params![download_id], |row| {
            let start: i64 = row.get(1)?;
            let end: i64 = row.get(2)?;
            let downloaded: i64 = row.get(3)?;
            Ok(SegmentProgress {
                index: row.get::<_, i64>(0)? as usize,
                start_byte: start.max(0) as u64,
                end_byte: end.max(0) as u64,
                downloaded_bytes: downloaded.max(0) as u64,
                total_bytes: (end - start + 1).max(0) as u64,
                speed_bps: 0,
                is_active: false,
                is_finished: row.get::<_, i64>(4).unwrap_or(0) != 0,
            })
        })?;
        rows.collect()
    }

    // ---------------------------------------------------------------- settings

    pub fn get_settings(&self) -> Settings {
        let conn = self.conn.lock().unwrap();
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'app'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);

        match raw {
            Some(json) => serde_json::from_str::<Settings>(&json)
                .map(|s| s.sanitized())
                .unwrap_or_default(),
            None => Settings::default().sanitized(),
        }
    }

    pub fn set_settings(&self, settings: &Settings) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(settings).unwrap_or_default();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('app', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------ queue

    /// Queued downloads in order, lowest position first.
    pub fn queued_in_order(&self) -> Vec<(String, i64)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, queue_position FROM downloads
             WHERE status = 'queued' ORDER BY queue_position ASC, created_at ASC",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };

        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    /// Rewrite queue positions so they are dense (0,1,2…) in the given order.
    ///
    /// A single transaction: a half-applied reorder would leave two rows claiming
    /// the same slot and the queue would start the wrong file.
    pub fn set_queue_order(&self, ids: &[String]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for (position, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE downloads SET queue_position = ?1 WHERE id = ?2",
                params![position as i64, id],
            )?;
        }
        tx.commit()
    }

    /// Change one download's status, optionally setting its queue position.
    pub fn set_download_status(
        &self,
        id: &str,
        status: &DownloadStatus,
        queue_position: Option<i64>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let label = match status {
            DownloadStatus::Queued => "queued",
            DownloadStatus::Downloading => "downloading",
            DownloadStatus::Paused => "paused",
            DownloadStatus::Completed => "completed",
            DownloadStatus::Error => "error",
            DownloadStatus::Cancelled => "cancelled",
        };

        match queue_position {
            Some(position) => conn.execute(
                "UPDATE downloads SET status = ?1, queue_position = ?2 WHERE id = ?3",
                params![label, position, id],
            )?,
            None => conn.execute(
                "UPDATE downloads SET status = ?1 WHERE id = ?2",
                params![label, id],
            )?,
        };
        Ok(())
    }

    /// Give a new download the next free slot at the end of the queue.
    pub fn next_queue_position(&self) -> i64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(MAX(queue_position), -1) + 1 FROM downloads",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
    }

    /// Persist the queue's own settings (enabled, parallelism, drain action).
    pub fn get_queue_config(&self) -> crate::schedule::QueueConfig {
        let conn = self.conn.lock().unwrap();
        let read = |key: &str| -> Option<String> {
            conn.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
        };

        let mut config = crate::schedule::QueueConfig::default();
        if let Some(value) = read("queue_enabled") {
            config.enabled = value == "1";
        }
        if let Some(value) = read("queue_parallel") {
            config.parallel = match value.as_str() {
                "sequential" => crate::schedule::ParallelMode::Sequential,
                "two" => crate::schedule::ParallelMode::Two,
                "three" => crate::schedule::ParallelMode::Three,
                "four" => crate::schedule::ParallelMode::Four,
                _ => crate::schedule::ParallelMode::Auto,
            };
        }
        if let Some(value) = read("queue_drain") {
            config.drain_action = match value.as_str() {
                "sleep" => crate::schedule::DrainAction::Sleep,
                "shut_down" => crate::schedule::DrainAction::ShutDown,
                _ => crate::schedule::DrainAction::Nothing,
            };
        }
        if let Some(value) = read("queue_paused") {
            config.paused = value == "1";
        }
        config
    }

    pub fn set_queue_config(&self, config: &crate::schedule::QueueConfig) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let parallel = match config.parallel {
            crate::schedule::ParallelMode::Sequential => "sequential",
            crate::schedule::ParallelMode::Two => "two",
            crate::schedule::ParallelMode::Three => "three",
            crate::schedule::ParallelMode::Four => "four",
            crate::schedule::ParallelMode::Auto => "auto",
        };
        let drain = match config.drain_action {
            crate::schedule::DrainAction::Nothing => "nothing",
            crate::schedule::DrainAction::Sleep => "sleep",
            crate::schedule::DrainAction::ShutDown => "shut_down",
        };

        for (key, value) in [
            ("queue_enabled", if config.enabled { "1" } else { "0" }),
            ("queue_parallel", parallel),
            ("queue_drain", drain),
            ("queue_paused", if config.paused { "1" } else { "0" }),
        ] {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `downloads` table exactly as the first release shipped it, without
    /// `queue_position`. Opening such a file must migrate it, not fail.
    fn legacy_db(path: &std::path::Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE downloads (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                filename TEXT NOT NULL,
                save_path TEXT NOT NULL,
                total_size INTEGER NOT NULL,
                downloaded_size INTEGER NOT NULL,
                status TEXT NOT NULL,
                category TEXT NOT NULL,
                supports_range INTEGER NOT NULL,
                etag TEXT,
                referer TEXT,
                connections INTEGER NOT NULL DEFAULT 8,
                created_at INTEGER NOT NULL,
                finished_at INTEGER,
                error_message TEXT
            );
            CREATE TABLE segments (
                download_id TEXT NOT NULL,
                segment_index INTEGER NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                downloaded_bytes INTEGER NOT NULL,
                is_finished INTEGER NOT NULL,
                PRIMARY KEY (download_id, segment_index)
            );
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO downloads (id, url, filename, save_path, total_size,
                downloaded_size, status, category, supports_range, connections, created_at)
            VALUES ('old1', 'https://e.com/a', 'a.bin', '/tmp/a.bin', 10, 10,
                'completed', 'Others', 1, 8, 100),
                   ('old2', 'https://e.com/b', 'b.bin', '/tmp/b.bin', 10, 0,
                'queued', 'Others', 1, 8, 200);
            ",
        )
        .unwrap();
    }

    #[test]
    fn upgrading_a_legacy_database_adds_queue_position() {
        let dir = std::env::temp_dir().join(format!("zdm-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.db");
        let _ = std::fs::remove_file(&path);
        legacy_db(&path);

        // The whole point: this used to fail with "no such column: queue_position".
        let db = Database::new(&path).expect("a legacy database must open, not abort");

        let queued = db.queued_in_order();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].0, "old2");
        assert_eq!(db.get_all_downloads().unwrap().len(), 2);

        // Dense, creation-ordered positions, so the queue has a real order.
        assert_eq!(db.next_queue_position(), 2);

        // Reopening must be a no-op, and the data must survive it.
        drop(db);
        let reopened = Database::new(&path).unwrap();
        assert_eq!(reopened.queued_in_order().len(), 1);
        assert_eq!(reopened.next_queue_position(), 2);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    fn sample_item(id: &str) -> DownloadItem {
        DownloadItem {
            id: id.to_string(),
            url: "https://example.com/f.bin".into(),
            filename: "f.bin".into(),
            save_path: "/tmp/f.bin".into(),
            total_size: 1000,
            downloaded_size: 0,
            status: DownloadStatus::Queued,
            category: "Others".into(),
            speed_bps: 0,
            eta_seconds: 0,
            progress_percent: 0.0,
            connections: 2,
            segments: vec![
                SegmentProgress {
                    index: 0,
                    start_byte: 0,
                    end_byte: 499,
                    downloaded_bytes: 0,
                    total_bytes: 500,
                    speed_bps: 0,
                    is_active: false,
                    is_finished: false,
                },
                SegmentProgress {
                    index: 1,
                    start_byte: 500,
                    end_byte: 999,
                    downloaded_bytes: 0,
                    total_bytes: 500,
                    speed_bps: 0,
                    is_active: false,
                    is_finished: false,
                },
            ],
            supports_range: true,
            etag: None,
            referer: None,
            created_at: 1,
            finished_at: None,
            error_message: None,
        }
    }

    #[test]
    fn roundtrips_download_and_segments() {
        let db = Database::new_in_memory().unwrap();
        let item = sample_item("a");
        db.insert_download(&item).unwrap();

        let loaded = db.get_all_downloads().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].segments.len(), 2);
        assert_eq!(loaded[0].status, DownloadStatus::Queued);
    }

    #[test]
    fn segment_offsets_survive_for_resume() {
        let db = Database::new_in_memory().unwrap();
        let mut item = sample_item("b");
        db.insert_download(&item).unwrap();

        item.segments[0].downloaded_bytes = 300;
        item.segments[1].downloaded_bytes = 120;
        db.save_segments("b", &item.segments).unwrap();

        let loaded = db.get_all_downloads().unwrap();
        assert_eq!(loaded[0].segments[0].downloaded_bytes, 300);
        assert_eq!(loaded[0].segments[1].downloaded_bytes, 120);
    }

    #[test]
    fn settings_default_then_persist() {
        let db = Database::new_in_memory().unwrap();
        let mut s = db.get_settings();
        assert_eq!(s.max_concurrent_downloads, 3);

        s.max_concurrent_downloads = 5;
        s.language = "fr".into();
        db.set_settings(&s).unwrap();

        let again = db.get_settings();
        assert_eq!(again.max_concurrent_downloads, 5);
        assert_eq!(again.language, "fr");
    }

    /// Queue positions must stay dense and ordered, or "move up" breaks.
    #[test]
    fn queue_order_is_dense_and_sorted() {
        let db = Database::new_in_memory().unwrap();

        let ids: Vec<String> = (0..4).map(|i| format!("d{i}")).collect();
        for (index, id) in ids.iter().enumerate() {
            let item = DownloadItem {
                id: id.clone(),
                url: "https://example.com/f".into(),
                filename: format!("{id}.bin"),
                save_path: "/tmp".into(),
                total_size: 100,
                downloaded_size: 0,
                status: DownloadStatus::Queued,
                category: "Others".into(),
                supports_range: true,
                etag: None,
                referer: None,
                connections: 8,
                speed_bps: 0,
                eta_seconds: 0,
                progress_percent: 0.0,
                created_at: index as i64,
                finished_at: None,
                error_message: None,
                segments: Vec::new(),
            };
            db.insert_download(&item).unwrap();
            db.set_download_status(id, &DownloadStatus::Queued, Some(index as i64))
                .unwrap();
        }

        let queued: Vec<String> = db.queued_in_order().into_iter().map(|(id, _)| id).collect();
        assert_eq!(queued, ids, "queue order must follow position");

        // Reverse it and re-read: the positions must be rewritten densely.
        let reversed: Vec<String> = ids.iter().rev().cloned().collect();
        db.set_queue_order(&reversed).unwrap();

        let after: Vec<(String, i64)> = db.queued_in_order();
        assert_eq!(
            after.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            reversed
        );
        assert_eq!(
            after.iter().map(|(_, pos)| *pos).collect::<Vec<_>>(),
            vec![0, 1, 2, 3],
            "positions must be dense after a reorder"
        );
    }

    /// The queue settings must survive a round trip and default to "off".
    #[test]
    fn queue_config_round_trips() {
        let db = Database::new_in_memory().unwrap();

        let default = db.get_queue_config();
        assert!(!default.enabled, "the queue must be opt-in");
        assert_eq!(default.drain_action, crate::schedule::DrainAction::Nothing);

        let config = crate::schedule::QueueConfig {
            enabled: true,
            parallel: crate::schedule::ParallelMode::Two,
            drain_action: crate::schedule::DrainAction::ShutDown,
            paused: true,
        };
        db.set_queue_config(&config).unwrap();

        let stored = db.get_queue_config();
        assert!(stored.enabled);
        assert_eq!(stored.parallel, crate::schedule::ParallelMode::Two);
        assert_eq!(stored.drain_action, crate::schedule::DrainAction::ShutDown);
        assert!(stored.paused);
    }

    /// A download removed from the queue becomes paused, never deleted.
    #[test]
    fn unqueueing_pauses_without_deleting() {
        let db = Database::new_in_memory().unwrap();
        let item = DownloadItem {
            id: "x".into(),
            url: "https://example.com/f".into(),
            filename: "f.bin".into(),
            save_path: "/tmp".into(),
            total_size: 10,
            downloaded_size: 0,
            status: DownloadStatus::Queued,
            category: "Others".into(),
            supports_range: true,
            etag: None,
            referer: None,
            connections: 4,
            speed_bps: 0,
            eta_seconds: 0,
            progress_percent: 0.0,
            created_at: 0,
            finished_at: None,
            error_message: None,
            segments: Vec::new(),
        };
        db.insert_download(&item).unwrap();

        db.set_download_status("x", &DownloadStatus::Paused, None)
            .unwrap();

        let all = db.get_all_downloads().unwrap();
        assert_eq!(all.len(), 1, "the row must still exist");
        assert_eq!(all[0].status, DownloadStatus::Paused);
    }

    #[test]
    fn settings_are_clamped_to_sane_values() {
        let db = Database::new_in_memory().unwrap();
        let mut s = db.get_settings();
        s.max_concurrent_downloads = 999;
        s.default_connections = 0;
        db.set_settings(&s).unwrap();

        let again = db.get_settings();
        assert_eq!(again.max_concurrent_downloads, 16);
        assert_eq!(again.default_connections, 1);
    }
}
