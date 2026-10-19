//! dm-cli — command-line front end for the engine.
//!
//! Usage:
//!   dm-cli add <url> [--dest DIR] [--segments N] [--store FILE]
//!   dm-cli list   [--store FILE]
//!   dm-cli daily  [--store FILE]

use engine::{download, format_bps, format_bytes, DailyStat, StatsStore};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn store_path(args: &[String]) -> PathBuf {
    args.iter()
        .position(|a| a == "--store")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/stats.json"))
}

fn flag(args: &[String], name: &str, default: &str) -> String {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let store = Arc::new(StatsStore::open(store_path(&args)).expect("open stats store"));

    match cmd {
        "add" => {
            let url = args.get(1).expect("usage: dm-cli add <url>").clone();
            let dest = PathBuf::from(flag(&args, "--dest", "downloads"));
            let segments: u32 = flag(&args, "--segments", "8").parse().unwrap_or(8);
            match download::start(store.clone(), &url, &dest, segments) {
                Ok(h) => {
                    println!("started {} -> {}", h.id, dest.display());
                    // wait for it to finish (CLI is synchronous for now)
                    loop {
                        std::thread::sleep(Duration::from_millis(500));
                        let Some(rec) = store.get(&h.id) else { continue };
                        match rec.status.as_str() {
                            "completed" => {
                                println!(
                                    "completed: {}  ({})  peak {}",
                                    rec.filename,
                                    format_bytes(rec.total_bytes),
                                    format_bps(rec.peak_bps)
                                );
                                break;
                            }
                            "failed" => {
                                eprintln!("failed: {}", rec.error.unwrap_or_default());
                                std::process::exit(1);
                            }
                            "cancelled" => {
                                eprintln!("cancelled");
                                std::process::exit(2);
                            }
                            _ => {
                                let pct = if rec.total_bytes > 0 {
                                    rec.downloaded_bytes * 100 / rec.total_bytes
                                } else {
                                    0
                                };
                                eprint!(
                                    "\r  {} / {} ({}%)",
                                    format_bytes(rec.downloaded_bytes),
                                    format_bytes(rec.total_bytes),
                                    pct
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
        "list" => {
            for r in store.list() {
                println!(
                    "{:<10} {:<10} {:>12}  peak {:>12}  {}",
                    r.id,
                    r.status,
                    format_bytes(r.total_bytes),
                    format_bps(r.peak_bps),
                    r.filename
                );
            }
        }
        "daily" => {
            let stats = store.daily_stats();
            if stats.is_empty() {
                println!("no downloads recorded yet");
                return;
            }
            println!("{:<12} {:>9} {:>12} {:>14}", "DAY", "COUNT", "TOTAL", "TOP SPEED");
            for DailyStat { day, downloads, total_bytes, top_bps, .. } in stats {
                println!(
                    "{:<12} {:>9} {:>12} {:>14}",
                    day,
                    downloads,
                    format_bytes(total_bytes),
                    format_bps(top_bps)
                );
            }
        }
        _ => {
            println!("dm-cli — download manager engine");
            println!("  dm-cli add <url> [--dest DIR] [--segments N] [--store FILE]");
            println!("  dm-cli list [--store FILE]");
            println!("  dm-cli daily [--store FILE]");
        }
    }
}
