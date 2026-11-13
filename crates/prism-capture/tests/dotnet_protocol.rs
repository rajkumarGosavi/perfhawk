use prism_capture::dotnet::{
    convert_nettrace_with, locate_helper, run_helper, DotnetProvider, HelperEvent, HelperPath,
};
use prism_capture::*;
use prism_model::{Fidelity, ProfileKind};
use prism_store::{SessionStatus, Store};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CPU: &str = "../../prism-import/tests/fixtures/go-cpu.pb.gz";
const HEAP: &str = "../../prism-import/tests/fixtures/go-heap.pb.gz";
const MUTEX: &str = "../../prism-import/tests/fixtures/go-mutex.pb.gz";

/// Serializes tests that touch process-wide environment variables.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn fake_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-dotnet-helper"))
}

fn fixture(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(rel);
    p.to_string_lossy().replace('\\', "/")
}

const HELLO: &str = r#"{"type":"hello","protocol":1,"helper_version":"0.1.0"}"#;

/// A helper handle that replays `lines` (a JSON-lines script) instead of profiling.
fn scripted(dir: &tempfile::TempDir, lines: &[String]) -> HelperPath {
    let script = dir.path().join("script.jsonl");
    std::fs::write(&script, lines.join("\n")).unwrap();
    HelperPath {
        program: fake_exe(),
        args_prefix: vec![],
        env: vec![("FAKE_SCRIPT".into(), script.to_string_lossy().into_owned())],
    }
}

fn art(kind: &str, fix: &str) -> String {
    format!(
        r#"{{"type":"artifact","kind":"{kind}","fixture":"{}","samples":3}}"#,
        fixture(fix)
    )
}

fn provider(h: HelperPath) -> DotnetProvider {
    DotnetProvider::with_helper(h)
}

const LIST_DONE: &str = r#"{"for":"list","type":"done","end":"complete"}"#;

fn pid_listing(pid: u32) -> String {
    format!(
        r#"{{"for":"list","type":"process","pid":{pid},"name":"app","command_line":"dotnet app.dll","runtime_version":"8.0.5"}}"#
    )
}

fn plan(kinds: Vec<ProfileKind>) -> CapturePlan {
    CapturePlan {
        kinds,
        duration: Duration::from_secs(2),
        timeline: false,
    }
}

fn detect_pid(p: &DotnetProvider, pid: u32) -> RuntimeInfo {
    p.detect(&Target::Pid(pid)).unwrap().expect("detected")
}

#[test]
fn hello_protocol_mismatch_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(
        &dir,
        &[r#"{"type":"hello","protocol":2,"helper_version":"9.9.9"}"#.to_string()],
    );
    let err = run_helper(&h, &["list".into()], &CancelToken::new(), &mut |_| {}).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("protocol 2 is not supported"), "{msg}");
    assert!(msg.contains("rebuild the helper"), "{msg}");
}

#[test]
fn malformed_line_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(&dir, &[HELLO.to_string(), "this is not json".to_string()]);
    let err = run_helper(&h, &["list".into()], &CancelToken::new(), &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("this is not json"), "{err}");
}

#[test]
fn missing_hello_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(&dir, &[r#"{"type":"progress","message":"hi"}"#.to_string()]);
    let err = run_helper(&h, &["list".into()], &CancelToken::new(), &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("hello"), "{err}");
}

#[test]
fn events_are_delivered_in_order() {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(
        &dir,
        &[
            HELLO.to_string(),
            r#"{"type":"progress","message":"p1"}"#.to_string(),
            r#"{"type":"warning","message":"w1"}"#.to_string(),
            r#"{"type":"done","end":"complete"}"#.to_string(),
        ],
    );
    let mut seen = Vec::new();
    run_helper(&h, &["list".into()], &CancelToken::new(), &mut |e| {
        seen.push(e)
    })
    .unwrap();
    assert!(matches!(&seen[0], HelperEvent::Hello { protocol: 1, .. }));
    assert!(matches!(&seen[1], HelperEvent::Progress(m) if m == "p1"));
    assert!(matches!(&seen[2], HelperEvent::Warning(m) if m == "w1"));
    assert!(matches!(
        &seen[3],
        HelperEvent::Done {
            end: CaptureEnd::Complete
        }
    ));
}

#[test]
fn detect_pid_via_list() {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(
        &dir,
        &[HELLO.to_string(), pid_listing(4321), LIST_DONE.to_string()],
    );
    let p = provider(h);
    let rt = detect_pid(&p, 4321);
    assert_eq!(rt.provider, "dotnet");
    assert_eq!(rt.pid, Some(4321));
    assert_eq!(rt.runtime_version.as_deref(), Some("8.0.5"));
    // A pid that is not listed gives a hint about why, rather than silence.
    match p.detect(&Target::Pid(999)) {
        Err(CaptureError::NotDetected(h)) => {
            assert!(h.contains("DOTNET_EnableDiagnostics"), "{h}");
            assert!(h.contains("TMPDIR"), "{h}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn detect_launch_recognises_dotnet_programs() {
    let dir = tempfile::TempDir::new().unwrap();
    let p = provider(scripted(&dir, &[HELLO.to_string()]));
    let launch = |prog: &str| Target::Launch {
        program: prog.to_string(),
        args: vec![],
        cwd: None,
    };
    assert!(p.detect(&launch("dotnet")).unwrap().is_some());
    assert!(p.detect(&launch("C:/x/DOTNET.EXE")).unwrap().is_some());
    assert!(p.detect(&launch("app.dll")).unwrap().is_some());
    // apphost: a .runtimeconfig.json sits beside it
    let app = dir.path().join("svc.exe");
    std::fs::write(&app, b"x").unwrap();
    std::fs::write(dir.path().join("svc.runtimeconfig.json"), b"{}").unwrap();
    assert!(p.detect(&launch(&app.to_string_lossy())).unwrap().is_some());
    let other = dir.path().join("other.exe");
    std::fs::write(&other, b"x").unwrap();
    assert!(p
        .detect(&launch(&other.to_string_lossy()))
        .unwrap()
        .is_none());
    assert!(p.detect(&Target::Url("http://x".into())).unwrap().is_none());
}

#[test]
fn capabilities_follow_the_matrix() {
    let dir = tempfile::TempDir::new().unwrap();
    let p = provider(scripted(&dir, &[HELLO.to_string()]));
    let rt = RuntimeInfo {
        provider: "dotnet",
        pid: Some(1),
        url: None,
        runtime_version: None,
        command: None,
    };
    let caps = p.capabilities(&rt);
    let get = |k| caps.iter().find(|c| c.kind == k).unwrap();
    assert_eq!(get(ProfileKind::Cpu).fidelity, Fidelity::Approximate);
    for k in [
        ProfileKind::Wall,
        ProfileKind::Memory,
        ProfileKind::Lock,
        ProfileKind::Threads,
    ] {
        assert_eq!(get(k).fidelity, Fidelity::Native, "{k}");
        assert!(get(k).available);
    }
    let blocking = get(ProfileKind::Blocking);
    assert!(!blocking.available);
    assert!(blocking.note.as_deref().unwrap().contains("Phase 1"));
}

#[test]
fn collect_imports_artifacts_with_fidelity() {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(
        &dir,
        &[
            HELLO.to_string(),
            pid_listing(77),
            LIST_DONE.to_string(),
            r#"{"type":"progress","message":"attaching"}"#.to_string(),
            art("cpu", CPU),
            art("memory", HEAP),
            art("lock", MUTEX),
            r#"{"type":"warning","message":"slow symbols"}"#.to_string(),
            r#"{"type":"done","end":"complete"}"#.to_string(),
        ],
    );
    let mut reg = ProviderRegistry::default();
    reg.register(Box::new(provider(h)));
    let home = tempfile::TempDir::new().unwrap();
    let store = Store::open(home.path()).unwrap();
    let req = RecordRequest {
        target: Target::Pid(77),
        plan: plan(vec![
            ProfileKind::Cpu,
            ProfileKind::Memory,
            ProfileKind::Lock,
        ]),
        name: None,
        tags: vec![],
    };
    let progress = Mutex::new(Vec::<String>::new());
    let s = record(&store, &reg, &req, &CancelToken::new(), &|m| {
        progress.lock().unwrap().push(m.to_string())
    })
    .unwrap();
    assert_eq!(s.status, SessionStatus::Complete);
    assert_eq!(s.profile_ids.len(), 3);
    let recs: Vec<_> = s
        .profile_ids
        .iter()
        .map(|i| store.resolve(i).unwrap())
        .collect();
    let by = |k| recs.iter().find(|r| r.kind == k).unwrap();
    assert_eq!(by(ProfileKind::Cpu).fidelity, Fidelity::Approximate);
    assert_eq!(by(ProfileKind::Memory).fidelity, Fidelity::Native);
    assert_eq!(by(ProfileKind::Lock).fidelity, Fidelity::Native);
    assert!(
        s.warnings.iter().any(|w| w == "slow symbols"),
        "{:?}",
        s.warnings
    );
    assert!(progress.lock().unwrap().iter().any(|m| m == "attaching"));
    assert_eq!(s.target.runtime, "dotnet");
}

#[test]
fn error_codes_map_to_messages() {
    let cases = [
        ("diagnostics_disabled", "DOTNET_EnableDiagnostics=0"),
        ("permission", "TMPDIR"),
        ("not_dotnet", "pid 77 is not a .NET 8+ process"),
        ("not_found", "no process with pid 77"),
        ("launch_failed", "raw helper text"),
        ("internal", "raw helper text"),
    ];
    for (code, expect) in cases {
        let dir = tempfile::TempDir::new().unwrap();
        let h = scripted(
            &dir,
            &[
                HELLO.to_string(),
                pid_listing(77),
                LIST_DONE.to_string(),
                format!(
                    r#"{{"for":"collect","type":"error","code":"{code}","message":"raw helper text"}}"#
                ),
            ],
        );
        let p = provider(h);
        let rt = detect_pid(&p, 77);
        let err = p
            .capture(
                &rt,
                &Target::Pid(77),
                &plan(vec![ProfileKind::Cpu]),
                &CancelToken::new(),
                &|_| {},
            )
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(expect), "{code}: {msg}");
        assert!(!msg.contains("Exception"), "{code}: {msg}");
    }
}

#[test]
fn target_exited_is_partial() {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(
        &dir,
        &[
            HELLO.to_string(),
            pid_listing(5),
            LIST_DONE.to_string(),
            art("wall", CPU),
            r#"{"type":"done","end":"target_exited"}"#.to_string(),
        ],
    );
    let mut reg = ProviderRegistry::default();
    reg.register(Box::new(provider(h)));
    let home = tempfile::TempDir::new().unwrap();
    let store = Store::open(home.path()).unwrap();
    let req = RecordRequest {
        target: Target::Pid(5),
        plan: plan(vec![ProfileKind::Wall]),
        name: None,
        tags: vec![],
    };
    let s = record(&store, &reg, &req, &CancelToken::new(), &|_| {}).unwrap();
    assert_eq!(s.status, SessionStatus::Partial);
    assert_eq!(s.profile_ids.len(), 1);
}

#[test]
fn stop_on_cancel() {
    let dir = tempfile::TempDir::new().unwrap();
    // The fake waits for "stop" on stdin, then reports a cancelled session with one artifact.
    let h = scripted(
        &dir,
        &[
            HELLO.to_string(),
            pid_listing(9),
            LIST_DONE.to_string(),
            r#"{"type":"wait_stop"}"#.to_string(),
            art("wall", CPU),
            r#"{"type":"done","end":"cancelled"}"#.to_string(),
        ],
    );
    let p = provider(h);
    let rt = detect_pid(&p, 9);
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        t2.cancel();
    });
    let start = Instant::now();
    let out = p
        .capture(
            &rt,
            &Target::Pid(9),
            &plan(vec![ProfileKind::Wall]),
            &token,
            &|_| {},
        )
        .unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(8),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(out.end, CaptureEnd::Cancelled);
    assert_eq!(out.artifacts.len(), 1);
    assert_eq!(out.artifacts[0].kind, ProfileKind::Wall);
}

#[test]
fn launch_passes_program_after_double_dash() {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(
        &dir,
        &[
            HELLO.to_string(),
            art("cpu", CPU),
            r#"{"type":"done","end":"complete"}"#.to_string(),
        ],
    );
    let p = provider(h);
    let target = Target::Launch {
        program: "dotnet".into(),
        args: vec!["app.dll".into(), "all".into(), "3".into()],
        cwd: None,
    };
    let rt = p.detect(&target).unwrap().unwrap();
    let out = p
        .capture(
            &rt,
            &target,
            &plan(vec![ProfileKind::Cpu]),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    assert_eq!(out.artifacts.len(), 1);
    assert_eq!(out.artifacts[0].fidelity, Fidelity::Approximate);
}

#[test]
fn locate_helper_env_then_sibling() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::TempDir::new().unwrap();
    let exe = dir.path().join(if cfg!(windows) { "h.exe" } else { "h" });
    std::fs::write(&exe, b"x").unwrap();
    let dll = dir.path().join("prism-dotnet.dll");
    std::fs::write(&dll, b"x").unwrap();

    std::env::set_var("PRISM_DOTNET", &exe);
    let h = locate_helper().unwrap();
    assert_eq!(h.program, exe);
    assert!(h.args_prefix.is_empty());

    std::env::set_var("PRISM_DOTNET", &dll);
    let h = locate_helper().unwrap();
    assert_eq!(h.program, PathBuf::from("dotnet"));
    assert_eq!(h.args_prefix, vec![dll.to_string_lossy().into_owned()]);

    std::env::set_var("PRISM_DOTNET", dir.path().join("missing.exe"));
    let err = locate_helper().unwrap_err().to_string();
    assert!(err.contains("PRISM_DOTNET"), "{err}");

    // Without the variable: the sibling of the current executable, else a build hint.
    std::env::remove_var("PRISM_DOTNET");
    let cur = std::env::current_exe().unwrap();
    let sibling = cur.with_file_name(if cfg!(windows) {
        "prism-dotnet.exe"
    } else {
        "prism-dotnet"
    });
    if sibling.exists() {
        assert_eq!(locate_helper().unwrap().program, sibling);
    } else {
        let err = locate_helper().unwrap_err().to_string();
        assert!(
            err.contains("helpers/prism-dotnet") || err.contains("helpers\\prism-dotnet"),
            "{err}"
        );
        std::fs::write(&sibling, b"x").unwrap();
        let found = locate_helper();
        let _ = std::fs::remove_file(&sibling);
        assert_eq!(found.unwrap().program, sibling);
    }
}

fn real_helper() -> Option<HelperPath> {
    if std::env::var("PRISM_E2E").as_deref() != Ok("1") {
        return None;
    }
    locate_helper().ok()
}

#[test]
#[ignore = "needs PRISM_E2E=1, dotnet on PATH and PRISM_DOTNET pointing at the real helper"]
fn dotnet_launch_demo_all_kinds() {
    let Some(h) = real_helper() else { return };
    let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll");
    assert!(demo.exists(), "build examples/dotnet-demo -c Release first");
    let p = provider(h);
    let target = Target::Launch {
        program: "dotnet".into(),
        args: vec![
            demo.to_string_lossy().into_owned(),
            "all".into(),
            "6".into(),
        ],
        cwd: None,
    };
    let rt = p.detect(&target).unwrap().unwrap();
    let kinds = vec![
        ProfileKind::Cpu,
        ProfileKind::Wall,
        ProfileKind::Memory,
        ProfileKind::Lock,
        ProfileKind::Threads,
    ];
    let out = p
        .capture(&rt, &target, &plan(kinds), &CancelToken::new(), &|_| {})
        .unwrap();
    assert_eq!(out.artifacts.len(), 5);
}

#[test]
#[ignore = "needs PRISM_E2E=1, dotnet on PATH and PRISM_DOTNET pointing at the real helper"]
fn dotnet_attach_by_pid() {
    let Some(h) = real_helper() else { return };
    let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll");
    let mut child = std::process::Command::new("dotnet")
        .arg(&demo)
        .args(["all", "20"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let p = provider(h);
    let rt = detect_pid(&p, child.id());
    let out = p
        .capture(
            &rt,
            &Target::Pid(child.id()),
            &plan(vec![ProfileKind::Wall, ProfileKind::Memory]),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(out.artifacts.len(), 2);
}

#[test]
fn convert_nettrace_returns_artifacts_with_fidelity() {
    let dir = tempfile::TempDir::new().unwrap();
    let trace = dir.path().join("x.nettrace");
    std::fs::write(&trace, b"nettrace").unwrap();
    let h = scripted(
        &dir,
        &[
            HELLO.to_string(),
            art("cpu", CPU),
            art("memory", HEAP),
            r#"{"type":"done","end":"complete"}"#.to_string(),
        ],
    );
    let arts = convert_nettrace_with(&h, &trace).unwrap();
    assert_eq!(arts.len(), 2);
    assert_eq!(arts[0].kind, ProfileKind::Cpu);
    assert_eq!(arts[0].fidelity, Fidelity::Approximate);
    assert_eq!(arts[1].fidelity, Fidelity::Native);
    assert!(!arts[0].bytes.is_empty());

    let err_h = scripted(
        &dir,
        &[
            HELLO.to_string(),
            r#"{"type":"error","code":"internal","message":"bad trace"}"#.to_string(),
        ],
    );
    let err = convert_nettrace_with(&err_h, &trace).unwrap_err();
    assert!(err.to_string().contains("bad trace"), "{err}");
}

#[test]
fn stop_grace_restarts_while_the_helper_keeps_reporting() {
    // After `stop` the real helper spends a long time on rundown and conversion but
    // keeps emitting progress; killing it at a fixed deadline would lose the capture.
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("PRISM_STOP_GRACE_MS", "2500");
    let dir = tempfile::TempDir::new().unwrap();
    let mut lines = vec![HELLO.to_string(), pid_listing(9), LIST_DONE.to_string()];
    lines.push(r#"{"for":"collect","type":"wait_stop"}"#.to_string());
    // 7 x 1.6 s = 11.2 s of shutdown work: longer than the old fixed 10 s deadline,
    // but never silent for longer than the 2.5 s idle grace.
    for i in 0..7 {
        lines.push(r#"{"for":"collect","type":"sleep","ms":1600}"#.to_string());
        lines.push(format!(
            r#"{{"for":"collect","type":"progress","message":"step {i}"}}"#
        ));
    }
    lines.push(art("wall", CPU));
    lines.push(r#"{"for":"collect","type":"done","end":"cancelled"}"#.to_string());
    let p = provider(scripted(&dir, &lines));
    let rt = detect_pid(&p, 9);
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        t2.cancel();
    });
    let res = p.capture(
        &rt,
        &Target::Pid(9),
        &plan(vec![ProfileKind::Wall]),
        &token,
        &|_| {},
    );
    std::env::remove_var("PRISM_STOP_GRACE_MS");
    let out = res.expect("capture survives a slow, chatty shutdown");
    assert_eq!(out.end, CaptureEnd::Cancelled);
    assert_eq!(out.artifacts.len(), 1);
}

#[test]
fn silent_helper_is_killed_after_the_grace() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("PRISM_STOP_GRACE_MS", "800");
    let dir = tempfile::TempDir::new().unwrap();
    let lines = vec![
        HELLO.to_string(),
        pid_listing(9),
        LIST_DONE.to_string(),
        r#"{"for":"collect","type":"wait_stop"}"#.to_string(),
        r#"{"for":"collect","type":"sleep","ms":20000}"#.to_string(),
    ];
    let p = provider(scripted(&dir, &lines));
    let rt = detect_pid(&p, 9);
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        t2.cancel();
    });
    let start = Instant::now();
    let res = p.capture(
        &rt,
        &Target::Pid(9),
        &plan(vec![ProfileKind::Wall]),
        &token,
        &|_| {},
    );
    std::env::remove_var("PRISM_STOP_GRACE_MS");
    assert!(res.unwrap_err().to_string().contains("did not stop"));
    assert!(start.elapsed() < Duration::from_secs(10));
}

// ---- V5 timelines ----

const WINDOWS: &str = "fixtures/cpu-windows.pb.gz";

fn launch_target() -> Target {
    Target::Launch {
        program: "dotnet".into(),
        args: vec!["app.dll".into()],
        cwd: None,
    }
}

fn timeline_plan(kinds: Vec<ProfileKind>, on: bool) -> CapturePlan {
    let mut p = plan(kinds);
    p.timeline = on;
    p
}

fn capture_with(lines: Vec<String>, plan: CapturePlan) -> (CaptureOutcome, Vec<String>) {
    let dir = tempfile::TempDir::new().unwrap();
    let h = scripted(&dir, &lines);
    let p = provider(h);
    let target = launch_target();
    let rt = p.detect(&target).unwrap().unwrap();
    let seen = Mutex::new(Vec::<String>::new());
    let out = p
        .capture(&rt, &target, &plan, &CancelToken::new(), &|m| {
            seen.lock().unwrap().push(m.to_string())
        })
        .unwrap();
    (out, seen.into_inner().unwrap())
}

fn script(art_line: String) -> Vec<String> {
    vec![
        HELLO.to_string(),
        r#"{"type":"echo_args"}"#.to_string(),
        art_line,
        r#"{"type":"done","end":"complete"}"#.to_string(),
    ]
}

#[test]
fn dotnet_cpu_with_labelled_samples_gets_a_20ms_timeline() {
    let (out, seen) = capture_with(
        script(art("cpu", WINDOWS)),
        timeline_plan(vec![ProfileKind::Cpu], true),
    );
    assert!(
        seen.iter().any(|m| m.contains("--bucket-ms 20")),
        "{seen:?}"
    );
    let a = &out.artifacts[0];
    let t = a.timeline.as_ref().expect("timeline");
    let n = prism_import::import_bytes(&a.bytes, &prism_import::ImportHints::default())
        .unwrap()
        .samples
        .len();
    assert!(t.validate(n));
    assert_eq!(t.bucket_ns, 20_000_000);
}

#[test]
fn dotnet_no_timeline_flag_is_not_passed_and_nothing_is_stored() {
    let (out, seen) = capture_with(
        script(art("cpu", WINDOWS)),
        timeline_plan(vec![ProfileKind::Cpu], false),
    );
    assert!(seen.iter().all(|m| !m.contains("--bucket-ms")), "{seen:?}");
    assert!(out.artifacts[0].timeline.is_none());
}

#[test]
fn dotnet_cpu_without_labels_has_no_timeline() {
    let (out, _) = capture_with(
        script(art("cpu", CPU)),
        timeline_plan(vec![ProfileKind::Cpu], true),
    );
    assert!(out.artifacts[0].timeline.is_none());
}

#[test]
fn dotnet_memory_never_gets_a_timeline() {
    let (out, seen) = capture_with(
        script(art("memory", HEAP)),
        timeline_plan(vec![ProfileKind::Memory], true),
    );
    assert!(seen.iter().all(|m| !m.contains("--bucket-ms")), "{seen:?}");
    assert!(out.artifacts[0].timeline.is_none());
}
