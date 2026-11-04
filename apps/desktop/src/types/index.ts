//! ZDM Frontend Types
//! Developed by Ramy Badraan

export type DownloadStatus = 'queued' | 'downloading' | 'paused' | 'completed' | 'error' | 'cancelled';

export type Category = 'Video' | 'Audio' | 'Programs' | 'Compressed' | 'Documents' | 'Others';

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
  referer?: string;
}

export interface VideoFormat {
  format_id: string;
  ext: string;
  resolution?: string;
  filesize?: number;
  vcodec?: string;
  acodec?: string;
  url?: string;
  fps?: number;
  note?: string;
  is_audio: boolean;
}

export interface VideoInfo {
  id: string;
  title: string;
  thumbnail?: string;
  duration_seconds?: number;
  uploader?: string;
  webpage_url: string;
  extractor?: string;
  formats: VideoFormat[];
}

export interface PlaylistEntry {
  id: string;
  url: string;
  title: string;
  duration_seconds?: number;
  thumbnail?: string;
  index: number;
  /** Estimated bytes for each offered height (keys are heights as strings). */
  sizes: Record<string, number>;
}

export interface PlaylistInfo {
  id: string;
  title: string;
  uploader?: string;
  thumbnail?: string;
  webpage_url: string;
  entries: PlaylistEntry[];
  /** Distinct heights across the playlist, tallest first. */
  heights: number[];
  /** Total bytes across all entries for each height. */
  total_sizes: Record<string, number>;
  total_duration_seconds: number;
}

export interface AppInfo {
  name: string;
  version: string;
  developer: string;
  license: string;
  architecture: string;
  os: string;
  engine: string;
  data_dir: string;
  download_dir: string;
  platform: 'windows' | 'linux' | 'macos';
}

export interface BrowserStatus {
  id: string;
  name: string;
  family: 'chromium' | 'gecko';
  is_installed: boolean;
  config_dir: string;
  native_host_ready: boolean;
  extension_pending: boolean;
  note: string;
}

export interface IntegrationReport {
  native_host_path: string;
  extension_dir: string;
  registered: string[];
  warnings: string[];
  firefox_policy_written: boolean;
}

export interface Settings {
  download_dir: string;
  max_concurrent_downloads: number;
  default_connections: number;
  language: string;
  theme: string;
  auto_start: boolean;
  capture_downloads: boolean;
  notifications: boolean;
  sniffer_enabled: boolean;
  speed_limit_kbps: number;
  keep_partial_on_cancel: boolean;
  intercept_extensions: string[];
  category_dirs: { [category: string]: string };
}

/** Category keys the backend understands for folder routing. */
export const CATEGORY_KEYS = [
  'Video',
  'Audio',
  'Compressed',
  'Documents',
  'Images',
  'Programs',
  'Others',
] as const;

/** Must match `DEFAULT_INTERCEPT_EXTENSIONS` in engine/types.rs. */
export const DEFAULT_INTERCEPT_EXTENSIONS: string[] = [
  'mp4', 'mkv', 'avi', 'webm', 'mov', 'flv', 'wmv', 'm4v', 'mpg', 'mpeg', 'ts',
  'mp3', 'flac', 'wav', 'm4a', 'aac', 'ogg', 'opus', 'wma',
  'zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'zst', 'tgz', 'iso',
  'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'epub', 'md', 'csv',
  'exe', 'msi', 'deb', 'rpm', 'appimage', 'apk', 'dmg', 'pkg', 'flatpak', 'snap', 'jar',
];

export interface CaptureStatus {
  bridge_running: boolean;
  bridge_port: number;
  capture_downloads: boolean;
  sniffer_enabled: boolean;
  intercept_extensions: string[];
}

export interface Diagnostics {
  tray_available: boolean;
  ffmpeg_available: boolean;
  ffmpeg_path?: string;
  autostart_enabled: boolean;
  autostart_kind: string;
  browser_report: IntegrationReport;
  data_dir: string;
  log_file: string;
  desktop: string;
}

export interface ProgressPayload {
  id: string;
  downloaded_bytes: number;
  total_bytes: number;
  speed_bps: number;
  eta_seconds: number;
  progress_percent: number;
  segments: SegmentProgress[];
}

export interface CompletedPayload {
  id: string;
  filename: string;
  save_path: string;
  total_size: number;
  category: string;
}
