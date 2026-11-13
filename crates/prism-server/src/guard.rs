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
