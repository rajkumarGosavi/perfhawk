#![allow(dead_code)]
//! Shared helpers: a server wired to a fake capture provider.

use perfhawk_capture::{
    CancelToken, Capability, CaptureError, CaptureOutcome, CapturePlan, Provider, ProviderRegistry,
    RuntimeInfo, Target,
};
use perfhawk_model::{Fidelity, ProfileKind};
use perfhawk_server::serve_with_registry;
use perfhawk_store::Store;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

/// Detects any pid target; cpu is available, wall is not.
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
        _rt: &RuntimeInfo,
        _target: &Target,
        _plan: &CapturePlan,
        _cancel: &CancelToken,
        _progress: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        Err(CaptureError::Target("fake capture is not implemented".into()))
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
