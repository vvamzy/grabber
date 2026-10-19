//! Multi-segment download engine.
//!
//! Follows IDM's model: probe the server for size + Range support, then open N
//! concurrent Range requests (one socket + fixed buffer each), writing each
//! segment to its own `.part` file, then concatenate into the final file. RAM
//! scales with active sockets, not a framework.

use crate::stats::{day_key, DownloadRecord, StatsStore};
use crate::winhttp::{Connection, Session};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) DownloadManager/0.1";
const READ_BUF: usize = 64 * 1024; // per-connection fixed buffer

pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String, // includes query string
    pub secure: bool,
}

pub fn parse_url(raw: &str) -> io::Result<ParsedUrl> {
    let u = url::Url::parse(raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let scheme = u.scheme().to_string();
    let secure = scheme == "https";
    let host = u.host_str().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no host"))?.to_string();
    let port = u.port_or_known_default().unwrap_or(if secure { 443 } else { 80 });
    let mut path = u.path().to_string();
    if let Some(q) = u.query() {
        path.push('?');
        path.push_str(q);
    }
    if path.is_empty() {
        path = "/".into();
    }
    Ok(ParsedUrl { scheme, host, port, path, secure })
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Shared live state for one running download.
struct Live {
    downloaded: AtomicU64,
    peak_bps: AtomicU64,
    cancel: AtomicBool,
    error: Mutex<Option<String>>,
}

pub struct DownloadHandle {
    pub id: String,
    pub cancel: Arc<AtomicBool>,
}

/// Probe size + range support with a HEAD (fallback to a 0-byte ranged GET).
fn probe(session: &Session, url: &ParsedUrl) -> io::Result<(u64, bool)> {
    let conn = session.connect(&url.host, url.port)?;
    let req = conn.request("HEAD", &url.path, url.secure, "")?;
    req.send(0)?;
    let status = req.status()?;
    let headers = req.headers_text().unwrap_or_default();

    if status >= 400 {
        // Some servers reject HEAD; retry with a tiny ranged GET.
        return probe_get(session, url);
    }
    let (len, ranges) = parse_headers(&headers);
    if len.unwrap_or(0) == 0 {
        // No usable Content-Length on HEAD; try a ranged GET for the real total.
        let (get_len, get_ranges) = probe_get(session, url)?;
        if get_len > 0 {
            return Ok((get_len, get_ranges));
        }
    }
    Ok((len.unwrap_or(0), ranges))
}

fn probe_get(session: &Session, url: &ParsedUrl) -> io::Result<(u64, bool)> {
    let conn = session.connect(&url.host, url.port)?;
    let req = conn.request("GET", &url.path, url.secure, "Range: bytes=0-0\r\n")?;
    req.send(0)?;
    let status = req.status()?;
    let headers = req.headers_text().unwrap_or_default();
    // drain body so the connection can be reused/closed cleanly
    let mut b = [0u8; 512];
    while req.read(&mut b).unwrap_or(0) > 0 {}
    let (total, ranges) = parse_content_range(&headers, status);
    Ok((total, ranges))
}

fn parse_headers(headers: &str) -> (Option<u64>, bool) {
    let mut len = None;
    let mut ranges = false;
    for line in headers.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            len = v.trim().parse::<u64>().ok();
        } else if lower.starts_with("accept-ranges:") && lower.contains("bytes") {
            ranges = true;
        }
    }
    (len, ranges)
}

fn parse_content_range(headers: &str, status: u16) -> (u64, bool) {
    let mut total = 0u64;
    let mut got_range = false;
    for line in headers.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-range:") {
            // "bytes 0-0/12345"
            got_range = true;
            if let Some((_, total_part)) = v.split_once('/') {
                total = total_part.trim().parse::<u64>().unwrap_or(0);
            }
        } else if lower.starts_with("content-length:") {
            if let Some(l) = lower.strip_prefix("content-length:") {
                if total == 0 {
                    total = l.trim().parse::<u64>().unwrap_or(0);
                }
            }
        }
    }
    let ranges = got_range || status == 206;
    (total, ranges)
}

/// Start a download. Returns a handle for cancellation.
pub fn start(
    store: Arc<StatsStore>,
    raw_url: &str,
    dest_dir: &Path,
    segments: u32,
) -> io::Result<DownloadHandle> {
    let url = parse_url(raw_url)?;
    let id = format!("dl_{}", now_ms());
    let filename = filename_from(&url);
    let dest = safe_join(dest_dir, &filename);

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_thread = cancel.clone();
    let id_thread = id.clone();

    let rec = DownloadRecord {
        id: id.clone(),
        url: raw_url.to_string(),
        filename: filename.clone(),
        total_bytes: 0,
        downloaded_bytes: 0,
        segments,
        status: "running".into(),
        started_at: now_ms(),
        completed_at: None,
        peak_bps: 0,
        avg_bps: 0,
        error: None,
    };
    let _ = store.upsert(&rec);

    let url_thread = url;
    std::thread::spawn(move || {
        let result = run(&store, &id_thread, &url_thread, &dest, segments, &cancel_thread);
        let mut rec = store.get(&id_thread).unwrap_or(rec);
        rec.completed_at = Some(now_ms());
        match result {
            Ok((bytes, peak, avg)) => {
                rec.status = "completed".into();
                rec.downloaded_bytes = bytes;
                rec.total_bytes = bytes;
                rec.peak_bps = peak;
                rec.avg_bps = avg;
            }
            Err(_e) if cancel_thread.load(Ordering::SeqCst) => {
                rec.status = "cancelled".into();
                rec.error = Some("cancelled".into());
            }
            Err(e) => {
                rec.status = "failed".into();
                rec.error = Some(e.to_string());
            }
        }
        let _ = store.upsert(&rec);
    });

    Ok(DownloadHandle { id, cancel })
}

fn filename_from(url: &ParsedUrl) -> String {
    let last = url.path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("download");
    let decoded = urlencoding_decode(last);
    if decoded.is_empty() { "download".into() } else { decoded }
}

fn urlencoding_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => { out.push(b); i += 3; }
                    Err(_) => { out.push(bytes[i]); i += 1; }
                }
            }
            b'+' => { out.push(b' '); i += 1; }
            b => { out.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn safe_join(dir: &Path, name: &str) -> PathBuf {
    let cleaned: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let cleaned = if cleaned.trim().is_empty() { "download".into() } else { cleaned };
    dir.join(cleaned)
}

/// The actual multi-segment transfer. Returns (bytes, peak_bps, avg_bps).
fn run(
    store: &Arc<StatsStore>,
    id: &str,
    url: &ParsedUrl,
    dest: &Path,
    segments: u32,
    cancel: &Arc<AtomicBool>,
) -> io::Result<(u64, u64, u64)> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let session = Session::open(USER_AGENT)?;
    let (total, ranges) = probe(&session, url)?;

    if total == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "server did not report a size"));
    }

    // Single stream fallback when the server ignores Range.
    if !ranges || segments <= 1 {
        let bytes = single_stream(&session, url, dest, total, cancel)?;
        let (peak, avg) = finish_progress(store, id, bytes, total);
        return Ok((bytes, peak, avg));
    }

    let segs = segments.min(total as u32).max(1);
    let per = total / segs as u64;
    let live = Arc::new(Live {
        downloaded: AtomicU64::new(0),
        peak_bps: AtomicU64::new(0),
        cancel: AtomicBool::new(false),
        error: Mutex::new(None),
    });

    let start_time = Instant::now();
    let mut handles = Vec::new();
    let part_paths: Vec<PathBuf> = (0..segs).map(|i| dest.with_extension(format!("part.{i}"))).collect();

    for i in 0..segs {
        let start = i as u64 * per;
        let end = if i == segs - 1 { total - 1 } else { start + per - 1 };
        let part = part_paths[i as usize].clone();
        let live = live.clone();
        let url = url.try_clone().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "cannot clone url"))?;
        let cancel = cancel.clone();
        handles.push(std::thread::spawn(move || {
            // Each segment thread owns its own WinHTTP session (connection
            // pool). This is the "one socket per segment" model.
            let result = Session::open(USER_AGENT).and_then(|sess| segment(&sess, &url, start, end, &part, &live, &cancel));
            if let Err(e) = result {
                if let Ok(mut g) = live.error.lock() {
                    if g.is_none() { *g = Some(e.to_string()); }
                }
                live.cancel.store(true, Ordering::SeqCst);
            }
        }));
    }

    // progress pump: periodically sample speed + persist progress
    let progress_live = live.clone();
    let progress_store = store.clone();
    let progress_id = id.to_string();
    let progress_total = total;
    let progress_cancel = cancel.clone();
    let progress = std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if progress_cancel.load(Ordering::SeqCst) {
            progress_live.cancel.store(true, Ordering::SeqCst);
        }
        let done = progress_live.downloaded.load(Ordering::Relaxed);
        let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
        let cur = (done as f64 / elapsed) as u64;
        if cur > progress_live.peak_bps.load(Ordering::Relaxed) {
            progress_live.peak_bps.store(cur, Ordering::Relaxed);
        }
        if let Some(mut rec) = progress_store.get(&progress_id) {
            rec.downloaded_bytes = done;
            rec.total_bytes = progress_total;
            rec.peak_bps = progress_live.peak_bps.load(Ordering::Relaxed);
            let _ = progress_store.upsert(&rec);
        }
        if done >= progress_total {
            break;
        }
    });

    for h in handles {
        let _ = h.join();
    }
    let _ = progress.join();

    if let Some(err) = live.error.lock().ok().and_then(|g| g.clone()) {
        cleanup_parts(&part_paths);
        return Err(io::Error::other(err));
    }
    if cancel.load(Ordering::SeqCst) {
        cleanup_parts(&part_paths);
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }

    // concatenate parts in order
    let mut out = File::create(dest)?;
    for part in &part_paths {
        let mut f = File::open(part)?;
        io::copy(&mut f, &mut out)?;
    }
    drop(out);
    cleanup_parts(&part_paths);

    let bytes = live.downloaded.load(Ordering::Relaxed).min(total);
    let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
    let avg = (bytes as f64 / elapsed) as u64;
    let peak = live.peak_bps.load(Ordering::Relaxed).max(avg);
    Ok((bytes, peak, avg))
}

impl ParsedUrl {
    fn try_clone(&self) -> Option<ParsedUrl> {
        Some(ParsedUrl {
            scheme: self.scheme.clone(),
            host: self.host.clone(),
            port: self.port,
            path: self.path.clone(),
            secure: self.secure,
        })
    }
}

fn segment(
    session: &Session,
    url: &ParsedUrl,
    start: u64,
    end: u64,
    part: &Path,
    live: &Live,
    cancel: &Arc<AtomicBool>,
) -> io::Result<()> {
    let conn: Connection = session.connect(&url.host, url.port)?;
    let range = format!("Range: bytes={start}-{end}\r\n");
    let req = conn.request("GET", &url.path, url.secure, &range)?;
    req.send(0)?;
    let status = req.status()?;
    if status >= 400 {
        return Err(io::Error::other(format!("HTTP {status} for range {start}-{end}")));
    }

    let mut out = File::create(part)?;
    let mut buf = vec![0u8; READ_BUF];
    let mut remaining = end - start + 1;
    while remaining > 0 {
        if cancel.load(Ordering::Relaxed) || live.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let want = (remaining as usize).min(READ_BUF);
        let n = req.read(&mut buf[..want])?;
        if n == 0 {
            break; // connection closed early
        }
        out.write_all(&buf[..n])?;
        remaining -= n as u64;
        live.downloaded.fetch_add(n as u64, Ordering::Relaxed);
    }
    Ok(())
}

fn single_stream(
    session: &Session,
    url: &ParsedUrl,
    dest: &Path,
    total: u64,
    cancel: &Arc<AtomicBool>,
) -> io::Result<u64> {
    let conn = session.connect(&url.host, url.port)?;
    let req = conn.request("GET", &url.path, url.secure, "")?;
    req.send(0)?;
    let status = req.status()?;
    if status >= 400 {
        return Err(io::Error::other(format!("HTTP {status}")));
    }
    let mut out = File::create(dest)?;
    let mut buf = vec![0u8; READ_BUF];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = req.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
    }
    Ok(done.min(total).max(done))
}

fn cleanup_parts(parts: &[PathBuf]) {
    for p in parts {
        let _ = fs::remove_file(p);
    }
}

/// Sample a final speed snapshot for the record.
fn finish_progress(store: &Arc<StatsStore>, id: &str, _bytes: u64, _total: u64) -> (u64, u64) {
    let rec = store.get(id);
    match rec {
        Some(r) => (r.peak_bps, r.avg_bps),
        None => (0, 0),
    }
}

// keep day_key referenced for callers that aggregate on completion
#[allow(dead_code)]
fn _day_of(ts: u64) -> String {
    day_key(ts)
}
