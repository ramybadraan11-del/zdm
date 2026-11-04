//! Real-transfer tests for the download engine.
//!
//! These start an actual local HTTP server with configurable range behaviour
//! and drive the real segment writer. They exist because the corruption and the
//! dead pause button were *interaction* bugs: correct-looking code that breaks
//! only when HTTP semantics meet parallel positional writes.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct TestServer {
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    addr: String,
}

impl TestServer {
    pub fn start(body: Vec<u8>, honour_ranges: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let hits_clone = Arc::clone(&hits);
        let stop_clone = Arc::clone(&stop);

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                hits_clone.fetch_add(1, Ordering::Relaxed);

                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();

                let range_header = request
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("range:"))
                    .and_then(|l| l.split_once('=').map(|(_, v)| v.trim().to_string()));

                let total = body.len();

                match (honour_ranges, range_header) {
                    (true, Some(spec)) => {
                        let (start, end) = parse_range(&spec, total);
                        let end = end.min(total - 1);
                        let slice = &body[start..=end];
                        let header = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                            slice.len(),
                            start,
                            end,
                            total
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(slice);
                    }
                    _ => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nContent-Type: application/octet-stream\r\nAccept-Ranges: none\r\nConnection: close\r\n\r\n"
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                }
                let _ = stream.flush();
            }
        });

        Self {
            hits,
            stop,
            addr,
        }
    }

    pub fn url(&self, name: &str) -> String {
        format!("http://{}/{name}", self.addr)
    }

    pub fn requests(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(&self.addr);
    }
}

fn parse_range(spec: &str, total: usize) -> (usize, usize) {
    let mut parts = spec.split('-');
    let start = parts
        .next()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let end = parts
        .next()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(total - 1);
    (start.min(total - 1), end.min(total - 1))
}

/// Deterministic payload: any overlap or gap in positional writes changes the
/// bytes, so a byte-for-byte comparison is a real corruption detector.
pub fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 37 + i / 251) % 251) as u8).collect()
}

/// Server that never sends `Content-Length`: the classic chunked response.
pub struct ChunkedServer {
    body: Vec<u8>,
    addr: String,
    stop: Arc<AtomicBool>,
}

impl ChunkedServer {
    pub fn start(body: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let body_clone = body.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);

                // Transfer-Encoding: chunked, no Content-Length, no Accept-Ranges.
                let header = "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(header.as_bytes());

                for piece in body_clone.chunks(1024) {
                    let _ = stream.write_all(format!("{:x}\r\n", piece.len()).as_bytes());
                    let _ = stream.write_all(piece);
                    let _ = stream.write_all(b"\r\n");
                }
                let _ = stream.write_all(b"0\r\n\r\n");
                let _ = stream.flush();
            }
        });

        Self { body, addr, stop }
    }

    pub fn url(&self, name: &str) -> String {
        format!("http://{}/{name}", self.addr)
    }

    pub fn len(&self) -> usize {
        self.body.len()
    }
}

impl Drop for ChunkedServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(&self.addr);
    }
}

/// Range-capable server that trickles the body so a test can pause mid-transfer.
pub struct SlowServer {
    addr: String,
    stop: Arc<AtomicBool>,
}

impl SlowServer {
    pub fn start(body: Vec<u8>, chunk_delay_bytes: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let body = body.clone();
                std::thread::spawn(move || {
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]).to_lowercase();

                    let range = request
                        .lines()
                        .find(|l| l.starts_with("range:"))
                        .and_then(|l| l.split_once('=').map(|(_, v)| v.trim().to_string()));

                    let total = body.len();
                    let (start, end) = match range {
                        Some(spec) => parse_range(&spec, total),
                        None => (0, total - 1),
                    };
                    let end = end.min(total - 1);
                    let slice = &body[start..=end];

                    let header = if request.contains("range:") {
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                            slice.len(), start, end, total
                        )
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                            slice.len()
                        )
                    };
                    let _ = stream.write_all(header.as_bytes());

                    for piece in slice.chunks(chunk_delay_bytes) {
                        if stream.write_all(piece).is_err() {
                            return;
                        }
                        let _ = stream.flush();
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                });
            }
        });

        Self { addr, stop }
    }

    pub fn url(&self, name: &str) -> String {
        format!("http://{}/{name}", self.addr)
    }
}

impl Drop for SlowServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(&self.addr);
    }
}

/// Server that accepts the probe (206) then answers 200 to real segment reads,
/// which is exactly how a CDN behaves when a cached object is replaced.
pub struct DeceptiveServer {
    body: Vec<u8>,
    addr: String,
    stop: Arc<AtomicBool>,
}

impl DeceptiveServer {
    pub fn start(body: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let body_clone = body.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_lowercase();

                // Honour only the 1-byte probe; ignore every real range request.
                if request.contains("range: bytes=0-0") {
                    let header = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        body_clone.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body_clone[..1]);
                } else {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body_clone.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body_clone);
                }
                let _ = stream.flush();
            }
        });

        Self { body, addr, stop }
    }

    pub fn url(&self, name: &str) -> String {
        format!("http://{}/{name}", self.addr)
    }

    pub fn len(&self) -> usize {
        self.body.len()
    }
}

impl Drop for DeceptiveServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(&self.addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_serves_full_body_without_ranges() {
        let body = pattern(64 * 1024);
        let server = TestServer::start(body.clone(), false);
        let resp = reqwest::blocking::get(server.url("f.bin")).expect("request");
        assert_eq!(resp.status(), 200);
        let received = resp.bytes().expect("body");
        assert_eq!(received.len(), body.len());
        assert_eq!(&received[..], &body[..]);
        assert!(server.requests() >= 1);
    }

    #[test]
    fn fixture_serves_206_when_ranges_supported() {
        let body = pattern(10_000);
        let server = TestServer::start(body.clone(), true);
        let client = reqwest::blocking::Client::new();
        let resp = client
            .get(server.url("f.bin"))
            .header("Range", "bytes=100-199")
            .send()
            .expect("request");
        assert_eq!(resp.status(), 206);
        let received = resp.bytes().expect("body");
        assert_eq!(received.len(), 100);
        assert_eq!(&received[..], &body[100..200]);
    }

    #[test]
    fn fixture_can_ignore_ranges() {
        let body = pattern(5_000);
        let server = TestServer::start(body.clone(), false);
        let client = reqwest::blocking::Client::new();
        let resp = client
            .get(server.url("f.bin"))
            .header("Range", "bytes=1000-1099")
            .send()
            .expect("request");
        // Exactly the condition that used to corrupt downloads.
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.bytes().expect("body").len(), body.len());
    }

    /// End-to-end check of the real engine against an unknown-length response.
    /// This is the case that previously produced a 1-byte "completed" file.
    #[tokio::test]
    async fn engine_downloads_an_unknown_length_stream_completely() {
        use crate::db::Database;
        use crate::engine::manager::DownloadManager;
        use std::sync::Arc;

        let body = pattern(48_000);
        let server = ChunkedServer::start(body.clone());

        let dir = std::env::temp_dir().join(format!("zdm-chunked-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("zdm.db");
        let db = Arc::new(Database::new(&db_path).unwrap());
        let manager = Arc::new(DownloadManager::new(db));

        let save = dir.join("stream.bin");
        let item = manager
            .add_download(crate::engine::types::NewDownloadRequest {
                url: server.url("stream.bin"),
                filename: Some("stream.bin".into()),
                save_path: Some(save.to_string_lossy().to_string()),
                connections: Some(4),
                category: None,
                start_now: Some(true),
                referer: None,
            })
            .await
            .expect("download added");

        // Wait for the transfer to settle (max ~15s).
        let mut final_item = item.clone();
        for _ in 0..150 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            final_item = manager
                .get_all_downloads()
                .unwrap()
                .into_iter()
                .find(|d| d.id == item.id)
                .unwrap_or(final_item);
            if final_item.status == crate::engine::types::DownloadStatus::Completed
                || final_item.status == crate::engine::types::DownloadStatus::Error
            {
                break;
            }
        }

        // `add_download` treats `save_path` as a folder and appends the filename,
        // so read the path the engine actually chose.
        let actual = std::path::PathBuf::from(&final_item.save_path);
        let written = std::fs::read(&actual).unwrap_or_default();
        assert_eq!(
            final_item.status,
            crate::engine::types::DownloadStatus::Completed,
            "unknown-length download must complete (error: {:?})",
            final_item.error_message
        );
        assert_eq!(
            written.len(),
            server.len(),
            "the whole stream must land on disk, not a truncated prefix"
        );
        assert_eq!(written, body, "content must be byte-identical");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End-to-end: a range-capable server must produce a byte-identical file via
    /// the real multi-segment engine. This is the primary "downloads are corrupt"
    /// regression guard.
    #[tokio::test]
    async fn engine_downloads_a_ranged_file_byte_identically() {
        use crate::db::Database;
        use crate::engine::manager::DownloadManager;
        use std::sync::Arc;

        let body = pattern(400_000);
        let server = TestServer::start(body.clone(), true);

        let dir = std::env::temp_dir().join(format!("zdm-ranged-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(Database::new(dir.join("zdm.db")).unwrap());
        let manager = Arc::new(DownloadManager::new(db));

        let item = manager
            .add_download(crate::engine::types::NewDownloadRequest {
                url: server.url("file.bin"),
                filename: Some("file.bin".into()),
                save_path: Some(dir.to_string_lossy().to_string()),
                connections: Some(8),
                category: None,
                start_now: Some(true),
                referer: None,
            })
            .await
            .expect("download added");

        let mut final_item = item.clone();
        for _ in 0..200 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            final_item = manager
                .get_all_downloads()
                .unwrap()
                .into_iter()
                .find(|d| d.id == item.id)
                .unwrap_or(final_item);
            if matches!(
                final_item.status,
                crate::engine::types::DownloadStatus::Completed
                    | crate::engine::types::DownloadStatus::Error
            ) {
                break;
            }
        }

        assert_eq!(
            final_item.status,
            crate::engine::types::DownloadStatus::Completed,
            "multi-segment download failed: {:?}",
            final_item.error_message
        );

        let written = std::fs::read(std::path::PathBuf::from(&final_item.save_path)).unwrap_or_default();
        assert_eq!(written.len(), body.len(), "size mismatch");
        assert_eq!(written, body, "parallel writes must reproduce the payload exactly");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End-to-end: pause must actually stop the writer, and resume must finish the
    /// file correctly from where it stopped. This is the "pause button does
    /// nothing" regression guard.
    #[tokio::test]
    async fn engine_pause_then_resume_completes_correctly() {
        use crate::db::Database;
        use crate::engine::manager::DownloadManager;
        use crate::engine::types::DownloadStatus;
        use std::sync::Arc;

        // A slow server so the transfer is still running when we pause it.
        let body = pattern(2_000_000);
        // Four connections each get their own throttled stream, so a large
        // chunk delay is required for the transfer to still be running at 700 ms.
        let server = SlowServer::start(body.clone(), 4_800);

        let dir = std::env::temp_dir().join(format!("zdm-pause-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(Database::new(dir.join("zdm.db")).unwrap());
        let manager = Arc::new(DownloadManager::new(db));

        let item = manager
            .add_download(crate::engine::types::NewDownloadRequest {
                url: server.url("file.bin"),
                filename: Some("file.bin".into()),
                save_path: Some(dir.to_string_lossy().to_string()),
                connections: Some(4),
                category: None,
                start_now: Some(true),
                referer: None,
            })
            .await
            .expect("download added");

        let save = std::path::PathBuf::from(&item.save_path);

        // Let some bytes land, then pause.
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        manager.pause_download(&item.id).await.expect("pause");

        let after_pause = std::fs::metadata(&save).map(|m| m.len()).unwrap_or(0);

        // The file must not keep growing after a pause.
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        let still = std::fs::metadata(&save).map(|m| m.len()).unwrap_or(0);
        assert_eq!(
            after_pause, still,
            "the writer kept running after pause ({after_pause} -> {still})"
        );

        let paused_row = manager
            .get_all_downloads()
            .unwrap()
            .into_iter()
            .find(|d| d.id == item.id)
            .unwrap();
        assert_eq!(paused_row.status, DownloadStatus::Paused);

        // Resume and let it finish.
        manager.resume_download(&item.id).await.expect("resume");

        let mut final_item = paused_row;
        for _ in 0..300 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            final_item = manager
                .get_all_downloads()
                .unwrap()
                .into_iter()
                .find(|d| d.id == item.id)
                .unwrap_or(final_item);
            if matches!(final_item.status, DownloadStatus::Completed | DownloadStatus::Error) {
                break;
            }
        }

        assert_eq!(
            final_item.status,
            DownloadStatus::Completed,
            "resume did not finish: {:?}",
            final_item.error_message
        );

        let written = std::fs::read(&save).unwrap_or_default();
        assert_eq!(written.len(), body.len(), "size after resume");
        assert_eq!(written, body, "resume must not corrupt the file");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End-to-end: a server that stops honouring ranges must NOT leave a corrupt
    /// file marked complete. The engine has to fail loudly instead.
    #[tokio::test]
    async fn engine_never_reports_a_deceptive_server_as_complete() {
        use crate::db::Database;
        use crate::engine::manager::DownloadManager;
        use crate::engine::types::DownloadStatus;
        use std::sync::Arc;

        let body = pattern(300_000);
        let server = DeceptiveServer::start(body.clone());

        let dir = std::env::temp_dir().join(format!("zdm-deceptive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(Database::new(dir.join("zdm.db")).unwrap());
        let manager = Arc::new(DownloadManager::new(db));

        let item = manager
            .add_download(crate::engine::types::NewDownloadRequest {
                url: server.url("file.bin"),
                filename: Some("file.bin".into()),
                save_path: Some(dir.to_string_lossy().to_string()),
                connections: Some(8),
                category: None,
                start_now: Some(true),
                referer: None,
            })
            .await
            .expect("download added");

        let mut final_item = item.clone();
        for _ in 0..200 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            final_item = manager
                .get_all_downloads()
                .unwrap()
                .into_iter()
                .find(|d| d.id == item.id)
                .unwrap_or(final_item);
            if matches!(final_item.status, DownloadStatus::Completed | DownloadStatus::Error) {
                break;
            }
        }

        // The engine may recover by downgrading to a single stream; what it must
        // never do is claim success while the bytes on disk are wrong.
        if final_item.status == DownloadStatus::Completed {
            let written =
                std::fs::read(std::path::PathBuf::from(&final_item.save_path)).unwrap_or_default();
            assert_eq!(
                written, body,
                "reported complete but the file does not match the source"
            );
        } else {
            assert_eq!(final_item.status, DownloadStatus::Error);
            assert!(
                final_item.error_message.is_some(),
                "a failed download must explain itself"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chunked_server_reports_no_content_length() {
        let body = pattern(5_000);
        let server = ChunkedServer::start(body.clone());
        let resp = reqwest::blocking::get(server.url("f.bin")).expect("request");

        assert_eq!(resp.status(), 200);
        // This is the case that used to be truncated to one byte and reported as
        // complete: no Content-Length means the size is genuinely unknown.
        assert_eq!(resp.content_length(), None);

        let received = resp.bytes().expect("body");
        assert_eq!(received.len(), server.len());
        assert_eq!(&received[..], &body[..]);
    }

    #[test]
    fn deceptive_server_reports_200_for_a_range_request() {
        let body = pattern(20_000);
        let server = DeceptiveServer::start(body.clone());
        let client = reqwest::blocking::Client::new();

        // The probe lies favourably.
        let probe = client
            .get(server.url("f.bin"))
            .header("Range", "bytes=0-0")
            .send()
            .expect("probe");
        assert_eq!(probe.status(), 206, "probe looks range-capable");

        // A real segment read is answered with the whole body.
        let real = client
            .get(server.url("f.bin"))
            .header("Range", "bytes=10000-19999")
            .send()
            .expect("segment");
        assert_eq!(
            real.status(),
            200,
            "the engine must treat this as a protocol downgrade"
        );
        assert_eq!(real.bytes().unwrap().len(), server.len());
    }

    #[test]
    fn scrambled_positional_writes_reproduce_the_payload() {
        #[cfg(unix)]
        use std::os::unix::fs::FileExt;
        #[cfg(windows)]
        use std::os::windows::fs::FileExt;

        let body = pattern(300_000);
        let dir = std::env::temp_dir().join(format!("zdm-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("assembled.bin");

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        file.set_len(body.len() as u64).unwrap();

        let chunk = body.len() / 7;
        let mut ranges: Vec<(usize, usize)> = (0..7)
            .map(|i| {
                let start = i * chunk;
                let end = if i == 6 { body.len() } else { (i + 1) * chunk };
                (start, end)
            })
            .collect();
        ranges.reverse();

        for (start, end) in ranges {
            #[cfg(unix)]
            file.write_all_at(&body[start..end], start as u64).unwrap();
            #[cfg(windows)]
            file.seek_write(&body[start..end], start as u64).unwrap();
        }
        file.sync_all().unwrap();
        drop(file);

        let written = std::fs::read(&path).unwrap();
        assert_eq!(written.len(), body.len());
        assert_eq!(written, body, "positional writes must reproduce the payload");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
