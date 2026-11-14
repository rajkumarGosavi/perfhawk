#![allow(dead_code)]
//! Shared helpers: a server wired to a fake capture provider.

use perfhawk_capture::{
    Artifact, CancelToken, Capability, CaptureEnd, CaptureError, CaptureOutcome, CapturePlan,
    Provider, ProviderRegistry, RuntimeInfo, Target,
};
use perfhawk_model::{Fidelity, ProfileKind};
use perfhawk_server::serve_with_registry;
use perfhawk_store::Store;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

const CPU: &[u8] = include_bytes!("../../../perfhawk-import/tests/fixtures/go-cpu.pb.gz");

fn cpu_outcome(end: CaptureEnd) -> CaptureOutcome {
    CaptureOutcome {
        artifacts: vec![Artifact {
            kind: ProfileKind::Cpu,
            fidelity: Fidelity::Native,
            file_name: "cpu.pb.gz".into(),
            bytes: CPU.to_vec(),
            warning: None,
            timeline: None,
        }],
        end,
        warnings: vec![],
    }
}

/// Detects any pid target; cpu is available, wall is not. Capture behaviour by pid:
/// 1 finishes at once, 2 runs for the whole window (or until cancelled),
/// 3 fails needing an elevated shell, 4 ends early because the target exited.
pub struct Fake;

impl Provider for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }
    fn detect(&self, target: &Target) -> Result<Option<RuntimeInfo>, CaptureError> {
        Ok(match target {
            Target::Pid(p) => Some(RuntimeInfo {
                provider: "fake",
                pid: Some(*p),
                url: None,
                runtime_version: None,
                command: None,
            }),
            _ => None,
        })
    }
    fn capabilities(&self, _rt: &RuntimeInfo) -> Vec<Capability> {
        vec![
            Capability {
                kind: ProfileKind::Cpu,
                fidelity: Fidelity::Native,
                available: true,
                note: None,
            },
            Capability {
                kind: ProfileKind::Wall,
                fidelity: Fidelity::Native,
                available: false,
                note: Some("wall is off in the fake".into()),
            },
        ]
    }
    fn capture(
        &self,
        rt: &RuntimeInfo,
        _target: &Target,
        plan: &CapturePlan,
        cancel: &CancelToken,
        progress: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        progress("fake capturing");
        match rt.pid {
            Some(2) => {
                let cancelled = cancel.wait_timeout(plan.duration);
                Ok(cpu_outcome(if cancelled {
                    CaptureEnd::Cancelled
                } else {
                    CaptureEnd::Complete
                }))
            }
            Some(3) => Err(CaptureError::Helper(
                "the trace needs an elevated shell".into(),
            )),
            Some(4) => Ok(cpu_outcome(CaptureEnd::TargetExited)),
            _ => Ok(cpu_outcome(CaptureEnd::Complete)),
        }
    }
}

pub async fn start() -> (tempfile::TempDir, Arc<Store>, SocketAddr) {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let mut registry = ProviderRegistry::default();
    registry.register(Box::new(Fake));
    let (addr, _h) = serve_with_registry(store.clone(), 0, Arc::new(registry))
        .await
        .unwrap();
    (dir, store, addr)
}

pub async fn get(addr: SocketAddr, path: &str) -> (u16, Value) {
    let r = reqwest::get(format!("http://{addr}{path}")).await.unwrap();
    let s = r.status().as_u16();
    (s, r.json().await.unwrap_or(Value::Null))
}

const H: &str = "X-Perfhawk-Request";

pub async fn post_record(addr: SocketAddr, body: Value, header: bool) -> (u16, Value) {
    let mut r = reqwest::Client::new()
        .post(format!("http://{addr}/api/record"))
        .json(&body);
    if header {
        r = r.header(H, "1");
    }
    let r = r.send().await.unwrap();
    (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
}

pub async fn delete(addr: SocketAddr, path: &str, header: bool) -> u16 {
    let mut r = reqwest::Client::new().delete(format!("http://{addr}{path}"));
    if header {
        r = r.header(H, "1");
    }
    r.send().await.unwrap().status().as_u16()
}

/// Polls `/api/record/current` until the job is not running.
pub async fn wait_finished(addr: SocketAddr) -> Value {
    for _ in 0..300 {
        let (st, v) = get(addr, "/api/record/current").await;
        if st == 200 && v["state"] != "running" {
            return v;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the job never finished");
}
