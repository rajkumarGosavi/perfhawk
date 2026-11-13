//! Client for the `perfhawk-dotnet` helper (JSON-lines protocol, version 1).

use crate::cancel::CancelToken;
use crate::types::*;
use perfhawk_model::ProfileKind;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const PROTOCOL: u64 = 1;
/// How long the helper may stay silent after `stop` before it is killed. The real helper
/// needs up to ~30 s for EventPipe rundown, then converts the trace, so this is an idle
/// limit that restarts on every event rather than a fixed deadline.
const DEFAULT_STOP_GRACE: Duration = Duration::from_secs(45);

fn stop_grace() -> Duration {
    std::env::var("PERFHAWK_STOP_GRACE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_STOP_GRACE)
}

/// How to start the helper: `program` plus leading arguments (e.g. `dotnet` + `[helper.dll]`).
#[derive(Debug, Clone, PartialEq)]
pub struct HelperPath {
    pub program: PathBuf,
    pub args_prefix: Vec<String>,
    /// Extra environment for the helper process (used by tests).
    pub env: Vec<(String, String)>,
}

const BUILD_HINT: &str = "build it with `dotnet publish helpers/perfhawk-dotnet -c Release -o <dir>` \
    and either put perfhawk-dotnet next to the perfhawk binary or set PERFHAWK_DOTNET to its path";

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "perfhawk-dotnet.exe"
    } else {
        "perfhawk-dotnet"
    }
}

/// `PERFHAWK_DOTNET`, then a `perfhawk-dotnet` next to the running executable.
pub fn locate_helper() -> Result<HelperPath, CaptureError> {
    if let Some(v) = std::env::var_os("PERFHAWK_DOTNET").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(v);
        if !path.is_file() {
            return Err(CaptureError::Helper(format!(
                "PERFHAWK_DOTNET points to {}, which does not exist",
                path.display()
            )));
        }
        let is_dll = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dll"));
        return Ok(if is_dll {
            HelperPath {
                program: PathBuf::from("dotnet"),
                args_prefix: vec![path.to_string_lossy().into_owned()],
                env: vec![],
            }
        } else {
            HelperPath {
                program: path,
                args_prefix: vec![],
                env: vec![],
            }
        });
    }
    if let Ok(cur) = std::env::current_exe() {
        let sibling = cur.with_file_name(exe_name());
        if sibling.is_file() {
            return Ok(HelperPath {
                program: sibling,
                args_prefix: vec![],
                env: vec![],
            });
        }
    }
    Err(CaptureError::Helper(format!(
        "the .NET helper (perfhawk-dotnet) was not found: {BUILD_HINT}"
    )))
}

#[derive(Debug, Clone, PartialEq)]
pub enum HelperEvent {
    Hello {
        protocol: u32,
        helper_version: String,
    },
    Process {
        pid: u32,
        name: String,
        command_line: Option<String>,
        runtime_version: Option<String>,
    },
    Progress(String),
    Artifact {
        kind: ProfileKind,
        path: PathBuf,
        samples: u64,
    },
    Warning(String),
    Error {
        code: String,
        message: String,
    },
    Done {
        end: CaptureEnd,
    },
}

fn parse_event(line: &str) -> Result<HelperEvent, CaptureError> {
    let malformed = || CaptureError::Helper(format!("malformed line from perfhawk-dotnet: {line}"));
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| malformed())?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    let req = |k: &str| s(k).ok_or_else(malformed);
    let ty = v
        .get("type")
        .and_then(|t| t.as_str())
        .ok_or_else(malformed)?;
    Ok(match ty {
        "hello" => HelperEvent::Hello {
            protocol: v
                .get("protocol")
                .and_then(|p| p.as_u64())
                .ok_or_else(malformed)? as u32,
            helper_version: s("helper_version").unwrap_or_default(),
        },
        "process" => HelperEvent::Process {
            pid: v
                .get("pid")
                .and_then(|p| p.as_u64())
                .ok_or_else(malformed)? as u32,
            name: s("name").unwrap_or_default(),
            command_line: s("command_line"),
            runtime_version: s("runtime_version"),
        },
        "progress" => HelperEvent::Progress(req("message")?),
        "warning" => HelperEvent::Warning(req("message")?),
        "artifact" => HelperEvent::Artifact {
            kind: ProfileKind::parse(&req("kind")?).ok_or_else(malformed)?,
            path: PathBuf::from(req("path")?),
            samples: v.get("samples").and_then(|p| p.as_u64()).unwrap_or(0),
        },
        "error" => HelperEvent::Error {
            code: req("code")?,
            message: s("message").unwrap_or_default(),
        },
        "done" => HelperEvent::Done {
            end: match req("end")?.as_str() {
                "complete" => CaptureEnd::Complete,
                "target_exited" => CaptureEnd::TargetExited,
                "cancelled" => CaptureEnd::Cancelled,
                _ => return Err(malformed()),
            },
        },
        _ => return Err(malformed()),
    })
}

/// Runs the helper with `args`, delivering every event (including `Hello`) to
/// `on_event`. Returns `Ok` once the helper wrote `done` or `error`; the caller
/// decides what an `Error` event means.
pub fn run_helper(
    h: &HelperPath,
    args: &[String],
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(HelperEvent),
) -> Result<(), CaptureError> {
    let mut cmd = Command::new(&h.program);
    cmd.args(&h.args_prefix)
        .args(args)
        .envs(h.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = cmd.spawn().map_err(|e| {
        CaptureError::Helper(format!(
            "could not run the .NET helper {}: {e}; {BUILD_HINT}",
            h.program.display()
        ))
    })?;
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = mpsc::channel::<std::io::Result<String>>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let stop = line.is_err();
            if tx.send(line).is_err() || stop {
                break;
            }
        }
    });

    let grace = stop_grace();
    let result = (|| -> Result<(), CaptureError> {
        let mut saw_hello = false;
        let mut finished = false;
        let mut stop_sent_at: Option<Instant> = None;
        loop {
            if stop_sent_at.is_none() && cancel.is_cancelled() {
                if let Some(mut s) = stdin.take() {
                    let _ = s.write_all(b"stop\n");
                    let _ = s.flush();
                    // Keep the pipe open: closing it would also read as "stop".
                    stdin = Some(s);
                }
                stop_sent_at = Some(Instant::now());
            }
            if stop_sent_at.is_some_and(|t| t.elapsed() > grace) {
                return Err(CaptureError::Helper(
                    "perfhawk-dotnet did not stop in time and was killed".into(),
                ));
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(line)) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let ev = parse_event(line)?;
                    if !saw_hello {
                        match &ev {
                            HelperEvent::Hello { protocol, .. } if *protocol as u64 == PROTOCOL => {
                                saw_hello = true;
                            }
                            HelperEvent::Hello { protocol, .. } => {
                                return Err(CaptureError::Helper(format!(
                                    "perfhawk-dotnet protocol {protocol} is not supported; rebuild the helper"
                                )))
                            }
                            _ => {
                                return Err(CaptureError::Helper(
                                    "perfhawk-dotnet did not start with a hello line; rebuild the helper"
                                        .into(),
                                ))
                            }
                        }
                    }
                    if stop_sent_at.is_some() {
                        stop_sent_at = Some(Instant::now()); // still working: restart the idle clock
                    }
                    if matches!(ev, HelperEvent::Done { .. } | HelperEvent::Error { .. }) {
                        finished = true;
                    }
                    on_event(ev);
                }
                Ok(Err(e)) => {
                    return Err(CaptureError::Helper(format!(
                        "could not read perfhawk-dotnet output: {e}"
                    )))
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return if saw_hello && finished {
                        Ok(())
                    } else if !saw_hello {
                        Err(CaptureError::Helper(
                            "perfhawk-dotnet produced no output (is it a valid helper build?)".into(),
                        ))
                    } else {
                        Err(CaptureError::Helper(
                            "perfhawk-dotnet exited without finishing the capture".into(),
                        ))
                    };
                }
            }
            if finished {
                return Ok(());
            }
        }
    })();

    drop(stdin);
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}

/// Every process the helper reports as a reachable .NET runtime.
pub fn list_dotnet_processes() -> Result<Vec<RuntimeInfo>, CaptureError> {
    list_with(&locate_helper()?)
}

pub(crate) fn list_with(h: &HelperPath) -> Result<Vec<RuntimeInfo>, CaptureError> {
    let mut out = Vec::new();
    let mut error = None;
    run_helper(
        h,
        &["list".to_string()],
        &CancelToken::new(),
        &mut |e| match e {
            HelperEvent::Process {
                pid,
                name,
                command_line,
                runtime_version,
            } => out.push(RuntimeInfo {
                provider: "dotnet",
                pid: Some(pid),
                url: None,
                runtime_version,
                command: command_line.or(Some(name)).filter(|c| !c.is_empty()),
            }),
            HelperEvent::Error { message, .. } => error = Some(message),
            _ => {}
        },
    )?;
    match error {
        Some(m) => Err(CaptureError::Helper(format!(
            "perfhawk-dotnet list failed: {m}"
        ))),
        None => Ok(out),
    }
}
