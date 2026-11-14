//! Non-elevated checks of the Windows system provider. (Real captures are in
//! etw_real.rs.)
#![cfg(windows)]

use perfhawk_capture::etw::{is_elevated, EtwProvider};
use perfhawk_capture::*;
use perfhawk_model::ProfileKind;
use std::time::Duration;

fn rt(pid: Option<u32>) -> RuntimeInfo {
    RuntimeInfo {
        provider: "go",
        pid,
        url: None,
        runtime_version: None,
        command: None,
    }
}

#[test]
fn provider_identity_and_overhead() {
    let p = EtwProvider::new();
    assert_eq!(p.id(), "etw");
    assert!(p.overhead().contains('%'));
}

#[test]
fn capabilities_when_not_elevated_name_the_privilege() {
    if is_elevated() {
        eprintln!("skipped: running elevated");
        return;
    }
    let caps = EtwProvider::new().capabilities(&rt(Some(1234)));
    assert_eq!(caps.len(), 3);
    for c in &caps {
        assert!(!c.available);
        let n = c.note.as_deref().unwrap();
        assert!(
            n.contains("administrator") && n.contains("elevated shell"),
            "{n}"
        );
    }
}

#[test]
fn capabilities_without_pid_say_no_pid() {
    for c in EtwProvider::new().capabilities(&rt(None)) {
        assert!(!c.available);
        assert!(c.note.as_deref().unwrap().contains("process id"));
    }
}

#[test]
fn registry_with_defaults_has_the_system_provider() {
    let caps = ProviderRegistry::with_defaults().system_capabilities(&rt(Some(1234)));
    assert_eq!(caps.len(), 3);
    for c in &caps {
        let n = c.note.as_deref().unwrap_or("");
        assert_ne!(n, NO_SYSTEM_PROVIDER_NOTE);
    }
}

#[test]
fn capture_without_elevation_is_unsupported_with_the_elevated_hint() {
    if is_elevated() {
        eprintln!("skipped: running elevated");
        return;
    }
    let plan = CapturePlan {
        kinds: vec![ProfileKind::BlockIo],
        duration: Duration::from_secs(1),
        timeline: true,
    };
    let err = EtwProvider::new()
        .capture(&rt(Some(1234)), &plan, &CancelToken::new(), &|_| {})
        .unwrap_err();
    match err {
        CaptureError::Unsupported { kind, reason } => {
            assert_eq!(kind, ProfileKind::BlockIo);
            assert!(reason.contains("elevated shell"), "{reason}");
        }
        e => panic!("unexpected error: {e}"),
    }
}
