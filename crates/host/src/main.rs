//! dm-host — browser native-messaging host (the IEMonitor.exe equivalent).
//!
//! The browser extension launches this binary and talks over stdin/stdout with
//! Chrome/Firefox native-messaging framing (4-byte little-endian length + JSON).
//! It forwards intercepted downloads to the dm-service loopback API.

use std::io::{self, Read, Write};

fn main() {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        // native messaging header: 4-byte LE length
        let mut len_buf = [0u8; 4];
        if input.read_exact(&mut len_buf).is_err() {
            break; // browser closed the pipe
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 || len > (1 << 20) {
            break;
        }
        let mut msg = vec![0u8; len];
        if input.read_exact(&mut msg).is_err() {
            break;
        }
        let text = String::from_utf8_lossy(&msg);
        let response = handle(&text);
        send(&response);
    }
}

/// Accept a JSON message from the extension, forward it to the service.
fn handle(msg: &str) -> String {
    #[derive(serde::Deserialize)]
    struct FromBrowser {
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        filename: Option<String>,
    }
    match serde_json::from_str::<FromBrowser>(msg) {
        Ok(m) => {
            if let Some(url) = m.url {
                let body = serde_json::json!({ "url": url, "filename": m.filename }).to_string();
                match forward(&body) {
                    Ok(service_resp) => {
                        // The service returns {"ok":true,...}. Parse it and pass
                        // it through rather than double-wrapping as a string.
                        match serde_json::from_str::<serde_json::Value>(&service_resp) {
                            Ok(v) => v.to_string(),
                            Err(_) => serde_json::json!({ "ok": false, "error": "bad service response" }).to_string(),
                        }
                    }
                    Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
                }
            } else {
                serde_json::json!({ "ok": false, "error": "no url" }).to_string()
            }
        }
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
    }
}

/// POST to the service loopback API over loopback HTTP (raw TCP, no deps).
fn forward(body: &str) -> io::Result<String> {
    use std::net::TcpStream;
    let mut stream = TcpStream::connect("127.0.0.1:7790")?;
    let req = format!(
        "POST /api/downloads HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(req.as_bytes())?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp)?;
    // body is after \r\n\r\n
    let body = resp.split("\r\n\r\n").nth(1).unwrap_or("{}");
    Ok(body.to_string())
}

fn send(json: &str) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let bytes = json.as_bytes();
    let _ = out.write_all(&(bytes.len() as u32).to_le_bytes());
    let _ = out.write_all(bytes);
    let _ = out.flush();
}
