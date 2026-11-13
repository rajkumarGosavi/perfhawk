//! Phase 1 acceptance tests against real Go and .NET targets.
//!
//! All tests are `#[ignore]`; CI runs them with `PRISM_E2E=1 ... -- --include-ignored`.
//! They need `go` and `dotnet` on PATH, a built `examples/dotnet-demo` (Release) and
//! `PRISM_DOTNET` pointing at the built prism-dotnet helper.

use prism_analysis::{diff_top, DiffInput, DiffNormalize, Filter};
use prism_capture::*;
use prism_model::{Fidelity, ProfileKind};
use prism_store::{SessionRecord, SessionStatus, Store};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn enabled() -> bool {
    std::env::var("PRISM_E2E").as_deref() == Ok("1")
}

fn go_demo_exe() -> &'static PathBuf {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("prism-e2e-godemo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(if cfg!(windows) {
            "go-demo.exe"
        } else {
            "go-demo"
        });
        let st = Command::new("go")
            .args(["build", "-o"])
            .arg(&exe)
            .arg(".")
            .current_dir(root().join("examples/go-demo"))
            .status()
            .expect("go on PATH");
        assert!(st.success(), "go build examples/go-demo");
        exe
    })
}

fn dotnet_demo_dll() -> PathBuf {
    let p = root().join("examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll");
    assert!(
        p.exists(),
        "build it first: dotnet build examples/dotnet-demo -c Release"
    );
    p
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

struct Killer(Child);

impl Drop for Killer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_go_demo(extra: &[&str]) -> Killer {
    let child = Command::new(go_demo_exe())
        .args(["-files=false", "-duration", "90s"])
        .args(extra)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Killer(child)
}

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::TempDir::new().unwrap();
    let s = Store::open(dir.path()).unwrap();
    (dir, s)
}

fn run(
    store: &Store,
    target: Target,
    kinds: Vec<ProfileKind>,
    secs: u64,
    name: &str,
) -> Result<SessionRecord, CaptureError> {
    record(
        store,
        &ProviderRegistry::with_defaults(),
        &RecordRequest {
            target,
            plan: CapturePlan {
                kinds,
                duration: Duration::from_secs(secs),
                timeline: false,
            },
            name: Some(name.into()),
            tags: vec![],
        },
        &CancelToken::new(),
        &|m| eprintln!("{m}"),
    )
}

/// Waits until the agent of `pid` answers.
fn wait_for_agent(pid: u32) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if prism_capture::go::discovery::discover(pid).is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("prism agent of pid {pid} never appeared");
}

fn profiles(store: &Store, s: &SessionRecord) -> Vec<prism_store::ProfileRecord> {
    s.profile_ids
        .iter()
        .map(|i| store.resolve(i).unwrap())
        .collect()
}

fn function_names(store: &Store, id: &str) -> Vec<String> {
    let p = store.load(id).unwrap();
    p.functions
        .iter()
        .map(|f| p.str(f.name).to_string())
        .collect()
}

#[test]
#[ignore = "e2e: needs PRISM_E2E=1, go"]
fn go_record_all_kinds_via_agent() {
    if !enabled() {
        return;
    }
    let child = spawn_go_demo(&[]);
    let pid = child.0.id();
    wait_for_agent(pid);
    let (_d, store) = store();
    let s = run(&store, Target::Pid(pid), vec![], 3, "go-agent").unwrap();
    assert_eq!(s.status, SessionStatus::Complete, "{:?}", s.warnings);
    let recs = profiles(&store, &s);
    assert_eq!(recs.len(), 6, "{recs:?}");
    let get = |k: ProfileKind| {
        recs.iter()
            .find(|r| r.kind == k)
            .unwrap_or_else(|| panic!("no {k}"))
    };
    assert_eq!(get(ProfileKind::Cpu).fidelity, Fidelity::Native);
    assert_eq!(get(ProfileKind::Wall).fidelity, Fidelity::Derived);
    for k in [
        ProfileKind::Memory,
        ProfileKind::Lock,
        ProfileKind::Blocking,
        ProfileKind::Threads,
    ] {
        assert_eq!(get(k).fidelity, Fidelity::Native, "{k}");
    }
    for k in [
        ProfileKind::Cpu,
        ProfileKind::Wall,
        ProfileKind::Memory,
        ProfileKind::Lock,
        ProfileKind::Threads,
    ] {
        assert!(get(k).sample_count >= 1, "{k} has no samples");
    }
    // The agent is present, so an empty blocking profile carries no warning.
    assert!(
        !s.warnings
            .iter()
            .any(|w| w.contains("disabled in the target")),
        "{:?}",
        s.warnings
    );
}

#[test]
#[ignore = "e2e: needs PRISM_E2E=1, go"]
fn go_record_url_plain_pprof() {
    if !enabled() {
        return;
    }
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let child = spawn_go_demo(&["-agent=false", "-pprof", &addr]);
    let url = format!("http://{addr}");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) && std::net::TcpStream::connect(&addr).is_err()
    {
        std::thread::sleep(Duration::from_millis(200));
    }
    let (_d, store) = store();
    // wall needs the agent
    let err = run(
        &store,
        Target::Url(url.clone()),
        vec![ProfileKind::Wall],
        2,
        "plain",
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            CaptureError::Unsupported {
                kind: ProfileKind::Wall,
                ..
            }
        ),
        "{err}"
    );
    assert!(err.to_string().contains("agent"), "{err}");
    let s = run(
        &store,
        Target::Url(url),
        vec![ProfileKind::Cpu, ProfileKind::Memory],
        2,
        "plain",
    )
    .unwrap();
    assert_eq!(s.status, SessionStatus::Complete, "{:?}", s.warnings);
    assert_eq!(profiles(&store, &s).len(), 2);
    drop(child);
}

#[test]
#[ignore = "e2e: needs PRISM_E2E=1, dotnet and PRISM_DOTNET"]
fn dotnet_record_launch_all_kinds() {
    if !enabled() {
        return;
    }
    let (_d, store) = store();
    let demo = dotnet_demo_dll();
    let kinds = vec![
        ProfileKind::Cpu,
        ProfileKind::Wall,
        ProfileKind::Memory,
        ProfileKind::Lock,
        ProfileKind::Threads,
    ];
    let s = run(
        &store,
        Target::Launch {
            program: "dotnet".into(),
            args: vec![
                demo.to_string_lossy().into_owned(),
                "all".into(),
                "6".into(),
            ],
            cwd: None,
        },
        kinds,
        3,
        "dn-launch",
    )
    .unwrap();
    assert_eq!(s.status, SessionStatus::Complete, "{:?}", s.warnings);
    let recs = profiles(&store, &s);
    assert_eq!(recs.len(), 5);
    let get = |k: ProfileKind| recs.iter().find(|r| r.kind == k).unwrap();
    assert_eq!(get(ProfileKind::Cpu).fidelity, Fidelity::Approximate);
    assert!(function_names(&store, &get(ProfileKind::Memory).id)
        .iter()
        .any(|n| n.contains("BuildReports")));
    assert!(function_names(&store, &get(ProfileKind::Lock).id)
        .iter()
        .any(|n| n.contains("ReserveInventory")));
}

#[test]
#[ignore = "e2e: needs PRISM_E2E=1, dotnet and PRISM_DOTNET"]
fn dotnet_record_attach_pid() {
    if !enabled() {
        return;
    }
    let child = Killer(
        Command::new("dotnet")
            .arg(dotnet_demo_dll())
            .args(["all", "30"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    std::thread::sleep(Duration::from_secs(2));
    let (_d, store) = store();
    let s = run(
        &store,
        Target::Pid(child.0.id()),
        vec![ProfileKind::Wall, ProfileKind::Memory, ProfileKind::Threads],
        3,
        "dn-attach",
    )
    .unwrap();
    assert_eq!(s.status, SessionStatus::Complete, "{:?}", s.warnings);
    assert_eq!(profiles(&store, &s).len(), 3);
}

#[test]
#[ignore = "e2e: needs PRISM_E2E=1, go"]
fn diff_two_go_sessions() {
    if !enabled() {
        return;
    }
    let child = spawn_go_demo(&[]);
    let pid = child.0.id();
    wait_for_agent(pid);
    let (_d, store) = store();
    let a = run(
        &store,
        Target::Pid(pid),
        vec![ProfileKind::Cpu],
        2,
        "diff-a",
    )
    .unwrap();
    let b = run(
        &store,
        Target::Pid(pid),
        vec![ProfileKind::Cpu],
        2,
        "diff-b",
    )
    .unwrap();
    let (pa, pb) = (
        store.load(&a.profile_ids[0]).unwrap(),
        store.load(&b.profile_ids[0]).unwrap(),
    );
    let (ia, ib) = prism_analysis::match_sample_type(&pa, &pb, Some("cpu")).unwrap();
    let rows = diff_top(
        DiffInput {
            profile: &pa,
            sample_type: ia,
        },
        DiffInput {
            profile: &pb,
            sample_type: ib,
        },
        &Filter::default(),
        DiffNormalize::None,
        10,
    );
    assert!(!rows.is_empty());
}

#[test]
#[ignore = "e2e: needs PRISM_E2E=1, dotnet and PRISM_DOTNET"]
fn target_exit_partial() {
    if !enabled() {
        return;
    }
    let (_d, store) = store();
    let s = run(
        &store,
        Target::Launch {
            program: "dotnet".into(),
            args: vec![
                dotnet_demo_dll().to_string_lossy().into_owned(),
                "cpu".into(),
                "1".into(),
            ],
            cwd: None,
        },
        vec![ProfileKind::Wall],
        5,
        "dn-exit",
    )
    .unwrap();
    assert_eq!(s.status, SessionStatus::Partial, "{:?}", s.warnings);
}
