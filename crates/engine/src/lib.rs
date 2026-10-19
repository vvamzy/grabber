//! Engine core: OS-native (WinHTTP) multi-segment downloads + daily stats.

pub mod download;
pub mod stats;
pub mod winhttp;

pub use stats::{format_bps, format_bytes, DailyStat, DownloadRecord, StatsStore};
