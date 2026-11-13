use prism_capture::go::discovery;
use prism_capture::*;
use prism_model::{Fidelity, ProfileKind};
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CPU: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-cpu.pb.gz");
const HEAP: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-heap.pb.gz");
const MUTEX: &[u8] = include_bytes!("../../prism-import/tests/fixtures/go-mutex.pb.gz");

/// A pprof profile with a sample type but no samples.
fn empty_pprof() -> Vec<u8> {
    let mut raw = vec![0x0a, 0x04, 0x08, 0x01, 0x10, 0x02];
    for s in ["", "contentions", "count"] {
        raw.push(0x32);
        raw.push(s.len() as u8);
        raw.extend_from_slice(s.as_bytes());
    }
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    enc.write_all(&raw).unwrap();
    enc.finish().unwrap()
}

#[derive(Clone)]
struct Route {
    status: u16,
    body: Vec<u8>,
    /// Sleep this many seconds (taken from the `seconds` query value) before answering.
    delay_by_seconds: bool,
}

struct Mock {
    base: String,
    hits: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn ok(body: &[u8], delay: bool) -> Route {
    Route {
        status: 200,
        body: body.to_vec(),
        delay_by_seconds: delay,
    }
}

fn mock(routes: Vec<(&str, Route)>) -> Mock {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let routes: HashMap<String, Route> = routes
        .into_iter()
        .map(|(p, r)| (p.to_string(), r))
        .collect();
    let hits = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let (h, s) = (hits.clone(), stop.clone());
    std::thread::spawn(move || {
        while !s.load(Ordering::SeqCst) {
            let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(50)) else {
                continue;
            };
            let url = req.url().to_string();
            h.lock().unwrap().push(url.clone());
            let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
            let route = routes.get(path).cloned();
            let secs: u64 = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("seconds="))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let s2 = s.clone();
            std::thread::spawn(move || match route {
                Some(r) => {
                    if r.delay_by_seconds {
                        // A stopped mock hangs up on in-flight requests, like a dead target.
                        let until = Instant::now() + Duration::from_secs(secs);
                        while Instant::now() < until {
                            if s2.load(Ordering::SeqCst) {
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(25));
                        }
                    }
                    let resp = tiny_http::Response::from_data(r.body).with_status_code(r.status);
                    let _ = req.respond(resp);
                }
                None => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                }
            });
        }
    });
    Mock { base, hits, stop }
}

const INFO: &str = r#"{"protocol":1,"pid":4242,"go_version":"go1.22.0"}"#;

fn agent_routes() -> Vec<(&'static str, Route)> {
    vec![
        ("/debug/prism/info", ok(INFO.as_bytes(), false)),
        ("/debug/pprof/profile", ok(CPU, true)),
        ("/debug/prism/wall", ok(CPU, true)),
        ("/debug/prism/contention", ok(MUTEX, true)),
        ("/debug/pprof/heap", ok(HEAP, false)),
        ("/debug/pprof/goroutine", ok(HEAP, false)),
    ]
}

fn plan(kinds: Vec<ProfileKind>, secs: u64) -> CapturePlan {
    CapturePlan {
        kinds,
        duration: Duration::from_secs(secs),
        timeline: false,
    }
}

fn detect_url(m: &Mock) -> (GoProvider, RuntimeInfo) {
    let p = GoProvider::new();
    let rt = p.detect(&Target::Url(m.base.clone())).unwrap().unwrap();
    (p, rt)
}

fn kind_of(o: &CaptureOutcome, k: ProfileKind) -> &Artifact {
    o.artifacts
        .iter()
        .find(|a| a.kind == k)
        .unwrap_or_else(|| panic!("no {k} artifact"))
}

#[test]
fn detect_by_url_with_agent() {
    let m = mock(agent_routes());
    let (p, rt) = detect_url(&m);
    assert_eq!(rt.provider, "go");
    assert_eq!(rt.pid, Some(4242));
    assert_eq!(rt.runtime_version.as_deref(), Some("go1.22.0"));
    let caps = p.capabilities(&rt);
    let get = |k| caps.iter().find(|c| c.kind == k).unwrap();
    assert!(caps.iter().all(|c| c.available));
    assert_eq!(get(ProfileKind::Cpu).fidelity, Fidelity::Native);
    assert_eq!(get(ProfileKind::Wall).fidelity, Fidelity::Derived);
    assert_eq!(get(ProfileKind::Memory).fidelity, Fidelity::Native);
    assert_eq!(get(ProfileKind::Lock).fidelity, Fidelity::Native);
    assert_eq!(get(ProfileKind::Blocking).fidelity, Fidelity::Native);
    assert_eq!(get(ProfileKind::Threads).fidelity, Fidelity::Native);
}

#[test]
fn detect_plain_pprof_marks_wall_unavailable() {
    let m = mock(vec![("/debug/pprof/", ok(b"<html>profiles</html>", false))]);
    let (p, rt) = detect_url(&m);
    assert_eq!(rt.runtime_version, None);
    let caps = p.capabilities(&rt);
    let wall = caps.iter().find(|c| c.kind == ProfileKind::Wall).unwrap();
    assert!(!wall.available);
    assert!(wall.note.as_deref().unwrap().contains("prism agent"));
    let lock = caps.iter().find(|c| c.kind == ProfileKind::Lock).unwrap();
    assert!(lock.available);
    assert!(lock
        .note
        .as_deref()
        .unwrap()
        .contains("SetMutexProfileFraction"));
}

#[test]
fn detect_non_go_url_is_unreachable_error() {
    let m = mock(vec![]);
    let err = GoProvider::new()
        .detect(&Target::Url(m.base.clone()))
        .unwrap_err();
    assert!(err.to_string().contains("pprof"), "{err}");
}

#[test]
fn capture_all_kinds_concurrently() {
    let m = mock(agent_routes());
    let (p, rt) = detect_url(&m);
    let kinds = vec![
        ProfileKind::Cpu,
        ProfileKind::Wall,
        ProfileKind::Lock,
        ProfileKind::Blocking,
        ProfileKind::Memory,
        ProfileKind::Threads,
    ];
    let start = Instant::now();
    let out = p
        .capture(
            &rt,
            &Target::Url(m.base.clone()),
            &plan(kinds, 2),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    let took = start.elapsed();
    assert!(took < Duration::from_secs(4), "serial capture? {took:?}");
    assert!(took >= Duration::from_secs(2), "{took:?}");
    assert_eq!(out.end, CaptureEnd::Complete);
    assert_eq!(out.artifacts.len(), 6);
    assert_eq!(kind_of(&out, ProfileKind::Wall).fidelity, Fidelity::Derived);
    let hits = m.hits.lock().unwrap().clone();
    assert!(
        hits.contains(&"/debug/pprof/profile?seconds=2".to_string()),
        "{hits:?}"
    );
    assert!(
        hits.contains(&"/debug/prism/wall?seconds=2".to_string()),
        "{hits:?}"
    );
    assert!(
        hits.contains(&"/debug/prism/contention?seconds=2&kind=mutex".to_string()),
        "{hits:?}"
    );
    assert!(
        hits.contains(&"/debug/prism/contention?seconds=2&kind=block".to_string()),
        "{hits:?}"
    );
    assert!(hits.contains(&"/debug/pprof/heap".to_string()), "{hits:?}");
    assert!(
        hits.contains(&"/debug/pprof/goroutine".to_string()),
        "{hits:?}"
    );
}

#[test]
fn plain_pprof_uses_stock_endpoints() {
    let m = mock(vec![
        ("/debug/pprof/", ok(b"x", false)),
        ("/debug/pprof/profile", ok(CPU, true)),
        ("/debug/pprof/mutex", ok(MUTEX, true)),
    ]);
    let (p, rt) = detect_url(&m);
    let out = p
        .capture(
            &rt,
            &Target::Url(m.base.clone()),
            &plan(vec![ProfileKind::Cpu, ProfileKind::Lock], 1),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    let hits = m.hits.lock().unwrap().clone();
    assert!(
        hits.contains(&"/debug/pprof/mutex?seconds=1".to_string()),
        "{hits:?}"
    );
}

#[test]
fn empty_mutex_without_agent_warns() {
    let m = mock(vec![
        ("/debug/pprof/", ok(b"x", false)),
        ("/debug/pprof/mutex", ok(&empty_pprof(), true)),
    ]);
    let (p, rt) = detect_url(&m);
    let out = p
        .capture(
            &rt,
            &Target::Url(m.base.clone()),
            &plan(vec![ProfileKind::Lock], 1),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    let a = kind_of(&out, ProfileKind::Lock);
    assert_eq!(
        a.warning.as_deref(),
        Some("mutex profiling is disabled in the target (call runtime.SetMutexProfileFraction or use the prism agent)")
    );
}

#[test]
fn empty_block_with_agent_has_no_warning() {
    let m = mock(vec![
        ("/debug/prism/info", ok(INFO.as_bytes(), false)),
        ("/debug/prism/contention", ok(&empty_pprof(), true)),
    ]);
    let (p, rt) = detect_url(&m);
    let out = p
        .capture(
            &rt,
            &Target::Url(m.base.clone()),
            &plan(vec![ProfileKind::Blocking], 1),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    assert_eq!(kind_of(&out, ProfileKind::Blocking).warning, None);
}

#[test]
fn connection_refused_midway_is_target_exited() {
    let m = mock(agent_routes());
    let (p, rt) = detect_url(&m);
    // Kill the target before capture starts: every request is refused.
    let url = m.base.clone();
    drop(m);
    std::thread::sleep(Duration::from_millis(300));
    let out = p
        .capture(
            &rt,
            &Target::Url(url),
            &plan(vec![ProfileKind::Cpu, ProfileKind::Memory], 1),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    assert_eq!(out.end, CaptureEnd::TargetExited);
    assert!(out.artifacts.is_empty());
}

#[test]
fn partial_data_kept_when_target_dies() {
    // heap answers immediately; cpu hangs up mid-request.
    let mut routes = agent_routes();
    routes.retain(|(p, _)| *p != "/debug/pprof/profile");
    let m = mock(routes);
    let (p, rt) = detect_url(&m);
    let out = p
        .capture(
            &rt,
            &Target::Url(m.base.clone()),
            &plan(vec![ProfileKind::Cpu, ProfileKind::Memory], 1),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    // cpu endpoint 404s => warning, memory still delivered
    assert!(out.artifacts.iter().any(|a| a.kind == ProfileKind::Memory));
    assert!(
        out.warnings.iter().any(|w| w.contains("cpu")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn cancel_returns_partial() {
    let m = mock(agent_routes());
    let (p, rt) = detect_url(&m);
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        t2.cancel();
    });
    let start = Instant::now();
    let out = p
        .capture(
            &rt,
            &Target::Url(m.base.clone()),
            &plan(vec![ProfileKind::Cpu, ProfileKind::Memory], 30),
            &token,
            &|_| {},
        )
        .unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(out.end, CaptureEnd::Cancelled);
    // memory is snapshotted immediately on cancel; cpu never finished
    assert!(out.artifacts.iter().any(|a| a.kind == ProfileKind::Memory));
    assert!(!out.artifacts.iter().any(|a| a.kind == ProfileKind::Cpu));
}

#[test]
fn discovery_stale_file_ignored() {
    let dir = tempfile::TempDir::new().unwrap();
    // Nothing listens on this port.
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    drop(l);
    std::fs::write(
        dir.path().join("4242.json"),
        format!(r#"{{"pid":4242,"addr":"{addr}","go_version":"go1.22.0","protocol":1}}"#),
    )
    .unwrap();
    assert!(discovery::discover_in(dir.path(), 4242).is_none());
    assert!(discovery::discover_in(dir.path(), 9999).is_none());
}

#[test]
fn discovery_live_file_found_and_pid_checked() {
    let m = mock(agent_routes());
    let dir = tempfile::TempDir::new().unwrap();
    let addr = m.base.trim_start_matches("http://");
    let write = |pid: u32| {
        std::fs::write(
            dir.path().join(format!("{pid}.json")),
            format!(r#"{{"pid":{pid},"addr":"{addr}","go_version":"go1.22.0","protocol":1}}"#),
        )
        .unwrap();
    };
    write(4242);
    let d = discovery::discover_in(dir.path(), 4242).expect("live agent");
    assert_eq!(d.url, m.base);
    assert_eq!(d.go_version.as_deref(), Some("go1.22.0"));
    // The endpoint reports pid 4242, so a file claiming another pid is stale (pid reuse).
    write(5555);
    assert!(discovery::discover_in(dir.path(), 5555).is_none());
}

#[test]
fn buildinfo_detection() {
    let dir = tempfile::TempDir::new().unwrap();
    let go = dir.path().join("goprog");
    let mut bytes = vec![0u8; 5000];
    bytes.extend_from_slice(b"\xff Go buildinf:");
    bytes.extend_from_slice(&[0u8; 100]);
    std::fs::write(&go, bytes).unwrap();
    let other = dir.path().join("other");
    std::fs::write(&other, vec![1u8; 9000]).unwrap();
    assert!(discovery::has_go_buildinfo(&go));
    assert!(!discovery::has_go_buildinfo(&other));
    assert!(!discovery::has_go_buildinfo(&dir.path().join("missing")));
}

#[test]
fn detect_launch_by_buildinfo_and_gotest_needs_go() {
    let dir = tempfile::TempDir::new().unwrap();
    let go = dir.path().join("goprog");
    let mut bytes = vec![0u8; 100];
    bytes.extend_from_slice(b"\xff Go buildinf:");
    std::fs::write(&go, bytes).unwrap();
    let p = GoProvider::new();
    let launch = Target::Launch {
        program: go.to_string_lossy().into_owned(),
        args: vec![],
        cwd: None,
    };
    let rt = p.detect(&launch).unwrap().expect("go launch detected");
    assert_eq!(rt.provider, "go");
    let not_go = Target::Launch {
        program: dir.path().join("nothing").to_string_lossy().into_owned(),
        args: vec![],
        cwd: None,
    };
    assert!(p.detect(&not_go).unwrap().is_none());
}

#[test]
fn gotest_capabilities_limited() {
    let p = GoProvider::new();
    let rt = RuntimeInfo {
        provider: "go",
        pid: None,
        url: None,
        runtime_version: None,
        command: Some("go test ./pkg".into()),
    };
    let caps = p.capabilities(&rt);
    let avail = |k| caps.iter().find(|c| c.kind == k).unwrap().available;
    assert!(avail(ProfileKind::Cpu) && avail(ProfileKind::Memory));
    assert!(avail(ProfileKind::Lock) && avail(ProfileKind::Blocking));
    assert!(!avail(ProfileKind::Wall) && !avail(ProfileKind::Threads));
}

fn e2e_enabled() -> bool {
    std::env::var("PRISM_E2E").as_deref() == Ok("1")
        && std::process::Command::new("go")
            .arg("version")
            .output()
            .is_ok()
}

fn go_app_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goapp")
}

fn build_goapp(out_dir: &std::path::Path) -> std::path::PathBuf {
    let exe = out_dir.join(if cfg!(windows) { "goapp.exe" } else { "goapp" });
    let st = std::process::Command::new("go")
        .args(["build", "-o"])
        .arg(&exe)
        .arg(".")
        .current_dir(go_app_dir())
        .status()
        .unwrap();
    assert!(st.success(), "go build failed");
    exe
}

#[test]
#[ignore = "needs PRISM_E2E=1 and go on PATH"]
fn go_attach_by_pid_via_discovery() {
    if !e2e_enabled() {
        return;
    }
    let dir = tempfile::TempDir::new().unwrap();
    let exe = build_goapp(dir.path());
    let mut child = std::process::Command::new(&exe)
        .env("PRISM_AGENT", "1")
        .env("GOAPP_SECONDS", "30")
        .spawn()
        .unwrap();
    let pid = child.id();
    let p = GoProvider::new();
    let mut rt = None;
    for _ in 0..100 {
        if let Some(r) = p.detect(&Target::Pid(pid)).unwrap() {
            rt = Some(r);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let rt = rt.expect("agent discovered");
    let out = p
        .capture(
            &rt,
            &Target::Pid(pid),
            &plan(
                vec![ProfileKind::Cpu, ProfileKind::Wall, ProfileKind::Memory],
                2,
            ),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(out.artifacts.len(), 3);
}

#[test]
#[ignore = "needs PRISM_E2E=1 and go on PATH"]
fn go_launch_with_agent() {
    if !e2e_enabled() {
        return;
    }
    let dir = tempfile::TempDir::new().unwrap();
    let exe = build_goapp(dir.path());
    let target = Target::Launch {
        program: exe.to_string_lossy().into_owned(),
        args: vec![],
        cwd: None,
    };
    let p = GoProvider::new();
    let rt = p.detect(&target).unwrap().expect("go program detected");
    let out = p
        .capture(
            &rt,
            &target,
            &plan(vec![ProfileKind::Cpu, ProfileKind::Memory], 2),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
}

#[test]
#[ignore = "needs PRISM_E2E=1 and go on PATH"]
fn go_test_mode_produces_cpu_and_heap() {
    if !e2e_enabled() {
        return;
    }
    let target = Target::GoTest {
        package: ".".into(),
        args: vec![],
    };
    let p = GoProvider::new();
    let rt = p.detect(&target).unwrap().expect("go on path");
    let old = std::env::current_dir().unwrap();
    std::env::set_current_dir(go_app_dir()).unwrap();
    let out = p.capture(
        &rt,
        &target,
        &plan(vec![ProfileKind::Cpu, ProfileKind::Memory], 1),
        &CancelToken::new(),
        &|_| {},
    );
    std::env::set_current_dir(old).unwrap();
    let out = out.unwrap();
    assert!(out.artifacts.iter().any(|a| a.kind == ProfileKind::Cpu));
    assert!(out.artifacts.iter().any(|a| a.kind == ProfileKind::Memory));
}

#[test]
fn target_dying_mid_window_ends_capture_promptly() {
    // A raw TCP "target" that accepts requests, never answers, then hangs up (dies).
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let streams = Arc::new(Mutex::new(Vec::new()));
    let acc = streams.clone();
    std::thread::spawn(move || {
        for st in listener.incoming().flatten() {
            acc.lock().unwrap().push(st);
        }
    });
    let killer = streams.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        for st in killer.lock().unwrap().drain(..) {
            let _ = st.shutdown(std::net::Shutdown::Both);
        }
    });
    let rt = RuntimeInfo {
        provider: "go",
        pid: Some(1),
        url: Some(base.clone()),
        runtime_version: Some("go1.22.0".into()),
        command: None,
    };
    let start = Instant::now();
    let out = GoProvider::new()
        .capture(
            &rt,
            &Target::Url(base),
            &plan(vec![ProfileKind::Cpu, ProfileKind::Memory], 30),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(out.end, CaptureEnd::TargetExited);
}

#[test]
fn discovery_ignores_non_loopback_addresses() {
    // A planted file must not make prism talk to (and import from) another host.
    let m = mock(agent_routes());
    let port = m.base.rsplit(':').next().unwrap().to_string();
    let dir = tempfile::TempDir::new().unwrap();
    for (i, addr) in ["0.0.0.0", "10.1.2.3", "example.com", "[2001:db8::1]"]
        .iter()
        .enumerate()
    {
        let pid = 4242 + i as u32;
        std::fs::write(
            dir.path().join(format!("{pid}.json")),
            format!(
                r#"{{"pid":{pid},"addr":"{addr}:{port}","go_version":"go1.22.0","protocol":1}}"#
            ),
        )
        .unwrap();
        assert!(discovery::discover_in(dir.path(), pid).is_none(), "{addr}");
    }
    assert!(
        m.hits.lock().unwrap().is_empty(),
        "no request may be sent to a non-loopback address: {:?}",
        m.hits.lock().unwrap()
    );
}

// ---- V5 windowed CPU timelines ----

const WINDOWS: &[u8] = include_bytes!("fixtures/cpu-windows.pb.gz");

fn timeline_plan(kinds: Vec<ProfileKind>, on: bool) -> CapturePlan {
    CapturePlan {
        kinds,
        duration: Duration::from_secs(1),
        timeline: on,
    }
}

fn run_capture(m: &Mock, p: &GoProvider, rt: &RuntimeInfo, plan: CapturePlan) -> CaptureOutcome {
    p.capture(
        rt,
        &Target::Url(m.base.clone()),
        &plan,
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap()
}

fn windows_routes() -> Vec<(&'static str, Route)> {
    let mut r = agent_routes();
    r.push(("/debug/prism/cpu-windows", ok(WINDOWS, true)));
    r
}

#[test]
fn cpu_with_timeline_uses_windows_and_returns_a_valid_timeline() {
    let m = mock(windows_routes());
    let (p, rt) = detect_url(&m);
    let out = run_capture(&m, &p, &rt, timeline_plan(vec![ProfileKind::Cpu], true));
    let a = kind_of(&out, ProfileKind::Cpu);
    let t = a.timeline.as_ref().expect("timeline");
    let n = prism_import::import_bytes(&a.bytes, &prism_import::ImportHints::default())
        .unwrap()
        .samples
        .len();
    assert!(
        t.validate(n),
        "{} entries for {n} samples",
        t.sample_bucket.len()
    );
    assert_eq!(t.bucket_ns, 250_000_000);
    assert!(t.bucket_count() >= 5, "{}", t.bucket_count());
    let hits = m.hits.lock().unwrap().clone();
    assert!(
        hits.iter()
            .any(|h| h.starts_with("/debug/prism/cpu-windows?") && h.contains("window_ms=250")),
        "{hits:?}"
    );
    assert!(
        hits.iter().all(|h| !h.starts_with("/debug/pprof/profile")),
        "{hits:?}"
    );
}

#[test]
fn cpu_without_timeline_uses_the_plain_endpoint() {
    let m = mock(windows_routes());
    let (p, rt) = detect_url(&m);
    let out = run_capture(&m, &p, &rt, timeline_plan(vec![ProfileKind::Cpu], false));
    assert!(kind_of(&out, ProfileKind::Cpu).timeline.is_none());
    let hits = m.hits.lock().unwrap().clone();
    assert!(
        hits.iter().any(|h| h.starts_with("/debug/pprof/profile")),
        "{hits:?}"
    );
}

#[test]
fn an_older_agent_without_windows_falls_back_to_plain_cpu() {
    let m = mock(agent_routes());
    let (p, rt) = detect_url(&m);
    let out = run_capture(&m, &p, &rt, timeline_plan(vec![ProfileKind::Cpu], true));
    let a = kind_of(&out, ProfileKind::Cpu);
    assert!(a.timeline.is_none());
    assert!(!a.bytes.is_empty());
}

#[test]
fn go_wall_has_no_timeline() {
    let m = mock(windows_routes());
    let (p, rt) = detect_url(&m);
    let out = run_capture(&m, &p, &rt, timeline_plan(vec![ProfileKind::Wall], true));
    assert!(kind_of(&out, ProfileKind::Wall).timeline.is_none());
}
