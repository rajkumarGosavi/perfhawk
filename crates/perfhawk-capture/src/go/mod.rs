//! Go provider: the perfhawk agent / `net/http/pprof` over HTTP, or `go test`.

pub mod discovery;
mod gotest;
mod http;

use crate::cancel::CancelToken;
use crate::provider::Provider;
use crate::types::*;
use perfhawk_model::{Fidelity, ProfileKind};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Default)]
pub struct GoProvider;

impl GoProvider {
    pub fn new() -> GoProvider {
        GoProvider
    }
}

enum Mode {
    GoTest,
    /// Reachable HTTP endpoint; `agent` is true when the perfhawk agent answers.
    Http {
        agent: bool,
    },
    /// A program we launch with the agent auto-started.
    LaunchAgent,
}

fn mode(rt: &RuntimeInfo) -> Mode {
    if rt
        .command
        .as_deref()
        .is_some_and(|c| c.starts_with("go test "))
    {
        Mode::GoTest
    } else if rt.url.is_some() {
        Mode::Http {
            agent: rt.runtime_version.is_some(),
        }
    } else {
        Mode::LaunchAgent
    }
}

fn cap(kind: ProfileKind, fidelity: Fidelity, note: Option<&str>) -> Capability {
    Capability {
        kind,
        fidelity,
        available: true,
        note: note.map(str::to_string),
    }
}

fn unavailable(kind: ProfileKind, note: &str) -> Capability {
    Capability {
        kind,
        fidelity: Fidelity::Native,
        available: false,
        note: Some(note.to_string()),
    }
}

fn describe_launch(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "linux")]
fn pid_is_go(pid: u32) -> bool {
    discovery::has_go_buildinfo(std::path::Path::new(&format!("/proc/{pid}/exe")))
}

#[cfg(not(target_os = "linux"))]
fn pid_is_go(_pid: u32) -> bool {
    false
}

impl Provider for GoProvider {
    fn id(&self) -> &'static str {
        "go"
    }

    fn detect(&self, target: &Target) -> Result<Option<RuntimeInfo>, CaptureError> {
        match target {
            Target::Url(u) => {
                let base = u.trim_end_matches('/').to_string();
                match discovery::agent_info(&base, Duration::from_secs(3)) {
                    Ok(Some(info)) => Ok(Some(RuntimeInfo {
                        provider: "go",
                        pid: Some(info.pid),
                        url: Some(base),
                        runtime_version: Some(info.go_version.unwrap_or_else(|| "go".into())),
                        command: None,
                    })),
                    Ok(None) | Err(_) => {
                        // No agent: accept a plain net/http/pprof endpoint.
                        let probe = format!("{base}/debug/pprof/");
                        match http::fetch(&probe, Duration::from_secs(3)) {
                            Ok(_) => Ok(Some(RuntimeInfo {
                                provider: "go",
                                pid: None,
                                url: Some(base),
                                runtime_version: None,
                                command: None,
                            })),
                            Err(e) => Err(CaptureError::Target(format!(
                                "no Go pprof endpoint at {base}: {}",
                                match e {
                                    http::FetchError::Gone(m) | http::FetchError::Other(m) => m,
                                    http::FetchError::Status(c, _) =>
                                        format!("{probe} answered HTTP {c}; is net/http/pprof imported?"),
                                }
                            ))),
                        }
                    }
                }
            }
            Target::Pid(pid) => match discovery::discover(*pid) {
                Some(d) => Ok(Some(RuntimeInfo {
                    provider: "go",
                    pid: Some(*pid),
                    url: Some(d.url),
                    runtime_version: Some(d.go_version.unwrap_or_else(|| "go".into())),
                    command: None,
                })),
                None if pid_is_go(*pid) => Err(CaptureError::NotDetected(
                    "Go process without the perfhawk agent: start the agent or pass --url to its net/http/pprof endpoint"
                        .into(),
                )),
                None => Ok(None),
            },
            Target::Launch { program, args, .. } => {
                let is_go = discovery::which(program)
                    .map(|p| discovery::has_go_buildinfo(&p))
                    .unwrap_or(false);
                Ok(is_go.then(|| RuntimeInfo {
                    provider: "go",
                    pid: None,
                    url: None,
                    runtime_version: None,
                    command: Some(describe_launch(program, args)),
                }))
            }
            Target::GoTest { package, .. } => {
                if discovery::which("go").is_none() {
                    return Err(CaptureError::Target(
                        "the go tool was not found on PATH".into(),
                    ));
                }
                Ok(Some(RuntimeInfo {
                    provider: "go",
                    pid: None,
                    url: None,
                    runtime_version: None,
                    command: Some(format!("go test {package}")),
                }))
            }
        }
    }

    fn capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability> {
        use ProfileKind::*;
        let n = Fidelity::Native;
        match mode(rt) {
            Mode::GoTest => vec![
                cap(Cpu, n.clone(), None),
                unavailable(Wall, "not available for go test (needs the perfhawk agent)"),
                cap(Memory, n.clone(), None),
                cap(Lock, n.clone(), None),
                cap(Blocking, n, None),
                unavailable(Threads, "not available for go test"),
            ],
            Mode::Http { agent: false } => vec![
                cap(Cpu, n.clone(), None),
                unavailable(Wall, "needs perfhawk agent"),
                cap(Memory, n.clone(), None),
                cap(
                    Lock,
                    n.clone(),
                    Some("depends on target's runtime.SetMutexProfileFraction"),
                ),
                cap(
                    Blocking,
                    n.clone(),
                    Some("depends on target's runtime.SetBlockProfileRate"),
                ),
                cap(Threads, n, None),
            ],
            Mode::Http { agent: true } | Mode::LaunchAgent => vec![
                cap(Cpu, n.clone(), None),
                cap(Wall, Fidelity::Derived, None),
                cap(Memory, n.clone(), None),
                cap(Lock, n.clone(), None),
                cap(Blocking, n.clone(), None),
                cap(Threads, n, None),
            ],
        }
    }

    fn capture(
        &self,
        rt: &RuntimeInfo,
        target: &Target,
        plan: &CapturePlan,
        cancel: &CancelToken,
        progress: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        match (mode(rt), target) {
            (Mode::GoTest, Target::GoTest { package, args }) => {
                gotest::capture_go_test(package, args, plan, cancel, progress)
            }
            (Mode::Http { agent }, _) => {
                let url = rt.url.as_deref().unwrap_or_default();
                http::capture_http(url, agent, plan, cancel, progress)
            }
            (Mode::LaunchAgent, Target::Launch { program, args, cwd }) => {
                launch_and_capture(program, args, cwd.as_deref(), plan, cancel, progress)
            }
            _ => Err(CaptureError::Target(
                "internal error: target does not match the detected Go runtime".into(),
            )),
        }
    }
}

fn launch_and_capture(
    program: &str,
    args: &[String],
    cwd: Option<&std::path::Path>,
    plan: &CapturePlan,
    cancel: &CancelToken,
    progress: &dyn Fn(&str),
) -> Result<CaptureOutcome, CaptureError> {
    let mut cmd = Command::new(program);
    cmd.args(args).env("PERFHAWK_AGENT", "1").stdin(Stdio::null());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| CaptureError::Target(format!("could not start {program}: {e}")))?;
    let pid = child.id();
    progress(&format!(
        "started {program} (pid {pid}); waiting for the perfhawk agent"
    ));
    let found = discovery::wait_for_discovery(pid, Duration::from_secs(10), cancel, &mut || {
        matches!(child.try_wait(), Ok(None))
    });
    let result = match found {
        None if cancel.is_cancelled() => Ok(CaptureOutcome {
            artifacts: vec![],
            end: CaptureEnd::Cancelled,
            warnings: vec![],
        }),
        None => Err(CaptureError::Target(
            "program did not start the perfhawk agent; import github.com/rajkumarGosavi/perfhawk/agents/go".into(),
        )),
        Some(d) => http::capture_http(&d.url, true, plan, cancel, progress),
    };
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}
