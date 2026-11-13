//! `go test` launch mode: the Go tool writes the profile files itself.

use crate::cancel::CancelToken;
use crate::types::*;
use perfhawk_model::{Fidelity, ProfileKind};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn flag_and_file(kind: ProfileKind) -> Option<(&'static str, &'static str)> {
    match kind {
        ProfileKind::Cpu => Some(("-cpuprofile", "cpu.pprof")),
        ProfileKind::Memory => Some(("-memprofile", "heap.pprof")),
        ProfileKind::Lock => Some(("-mutexprofile", "mutex.pprof")),
        ProfileKind::Blocking => Some(("-blockprofile", "block.pprof")),
        _ => None,
    }
}

fn tail(path: &Path, lines: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

struct TempDir(std::path::PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn capture_go_test(
    package: &str,
    extra: &[String],
    plan: &CapturePlan,
    cancel: &CancelToken,
    progress: &dyn Fn(&str),
) -> Result<CaptureOutcome, CaptureError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let dir =
        TempDir(std::env::temp_dir().join(format!("perfhawk-gotest-{}-{nanos}", std::process::id())));
    std::fs::create_dir_all(&dir.0)?;
    let out_dir = &dir.0;

    let mut cmd = Command::new("go");
    cmd.arg("test")
        .arg("-count=1")
        .arg("-o")
        .arg(out_dir.join("test.bin"));
    let mut files = Vec::new();
    for &k in &plan.kinds {
        if let Some((flag, file)) = flag_and_file(k) {
            cmd.arg(flag).arg(out_dir.join(file));
            files.push((k, file));
        }
    }
    cmd.arg(package).args(extra);
    let (so, se) = (out_dir.join("stdout.txt"), out_dir.join("stderr.txt"));
    cmd.stdout(Stdio::from(std::fs::File::create(&so)?))
        .stderr(Stdio::from(std::fs::File::create(&se)?));
    progress("note: duration does not apply to go test; the test run decides how long it lasts");
    progress(&format!("running go test {package}"));
    let mut child = cmd
        .spawn()
        .map_err(|e| CaptureError::Target(format!("could not run go: {e}")))?;

    let mut warnings = Vec::new();
    let mut end = CaptureEnd::Complete;
    let status = loop {
        match child.try_wait()? {
            Some(st) => break Some(st),
            None => {
                if cancel.wait_timeout(Duration::from_millis(100)) {
                    let _ = child.kill();
                    let _ = child.wait();
                    end = CaptureEnd::Cancelled;
                    break None;
                }
            }
        }
    };
    if let Some(st) = status {
        if !st.success() {
            let output = format!("{}\n{}", tail(&so, 15), tail(&se, 15));
            warnings.push(format!("go test exited with {st}: {}", output.trim()));
        }
    }
    let mut artifacts = Vec::new();
    for (kind, file) in files {
        match std::fs::read(out_dir.join(file)) {
            Ok(bytes) if !bytes.is_empty() => artifacts.push(Artifact {
                kind,
                fidelity: Fidelity::Native,
                file_name: file.to_string(),
                bytes,
                warning: None,
                timeline: None,
            }),
            _ => {}
        }
    }
    if artifacts.is_empty() && end == CaptureEnd::Complete {
        let detail = if warnings.is_empty() {
            format!("{}\n{}", tail(&so, 15), tail(&se, 15))
        } else {
            warnings.join("; ")
        };
        return Err(CaptureError::Target(format!(
            "go test produced no profiles: {}",
            detail.trim()
        )));
    }
    Ok(CaptureOutcome {
        artifacts,
        end,
        warnings,
    })
}
