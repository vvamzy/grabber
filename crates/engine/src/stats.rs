//! On-disk download history + the daily aggregation the UI shows.
//!
//! Mirrors the IDM idea: the engine records one entry per completed download
//! (bytes, peak speed, timestamp). Daily stats are derived from that log so the
//! dashboard can show downloads/day, total size/day, and the day's top speed
//! without keeping a live UI resident.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub segments: u32,
    pub status: String, // "running" | "completed" | "failed" | "cancelled"
    pub started_at: u64,     // unix ms
    pub completed_at: Option<u64>,
    pub peak_bps: u64,       // bytes/sec peak
    pub avg_bps: u64,
    pub error: Option<String>,
}

/// One day's roll-up, exactly what the stats cards/table render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStat {
    pub day: String,          // "YYYY-MM-DD" (local)
    pub downloads: u32,       // completed count
    pub total_bytes: u64,     // summed completed size
    pub top_bps: u64,         // max peak speed among the day's downloads
    pub top_speed_name: Option<String>,
    pub attempts: u32,        // everything started that day
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    downloads: Vec<DownloadRecord>,
}

pub struct StatsStore {
    path: PathBuf,
}

impl StatsStore {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self { path })
    }

    fn load(&self) -> Store {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, store: &Store) -> io::Result<()> {
        let json = serde_json::to_string_pretty(store).unwrap_or_default();
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?; // atomic swap
        Ok(())
    }

    pub fn upsert(&self, rec: &DownloadRecord) -> io::Result<()> {
        let mut store = self.load();
        if let Some(existing) = store.downloads.iter_mut().find(|d| d.id == rec.id) {
            *existing = rec.clone();
        } else {
            store.downloads.push(rec.clone());
        }
        self.save(&store)
    }

    pub fn remove(&self, id: &str) -> io::Result<bool> {
        let mut store = self.load();
        let before = store.downloads.len();
        store.downloads.retain(|d| d.id != id);
        let removed = store.downloads.len() < before;
        if removed {
            self.save(&store)?;
        }
        Ok(removed)
    }

    pub fn get(&self, id: &str) -> Option<DownloadRecord> {
        self.load().downloads.into_iter().find(|d| d.id == id)
    }

    /// Newest-first list of every recorded download.
    pub fn list(&self) -> Vec<DownloadRecord> {
        let mut v = self.load().downloads;
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        v
    }

    /// Group completed downloads by local calendar day and roll up the stats.
    pub fn daily_stats(&self) -> Vec<DailyStat> {
        use std::collections::HashMap;
        let mut by_day: HashMap<String, DailyStat> = HashMap::new();

        for r in &self.load().downloads {
            let ts = r.completed_at.unwrap_or(r.started_at);
            if ts == 0 {
                continue;
            }
            let key = day_key(ts);
            let entry = by_day.entry(key.clone()).or_insert_with(|| DailyStat {
                day: key,
                downloads: 0,
                total_bytes: 0,
                top_bps: 0,
                top_speed_name: None,
                attempts: 0,
            });

            entry.attempts += 1;
            if r.status == "completed" {
                entry.downloads += 1;
                entry.total_bytes += if r.total_bytes > 0 {
                    r.total_bytes
                } else {
                    r.downloaded_bytes
                };
                if r.peak_bps > entry.top_bps {
                    entry.top_bps = r.peak_bps;
                    entry.top_speed_name = Some(r.filename.clone());
                }
            }
        }

        let mut v: Vec<DailyStat> = by_day.into_values().collect();
        v.sort_by(|a, b| b.day.cmp(&a.day)); // newest day first
        v
    }
}

/// "YYYY-MM-DD" in the local timezone, from unix ms.
pub fn day_key(unix_ms: u64) -> String {
    use chrono::{TimeZone, Utc};
    let secs = (unix_ms / 1000) as i64;
    let millis = (unix_ms % 1000) as u32;
    Utc.timestamp_opt(secs, millis)
        .single()
        .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

// ---- human formatting (used by CLI + mirrored in the UI) ----

pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if n == 0 {
        return "0 B".into();
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 || v >= 100.0 {
        format!("{:.0} {}", v, UNITS[i])
    } else {
        format!("{:.1} {}", v, UNITS[i])
    }
}

pub fn format_bps(bps: u64) -> String {
    format!("{}/s", format_bytes(bps))
}
