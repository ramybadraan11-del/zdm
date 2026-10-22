//! zdm-host: Native Messaging Host for Zyzoom Download Manager
//! Developed by Ramy Badraan
//!
//! Receives native messaging packets from Chromium & Firefox extensions
//! and forwards them to the running ZDM Desktop core over local socket/IPC.

use byteorder::{NativeEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct DownloadMessage {
    #[serde(rename = "type")]
    msg_type: String,
    url: Option<String>,
    filename: Option<String>,
    referer: Option<String>,
    user_agent: Option<String>,
    cookies: Option<String>,
    headers: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HostResponse {
    status: String,
    message: String,
    app: String,
    author: String,
}

fn send_response(response: &HostResponse) -> io::Result<()> {
    let json_bytes = serde_json::to_vec(response)?;
    let mut stdout = io::stdout().lock();
    stdout.write_u32::<NativeEndian>(json_bytes.len() as u32)?;
    stdout.write_all(&json_bytes)?;
    stdout.flush()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stdin = io::stdin().lock();

    loop {
        let length = match stdin.read_u32::<NativeEndian>() {
            Ok(len) => len as usize,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };

        if length == 0 || length > 10 * 1024 * 1024 {
            // Guard against invalid message size
            break;
        }

        let mut buffer = vec![0u8; length];
        stdin.read_exact(&mut buffer)?;

        if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&buffer) {
            // Forward to ZDM Desktop local IPC port (default 46800)
            let mut stream_res = TcpStream::connect("127.0.0.1:46800").await;
            if stream_res.is_err() {
                // If ZDM is not running, auto-launch it just like IDM
                let home_bin = std::env::var("HOME")
                    .map(|h| format!("{}/.local/bin/zdm", h))
                    .unwrap_or_default();

                let mut launched = false;
                if !home_bin.is_empty() && std::path::Path::new(&home_bin).exists() {
                    if std::process::Command::new(&home_bin).spawn().is_ok() {
                        launched = true;
                    }
                }
                if !launched {
                    for c in ["zdm", "/usr/local/bin/zdm"] {
                        if std::process::Command::new(c).spawn().is_ok() {
                            launched = true;
                            break;
                        }
                    }
                }

                if launched {
                    for _ in 0..25 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        if let Ok(s) = TcpStream::connect("127.0.0.1:46800").await {
                            stream_res = Ok(s);
                            break;
                        }
                    }
                }
            }

            let forwarded = if let Ok(mut stream) = stream_res {
                let payload = serde_json::to_vec(&msg).unwrap_or_default();
                let _ = stream.write_all(&payload).await;
                let _ = stream.write_all(b"\n").await;
                true
            } else {
                false
            };

            let resp = HostResponse {
                status: if forwarded { "queued".into() } else { "desktop_not_running".into() },
                message: if forwarded {
                    "Sent to ZDM Core".into()
                } else {
                    "ZDM Desktop is offline. Please launch ZDM.".into()
                },
                app: "Zyzoom Download Manager (ZDM)".into(),
                author: "Developed by Ramy Badraan".into(),
            };

            let _ = send_response(&resp);
        }
    }

    Ok(())
}
