//! ZDM Engine Types & Models
//! Developed by Ramy Badraan

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentProgress {
    pub index: usize,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub is_active: bool,
    pub is_finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded_size: u64,
    pub status: DownloadStatus,
    pub category: String, // Video, Audio, Archives, Documents, Programs, Others
    pub speed_bps: u64,
    pub eta_seconds: u64,
    pub progress_percent: f32,
    pub connections: usize,
    pub segments: Vec<SegmentProgress>,
    pub supports_range: bool,
    pub etag: Option<String>,
    pub referer: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlMetadata {
    pub url: String,
    pub filename: String,
    pub total_size: u64,
    pub supports_range: bool,
    pub mime_type: String,
    pub category: String,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    pub total_speed_bps: u64,
    pub active_count: usize,
    pub queued_count: usize,
    pub completed_count: usize,
    pub disk_free_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDownloadRequest {
    pub url: String,
    pub filename: Option<String>,
    pub save_path: Option<String>,
    pub connections: Option<usize>,
    pub category: Option<String>,
    pub start_now: Option<bool>,
}
