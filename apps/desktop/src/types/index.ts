//! ZDM Frontend Types
//! Developed by Ramy Badraan

export type DownloadStatus = 'queued' | 'downloading' | 'paused' | 'completed' | 'error' | 'cancelled';

export interface SegmentProgress {
  index: number;
  start_byte: number;
  end_byte: number;
  downloaded_bytes: number;
  total_bytes: number;
  speed_bps: number;
  is_active: boolean;
  is_finished: boolean;
}

export interface DownloadItem {
  id: string;
  url: string;
  filename: string;
  save_path: string;
  total_size: number;
  downloaded_size: number;
  status: DownloadStatus;
  category: string;
  speed_bps: number;
  eta_seconds: number;
  progress_percent: number;
  connections: number;
  segments: SegmentProgress[];
  supports_range: boolean;
  etag?: string;
  referer?: string;
  created_at: number;
  finished_at?: number;
  error_message?: string;
}

export interface UrlMetadata {
  url: string;
  filename: string;
  total_size: number;
  supports_range: boolean;
  mime_type: string;
  category: string;
  etag?: string;
}

export interface EngineStats {
  total_speed_bps: number;
  active_count: number;
  queued_count: number;
  completed_count: number;
  disk_free_bytes: number;
}

export interface NewDownloadRequest {
  url: string;
  filename?: string;
  save_path?: string;
  connections?: number;
  category?: string;
  start_now?: boolean;
}

export interface VideoFormat {
  format_id: string;
  ext: string;
  resolution?: string;
  filesize?: number;
  vcodec?: string;
  acodec?: string;
  url?: string;
}

export interface VideoInfo {
  id: string;
  title: string;
  thumbnail?: string;
  duration_seconds?: number;
  uploader?: string;
  formats: VideoFormat[];
}

export interface AppInfo {
  name: string;
  version: string;
  developer: string;
  license: string;
  architecture: string;
  os: string;
  engine: string;
}
