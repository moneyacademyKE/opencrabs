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

use axum::extract::Query;
use axum::http::{HeaderValue, header};
use axum::response::Response;
use std::collections::HashMap;

const INDEX_HTML: &str = include_str!("miniapp/index.html");
const APP_JS: &str = include_str!("miniapp/app.js");
const CONV_JS: &str = include_str!("miniapp/conv-ui.js");
const TASKS_JS: &str = include_str!("miniapp/tasks-ui.js");
const CMD_JS: &str = include_str!("miniapp/cmd-ui.js");
const STYLE_CSS: &str = include_str!("miniapp/style.css");

/// Serve an embedded asset with a content type and aggressive anti-caching
/// so WebViews (Telegram Desktop, iOS WebKit) always fetch the fresh binary payload.
fn asset(body: &'static str, content_type: &'static str) -> Response {
    let mut resp = Response::new(axum::body::Body::from(body));
    let headers = resp.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
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

/// GET /a2a/app/conv-ui.js — conversation rendering + markdown subset.
pub async fn serve_conv_js() -> Response {
    asset(CONV_JS, "application/javascript; charset=utf-8")
}

/// GET /a2a/app/tasks-ui.js — streaming send / poll fallback / stop.
pub async fn serve_tasks_js() -> Response {
    asset(TASKS_JS, "application/javascript; charset=utf-8")
}

/// GET /a2a/app/cmd-ui.js — slash palette, dispatch, panels.
pub async fn serve_cmd_js() -> Response {
    asset(CMD_JS, "application/javascript; charset=utf-8")
}

/// GET /a2a/app/style.css — the look.
pub async fn serve_css() -> Response {
    asset(STYLE_CSS, "text/css; charset=utf-8")
}

/// GET / — the bare domain lands on the app, not a 404.
pub async fn root_redirect() -> axum::response::Redirect {
    axum::response::Redirect::temporary("/a2a/app")
}

/// GET /a2a/app/beacon — client-side diagnostic log endpoint.
/// The MiniApp JS fires this at every decision point so we can trace
/// the boot flow from server logs (WebView consoles are invisible).
pub async fn beacon(Query(params): Query<HashMap<String, String>>) -> &'static str {
    let step = params.get("s").map(|s| s.as_str()).unwrap_or("?");
    let detail = params.get("d").map(|s| s.as_str()).unwrap_or("");
    tracing::info!(step = step, detail = detail, "miniapp beacon");
    "ok"
}
