//! Known-answer checks of Doctor against the Go demo in its single-purpose modes.
//!
//! All tests are `#[ignore]`; run them with `PERFHAWK_E2E=1 cargo test -p perfhawk-cli
//! --test doctor_e2e -- --include-ignored`. They need `go` on PATH.

use perfhawk_analysis::Verdict;
use perfhawk_capture::{record, CancelToken, CapturePlan, ProviderRegistry, RecordRequest, Target};
use perfhawk_server::doctor::diagnose_session;
use perfhawk_store::Store;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn enabled() -> bool {
    std::env::var("PERFHAWK_E2E").as_deref() == Ok("1")
}

fn go_demo_exe() -> &'static PathBuf {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| {
        let dir =
            std::env::temp_dir().join(format!("perfhawk-doctor-e2e-godemo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(if cfg!(windows) { "go-demo.exe" } else { "go-demo" });
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

struct Killer(Child);

impl Drop for Killer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_demo(mode: &str) -> Killer {
    let child = Command::new(go_demo_exe())
        .args(["-files=false", "-duration", "90s", "-pprof", "", "-mode", mode])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Killer(child)
}

fn wait_for_agent(pid: u32) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if perfhawk_capture::go::discovery::discover(pid).is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("perfhawk agent of pid {pid} never appeared");
}

fn diagnose_mode(mode: &str) -> perfhawk_analysis::Diagnosis {
    let demo = spawn_demo(mode);
    let pid = demo.0.id();
    wait_for_agent(pid);
    let dir = tempfile::TempDir::new().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let session = record(
        &store,
        &ProviderRegistry::with_defaults(),
        &RecordRequest {
            target: Target::Pid(pid),
            plan: CapturePlan {
                kinds: vec![],
                duration: Duration::from_secs(10),
                timeline: true,
            },
            name: Some(format!("doctor-{mode}")),
            tags: vec![],
        },
        &CancelToken::new(),
        &|m| eprintln!("{m}"),
    )
    .unwrap();
    let d = diagnose_session(&store, &session).unwrap();
    eprintln!("{}", serde_json::to_string_pretty(&d).unwrap());
    d
}

#[test]
#[ignore = "e2e: needs PERFHAWK_E2E=1, go"]
fn doctor_cpu_mode_is_cpu_bound() {
    if !enabled() {
        return;
    }
    assert_eq!(diagnose_mode("cpu").verdict, Verdict::CpuBound);
}

#[test]
#[ignore = "e2e: needs PERFHAWK_E2E=1, go"]
fn doctor_lock_mode_is_lock_bound() {
    if !enabled() {
        return;
    }
    assert_eq!(diagnose_mode("lock").verdict, Verdict::LockBound);
}

#[test]
#[ignore = "e2e: needs PERFHAWK_E2E=1, go"]
fn doctor_disk_mode_is_io_likely() {
    if !enabled() {
        return;
    }
    let d = diagnose_mode("disk");
    assert_eq!(d.verdict, Verdict::IoBound);
    let f = d.findings.iter().find(|f| f.rule == "io_bound").unwrap();
    assert_eq!(f.confidence, perfhawk_analysis::Confidence::Low);
    assert!(f.next.iter().any(|n| n.follow_up.is_some()));
}
