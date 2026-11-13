use prism_capture::*;
use prism_model::{Fidelity, ProfileKind};
use prism_store::{SessionStatus, Store};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CPU: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-cpu.pb.gz");
const HEAP: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-heap.pb.gz");

type Script = Box<dyn Fn(&CancelToken) -> Result<CaptureOutcome, CaptureError> + Send + Sync>;

struct FakeProvider {
    caps: Vec<Capability>,
    script: Script,
}

fn cap(kind: ProfileKind, available: bool) -> Capability {
    Capability {
        kind,
        fidelity: Fidelity::Native,
        available,
        note: (!available).then(|| "not here".to_string()),
    }
}

fn art(kind: ProfileKind, fidelity: Fidelity, bytes: &[u8]) -> Artifact {
    Artifact {
        kind,
        fidelity,
        file_name: format!("{}.pb.gz", kind.as_str()),
        bytes: bytes.to_vec(),
        warning: None,
        timeline: None,
    }
}

fn runtime(provider: &'static str, pid: u32) -> RuntimeInfo {
    RuntimeInfo {
        provider,
        pid: Some(pid),
        url: None,
        runtime_version: Some("1.0".into()),
        command: None,
    }
}

impl Provider for FakeProvider {
    fn id(&self) -> &'static str {
        "fake"
    }
    fn detect(&self, target: &Target) -> Result<Option<RuntimeInfo>, CaptureError> {
        Ok(match target {
            Target::Pid(p) => Some(runtime("fake", *p)),
            _ => None,
        })
    }
    fn capabilities(&self, _rt: &RuntimeInfo) -> Vec<Capability> {
        self.caps.clone()
    }
    fn capture(
        &self,
        _rt: &RuntimeInfo,
        _t: &Target,
        _plan: &CapturePlan,
        cancel: &CancelToken,
        _progress: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        (self.script)(cancel)
    }
}

fn setup<F>(caps: Vec<Capability>, script: F) -> (tempfile::TempDir, Store, ProviderRegistry)
where
    F: Fn(&CancelToken) -> Result<CaptureOutcome, CaptureError> + Send + Sync + 'static,
{
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let mut reg = ProviderRegistry::default();
    reg.register(Box::new(FakeProvider {
        caps,
        script: Box::new(script),
    }));
    (tmp, store, reg)
}

fn req(kinds: Vec<ProfileKind>) -> RecordRequest {
    RecordRequest {
        target: Target::Pid(77),
        plan: CapturePlan {
            kinds,
            duration: Duration::from_secs(1),
            timeline: false,
        },
        name: None,
        tags: vec!["t".into()],
    }
}

fn two_artifacts(end: CaptureEnd) -> CaptureOutcome {
    CaptureOutcome {
        artifacts: vec![
            art(ProfileKind::Cpu, Fidelity::Approximate, CPU),
            art(ProfileKind::Memory, Fidelity::Native, HEAP),
        ],
        end,
        warnings: vec![],
    }
}

#[test]
fn parse_duration_cases() {
    assert!(parse_duration("500ms").is_err());
    assert_eq!(parse_duration("1s").unwrap(), Duration::from_secs(1));
    assert_eq!(parse_duration("1000ms").unwrap(), Duration::from_secs(1));
    assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
    assert_eq!(parse_duration("10m").unwrap(), Duration::from_secs(600));
    assert!(parse_duration("11m").is_err());
    assert!(parse_duration("abc").is_err());
    assert!(parse_duration("").is_err());
    assert!(parse_duration("5").is_err());
    assert!(parse_duration("-5s").is_err());
}

#[test]
fn record_complete_imports_artifacts() {
    let caps = vec![cap(ProfileKind::Cpu, true), cap(ProfileKind::Memory, true)];
    let (_t, store, reg) = setup(caps, |_| Ok(two_artifacts(CaptureEnd::Complete)));
    let s = record(
        &store,
        &reg,
        &req(vec![ProfileKind::Cpu, ProfileKind::Memory]),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    assert_eq!(s.status, SessionStatus::Complete);
    assert_eq!(s.name, "fake-77");
    assert_eq!(s.target.runtime, "fake");
    assert_eq!(s.target.pid, Some(77));
    assert_eq!(s.profile_ids.len(), 2);
    let recs: Vec<_> = s
        .profile_ids
        .iter()
        .map(|id| store.resolve(id).unwrap())
        .collect();
    let cpu = recs.iter().find(|r| r.kind == ProfileKind::Cpu).unwrap();
    assert_eq!(cpu.fidelity, Fidelity::Approximate);
    assert_eq!(cpu.session_id.as_deref(), Some(s.id.as_str()));
    assert_eq!(cpu.tags, vec!["t".to_string()]);
    let mem = recs.iter().find(|r| r.kind == ProfileKind::Memory).unwrap();
    assert_eq!(mem.fidelity, Fidelity::Native);
}

#[test]
fn record_target_exit_is_partial() {
    let caps = vec![cap(ProfileKind::Cpu, true)];
    let (_t, store, reg) = setup(caps, |_| {
        Ok(CaptureOutcome {
            artifacts: vec![art(ProfileKind::Cpu, Fidelity::Native, CPU)],
            end: CaptureEnd::TargetExited,
            warnings: vec!["exited early".into()],
        })
    });
    let s = record(&store, &reg, &req(vec![]), &CancelToken::new(), &|_| {}).unwrap();
    assert_eq!(s.status, SessionStatus::Partial);
    assert_eq!(s.profile_ids.len(), 1);
    assert!(s.warnings.iter().any(|w| w == "exited early"));
}

#[test]
fn record_cancel_is_cancelled() {
    let caps = vec![cap(ProfileKind::Cpu, true)];
    let (_t, store, reg) = setup(caps, |cancel| {
        assert!(cancel.wait_timeout(Duration::from_secs(10)));
        Ok(CaptureOutcome {
            artifacts: vec![art(ProfileKind::Cpu, Fidelity::Native, CPU)],
            end: CaptureEnd::Cancelled,
            warnings: vec![],
        })
    });
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        t2.cancel();
    });
    let s = record(&store, &reg, &req(vec![]), &token, &|_| {}).unwrap();
    assert_eq!(s.status, SessionStatus::Cancelled);
    assert_eq!(s.profile_ids.len(), 1);
}

#[test]
fn unsupported_kind_fails_before_session() {
    let caps = vec![cap(ProfileKind::Cpu, true), cap(ProfileKind::Wall, false)];
    let (_t, store, reg) = setup(caps, |_| panic!("capture must not run"));
    let err = record(
        &store,
        &reg,
        &req(vec![ProfileKind::Wall]),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap_err();
    match err {
        CaptureError::Unsupported { kind, reason } => {
            assert_eq!(kind, ProfileKind::Wall);
            assert!(reason.contains("not here"), "{reason}");
        }
        e => panic!("{e}"),
    }
    assert!(store.list_sessions().unwrap().is_empty());
}

#[test]
fn capture_error_marks_failed() {
    let caps = vec![cap(ProfileKind::Cpu, true)];
    let (_t, store, reg) = setup(caps, |_| Err(CaptureError::Target("boom".into())));
    let err = record(&store, &reg, &req(vec![]), &CancelToken::new(), &|_| {}).unwrap_err();
    assert!(err.to_string().contains("boom"), "{err}");
    let sessions = store.list_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].status, SessionStatus::Failed);
    assert!(sessions[0].warnings.iter().any(|w| w.contains("boom")));
}

type SeenKinds = Arc<Mutex<Option<Vec<ProfileKind>>>>;

struct Spy(SeenKinds, Vec<Capability>);

impl Provider for Spy {
    fn id(&self) -> &'static str {
        "spy"
    }
    fn detect(&self, _t: &Target) -> Result<Option<RuntimeInfo>, CaptureError> {
        Ok(Some(runtime("spy", 1)))
    }
    fn capabilities(&self, _rt: &RuntimeInfo) -> Vec<Capability> {
        self.1.clone()
    }
    fn capture(
        &self,
        _: &RuntimeInfo,
        _: &Target,
        plan: &CapturePlan,
        _: &CancelToken,
        _: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        *self.0.lock().unwrap() = Some(plan.kinds.clone());
        Ok(CaptureOutcome {
            artifacts: vec![],
            end: CaptureEnd::Complete,
            warnings: vec![],
        })
    }
}

#[test]
fn empty_kinds_means_all_available() {
    let caps = vec![
        cap(ProfileKind::Cpu, true),
        cap(ProfileKind::Wall, false),
        cap(ProfileKind::Memory, true),
    ];
    let seen: SeenKinds = Arc::new(Mutex::new(None));
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let mut reg = ProviderRegistry::default();
    reg.register(Box::new(Spy(seen.clone(), caps)));
    let s = record(&store, &reg, &req(vec![]), &CancelToken::new(), &|_| {}).unwrap();
    let want = vec![ProfileKind::Cpu, ProfileKind::Memory];
    assert_eq!(seen.lock().unwrap().clone().unwrap(), want);
    assert_eq!(s.requested_kinds, want);
}

#[test]
fn no_provider_detects_target() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let reg = ProviderRegistry::default();
    let err = record(&store, &reg, &req(vec![]), &CancelToken::new(), &|_| {}).unwrap_err();
    assert!(matches!(err, CaptureError::NotDetected(_)), "{err}");
}

#[test]
fn cancel_token_wait_timeout() {
    let t = CancelToken::new();
    assert!(!t.is_cancelled());
    let start = std::time::Instant::now();
    assert!(!t.wait_timeout(Duration::from_millis(50)));
    assert!(start.elapsed() >= Duration::from_millis(45));
    let t2 = t.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        t2.cancel();
    });
    assert!(t.wait_timeout(Duration::from_secs(10)));
    assert!(t.is_cancelled());
    assert!(t.wait_timeout(Duration::from_secs(10)));
}

#[test]
fn cancelled_token_wins_over_target_exited() {
    // Ctrl-C reaches the whole process group, so the target may die at the same
    // moment; the user's cancellation is what the session records.
    let caps = vec![cap(ProfileKind::Cpu, true)];
    let token = CancelToken::new();
    let t2 = token.clone();
    let (_t, store, reg) = setup(caps, move |_| {
        t2.cancel();
        Ok(CaptureOutcome {
            artifacts: vec![art(ProfileKind::Cpu, Fidelity::Native, CPU)],
            end: CaptureEnd::TargetExited,
            warnings: vec![],
        })
    });
    let s = record(&store, &reg, &req(vec![]), &token, &|_| {}).unwrap();
    assert_eq!(s.status, SessionStatus::Cancelled);
    assert_eq!(s.profile_ids.len(), 1);
}

// ---- V5 timelines ----

fn cpu_samples() -> usize {
    prism_import::import_bytes(CPU, &prism_import::ImportHints::default())
        .unwrap()
        .samples
        .len()
}

fn timeline(n: usize) -> prism_model::Timeline {
    prism_model::Timeline {
        bucket_ns: 20_000_000,
        start_unix_ns: 1_700_000_000_000_000_000,
        total_buckets: 0,
        sample_bucket: (0..n as u32).map(|i| i * 50).collect(),
    }
}

fn cpu_with_timeline(t: Option<prism_model::Timeline>) -> CaptureOutcome {
    let mut a = art(ProfileKind::Cpu, Fidelity::Native, CPU);
    a.timeline = t;
    CaptureOutcome {
        artifacts: vec![a],
        end: CaptureEnd::Complete,
        warnings: vec![],
    }
}

fn timeline_req(on: bool) -> RecordRequest {
    let mut r = req(vec![ProfileKind::Cpu]);
    r.plan.timeline = on;
    r
}

#[test]
fn artifact_timeline_is_stored_with_the_profile() {
    let n = cpu_samples();
    let t = timeline(n);
    let t2 = t.clone();
    let (_t, store, reg) = setup(vec![cap(ProfileKind::Cpu, true)], move |_| {
        Ok(cpu_with_timeline(Some(t2.clone())))
    });
    let s = record(
        &store,
        &reg,
        &timeline_req(true),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    assert_eq!(s.status, SessionStatus::Complete);
    let stored = store.load_timeline(&s.profile_ids[0]).unwrap().unwrap();
    assert_eq!(stored, t);
    assert!(stored.validate(n));
    assert_eq!(*stored.sample_bucket.last().unwrap() as usize, (n - 1) * 50);
}

#[test]
fn runtime_without_timestamps_records_a_warning_and_no_timeline() {
    let (_t, store, reg) = setup(vec![cap(ProfileKind::Cpu, true)], |_| {
        Ok(cpu_with_timeline(None))
    });
    let s = record(
        &store,
        &reg,
        &timeline_req(true),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    assert!(
        s.warnings
            .iter()
            .any(|w| w.contains("no time resolution for cpu")),
        "{:?}",
        s.warnings
    );
    assert!(store.load_timeline(&s.profile_ids[0]).unwrap().is_none());
}

#[test]
fn no_timeline_flag_stores_none_and_warns_nothing() {
    let n = cpu_samples();
    let (_t, store, reg) = setup(vec![cap(ProfileKind::Cpu, true)], move |_| {
        Ok(cpu_with_timeline(Some(timeline(n))))
    });
    let s = record(
        &store,
        &reg,
        &timeline_req(false),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    assert!(store.load_timeline(&s.profile_ids[0]).unwrap().is_none());
    assert!(
        s.warnings.iter().all(|w| !w.contains("time resolution")),
        "{:?}",
        s.warnings
    );
}

#[test]
fn cancelled_capture_still_stores_its_partial_timeline() {
    let n = cpu_samples();
    let (_t, store, reg) = setup(vec![cap(ProfileKind::Cpu, true)], move |_| {
        let mut o = cpu_with_timeline(Some(timeline(n)));
        o.end = CaptureEnd::Cancelled;
        Ok(o)
    });
    let s = record(
        &store,
        &reg,
        &timeline_req(true),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    assert_eq!(s.status, SessionStatus::Cancelled);
    assert!(store.load_timeline(&s.profile_ids[0]).unwrap().is_some());
}

#[test]
fn a_mismatched_timeline_is_dropped_with_a_warning_but_the_profile_is_kept() {
    let (_t, store, reg) = setup(vec![cap(ProfileKind::Cpu, true)], |_| {
        Ok(cpu_with_timeline(Some(timeline(9999))))
    });
    let s = record(
        &store,
        &reg,
        &timeline_req(true),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    assert_eq!(s.profile_ids.len(), 1);
    assert!(
        s.warnings.iter().any(|w| w.contains("timeline")),
        "{:?}",
        s.warnings
    );
    assert!(store.load_timeline(&s.profile_ids[0]).unwrap().is_none());
}
