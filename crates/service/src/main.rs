//! dm-service — the always-on Windows service.
//!
//! Registers with the Service Control Manager so it starts at boot and survives
//! logout, exactly like IDMan.exe staying resident behind the tray. It hosts the
//! download engine + stats store and a loopback JSON API on 127.0.0.1:7790 that
//! the UI and browser host talk to.
//!
//! Run with `--console` for local development (no SCM required).

use engine::{download, StatsStore};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

mod webui;

const SERVICE_NAME: &str = "DownloadManager";
const API_PORT: u16 = 7790;

/// One running job's cancel handle, keyed by download id.
type Jobs = Arc<Mutex<HashMap<String, download::DownloadHandle>>>;

fn data_dir() -> PathBuf {
    std::env::var("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("download-manager")
}

/// Run the loopback API until `shutdown` flips true.
fn run_api(shutdown: &AtomicBool) {
    let store = Arc::new(match StatsStore::open(data_dir().join("stats.json")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("dm-service: cannot open stats store: {e}");
            return;
        }
    });
    let jobs: Jobs = Arc::new(Mutex::new(HashMap::new()));

    let listener = match TcpListener::bind(("127.0.0.1", API_PORT)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("dm-service: cannot bind api port {API_PORT}: {e}");
            return;
        }
    };
    let _ = listener.set_nonblocking(true);
    eprintln!("dm-service: API on http://127.0.0.1:{API_PORT}");

    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let store = store.clone();
                let jobs = jobs.clone();
                std::thread::spawn(move || {
                    let _ = handle_conn(stream, &store, &jobs);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => {
                eprintln!("dm-service: accept error: {e}");
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }
}

fn handle_conn(mut stream: TcpStream, store: &Arc<StatsStore>, jobs: &Jobs) -> std::io::Result<()> {
    let mut buf = vec![0u8; 16384];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let mut lines = req.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (target.to_string(), None),
    };

    // Static dashboard first (GET only) — served from the embedded webui.
    if method == "GET" {
        if let Some((ctype, body)) = webui::serve(&path) {
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes())?;
            return Ok(());
        }
    }

    let (status, body) = route(method, &path, query.as_deref(), &req, store, jobs);
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes())?;
    Ok(())
}

fn route(
    method: &str,
    path: &str,
    query: Option<&str>,
    raw_req: &str,
    store: &Arc<StatsStore>,
    jobs: &Jobs,
) -> (u16, String) {
    match (method, path) {
        ("GET", "/api/downloads") => {
            (200, serde_json::to_string(&store.list()).unwrap_or_else(|_| "[]".into()))
        }
        ("GET", "/api/stats/daily") => {
            (200, serde_json::to_string(&store.daily_stats()).unwrap_or_else(|_| "[]".into()))
        }
        ("GET", "/api/ping") => (200, r#"{"ok":true,"service":"dm"}"#.into()),
        ("POST", "/api/downloads") => {
            let body = raw_req.split("\r\n\r\n").nth(1).unwrap_or("");
            #[derive(serde::Deserialize)]
            struct AddReq {
                url: String,
                #[serde(default)]
                segments: Option<u32>,
            }
            match serde_json::from_str::<AddReq>(body) {
                Ok(a) => {
                    let dest = data_dir().join("downloads");
                    let segs = a.segments.unwrap_or(8);
                    match download::start(store.clone(), &a.url, &dest, segs) {
                        Ok(h) => {
                            if let Ok(mut j) = jobs.lock() {
                                j.insert(h.id.clone(), h);
                            }
                            (200, r#"{"ok":true}"#.into())
                        }
                        Err(e) => (500, format!(r#"{{"error":"{e}"}}"#)),
                    }
                }
                Err(e) => (400, format!(r#"{{"error":"{e}"}}"#)),
            }
        }
        ("POST", p) if p.starts_with("/api/downloads/") && p.ends_with("/cancel") => {
            let id = p.trim_start_matches("/api/downloads/").trim_end_matches("/cancel");
            if let Ok(mut j) = jobs.lock() {
                if let Some(job) = j.remove(id) {
                    job.cancel.store(true, Ordering::SeqCst);
                }
            }
            (200, r#"{"ok":true}"#.into())
        }
        ("DELETE", p) if p.starts_with("/api/downloads/") => {
            let id = p.trim_start_matches("/api/downloads/");
            match store.remove(id) {
                Ok(true) => (200, r#"{"ok":true}"#.into()),
                Ok(false) => (404, r#"{"error":"not found"}"#.into()),
                Err(e) => (500, format!(r#"{{"error":"{e}"}}"#)),
            }
        }
        _ => {
            let _ = query;
            (404, r#"{"error":"not found"}"#.into())
        }
    }
}

/// Console / service entry. `--console` runs it as a plain foreground process.
fn main() {
    let console = std::env::args().any(|a| a == "--console");
    if console {
        eprintln!("dm-service: console mode, Ctrl+C to quit");
        let shutdown = AtomicBool::new(false);
        run_api(&shutdown);
        return;
    }

    // Hand control to the Service Control Manager. This only returns on error
    // (e.g. we weren't launched by the SCM) — then fall back to console mode so
    // the binary is always usable.
    if let Err(e) = start_service_dispatcher() {
        eprintln!("dm-service: not running as a service ({e}); using console mode");
        let shutdown = AtomicBool::new(false);
        run_api(&shutdown);
    }
}

/// Route to the SCM dispatcher via the generated FFI shim.
#[cfg(windows)]
fn start_service_dispatcher() -> Result<(), windows_service::Error> {
    windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

// Generate the FFI `service_main` shim the SCM calls; it delegates to our
// higher-level `service_main(Vec<OsString>)`.
#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, service_main);

/// The higher-level service body, called on a background thread by the SCM via
/// the FFI shim generated below.
#[cfg(windows)]
fn service_main(_args: Vec<std::ffi::OsString>) {
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    static STOP: AtomicBool = AtomicBool::new(false);

    let event_handler = move |control: ServiceControl| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            STOP.store(true, Ordering::SeqCst);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };

    let status_handle = match service_control_handler::register(SERVICE_NAME, event_handler) {
        Ok(h) => h,
        Err(_) => return,
    };

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(3),
        process_id: None,
    });

    run_api(&STOP);

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    });
}
