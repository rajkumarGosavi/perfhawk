//! Capture endpoints: what can be recorded and what each target supports.

use super::{ApiError, ApiResult, St, Q};
use axum::{extract::State, response::Response, routing::get, Json, Router};
use perfhawk_capture::{list_targets, CaptureError, Target};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub fn routes() -> Router<St> {
    Router::new()
        .route("/api/targets", get(targets_h))
        .route("/api/capabilities", get(capabilities_h))
}

/// Exactly one of `pid` or `url`; a url must be http(s) on a loopback host with no
/// credentials. The browser can attach to local processes only.
pub(crate) fn parse_target(pid: Option<u32>, url: Option<&str>) -> ApiResult<Target> {
    match (pid, url) {
        (Some(_), Some(_)) | (None, None) => {
            Err(ApiError::bad("choose one target: a pid or a url"))
        }
        (Some(0), None) => Err(ApiError::bad("pid must be a positive number")),
        (Some(p), None) => Ok(Target::Pid(p)),
        (None, Some(u)) => {
            let bad = || ApiError::bad("url must be http or https on 127.0.0.1, localhost or [::1]");
            let parsed = url::Url::parse(u).map_err(|_| bad())?;
            if !matches!(parsed.scheme(), "http" | "https")
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(bad());
            }
            let loopback = match parsed.host() {
                Some(url::Host::Domain(d)) => d == "localhost",
                Some(url::Host::Ipv4(a)) => a.is_loopback(),
                Some(url::Host::Ipv6(a)) => a.is_loopback(),
                None => false,
            };
            if loopback {
                Ok(Target::Url(u.to_string()))
            } else {
                Err(bad())
            }
        }
    }
}

pub(crate) fn capture_error(e: CaptureError) -> ApiError {
    match e {
        CaptureError::NotDetected(m) => ApiError::bad_code("not_detected", m),
        CaptureError::Unsupported { .. } => ApiError::bad_code("unsupported", e.to_string()),
        CaptureError::Store(s) => ApiError::from(s),
        other => ApiError::bad_code("capture_failed", other.to_string()),
    }
}

async fn targets_h() -> ApiResult<Response> {
    let (rows, warning) = tokio::task::spawn_blocking(list_targets)
        .await
        .map_err(ApiError::internal)?;
    let targets: Vec<_> = rows
        .into_iter()
        .map(|t| {
            let what = t.command.as_deref().unwrap_or("");
            let version = t.version.as_deref().map(|v| format!(" {v}")).unwrap_or_default();
            json!({
                "kind": "pid",
                "label": format!("{}{} pid {} {}", t.runtime, version, t.pid, what).trim_end().to_string(),
                "pid": t.pid,
                "url": null,
                "runtime": t.runtime,
                "version": t.version,
                "agent": t.agent,
                "command": t.command,
            })
        })
        .collect();
    Ok(axum::response::IntoResponse::into_response(Json(
        json!({ "targets": targets, "warning": warning }),
    )))
}

#[derive(Deserialize)]
struct CapQuery {
    pid: Option<u32>,
    url: Option<String>,
}

async fn capabilities_h(State(s): State<St>, Q(q): Q<CapQuery>) -> ApiResult<Response> {
    let target = parse_target(q.pid, q.url.as_deref())?;
    let registry = Arc::clone(&s.registry);
    let out = tokio::task::spawn_blocking(move || {
        let (provider, rt) = registry.detect(&target)?;
        let mut caps = provider.capabilities(&rt);
        caps.extend(registry.system_capabilities(&rt));
        Ok::<_, CaptureError>((rt.provider, caps))
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(capture_error)?;
    let kinds: Vec<_> = out
        .1
        .iter()
        .map(|c| {
            json!({
                "kind": c.kind,
                "available": c.available,
                "fidelity": c.fidelity,
                "note": c.note,
            })
        })
        .collect();
    Ok(axum::response::IntoResponse::into_response(Json(
        json!({ "runtime": out.0, "kinds": kinds }),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_rejects_non_loopback() {
        for u in [
            "http://evil.com",
            "http://127.0.0.1.evil.com",
            "file:///x",
            "http://localhost@evil.com/",
            "ftp://localhost",
            "not a url",
        ] {
            assert!(parse_target(None, Some(u)).is_err(), "{u}");
        }
    }

    #[test]
    fn parse_target_accepts_loopback() {
        for u in ["http://127.0.0.1:6060", "https://localhost/x", "http://[::1]:1"] {
            assert_eq!(
                parse_target(None, Some(u)).unwrap(),
                Target::Url(u.to_string()),
                "{u}"
            );
        }
        assert_eq!(parse_target(Some(5), None).unwrap(), Target::Pid(5));
    }

    #[test]
    fn parse_target_needs_exactly_one() {
        assert!(parse_target(None, None).is_err());
        assert!(parse_target(Some(1), Some("http://localhost")).is_err());
        assert!(parse_target(Some(0), None).is_err());
    }
}
