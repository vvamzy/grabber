//! Loopback client for the dm-service JSON API.
//!
//! Raw TCP + minimal HTTP/1.1 (the service always answers `Connection: close`),
//! so we need no HTTP crate. Used by the Win32 window to fetch the download list
//! and daily stats and to enqueue new downloads.

use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const API: &str = "127.0.0.1:7790";
const CONNECT_TIMEOUT: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, Deserialize)]
pub struct Download {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub segments: u32,
    pub status: String,
    #[serde(default)]
    pub peak_bps: u64,
    #[serde(default)]
    pub avg_bps: u64,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DailyStat {
    pub day: String,
    pub downloads: u32,
    pub total_bytes: u64,
    pub top_bps: u64,
    #[serde(default)]
    pub top_speed_name: Option<String>,
    #[serde(default)]
    pub attempts: u32,
}

/// Split an HTTP response into (status, body), tolerant of the simple service.
fn split_response(raw: &str) -> (u16, String) {
    let (head, body) = match raw.split_once("\r\n\r\n") {
        Some((h, b)) => (h, b),
        None => (raw, ""),
    };
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    // Handle Content-Length (service sends it; body isn't chunked).
    (status, body.to_string())
}

fn http(method: &str, path: &str, body: Option<&str>) -> Result<(u16, String), String> {
    let addr = API.to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| "bad api addr".to_string())?;
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    let b = body.unwrap_or("");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n{}\r\n{}",
        if b.is_empty() {
            String::new()
        } else {
            format!("Content-Type: application/json\r\nContent-Length: {}\r\n", b.len())
        },
        b
    );
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;

    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    Ok(split_response(&raw))
}

pub fn ping() -> bool {
    http("GET", "/api/ping", None).map(|(s, _)| s == 200).unwrap_or(false)
}

pub fn list_downloads() -> Result<Vec<Download>, String> {
    let (s, body) = http("GET", "/api/downloads", None)?;
    if s != 200 {
        return Err(format!("http {s}"));
    }
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

pub fn daily_stats() -> Result<Vec<DailyStat>, String> {
    let (s, body) = http("GET", "/api/stats/daily", None)?;
    if s != 200 {
        return Err(format!("http {s}"));
    }
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

pub fn add_download(url: &str, segments: u32) -> Result<(), String> {
    let payload = serde_json::json!({ "url": url, "segments": segments }).to_string();
    let (s, body) = http("POST", "/api/downloads", Some(&payload))?;
    if s != 200 {
        return Err(if body.is_empty() { format!("http {s}") } else { body });
    }
    Ok(())
}

pub fn cancel_download(id: &str) -> Result<(), String> {
    let (s, _) = http("POST", &format!("/api/downloads/{id}/cancel"), None)?;
    if s != 200 {
        return Err(format!("http {s}"));
    }
    Ok(())
}

pub fn delete_download(id: &str) -> Result<(), String> {
    let (s, _) = http("DELETE", &format!("/api/downloads/{id}"), None)?;
    if s != 200 {
        return Err(format!("http {s}"));
    }
    Ok(())
}
