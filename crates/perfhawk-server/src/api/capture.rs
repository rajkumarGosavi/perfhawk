//! Capture endpoints: what can be recorded and what each target supports.

use super::{require_custom_header, ApiError, ApiResult, St, Q};
use crate::jobs::{JobBusy, JobError};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
    Json, Router,
};
use perfhawk_capture::{
    list_targets, CaptureError, CapturePlan, RecordRequest, Target, MAX_DURATION, MIN_DURATION,
};
use perfhawk_model::ProfileKind;
use perfhawk_store::SessionStatus;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_NAME_CHARS: usize = 200;
const MAX_TAGS: usize = 20;
const MAX_TAG_CHARS: usize = 100;

pub fn routes() -> Router<St> {
    Router::new()
        .route("/api/targets", get(targets_h))
        .route("/api/capabilities", get(capabilities_h))
        .route("/api/record", axum::routing::post(record_h))
        .route("/api/record/current", get(current_h))
        .route("/api/record/{job_id}", delete(cancel_h))
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
            let bad =
                || ApiError::bad("url must be http or https on 127.0.0.1, localhost or [::1]");
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

#[derive(Deserialize)]
struct RecordBody {
    pid: Option<u32>,
    url: Option<String>,
    kinds: Option<Vec<String>>,
    duration_secs: u64,
    name: Option<String>,
    tags: Option<Vec<String>>,
    timeline: Option<bool>,
}

fn parse_kind(s: &str) -> ApiResult<ProfileKind> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| {
        ApiError::bad(format!(
            "invalid kind '{s}' (expected cpu|wall|memory|lock|blocking|block_io|off_cpu|sched_delay|threads)"
        ))
    })
}

/// The command that repeats this capture from a terminal.
fn rerun_command(target: &Target, kinds: &[ProfileKind], secs: u64) -> String {
    let t = match target {
        Target::Pid(p) => format!("--pid {p}"),
        Target::Url(u) => format!("--url {u}"),
        _ => String::new(),
    };
    let k = if kinds.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = kinds.iter().map(|k| k.as_str()).collect();
        format!(" --kinds {}", names.join(","))
    };
    format!("perfhawk record {t}{k} --duration {secs}s")
}

fn job_error(e: CaptureError, rerun: &str) -> JobError {
    let message = e.to_string();
    if message.contains("elevated shell") {
        return JobError {
            code: "needs_privilege".into(),
            message: format!(
                "{message}
run: {rerun}"
            ),
        };
    }
    let code = match e {
        CaptureError::Unsupported { .. } => "unsupported",
        CaptureError::NotDetected(_) => "not_detected",
        _ => "capture_failed",
    };
    JobError {
        code: code.into(),
        message,
    }
}

async fn record_h(State(s): State<St>, headers: HeaderMap, body: Body) -> ApiResult<Response> {
    require_custom_header(&headers)?;
    let bytes = axum::body::to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| ApiError::bad("request body is too large"))?;
    let b: RecordBody = serde_json::from_slice(&bytes)
        .map_err(|e| ApiError::bad(format!("invalid request: {e}")))?;
    let target = parse_target(b.pid, b.url.as_deref())?;
    let duration = Duration::from_secs(b.duration_secs);
    if !(MIN_DURATION..=MAX_DURATION).contains(&duration) {
        return Err(ApiError::bad(format!(
            "duration_secs must be from {} to {} seconds",
            MIN_DURATION.as_secs(),
            MAX_DURATION.as_secs()
        )));
    }
    let kinds = b
        .kinds
        .unwrap_or_default()
        .iter()
        .map(|k| parse_kind(k))
        .collect::<ApiResult<Vec<_>>>()?;
    let name = b.name.filter(|n| !n.is_empty());
    let tags = b.tags.unwrap_or_default();
    if name
        .as_ref()
        .is_some_and(|n| n.chars().count() > MAX_NAME_CHARS)
        || tags.len() > MAX_TAGS
        || tags.iter().any(|t| t.chars().count() > MAX_TAG_CHARS)
    {
        return Err(ApiError::bad(format!(
            "name is at most {MAX_NAME_CHARS} characters; at most {MAX_TAGS} tags of {MAX_TAG_CHARS} characters"
        )));
    }
    let rerun = rerun_command(&target, &kinds, b.duration_secs);
    let req = RecordRequest {
        target,
        plan: CapturePlan {
            kinds,
            duration,
            timeline: b.timeline.unwrap_or(false),
        },
        name,
        tags,
    };
    let (store, registry) = (Arc::clone(&s.store), Arc::clone(&s.registry));
    let started = s.jobs.start(move |cancel, progress| {
        match perfhawk_capture::record(&store, &registry, &req, cancel, progress) {
            Ok(session) => match session.status {
                SessionStatus::Failed | SessionStatus::Running => Err(JobError {
                    code: "capture_failed".into(),
                    message: if session.warnings.is_empty() {
                        "the capture failed".into()
                    } else {
                        session.warnings.join("; ")
                    },
                }),
                _ => Ok(session.short_id),
            },
            Err(e) => Err(job_error(e, &rerun)),
        }
    });
    match started {
        Ok(snap) => {
            Ok((StatusCode::ACCEPTED, Json(json!({ "job_id": snap.job_id }))).into_response())
        }
        Err(JobBusy) => Err(ApiError::conflict(
            "a capture is already running: wait for it to finish or cancel it",
        )),
    }
}

async fn current_h(State(s): State<St>) -> Response {
    match s.jobs.snapshot() {
        Some(j) => Json(j).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn cancel_h(
    State(s): State<St>,
    Path(job_id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_custom_header(&headers)?;
    if s.jobs.cancel(&job_id) {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "no such capture".into(),
        })
    }
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
        for u in [
            "http://127.0.0.1:6060",
            "https://localhost/x",
            "http://[::1]:1",
        ] {
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
