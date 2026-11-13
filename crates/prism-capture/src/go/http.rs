//! Capturing from a Go process over HTTP (`net/http/pprof` + the prism agent).

use crate::cancel::CancelToken;
use crate::types::*;
use prism_model::{Fidelity, ProfileKind};
use std::collections::HashMap;
use std::io::Read;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// CPU timeline resolution for Go: pprof windows of this length (see docs/perf/timestamps.md).
const WINDOW_MS: u64 = 250;
const MAX_BODY: u64 = 512 * 1024 * 1024;
/// How long snapshot kinds (heap, goroutines) may take after a cancel.
const CANCEL_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub(crate) enum FetchError {
    /// The target went away (refused, reset, hung up).
    Gone(String),
    Status(u16, String),
    Other(String),
}

pub(crate) fn fetch(url: &str, timeout: Duration) -> Result<Vec<u8>, FetchError> {
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    match agent.get(url).call() {
        Ok(resp) => {
            let mut body = Vec::new();
            resp.into_reader()
                .take(MAX_BODY)
                .read_to_end(&mut body)
                .map_err(|e| FetchError::Gone(e.to_string()))?;
            Ok(body)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            Err(FetchError::Status(code, text.trim().to_string()))
        }
        Err(ureq::Error::Transport(t)) => match t.kind() {
            ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Io => {
                Err(FetchError::Gone(t.to_string()))
            }
            _ => Err(FetchError::Other(t.to_string())),
        },
    }
}

fn fidelity_for(kind: ProfileKind) -> Fidelity {
    match kind {
        ProfileKind::Wall => Fidelity::Derived,
        _ => Fidelity::Native,
    }
}

/// `(path-and-query, windowed)`: windowed kinds block for the whole duration;
/// the others are snapshots taken at the end of the window.
fn endpoint(kind: ProfileKind, has_agent: bool, secs: u64) -> Option<(String, bool)> {
    Some(match kind {
        ProfileKind::Cpu => (format!("/debug/pprof/profile?seconds={secs}"), true),
        ProfileKind::Wall if has_agent => (format!("/debug/prism/wall?seconds={secs}"), true),
        ProfileKind::Lock if has_agent => (
            format!("/debug/prism/contention?seconds={secs}&kind=mutex"),
            true,
        ),
        ProfileKind::Lock => (format!("/debug/pprof/mutex?seconds={secs}"), true),
        ProfileKind::Blocking if has_agent => (
            format!("/debug/prism/contention?seconds={secs}&kind=block"),
            true,
        ),
        ProfileKind::Blocking => (format!("/debug/pprof/block?seconds={secs}"), true),
        ProfileKind::Memory => ("/debug/pprof/heap".to_string(), false),
        ProfileKind::Threads => ("/debug/pprof/goroutine".to_string(), false),
        _ => return None,
    })
}

fn disabled_warning(kind: ProfileKind) -> Option<&'static str> {
    match kind {
        ProfileKind::Lock => Some(
            "mutex profiling is disabled in the target (call runtime.SetMutexProfileFraction or use the prism agent)",
        ),
        ProfileKind::Blocking => Some(
            "block profiling is disabled in the target (call runtime.SetBlockProfileRate or use the prism agent)",
        ),
        _ => None,
    }
}

fn is_empty_profile(bytes: &[u8]) -> bool {
    prism_import::import_bytes(bytes, &prism_import::ImportHints::default())
        .map(|p| p.samples.is_empty())
        .unwrap_or(false)
}

enum Msg {
    Done(Artifact),
    Failed {
        kind: ProfileKind,
        gone: bool,
        text: String,
    },
}

/// Captures `plan.kinds` concurrently from the endpoint at `base`.
pub(crate) fn capture_http(
    base: &str,
    has_agent: bool,
    plan: &CapturePlan,
    cancel: &CancelToken,
    progress: &dyn Fn(&str),
) -> Result<CaptureOutcome, CaptureError> {
    let secs = plan.duration.as_secs() + u64::from(plan.duration.subsec_nanos() > 0);
    let secs = secs.max(1);
    let timeout = Duration::from_secs(secs + 15);
    let base = base.trim_end_matches('/').to_string();
    let (tx, rx) = mpsc::channel::<Msg>();
    let mut warnings = Vec::new();
    let mut pending: HashMap<ProfileKind, bool> = HashMap::new(); // kind -> windowed

    for &kind in &plan.kinds {
        let Some((plain_path, windowed)) = endpoint(kind, has_agent, secs) else {
            warnings.push(format!("{kind}: not supported for Go targets"));
            continue;
        };
        // With the agent, CPU can be captured in short windows to get a timeline.
        let use_windows = kind == ProfileKind::Cpu && has_agent && plan.timeline;
        let path = if use_windows {
            format!("/debug/prism/cpu-windows?seconds={secs}&window_ms={WINDOW_MS}")
        } else {
            plain_path.clone()
        };
        progress(&format!("capturing {kind} from {path}"));
        pending.insert(kind, windowed);
        let (tx, cancel, url) = (tx.clone(), cancel.clone(), format!("{base}{path}"));
        let fallback_url = format!("{base}{plain_path}");
        std::thread::spawn(move || {
            if !windowed {
                // Snapshot kinds are taken at the end of the window, or at once on cancel.
                cancel.wait_timeout(Duration::from_secs(secs));
            }
            let started_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as i64);
            let mut got_windows = use_windows;
            let mut result = fetch(&url, timeout);
            if use_windows && matches!(result, Err(FetchError::Status(404, _))) {
                // An older agent without the windowed endpoint: plain CPU, no timeline.
                got_windows = false;
                result = fetch(&fallback_url, timeout);
            }
            let msg = match result {
                Ok(bytes) => {
                    let timeline = if got_windows {
                        prism_import::import_bytes(&bytes, &prism_import::ImportHints::default())
                            .ok()
                            .and_then(|p| {
                                crate::timeline::timeline_from_labels(
                                    &p,
                                    WINDOW_MS * 1_000_000,
                                    started_ns,
                                )
                            })
                    } else {
                        None
                    };
                    let warning = if !has_agent && is_empty_profile(&bytes) {
                        disabled_warning(kind).map(str::to_string)
                    } else {
                        None
                    };
                    Msg::Done(Artifact {
                        kind,
                        fidelity: fidelity_for(kind),
                        file_name: format!("{kind}.pb.gz"),
                        bytes,
                        warning,
                        timeline,
                    })
                }
                Err(FetchError::Gone(text)) => Msg::Failed {
                    kind,
                    gone: true,
                    text,
                },
                Err(FetchError::Status(code, body)) => Msg::Failed {
                    kind,
                    gone: false,
                    text: format!("HTTP {code} {body}").trim().to_string(),
                },
                Err(FetchError::Other(text)) => Msg::Failed {
                    kind,
                    gone: false,
                    text,
                },
            };
            let _ = tx.send(msg);
        });
    }
    drop(tx);

    let mut artifacts: HashMap<ProfileKind, Artifact> = HashMap::new();
    let mut gone = false;
    let mut cancelled = false;
    let mut grace_deadline: Option<Instant> = None;
    while !pending.is_empty() {
        if !cancelled && cancel.is_cancelled() {
            cancelled = true;
            grace_deadline = Some(Instant::now() + CANCEL_GRACE);
        }
        if cancelled {
            // Windowed requests are abandoned; wait briefly for snapshots only.
            if !pending.values().any(|windowed| !windowed)
                || grace_deadline.is_some_and(|d| Instant::now() >= d)
            {
                break;
            }
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Msg::Done(a)) => {
                pending.remove(&a.kind);
                artifacts.insert(a.kind, a);
            }
            Ok(Msg::Failed {
                kind,
                gone: g,
                text,
            }) => {
                pending.remove(&kind);
                gone |= g;
                warnings.push(format!("{kind}: {text}"));
                if gone {
                    // The target is gone: nothing else can arrive, so do not wait out the window.
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let end = if cancelled {
        CaptureEnd::Cancelled
    } else if gone {
        CaptureEnd::TargetExited
    } else {
        CaptureEnd::Complete
    };
    let ordered: Vec<Artifact> = plan
        .kinds
        .iter()
        .filter_map(|k| artifacts.remove(k))
        .collect();
    if ordered.is_empty() && end == CaptureEnd::Complete && !plan.kinds.is_empty() {
        return Err(CaptureError::Target(format!(
            "capture failed: {}",
            warnings.join("; ")
        )));
    }
    Ok(CaptureOutcome {
        artifacts: ordered,
        end,
        warnings,
    })
}
