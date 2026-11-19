//! .NET provider: drives the `perfhawk-dotnet` helper (EventPipe) and reads back pprof files.

mod helper;

pub use helper::{
    list_dotnet_processes, locate_helper, run_helper, HelperEvent, HelperPath, PROTOCOL,
};

use crate::cancel::CancelToken;
use crate::provider::Provider;
use crate::types::*;
use perfhawk_model::{Fidelity, ProfileKind};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct DotnetProvider {
    helper: Option<HelperPath>,
}

impl DotnetProvider {
    pub fn new() -> DotnetProvider {
        DotnetProvider { helper: None }
    }

    /// Uses a fixed helper instead of locating one (tests).
    pub fn with_helper(h: HelperPath) -> DotnetProvider {
        DotnetProvider { helper: Some(h) }
    }

    fn helper(&self) -> Result<HelperPath, CaptureError> {
        match &self.helper {
            Some(h) => Ok(h.clone()),
            None => locate_helper(),
        }
    }
}

const PID_HINT: &str = "not found among reachable .NET 8+ processes (it may not be .NET, may have \
    DOTNET_EnableDiagnostics=0, may run as another user, or on Linux/macOS may use a different TMPDIR)";

fn is_dotnet_program(program: &str) -> bool {
    let p = Path::new(program);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if stem == "dotnet" || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")) {
        return true;
    }
    p.with_extension("runtimeconfig.json").is_file()
}

/// The helper's `--kinds` value: profile kinds, plus `gc` and `counters` when wanted.
fn kinds_arg(kinds: &[ProfileKind], gc: bool, counters: bool) -> String {
    let mut v: Vec<&str> = kinds.iter().map(|k| k.as_str()).collect();
    if gc {
        v.push("gc");
    }
    if counters {
        v.push("counters");
    }
    v.join(",")
}

/// Parses a helper-written `counters.json`, turning a bad file into a warning.
fn read_counters(path: &Path, warnings: &mut Vec<String>) -> Option<perfhawk_model::CounterData> {
    let parsed = std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| perfhawk_model::CounterData::from_json(&b).map_err(|e| e.to_string()));
    match parsed {
        Ok(c) => Some(c),
        Err(e) => {
            warnings.push(format!(
                "counters: ignoring invalid counter data from the helper ({e})"
            ));
            None
        }
    }
}

/// Parses a helper-written `gc.json`, turning a bad file into a warning.
fn read_gc(path: &Path, warnings: &mut Vec<String>) -> Option<perfhawk_model::GcData> {
    let parsed = std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| perfhawk_model::GcData::from_json(&b).map_err(|e| e.to_string()));
    match parsed {
        Ok(g) => Some(g),
        Err(e) => {
            warnings.push(format!("GC data from the helper was unusable: {e}"));
            None
        }
    }
}

/// Width of the time buckets the helper labels samples with.
const BUCKET_MS: u64 = 20;

/// A timeline from the helper's per-sample `perfhawk.bucket` labels, or `None` when the
/// profile has none (for example an older helper).
fn timeline_of(bytes: &[u8]) -> Option<perfhawk_model::Timeline> {
    let p = perfhawk_import::import_bytes(bytes, &perfhawk_import::ImportHints::default()).ok()?;
    crate::timeline::timeline_from_labels(
        &p,
        BUCKET_MS * 1_000_000,
        p.meta.captured_at_unix_ns.unwrap_or(0),
    )
}

fn fidelity_for(kind: ProfileKind) -> Fidelity {
    match kind {
        ProfileKind::Cpu => Fidelity::Approximate,
        _ => Fidelity::Native,
    }
}

/// Turns a helper error event into a message that names the cause and a fix.
fn explain_error(code: &str, message: &str, pid: Option<u32>) -> String {
    let pid_text = pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into());
    match code {
        "diagnostics_disabled" => {
            "the target has DOTNET_EnableDiagnostics=0 (or its diagnostics endpoint is unreachable); \
             restart it with diagnostics enabled"
                .to_string()
        }
        "permission" => "the target runs as another user, or TMPDIR differs (Linux/macOS: run perfhawk \
             with the same TMPDIR as the target)"
            .to_string(),
        "not_dotnet" => format!("pid {pid_text} is not a .NET 8+ process"),
        "not_found" => format!("no process with pid {pid_text}"),
        "launch_failed" => message.to_string(),
        _ => format!("perfhawk-dotnet failed: {message}"),
    }
}

/// Converts a `.nettrace` file into one artifact per kind using the helper.
pub fn convert_nettrace(path: &Path) -> Result<Vec<Artifact>, CaptureError> {
    convert_nettrace_with(&locate_helper()?, path)
}

/// Extracts the GC data from a `.nettrace` file using the helper.
pub fn convert_nettrace_gc(path: &Path) -> Result<perfhawk_model::GcData, CaptureError> {
    convert_nettrace_gc_with(&locate_helper()?, path)
}

pub fn convert_nettrace_gc_with(
    h: &HelperPath,
    path: &Path,
) -> Result<perfhawk_model::GcData, CaptureError> {
    let dir = convert_dir()?;
    let args = vec![
        "convert".to_string(),
        "--in".into(),
        path.to_string_lossy().into_owned(),
        "--out".into(),
        dir.0.to_string_lossy().into_owned(),
        "--kinds".into(),
        "gc".into(),
    ];
    let mut gc = None;
    let mut warnings = Vec::new();
    let mut error = None;
    run_helper(h, &args, &CancelToken::new(), &mut |ev| match ev {
        HelperEvent::Gc { path, .. } => gc = read_gc(&path, &mut warnings),
        HelperEvent::Error { code, message } => error = Some((code, message)),
        _ => {}
    })?;
    if let Some((code, message)) = error {
        return Err(CaptureError::Target(explain_error(&code, &message, None)));
    }
    match gc {
        Some(g) if !g.events.is_empty() => Ok(g),
        Some(_) => Err(CaptureError::Target(format!(
            "no GC events in {}; the trace must include the Microsoft-Windows-DotNETRuntime provider with the GC keyword (0x1)",
            path.display()
        ))),
        None => {
            Err(CaptureError::Target(warnings.pop().unwrap_or_else(|| {
                format!("no GC data in {}", path.display())
            })))
        }
    }
}

/// Takes a heap snapshot of a running .NET process using the helper.
///
/// This forces a full (gen2) GC in the target, which pauses it for as long as the heap walk
/// takes. `timeout_ms` bounds the wait; a snapshot cut short comes back with `complete == false`.
pub fn capture_heap_snapshot(
    pid: u32,
    timeout_ms: u64,
    cancel: &CancelToken,
) -> Result<perfhawk_model::HeapSnapshot, CaptureError> {
    capture_heap_snapshot_with(&locate_helper()?, pid, timeout_ms, cancel)
}

pub fn capture_heap_snapshot_with(
    h: &HelperPath,
    pid: u32,
    timeout_ms: u64,
    cancel: &CancelToken,
) -> Result<perfhawk_model::HeapSnapshot, CaptureError> {
    let dir = convert_dir()?;
    let out = dir.0.join("heap.bin");
    let args = vec![
        "snapshot".to_string(),
        "--pid".into(),
        pid.to_string(),
        "--out".into(),
        out.to_string_lossy().into_owned(),
        "--timeout-ms".into(),
        timeout_ms.to_string(),
    ];
    let mut heap = None;
    let mut error = None;
    run_helper(h, &args, cancel, &mut |ev| match ev {
        HelperEvent::Heap { path, .. } => heap = Some(path),
        HelperEvent::Error { code, message } => error = Some((code, message)),
        _ => {}
    })?;
    if let Some((code, message)) = error {
        return Err(CaptureError::Target(explain_error(
            &code,
            &message,
            Some(pid),
        )));
    }
    let path =
        heap.ok_or_else(|| CaptureError::Helper("the helper wrote no heap snapshot".into()))?;
    let bytes = std::fs::read(&path)?;
    perfhawk_model::HeapSnapshot::from_bytes(&bytes)
        .map_err(|e| CaptureError::Helper(format!("unusable heap snapshot from the helper: {e}")))
}

fn convert_dir() -> Result<TempDir, CaptureError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let dir = TempDir(
        std::env::temp_dir().join(format!("perfhawk-nettrace-{}-{nanos}", std::process::id())),
    );
    std::fs::create_dir_all(&dir.0)?;
    Ok(dir)
}

pub fn convert_nettrace_with(h: &HelperPath, path: &Path) -> Result<Vec<Artifact>, CaptureError> {
    let dir = convert_dir()?;
    let args = vec![
        "convert".to_string(),
        "--in".into(),
        path.to_string_lossy().into_owned(),
        "--out".into(),
        dir.0.to_string_lossy().into_owned(),
        "--kinds".into(),
        "cpu,wall,memory,lock,threads".into(),
    ];
    let mut artifacts = Vec::new();
    let mut error = None;
    run_helper(h, &args, &CancelToken::new(), &mut |ev| match ev {
        HelperEvent::Artifact { kind, path, .. } => {
            if let Ok(bytes) = std::fs::read(&path) {
                artifacts.push(Artifact {
                    kind,
                    fidelity: fidelity_for(kind),
                    file_name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    bytes,
                    warning: None,
                    timeline: None,
                });
            }
        }
        HelperEvent::Error { code, message } => error = Some((code, message)),
        _ => {}
    })?;
    match error {
        Some((code, message)) => Err(CaptureError::Target(explain_error(&code, &message, None))),
        None => Ok(artifacts),
    }
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Provider for DotnetProvider {
    fn id(&self) -> &'static str {
        "dotnet"
    }

    fn detect(&self, target: &Target) -> Result<Option<RuntimeInfo>, CaptureError> {
        match target {
            Target::Pid(pid) => {
                let helper = match self.helper() {
                    Ok(h) => h,
                    Err(e) => return Err(CaptureError::NotDetected(e.to_string())),
                };
                let procs = helper::list_with(&helper)?;
                match procs.into_iter().find(|p| p.pid == Some(*pid)) {
                    Some(rt) => Ok(Some(rt)),
                    None => Err(CaptureError::NotDetected(format!("pid {pid}: {PID_HINT}"))),
                }
            }
            Target::Launch { program, args, .. } if is_dotnet_program(program) => {
                let command = std::iter::once(program.clone())
                    .chain(args.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(Some(RuntimeInfo {
                    provider: "dotnet",
                    pid: None,
                    url: None,
                    runtime_version: None,
                    command: Some(command),
                }))
            }
            _ => Ok(None),
        }
    }

    fn supports_gc(&self) -> bool {
        true
    }

    fn supports_counters(&self) -> bool {
        true
    }

    fn capabilities(&self, _rt: &RuntimeInfo) -> Vec<Capability> {
        use ProfileKind::*;
        let cap = |kind, fidelity, available, note: Option<&str>| Capability {
            kind,
            fidelity,
            available,
            note: note.map(str::to_string),
        };
        vec![
            cap(
                Cpu,
                Fidelity::Approximate,
                true,
                Some("derived from managed-thread samples"),
            ),
            cap(Wall, Fidelity::Native, true, None),
            cap(Memory, Fidelity::Native, true, None),
            cap(Lock, Fidelity::Native, true, None),
            cap(
                Blocking,
                Fidelity::Native,
                false,
                Some("not in Phase 1 for .NET"),
            ),
            cap(Threads, Fidelity::Native, true, None),
        ]
    }

    fn capture(
        &self,
        rt: &RuntimeInfo,
        target: &Target,
        plan: &CapturePlan,
        cancel: &CancelToken,
        progress: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        let helper = self.helper()?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir = TempDir(
            std::env::temp_dir().join(format!("perfhawk-dotnet-{}-{nanos}", std::process::id())),
        );
        std::fs::create_dir_all(&dir.0)?;

        let duration_ms = plan.duration.as_millis().to_string();
        // Ask the helper to label cpu/wall samples with their 20 ms time bucket.
        let want_time = plan.timeline
            && plan
                .kinds
                .iter()
                .any(|k| matches!(k, ProfileKind::Cpu | ProfileKind::Wall));
        let bucket_args: Vec<String> = if want_time {
            vec!["--bucket-ms".into(), BUCKET_MS.to_string()]
        } else {
            vec![]
        };
        let mut args: Vec<String> = Vec::new();
        match target {
            Target::Pid(pid) => args.extend(["collect".into(), "--pid".into(), pid.to_string()]),
            Target::Launch {
                program,
                args: pargs,
                ..
            } => {
                args.push("launch".into());
                let tail: Vec<String> = std::iter::once(program.clone())
                    .chain(pargs.iter().cloned())
                    .collect();
                args.extend([
                    "--duration-ms".into(),
                    duration_ms.clone(),
                    "--kinds".into(),
                    kinds_arg(&plan.kinds, plan.gc, plan.counters),
                    "--out".into(),
                    dir.0.to_string_lossy().into_owned(),
                ]);
                args.extend(bucket_args.clone());
                args.push("--".into());
                args.extend(tail);
            }
            _ => {
                return Err(CaptureError::Target(
                    "the .NET provider needs a pid or a program to launch".into(),
                ))
            }
        }
        if matches!(target, Target::Pid(_)) {
            args.extend([
                "--duration-ms".into(),
                duration_ms,
                "--kinds".into(),
                kinds_arg(&plan.kinds, plan.gc, plan.counters),
                "--out".into(),
                dir.0.to_string_lossy().into_owned(),
            ]);
            args.extend(bucket_args);
        }

        let mut artifacts: Vec<Artifact> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut gc: Option<perfhawk_model::GcData> = None;
        let mut counters: Option<perfhawk_model::CounterData> = None;
        let mut end = None;
        let mut error: Option<(String, String)> = None;
        let mut read_err: Option<std::io::Error> = None;
        run_helper(&helper, &args, cancel, &mut |ev| match ev {
            HelperEvent::Progress(m) => progress(&m),
            HelperEvent::Warning(w) => warnings.push(w),
            HelperEvent::Artifact { kind, path, .. } => match std::fs::read(&path) {
                Ok(bytes) => artifacts.push(Artifact {
                    timeline: if want_time && matches!(kind, ProfileKind::Cpu | ProfileKind::Wall) {
                        timeline_of(&bytes)
                    } else {
                        None
                    },
                    kind,
                    fidelity: fidelity_for(kind),
                    file_name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("{kind}.pb.gz")),
                    bytes,
                    warning: None,
                }),
                Err(e) => read_err = Some(e),
            },
            HelperEvent::Gc { path, .. } => gc = read_gc(&path, &mut warnings),
            HelperEvent::Counters { path, .. } => {
                counters = read_counters(&path, &mut warnings)
            }
            HelperEvent::Error { code, message } => error = Some((code, message)),
            HelperEvent::Done { end: e } => end = Some(e),
            HelperEvent::Hello { .. } | HelperEvent::Process { .. } | HelperEvent::Heap { .. } => {}
        })?;

        if let Some((code, message)) = error {
            return Err(CaptureError::Target(explain_error(&code, &message, rt.pid)));
        }
        if let Some(e) = read_err {
            return Err(e.into());
        }
        Ok(CaptureOutcome {
            artifacts,
            gc,
            counters,
            end: end.unwrap_or(CaptureEnd::Complete),
            warnings,
        })
    }
}
