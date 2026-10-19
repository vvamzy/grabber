//! The dashboard, baked into the service binary.
//!
//! `include_str!` embeds the web assets at compile time so dm-service.exe is a
//! single self-contained file that serves both the JSON API and the UI. No
//! separate frontend server, no runtime files to ship.

pub const INDEX_HTML: &str = include_str!("webui/index.html");
pub const STYLE_CSS: &str = include_str!("webui/style.css");
pub const APP_JS: &str = include_str!("webui/app.js");

/// Map a request path to (content_type, body) for the static dashboard.
pub fn serve(path: &str) -> Option<(&'static str, &'static str)> {
    match path {
        "/" | "/index.html" => Some(("text/html; charset=utf-8", INDEX_HTML)),
        "/style.css" => Some(("text/css; charset=utf-8", STYLE_CSS)),
        "/app.js" => Some(("application/javascript; charset=utf-8", APP_JS)),
        _ => None,
    }
}
