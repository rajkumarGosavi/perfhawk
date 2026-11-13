//! DNS-rebinding / cross-origin guard: only loopback Host (and Origin) values.

use crate::{api::ApiError, AppState};
use axum::{
    extract::{Request, State},
    http::header::{HOST, ORIGIN},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

fn host_ok(host: &str, port: Option<u16>) -> bool {
    let host = host.to_ascii_lowercase();
    let (name, p) = match host.rsplit_once(':') {
        Some((n, p)) if !p.contains(']') => (n, Some(p)),
        _ => (host.as_str(), None),
    };
    if !matches!(name, "127.0.0.1" | "localhost" | "[::1]") {
        return false;
    }
    match (port, p) {
        (Some(want), Some(got)) => got.parse::<u16>().ok() == Some(want),
        (Some(_), None) => false,
        (None, _) => true,
    }
}

pub async fn host_origin_guard(
    State(s): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let h = req.headers();
    let host_good = h
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| host_ok(v, s.port));
    if !host_good {
        return ApiError::forbidden("host not allowed").into_response();
    }
    if let Some(o) = h.get(ORIGIN) {
        let origin_good = o
            .to_str()
            .ok()
            .and_then(|v| v.strip_prefix("http://"))
            .is_some_and(|v| host_ok(v, s.port));
        if !origin_good {
            return ApiError::forbidden("origin not allowed").into_response();
        }
    }
    next.run(req).await
}

/// Browser hardening headers for every response (UI and API).
///
/// The server is local-only; these stop another page from framing the UI
/// (clickjacking), sniffing types, or leaking the URL, and restrict what the
/// embedded UI itself may load.
pub async fn security_headers(req: Request, next: Next) -> Response {
    use axum::http::{header::HeaderName, HeaderValue};
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let set = |h: &mut axum::http::HeaderMap, k: &'static str, v: &'static str| {
        h.insert(HeaderName::from_static(k), HeaderValue::from_static(v));
    };
    set(h, "x-frame-options", "DENY");
    set(h, "x-content-type-options", "nosniff");
    set(h, "referrer-policy", "no-referrer");
    set(
        h,
        "content-security-policy",
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';          img-src 'self' data: blob:; connect-src 'self'; object-src 'none';          base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
    );
    res
}
