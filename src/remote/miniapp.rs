//! Mini App: the gateway's own web face, served same-origin at `/a2a/app`.
//!
//! Phase 3 v1 (spec: `research/remote-phase3-plan.md` in the OpenCrabs
//! home): an embedded, dependency-free SPA — pair-code login against
//! `POST /a2a/pair`, then `message/send` + `tasks/get` polling through the
//! same Bearer gate every other client uses. Serving the app from the
//! gateway keeps it same-origin: zero CORS configuration, zero extra
//! infrastructure, one auth surface. The shell is public like `/a2a/health`
//! — it embeds no secrets; the pair rate limiter and the JSON-RPC Bearer
//! gate are the boundary.

use axum::http::{HeaderValue, header};
use axum::response::Response;

const INDEX_HTML: &str = include_str!("miniapp/index.html");
const APP_JS: &str = include_str!("miniapp/app.js");
const STYLE_CSS: &str = include_str!("miniapp/style.css");

/// Serve an embedded asset with a content type and `no-cache` — the binary
/// is the cache key: assets only change when the daemon does.
fn asset(body: &'static str, content_type: &'static str) -> Response {
    let mut resp = Response::new(axum::body::Body::from(body));
    let headers = resp.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

/// GET /a2a/app — the SPA shell.
pub async fn serve_index() -> Response {
    asset(INDEX_HTML, "text/html; charset=utf-8")
}

/// GET /a2a/app/app.js — the client.
pub async fn serve_js() -> Response {
    asset(APP_JS, "application/javascript; charset=utf-8")
}

/// GET /a2a/app/style.css — the look.
pub async fn serve_css() -> Response {
    asset(STYLE_CSS, "text/css; charset=utf-8")
}
