//! ZDM SQLite Database Manager
//! Developed by Ramy Badraan

use crate::engine::types::{DownloadItem, DownloadStatus, SegmentProgress};
use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

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
                PRIMARY KEY (download_id, segment_index),
                FOREIGN KEY (download_id) REFERENCES downloads(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
            CREATE INDEX IF NOT EXISTS idx_segments_download ON segments(download_id);
            ",
        )?;
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
                format!("{:?}", item.status).to_lowercase(),
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

        // Insert segments
        tx.execute(
            "DELETE FROM segments WHERE download_id = ?1",
            params![item.id],
        )?;

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

        tx.commit()?;
        Ok(())
    }

    pub fn update_progress(&self, id: &str, downloaded: u64, status: &DownloadStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET downloaded_size = ?1, status = ?2 WHERE id = ?3",
            params![
                downloaded as i64,
                format!("{:?}", status).to_lowercase(),
                id
            ],
        )?;
        Ok(())
    }

    pub fn mark_completed(&self, id: &str, finished_at: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = 'completed', finished_at = ?1 WHERE id = ?2",
            params![finished_at, id],
        )?;
        Ok(())
    }

    pub fn set_status(&self, id: &str, status: DownloadStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = ?1 WHERE id = ?2",
            params![format!("{:?}", status).to_lowercase(), id],
        )?;
        Ok(())
    }

    pub fn delete_download(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM segments WHERE download_id = ?1", params![id])?;
        conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_all_downloads(&self) -> Result<Vec<DownloadItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, filename, save_path, total_size, downloaded_size,
                    status, category, supports_range, etag, referer, connections,
                    created_at, finished_at, error_message
             FROM downloads ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let status_str: String = row.get(6)?;
            let status = match status_str.as_str() {
                "downloading" => DownloadStatus::Downloading,
                "paused" => DownloadStatus::Paused,
                "completed" => DownloadStatus::Completed,
                "error" => DownloadStatus::Error,
                "cancelled" => DownloadStatus::Cancelled,
                _ => DownloadStatus::Queued,
            };

            let total_size: i64 = row.get(4)?;
            let downloaded_size: i64 = row.get(5)?;
            let total = total_size as u64;
            let downloaded = downloaded_size as u64;
            let progress_percent = if total > 0 {
                (downloaded as f32 / total as f32) * 100.0
            } else {
                0.0
            };

            let supports_range_int: i32 = row.get(8)?;

            Ok(DownloadItem {
                id: row.get(0)?,
                url: row.get(1)?,
                filename: row.get(2)?,
                save_path: row.get(3)?,
                total_size: total,
                downloaded_size: downloaded,
                status,
                category: row.get(7)?,
                speed_bps: 0,
                eta_seconds: 0,
                progress_percent,
                connections: row.get::<_, i64>(11)? as usize,
                segments: Vec::new(),
                supports_range: supports_range_int == 1,
                etag: row.get(9)?,
                referer: row.get(10)?,
                created_at: row.get(12)?,
                finished_at: row.get(13)?,
                error_message: row.get(14)?,
            })
        })?;

        let mut items = Vec::new();
        for r in rows {
            if let Ok(mut item) = r {
                let mut seg_stmt = conn.prepare(
                    "SELECT segment_index, start_byte, end_byte, downloaded_bytes, is_finished
                     FROM segments WHERE download_id = ?1 ORDER BY segment_index ASC",
                )?;
                let seg_rows = seg_stmt.query_map(params![item.id], |srow| {
                    let start: i64 = srow.get(1)?;
                    let end: i64 = srow.get(2)?;
                    let dl: i64 = srow.get(3)?;
                    let is_fin: i32 = srow.get(4)?;
                    let total = if end >= start { (end - start + 1) as u64 } else { 0 };

                    Ok(SegmentProgress {
                        index: srow.get::<_, i64>(0)? as usize,
                        start_byte: start as u64,
                        end_byte: end as u64,
                        downloaded_bytes: dl as u64,
                        total_bytes: total,
                        speed_bps: 0,
                        is_active: false,
                        is_finished: is_fin == 1,
                    })
                })?;

                for s in seg_rows {
                    if let Ok(seg) = s {
                        item.segments.push(seg);
                    }
                }

                items.push(item);
            }
        }

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_insert_and_retrieve() {
        let db = Database::new_in_memory().unwrap();
        let item = DownloadItem {
            id: "test-123".into(),
            url: "https://example.com/test.iso".into(),
            filename: "test.iso".into(),
            save_path: "/tmp/test.iso".into(),
            total_size: 1000,
            downloaded_size: 250,
            status: DownloadStatus::Downloading,
            category: "Compressed".into(),
            speed_bps: 50000,
            eta_seconds: 15,
            progress_percent: 25.0,
            connections: 4,
            segments: vec![
                SegmentProgress {
                    index: 0,
                    start_byte: 0,
                    end_byte: 249,
                    downloaded_bytes: 250,
                    total_bytes: 250,
                    speed_bps: 0,
                    is_active: false,
                    is_finished: true,
                },
                SegmentProgress {
                    index: 1,
                    start_byte: 250,
                    end_byte: 499,
                    downloaded_bytes: 0,
                    total_bytes: 250,
                    speed_bps: 0,
                    is_active: true,
                    is_finished: false,
                },
            ],
            supports_range: true,
            etag: Some("etag-abc".into()),
            referer: None,
            created_at: 1000000,
            finished_at: None,
            error_message: None,
        };

        db.insert_download(&item).unwrap();
        let all = db.get_all_downloads().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].filename, "test.iso");
        assert_eq!(all[0].segments.len(), 2);
        assert!(all[0].segments[0].is_finished);

        // Update progress
        db.update_progress("test-123", 500, &DownloadStatus::Downloading).unwrap();
        let all2 = db.get_all_downloads().unwrap();
        assert_eq!(all2[0].downloaded_size, 500);

        // Delete
        db.delete_download("test-123").unwrap();
        let all3 = db.get_all_downloads().unwrap();
        assert_eq!(all3.len(), 0);
    }
}
