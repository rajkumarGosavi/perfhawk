use crate::cancel::CancelToken;
use crate::provider::ProviderRegistry;
use crate::system::is_system_kind;
use crate::types::*;
use perfhawk_model::ProfileKind;
use perfhawk_store::{ImportOptions, SessionRecord, SessionStatus, Store, TargetInfo};
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
    progress: &(dyn Fn(&str) + Sync),
) -> Result<SessionRecord, CaptureError> {
    let (provider, rt) = registry.detect(&req.target)?;
    let caps = provider.capabilities(&rt);
    let sys_caps = registry.system_capabilities(&rt);

    let kinds: Vec<ProfileKind> = if req.plan.kinds.is_empty() {
        caps.iter()
            .filter(|c| c.available)
            .map(|c| c.kind)
            .collect()
    } else {
        for k in &req.plan.kinds {
            let pool = if is_system_kind(*k) { &sys_caps } else { &caps };
            match pool.iter().find(|c| c.kind == *k) {
                Some(c) if c.available => {}
                Some(c) => {
                    return Err(CaptureError::Unsupported {
                        kind: *k,
                        reason: c.note.clone().unwrap_or_else(|| {
                            format!("not supported by the {} provider", rt.provider)
                        }),
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
        timeline: req.plan.timeline,
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

    let kind_list: Vec<&str> = kinds.iter().map(|k| k.as_str()).collect();
    let what = match (rt.pid, &rt.url, &rt.command) {
        (Some(pid), _, _) => format!("pid {pid}"),
        (None, Some(u), _) => u.clone(),
        (None, None, Some(c)) => c.clone(),
        _ => "target".into(),
    };
    let lang_kinds: Vec<ProfileKind> = kinds
        .iter()
        .copied()
        .filter(|k| !is_system_kind(*k))
        .collect();
    let sys_kinds: Vec<ProfileKind> = kinds
        .iter()
        .copied()
        .filter(|k| is_system_kind(*k))
        .collect();
    let sys_provider = if sys_kinds.is_empty() {
        None
    } else {
        registry.system_provider_for(&rt, &sys_kinds)
    };
    if let Some(sp) = sys_provider {
        progress(&format!(
            "[{}] overhead: {}; capped at {}s",
            sp.id(),
            sp.overhead(),
            req.plan.duration.as_secs()
        ));
    }
    let lang_plan = CapturePlan {
        kinds: lang_kinds.clone(),
        ..plan.clone()
    };
    let sys_plan = CapturePlan {
        kinds: sys_kinds.clone(),
        ..plan.clone()
    };
    progress(&format!(
        "[{}] capturing {} for {}s ({what})",
        rt.provider,
        kind_list.join(", "),
        req.plan.duration.as_secs()
    ));
    let (lang_res, sys_res) = std::thread::scope(|scope| {
        let sys_handle = sys_provider
            .map(|sp| scope.spawn(|| sp.capture(&rt, &sys_plan, cancel, progress)));
        let lang_res = if lang_kinds.is_empty() {
            None
        } else {
            Some(provider.capture(&rt, &req.target, &lang_plan, cancel, progress))
        };
        let sys_res = sys_handle.map(|h| {
            h.join()
                .unwrap_or_else(|_| Err(CaptureError::Helper("system capture panicked".into())))
        });
        (lang_res, sys_res)
    });
    let sys_label = sys_kinds
        .iter()
        .map(|k| k.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mut degraded = false;
    let mut outcome = match (lang_res, &sys_res) {
        (Some(Ok(o)), _) => o,
        (Some(Err(e)), _) => {
            store.finish_session(
                &session.id,
                SessionStatus::Failed,
                elapsed_ms(started),
                &[e.to_string()],
            )?;
            return Err(e);
        }
        // Only OS-level kinds were requested: nothing else to fall back on.
        (None, Some(Err(e))) => {
            store.finish_session(
                &session.id,
                SessionStatus::Failed,
                elapsed_ms(started),
                &[format!("{sys_label}: {e}")],
            )?;
            return Err(CaptureError::Helper(format!("{sys_label}: {e}")));
        }
        (None, _) => CaptureOutcome {
            artifacts: vec![],
            end: CaptureEnd::Complete,
            warnings: vec![],
        },
    };
    match sys_res {
        Some(Ok(o)) => {
            outcome.artifacts.extend(o.artifacts);
            outcome.warnings.extend(o.warnings);
            outcome.end = worst_end(outcome.end, o.end);
        }
        Some(Err(e)) if !lang_kinds.is_empty() => {
            outcome.warnings.push(format!("{sys_label}: {e}"));
            degraded = true;
        }
        _ => {}
    }

    let mut warnings = outcome.warnings;
    for a in outcome.artifacts {
        if let Some(w) = &a.warning {
            warnings.push(format!("{}: {w}", a.kind));
        }
        let wants_time = matches!(
            a.kind,
            ProfileKind::Cpu
                | ProfileKind::Wall
                | ProfileKind::BlockIo
                | ProfileKind::OffCpu
                | ProfileKind::SchedDelay
        );
        if req.plan.timeline && wants_time && a.timeline.is_none() {
            warnings.push(format!(
                "no time resolution for {}: the {} provider gave no timestamps",
                a.kind, rt.provider
            ));
        }
        let timeline = if req.plan.timeline {
            a.timeline.clone()
        } else {
            None
        };
        let opts = |timeline| ImportOptions {
            name: Some(format!("{name}-{}", a.kind)),
            file_name: Some(a.file_name.clone()),
            kind_override: Some(a.kind),
            tags: req.tags.clone(),
            fidelity_override: Some(a.fidelity.clone()),
            session_id: Some(session.id.clone()),
            timeline,
        };
        let had_timeline = timeline.is_some();
        match store.import(&a.bytes, opts(timeline)) {
            Ok(_) => {}
            Err(perfhawk_store::StoreError::InvalidTimeline(why)) if had_timeline => {
                warnings.push(format!("{}: timeline dropped ({why})", a.kind));
                if let Err(e) = store.import(&a.bytes, opts(None)) {
                    warnings.push(format!("could not import {} profile: {e}", a.kind));
                    degraded = true;
                }
            }
            Err(e) => {
                warnings.push(format!("could not import {} profile: {e}", a.kind));
                degraded = true;
            }
        }
    }
    // A terminal Ctrl-C reaches the whole process group, so the target can die at the
    // same moment the user cancels; the user's cancellation is what gets recorded.
    let mut status = match outcome.end {
        _ if cancel.is_cancelled() => SessionStatus::Cancelled,
        CaptureEnd::Complete => SessionStatus::Complete,
        CaptureEnd::TargetExited => SessionStatus::Partial,
        CaptureEnd::Cancelled => SessionStatus::Cancelled,
    };
    if degraded && status == SessionStatus::Complete {
        status = SessionStatus::Partial;
    }
    Ok(store.finish_session(&session.id, status, elapsed_ms(started), &warnings)?)
}

/// The most severe of two capture endings: Cancelled, then TargetExited, then Complete.
fn worst_end(a: CaptureEnd, b: CaptureEnd) -> CaptureEnd {
    use CaptureEnd::*;
    match (a, b) {
        (Cancelled, _) | (_, Cancelled) => Cancelled,
        (TargetExited, _) | (_, TargetExited) => TargetExited,
        _ => Complete,
    }
}
