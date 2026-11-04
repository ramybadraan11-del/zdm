//! ZDM Local Bridge
//! Developed by Ramy Badraan
//!
//! A tiny HTTP server on 127.0.0.1 that the browser extension and `zdm-host`
//! talk to. It exists because native messaging alone is brittle:
//!
//! * Chrome only trusts a native host whose manifest lives in the registry and
//!   whose `allowed_origins` matches exactly — easy to get wrong on Windows,
//!   and silently fatal when wrong.
//! * `fetch(..., {mode:'no-cors'})` from the extension still needs the server to
//!   answer CORS preflights for POSTs, which the old implementation never did,
//!   so every capture from a Windows browser failed.
//!
//! The bridge speaks plain HTTP/1.1 over a raw socket: no web framework, a
//! bounded request size, a per-origin rate limit, and it binds loopback only.

use crate::engine::manager::DownloadManager;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const BRIDGE_PORT: u16 = 46800;
const MAX_BODY: usize = 64 * 1024;

/// Set once the bridge task successfully bound its listener. The UI shows this
/// in Diagnostics so capture problems are visible without reading logs.
pub static BRIDGE_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the bridge bound its listener at startup.
pub fn bridge_started() -> bool {
    BRIDGE_STARTED.load(std::sync::atomic::Ordering::SeqCst)
}

pub struct BridgeState {
    app: AppHandle,
    manager: Arc<DownloadManager>,
    /// Last accepted request per origin, used to throttle abuse.
    last_seen: Mutex<HashMap<String, Instant>>,
}

pub fn start(manager: Arc<DownloadManager>, app: AppHandle) {
    let state = Arc::new(BridgeState {
        app,
        manager,
        last_seen: Mutex::new(HashMap::new()),
    });

    tauri::async_runtime::spawn(async move {
        let addr = format!("127.0.0.1:{BRIDGE_PORT}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(err) => {
                eprintln!("[ZDM bridge] cannot bind {addr}: {err}");
                return;
            }
        };
        BRIDGE_STARTED.store(true, std::sync::atomic::Ordering::SeqCst);
        println!("[ZDM bridge] listening on http://{addr}");

        loop {
            let Ok((socket, _peer)) = listener.accept().await else {
                continue;
            };
            let state = Arc::clone(&state);
            tokio::spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(10), handle(state, socket)).await;
            });
        }
    });
}

struct Request {
    method: String,
    path: String,
    origin: Option<String>,
    body: String,
}

async fn handle(state: Arc<BridgeState>, mut socket: TcpStream) -> std::io::Result<()> {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 2048];

    // Read until the header block is complete.
    let header_end = loop {
        let n = socket.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..n]);

        if let Some(idx) = find_headers_end(&buffer) {
            break idx;
        }
        if buffer.len() > MAX_BODY {
            return write_plain(&mut socket, 431, "Request too large").await;
        }
    };

    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = header_text.lines();
    let Some(request_line) = lines.next() else {
        return write_plain(&mut socket, 400, "Bad request").await;
    };

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    let mut origin: Option<String> = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            match name.trim().to_lowercase().as_str() {
                "content-length" => content_length = value.trim().parse().unwrap_or(0),
                "origin" => origin = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }

    if content_length > MAX_BODY {
        return write_plain(&mut socket, 413, "Payload too large").await;
    }

    let mut body_bytes = buffer[header_end + 4..].to_vec();
    while body_bytes.len() < content_length {
        let n = socket.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        body_bytes.extend_from_slice(&chunk[..n]);
    }

    let request = Request {
        method,
        path,
        origin,
        body: String::from_utf8_lossy(&body_bytes[..content_length.min(body_bytes.len())])
            .to_string(),
    };

    // Refuse a web page outright: this must happen before any side effect.
    if !origin_allowed(request.origin.as_deref()) {
        return write_plain(&mut socket, 403, "Origin not allowed").await;
    }

    let allow_origin = cors_origin(request.origin.as_deref());

    // Preflight: browsers abort the real POST without this.
    if request.method == "OPTIONS" {
        return write_response(&mut socket, 204, "No Content", "application/json", "", allow_origin)
            .await;
    }

    // Liveness probe used by the popup, and the channel through which the
    // extension learns the user's capture preferences. Without this the two
    // switches in Settings would be decorative.
    if request.method == "GET" && (request.path == "/" || request.path.starts_with("/ping")) {
        let settings = state.manager.settings();
        let payload = serde_json::json!({
            "status": "ok",
            "app": "Zyzoom Download Manager",
            "version": env!("CARGO_PKG_VERSION"),
            "author": "Ramy Badraan",
            "capture_downloads": settings.capture_downloads,
            "sniffer_enabled": settings.sniffer_enabled,
            // The extension only takes over these types; images stay with the
            // browser. When empty, nothing is captured automatically.
            "intercept_extensions": settings.intercept_extensions,
            // The extension renders its floating button in this language, so it
            // must follow the desktop app rather than the browser's own locale.
            "language": crate::i18n::Lang::detect(&settings.language).code(),
        });
        return write_response(
            &mut socket,
            200,
            "OK",
            "application/json",
            &payload.to_string(),
            allow_origin,
        )
        .await;
    }

    if request.method != "POST" {
        return write_plain(&mut socket, 405, "Method not allowed").await;
    }


    // Throttle so a runaway page script cannot saturate the UI event loop.
    //
    // The key includes the URL: a person clicking "download" on two different
    // videos in quick succession is normal, and keying on the origin alone
    // rejected the second click with 429 — which the extension reported as
    // "ZDM is not running". Only an identical request to the same URL within the
    // window is treated as a runaway loop.
    //
    // The guard is scoped: parking_lot guards are not Send and must be dropped
    // before the awaits below.
    let throttled = {
        // Peek at the raw body for the URL: `parsed` is not built until later,
        // and the throttle must not depend on the payload being well-formed.
        let origin = request.origin.clone().unwrap_or_else(|| "anon".into());
        let key = extract_url_for_throttle(&request.body)
            .map(|url| format!("{origin}|{url}"))
            .unwrap_or(origin);
        let mut seen = state.last_seen.lock();
        let now = Instant::now();
        let too_soon = seen
            .get(&key)
            .map(|last| now.duration_since(*last) < Duration::from_millis(120))
            .unwrap_or(false);
        if !too_soon {
            seen.insert(key, now);
        }
        too_soon
    };
    if throttled {
        return write_plain(&mut socket, 429, "Too many requests").await;
    }

    let parsed: serde_json::Value = match serde_json::from_str(&request.body) {
        Ok(v) => v,
        Err(_) => {
            return write_response(
                &mut socket,
                400,
                "Bad Request",
                "application/json",
                r#"{"status":"error","error":"invalid json"}"#,
                allow_origin,
            )
            .await
        }
    };

    let msg_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("add_download");
    let url = parsed.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if msg_type == "focus_window" {
        // Explicit user request (extension popup "Open ZDM"): this one may focus.
        focus_main(&state.app);
        return write_response(
            &mut socket,
            200,
            "OK",
            "application/json",
            r#"{"status":"focused"}"#,
            allow_origin,
        )
        .await;
    }

    if url.is_empty() {
        return write_response(
            &mut socket,
            400,
            "Bad Request",
            "application/json",
            r#"{"status":"error","error":"missing url"}"#,
            allow_origin,
        )
        .await;
    }

    // Deliberately NOT focusing the manager: a captured link opens the floating
    // prompt instead, so the app window never jumps in front of what you are
    // reading.
    // A video page gets the same floating window, in video mode: it lists the
    // available qualities instead of file facts. Emitting an event to the manager
    // (the old behaviour) did nothing visible while the window sat in the tray.
    if msg_type == "open_video_modal" {
        let referer = parsed.get("referer").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        return match crate::prompt::show(&state.app, &url, referer, crate::prompt::PromptMode::Video) {
            Ok(()) => {
                write_response(
                    &mut socket,
                    200,
                    "OK",
                    "application/json",
                    r#"{"status":"prompt_shown"}"#,
                    allow_origin,
                )
                .await
            }
            Err(err) => {
                let payload = serde_json::json!({ "status": "error", "error": err });
                write_response(&mut socket, 200, "OK", "application/json", &payload.to_string(), allow_origin).await
            }
        };
    }

    // Show the floating prompt with the file facts and the three choices.
    // Nothing is downloaded until the user confirms there.
    let referer = parsed.get("referer").and_then(|v| v.as_str()).filter(|s| !s.is_empty());

    let mode = crate::prompt::classify(&url);
    match crate::prompt::show(&state.app, &url, referer, mode) {
        Ok(()) => {
            write_response(
                &mut socket,
                200,
                "OK",
                "application/json",
                r#"{"status":"prompt_shown"}"#,
                allow_origin,
            )
            .await
        }
        Err(err) => {
            // The prompt is the primary UX; if it cannot open, say so instead of
            // silently starting a download the user never approved.
            let payload = serde_json::json!({ "status": "error", "error": err });
            write_response(&mut socket, 200, "OK", "application/json", &payload.to_string(), allow_origin)
                .await
        }
    }
}

fn focus_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Only a browser extension may drive the bridge.
///
/// Returning `*` for everything (the previous behaviour) let *any* web page
/// `fetch()` this endpoint, satisfy the preflight and start downloads, steal
/// focus, or open the video dialog. Loopback binding does not help: a page in the
/// user's browser is already on loopback from the server's point of view.
/// Native tools (zdm-host, curl) send no Origin and are still accepted.
fn origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true, // native messaging host / curl
        Some(value) => {
            let lower = value.to_ascii_lowercase();
            lower.starts_with("chrome-extension://")
                || lower.starts_with("moz-extension://")
                || lower.starts_with("safari-web-extension://")
        }
    }
}

/// Echo the caller's origin when it is an extension, otherwise deny it.
fn cors_origin(origin: Option<&str>) -> &'static str {
    if (origin.is_some() && origin_allowed(origin)) || origin.is_none() {
        "*"
    } else {
        "null"
    }
}

async fn write_plain(socket: &mut TcpStream, status: u16, message: &str) -> std::io::Result<()> {
    let reason = match status {
        400 => "Bad Request",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    write_response(socket, status, reason, "text/plain", message, "*").await
}

async fn write_response(
    socket: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &str,
    allow_origin: &str,
) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Access-Control-Allow-Origin: {allow_origin}\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n{body}",
        len = body.len()
    );
    socket.write_all(response.as_bytes()).await?;
    socket.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_terminator_is_found() {
        // "GET / HTTP/1.1\r\nHost: x" is 23 bytes, so the blank line starts at 23.
        let request = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        assert_eq!(find_headers_end(request), Some(23));
        assert_eq!(find_headers_end(b"GET /"), None);
        // Two headers in a row must still find the first terminator.
        let two = b"A\r\n\r\nB\r\n\r\n";
        assert_eq!(find_headers_end(two), Some(1));
    }
}

/// Pull the `url` field out of a JSON body without parsing it.
///
/// The rate limiter runs before the payload is validated, and a malformed body
/// must not bypass the guard — so this is a tolerant scan, not a parse.
fn extract_url_for_throttle(body: &str) -> Option<String> {
    let needle = "\"url\"";
    let start = body.find(needle)? + needle.len();
    let rest = body[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut url = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(url),
            '\\' => {
                if let Some(escaped) = chars.next() {
                    url.push(escaped);
                }
            }
            other => url.push(other),
        }
    }
    None
}

#[cfg(test)]
mod throttle_tests {
    use super::extract_url_for_throttle;

    #[test]
    fn finds_the_url_in_a_normal_body() {
        let body = r#"{"type":"open_video_modal","url":"https://youtu.be/abc","referer":"https://x"}"#;
        assert_eq!(
            extract_url_for_throttle(body).as_deref(),
            Some("https://youtu.be/abc")
        );
    }

    #[test]
    fn url_may_come_first() {
        let body = r#"{"url":"https://a.com/z.zip","type":"download_link"}"#;
        assert_eq!(
            extract_url_for_throttle(body).as_deref(),
            Some("https://a.com/z.zip")
        );
    }

    #[test]
    fn escaped_characters_are_decoded() {
        let body = r#"{"url":"https://a.com/a\"b"}"#;
        assert_eq!(
            extract_url_for_throttle(body).as_deref(),
            Some("https://a.com/a\"b")
        );
    }

    #[test]
    fn a_body_without_a_url_yields_none() {
        assert!(extract_url_for_throttle(r#"{"type":"ping"}"#).is_none());
        assert!(extract_url_for_throttle("not json").is_none());
        assert!(extract_url_for_throttle(r#"{"url":}"#).is_none());
    }

    /// The reported bug: two different videos clicked in quick succession were
    /// throttled because the key ignored the URL, and the extension reported it
    /// as "ZDM is not running".
    #[test]
    fn different_urls_produce_different_keys() {
        let first = extract_url_for_throttle(r#"{"url":"https://youtu.be/one"}"#).unwrap();
        let second = extract_url_for_throttle(r#"{"url":"https://youtu.be/two"}"#).unwrap();
        assert_ne!(first, second);
    }
}
