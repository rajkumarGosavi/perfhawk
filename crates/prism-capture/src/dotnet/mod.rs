//! .NET provider: drives the `prism-dotnet` helper (EventPipe) and reads back pprof files.

mod helper;

pub use helper::{
    list_dotnet_processes, locate_helper, run_helper, HelperEvent, HelperPath, PROTOCOL,
};

use crate::cancel::CancelToken;
use crate::provider::Provider;
use crate::types::*;
use prism_model::{Fidelity, ProfileKind};
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

fn kinds_arg(kinds: &[ProfileKind]) -> String {
    kinds
        .iter()
        .map(|k| k.as_str())
        .collect::<Vec<_>>()
        .join(",")
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
        "permission" => "the target runs as another user, or TMPDIR differs (Linux/macOS: run prism \
             with the same TMPDIR as the target)"
            .to_string(),
        "not_dotnet" => format!("pid {pid_text} is not a .NET 8+ process"),
        "not_found" => format!("no process with pid {pid_text}"),
        "launch_failed" => message.to_string(),
        _ => format!("prism-dotnet failed: {message}"),
    }
}

/// Converts a `.nettrace` file into one artifact per kind using the helper.
pub fn convert_nettrace(path: &Path) -> Result<Vec<Artifact>, CaptureError> {
    convert_nettrace_with(&locate_helper()?, path)
}

pub fn convert_nettrace_with(h: &HelperPath, path: &Path) -> Result<Vec<Artifact>, CaptureError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let dir = TempDir(
        std::env::temp_dir().join(format!("prism-nettrace-{}-{nanos}", std::process::id())),
    );
    std::fs::create_dir_all(&dir.0)?;
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
            std::env::temp_dir().join(format!("prism-dotnet-{}-{nanos}", std::process::id())),
        );
        std::fs::create_dir_all(&dir.0)?;

        let duration_ms = plan.duration.as_millis().to_string();
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
                    kinds_arg(&plan.kinds),
                    "--out".into(),
                    dir.0.to_string_lossy().into_owned(),
                    "--".into(),
                ]);
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
                kinds_arg(&plan.kinds),
                "--out".into(),
                dir.0.to_string_lossy().into_owned(),
            ]);
        }

        let mut artifacts: Vec<Artifact> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut end = None;
        let mut error: Option<(String, String)> = None;
        let mut read_err: Option<std::io::Error> = None;
        run_helper(&helper, &args, cancel, &mut |ev| match ev {
            HelperEvent::Progress(m) => progress(&m),
            HelperEvent::Warning(w) => warnings.push(w),
            HelperEvent::Artifact { kind, path, .. } => match std::fs::read(&path) {
                Ok(bytes) => artifacts.push(Artifact {
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
            HelperEvent::Error { code, message } => error = Some((code, message)),
            HelperEvent::Done { end: e } => end = Some(e),
            HelperEvent::Hello { .. } | HelperEvent::Process { .. } => {}
        })?;

        if let Some((code, message)) = error {
            return Err(CaptureError::Target(explain_error(&code, &message, rt.pid)));
        }
        if let Some(e) = read_err {
            return Err(e.into());
        }
        Ok(CaptureOutcome {
            artifacts,
            end: end.unwrap_or(CaptureEnd::Complete),
            warnings,
        })
    }
}
