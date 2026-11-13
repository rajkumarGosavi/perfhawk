use prism_capture::*;
use prism_model::{file_leaf, Fidelity, ProfileKind, Unit};
use prism_store::{SessionStatus, Store};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CPU: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-cpu.pb.gz");

type LangScript = Box<dyn Fn() -> Result<CaptureOutcome, CaptureError> + Send + Sync>;
type SysScript = Box<dyn Fn(&CapturePlan) -> Result<CaptureOutcome, CaptureError> + Send + Sync>;

fn cap(kind: ProfileKind, available: bool) -> Capability {
    Capability {
        kind,
        fidelity: Fidelity::Native,
        available,
        note: (!available).then(|| "not here".to_string()),
    }
}

struct Lang(LangScript);

impl Provider for Lang {
    fn id(&self) -> &'static str {
        "fake"
    }
    fn detect(&self, t: &Target) -> Result<Option<RuntimeInfo>, CaptureError> {
        Ok(match t {
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
    fn capabilities(&self, _: &RuntimeInfo) -> Vec<Capability> {
        vec![cap(ProfileKind::Cpu, true)]
    }
    fn capture(
        &self,
        _: &RuntimeInfo,
        _: &Target,
        _: &CapturePlan,
        _: &CancelToken,
        _: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        (self.0)()
    }
}

struct Sys {
    script: SysScript,
    called: Arc<AtomicBool>,
}

impl SystemProvider for Sys {
    fn id(&self) -> &'static str {
        "fake-os"
    }
    fn capabilities(&self, _: &RuntimeInfo) -> Vec<Capability> {
        SYSTEM_KINDS.iter().map(|k| cap(*k, true)).collect()
    }
    fn overhead(&self) -> &'static str {
        "about 2% CPU"
    }
    fn capture(
        &self,
        _: &RuntimeInfo,
        plan: &CapturePlan,
        _: &CancelToken,
        _: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        self.called.store(true, Ordering::SeqCst);
        (self.script)(plan)
    }
}

fn cpu_artifact() -> Artifact {
    Artifact {
        kind: ProfileKind::Cpu,
        fidelity: Fidelity::Native,
        file_name: "cpu.pb.gz".into(),
        bytes: CPU.to_vec(),
        warning: None,
        timeline: None,
    }
}

fn io_artifact() -> Artifact {
    let types = [
        ("io_time", Unit::Nanoseconds),
        ("io_bytes", Unit::Bytes),
    ];
    event_artifact(
        &EventSpec {
            kind: ProfileKind::BlockIo,
            fidelity: Fidelity::Native,
            sample_types: &types,
            start_unix_ns: 0,
            duration_ns: 1_000_000_000,
            bucket_ns: 1_000_000_000,
        },
        &[EventRow {
            stack: vec!["main".into(), file_leaf("/a")],
            values: vec![1_000, 1],
            bucket: 0,
        }],
    )
}

fn outcome(artifacts: Vec<Artifact>) -> CaptureOutcome {
    CaptureOutcome {
        artifacts,
        end: CaptureEnd::Complete,
        warnings: vec![],
    }
}

fn req(kinds: Vec<ProfileKind>) -> RecordRequest {
    RecordRequest {
        target: Target::Pid(7),
        plan: CapturePlan {
            kinds,
            duration: Duration::from_secs(5),
            timeline: false,
        },
        name: None,
        tags: vec![],
    }
}

struct Rig {
    _tmp: tempfile::TempDir,
    store: Store,
    reg: ProviderRegistry,
    sys_called: Arc<AtomicBool>,
}

fn rig(lang: LangScript, sys: Option<SysScript>) -> Rig {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let mut reg = ProviderRegistry::default();
    reg.register(Box::new(Lang(lang)));
    let sys_called = Arc::new(AtomicBool::new(false));
    if let Some(script) = sys {
        reg.register_system(Box::new(Sys {
            script,
            called: sys_called.clone(),
        }));
    }
    Rig {
        _tmp: tmp,
        store,
        reg,
        sys_called,
    }
}

fn go(r: &Rig, kinds: Vec<ProfileKind>) -> Result<prism_store::SessionRecord, CaptureError> {
    record(&r.store, &r.reg, &req(kinds), &CancelToken::new(), &|_| {})
}

#[test]
fn system_kinds_not_default() {
    let r = rig(
        Box::new(|| Ok(outcome(vec![cpu_artifact()]))),
        Some(Box::new(|_| Ok(outcome(vec![io_artifact()])))),
    );
    let s = go(&r, vec![]).unwrap();
    assert_eq!(s.profile_ids.len(), 1);
    assert!(!r.sys_called.load(Ordering::SeqCst));
}

#[test]
fn system_kind_unavailable_is_unsupported() {
    let r = rig(Box::new(|| panic!("language capture must not run")), None);
    let err = go(&r, vec![ProfileKind::BlockIo]).unwrap_err();
    match err {
        CaptureError::Unsupported { kind, reason } => {
            assert_eq!(kind, ProfileKind::BlockIo);
            assert!(reason.contains("ETW on Windows"), "{reason}");
        }
        e => panic!("unexpected {e}"),
    }
}

#[test]
fn language_and_system_merge_into_one_session() {
    let r = rig(
        Box::new(|| Ok(outcome(vec![cpu_artifact()]))),
        Some(Box::new(|plan| {
            assert_eq!(plan.kinds, vec![ProfileKind::BlockIo]);
            Ok(outcome(vec![io_artifact()]))
        })),
    );
    let s = go(&r, vec![ProfileKind::Cpu, ProfileKind::BlockIo]).unwrap();
    assert_eq!(s.status, SessionStatus::Complete, "{:?}", s.warnings);
    let kinds: Vec<_> = s
        .profile_ids
        .iter()
        .map(|id| r.store.resolve(id).unwrap().kind)
        .collect();
    assert!(kinds.contains(&ProfileKind::Cpu) && kinds.contains(&ProfileKind::BlockIo));
}

#[test]
fn system_failure_makes_partial_with_warning() {
    let r = rig(
        Box::new(|| Ok(outcome(vec![cpu_artifact()]))),
        Some(Box::new(|_| Err(CaptureError::Helper("access denied".into())))),
    );
    let s = go(&r, vec![ProfileKind::Cpu, ProfileKind::BlockIo]).unwrap();
    assert_eq!(s.status, SessionStatus::Partial);
    assert!(
        s.warnings.iter().any(|w| w.contains("access denied")),
        "{:?}",
        s.warnings
    );
    assert_eq!(s.profile_ids.len(), 1);
}

#[test]
fn system_only_does_not_call_language_capture() {
    let r = rig(
        Box::new(|| panic!("language capture must not run")),
        Some(Box::new(|_| Ok(outcome(vec![io_artifact()])))),
    );
    let s = go(&r, vec![ProfileKind::BlockIo]).unwrap();
    assert_eq!(s.status, SessionStatus::Complete, "{:?}", s.warnings);
    assert_eq!(s.profile_ids.len(), 1);
}

#[test]
fn overhead_is_reported_before_capture() {
    let r = rig(
        Box::new(|| Ok(outcome(vec![cpu_artifact()]))),
        Some(Box::new(|_| Ok(outcome(vec![io_artifact()])))),
    );
    let lines = Mutex::new(Vec::<String>::new());
    record(
        &r.store,
        &r.reg,
        &req(vec![ProfileKind::Cpu, ProfileKind::BlockIo]),
        &CancelToken::new(),
        &|m| lines.lock().unwrap().push(m.to_string()),
    )
    .unwrap();
    let lines = lines.into_inner().unwrap();
    let over = lines.iter().position(|l| l.contains("overhead")).unwrap();
    let cap = lines.iter().position(|l| l.contains("capturing")).unwrap();
    assert!(over < cap, "{lines:?}");
    assert!(lines[over].contains("about 2% CPU") && lines[over].contains("5s"));
}

fn timeline_req(kinds: Vec<ProfileKind>) -> RecordRequest {
    let mut r = req(kinds);
    r.plan.timeline = true;
    r
}

#[test]
fn system_timelines_are_kept() {
    let r = rig(
        Box::new(|| panic!("language capture must not run")),
        Some(Box::new(|_| Ok(outcome(vec![io_artifact()])))),
    );
    let s = record(
        &r.store,
        &r.reg,
        &timeline_req(vec![ProfileKind::BlockIo]),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    assert!(r.store.load_timeline(&s.profile_ids[0]).unwrap().is_some());
}

#[test]
fn system_artifact_without_timeline_warns() {
    let r = rig(
        Box::new(|| panic!("language capture must not run")),
        Some(Box::new(|_| {
            let mut a = io_artifact();
            a.timeline = None;
            Ok(outcome(vec![a]))
        })),
    );
    let s = record(
        &r.store,
        &r.reg,
        &timeline_req(vec![ProfileKind::BlockIo]),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    assert!(
        s.warnings
            .iter()
            .any(|w| w.contains("no time resolution for block_io")),
        "{:?}",
        s.warnings
    );
}
