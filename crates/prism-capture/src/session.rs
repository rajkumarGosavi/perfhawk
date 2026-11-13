use crate::cancel::CancelToken;
use crate::provider::ProviderRegistry;
use crate::types::*;
use prism_model::ProfileKind;
use prism_store::{ImportOptions, SessionRecord, SessionStatus, Store, TargetInfo};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct RecordRequest {
    pub target: Target,
    pub plan: CapturePlan,
    pub name: Option<String>,
    pub tags: Vec<String>,
}

fn default_session_name(rt: &RuntimeInfo, target: &Target) -> String {
    let suffix = match (rt.pid, target) {
        (Some(pid), _) => pid.to_string(),
        (None, Target::Launch { program, .. }) => std::path::Path::new(program)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "program".into()),
        (None, Target::GoTest { .. }) => "test".into(),
        (None, Target::Url(_)) => "url".into(),
        (None, Target::Pid(p)) => p.to_string(),
    };
    format!("{}-{}", rt.provider, suffix)
}

/// Detects the runtime, captures, and imports the results as one session.
pub fn record(
    store: &Store,
    registry: &ProviderRegistry,
    req: &RecordRequest,
    cancel: &CancelToken,
    progress: &dyn Fn(&str),
) -> Result<SessionRecord, CaptureError> {
    let (provider, rt) = registry.detect(&req.target)?;
    let caps = provider.capabilities(&rt);

    let kinds: Vec<ProfileKind> = if req.plan.kinds.is_empty() {
        caps.iter().filter(|c| c.available).map(|c| c.kind).collect()
    } else {
        for k in &req.plan.kinds {
            match caps.iter().find(|c| c.kind == *k) {
                Some(c) if c.available => {}
                Some(c) => {
                    return Err(CaptureError::Unsupported {
                        kind: *k,
                        reason: c
                            .note
                            .clone()
                            .unwrap_or_else(|| format!("not supported by the {} provider", rt.provider)),
                    })
                }
                None => {
                    return Err(CaptureError::Unsupported {
                        kind: *k,
                        reason: format!("not supported by the {} provider", rt.provider),
                    })
                }
            }
        }
        req.plan.kinds.clone()
    };
    let plan = CapturePlan {
        kinds: kinds.clone(),
        duration: req.plan.duration,
    };

    let name = req
        .name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| default_session_name(&rt, &req.target));
    let info = TargetInfo {
        runtime: rt.provider.to_string(),
        pid: rt.pid,
        command: rt.command.clone(),
        url: rt.url.clone(),
        runtime_version: rt.runtime_version.clone(),
    };
    let session = store.create_session(&name, info, &kinds, &req.tags)?;
    let started = Instant::now();
    let elapsed_ms = |s: Instant| s.elapsed().as_millis() as i64;

    let outcome = match provider.capture(&rt, &req.target, &plan, cancel, progress) {
        Ok(o) => o,
        Err(e) => {
            store.finish_session(
                &session.id,
                SessionStatus::Failed,
                elapsed_ms(started),
                &[e.to_string()],
            )?;
            return Err(e);
        }
    };

    let mut warnings = outcome.warnings;
    let mut degraded = false;
    for a in outcome.artifacts {
        if let Some(w) = &a.warning {
            warnings.push(format!("{}: {w}", a.kind));
        }
        let opts = ImportOptions {
            name: Some(format!("{name}-{}", a.kind)),
            file_name: Some(a.file_name.clone()),
            kind_override: Some(a.kind),
            tags: req.tags.clone(),
            fidelity_override: Some(a.fidelity.clone()),
            session_id: Some(session.id.clone()),
        };
        if let Err(e) = store.import(&a.bytes, opts) {
            warnings.push(format!("could not import {} profile: {e}", a.kind));
            degraded = true;
        }
    }
    let mut status = match outcome.end {
        CaptureEnd::Complete => SessionStatus::Complete,
        CaptureEnd::TargetExited => SessionStatus::Partial,
        CaptureEnd::Cancelled => SessionStatus::Cancelled,
    };
    if degraded && status == SessionStatus::Complete {
        status = SessionStatus::Partial;
    }
    Ok(store.finish_session(&session.id, status, elapsed_ms(started), &warnings)?)
}
