//! Real ETW tests. Windows only. The elevated ones are `#[ignore]`d and need
//! `PERFHAWK_ETW=1` plus an administrator shell:
//!
//!     PERFHAWK_ETW=1 cargo test -p perfhawk-capture --test etw_real -- --ignored --nocapture --test-threads=1
//!
//! `--test-threads=1` is required: on Windows builds that fall back to the classic
//! "NT Kernel Logger", only one kernel trace can run at a time.
#![cfg(windows)]

use perfhawk_capture::etw::{is_elevated, EtwError, RawTrace};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use windows::core::GUID;

fn enabled() -> bool {
    std::env::var("PERFHAWK_ETW").as_deref() == Ok("1")
}

#[test]
fn start_without_elevation_returns_not_elevated() {
    if is_elevated() {
        eprintln!("skipped: running elevated");
        return;
    }
    let err = match RawTrace::start("perfhawk-test-noadmin", 0x1, &[], |_| {}) {
        Ok(_) => panic!("start must fail without elevation"),
        Err(e) => e,
    };
    assert!(matches!(err, EtwError::NotElevated));
    let msg = err.to_string();
    assert!(msg.contains("elevated"), "{msg}");
}

#[test]
#[ignore = "needs an elevated shell and PERFHAWK_ETW=1"]
fn kernel_trace_sees_file_io_and_stack_walks() {
    if !enabled() || !is_elevated() {
        eprintln!("skipped: needs PERFHAWK_ETW=1 and an elevated shell");
        return;
    }
    let file_io = GUID::from_u128(0x90cbdc39_4a3e_11d1_84f4_0000f80464e3);
    let stack_walk = GUID::from_u128(0xdef2fe46_7bd6_4b80_bd94_f57fe20d0ce3);
    let file_events = Arc::new(AtomicU64::new(0));
    let stack_events = Arc::new(AtomicU64::new(0));
    let (f2, s2) = (file_events.clone(), stack_events.clone());
    let trace = RawTrace::start(
        &format!("perfhawk-test-{}", std::process::id()),
        0x0200_0000 | 0x0400_0000 | 0x2, // FILE_IO | FILE_IO_INIT | THREAD
        &[(file_io, 67), (file_io, 68)],
        move |r| {
            if r.provider == file_io {
                f2.fetch_add(1, Ordering::Relaxed);
            } else if r.provider == stack_walk {
                s2.fetch_add(1, Ordering::Relaxed);
            }
        },
    )
    .expect("trace starts when elevated");
    let p = std::env::temp_dir().join("perfhawk-etw-test.bin");
    for _ in 0..20 {
        std::fs::write(&p, vec![1u8; 1 << 20]).unwrap();
        std::fs::File::open(&p).unwrap().sync_all().ok();
    }
    let counters = trace.stop();
    let _ = std::fs::remove_file(&p);
    assert!(counters.qpc_freq > 0);
    assert!(file_events.load(Ordering::Relaxed) > 0, "no FileIo events");
    assert!(
        stack_events.load(Ordering::Relaxed) > 0,
        "no stack-walk events"
    );
}

#[test]
#[ignore = "needs an elevated shell and PERFHAWK_ETW=1"]
fn provider_captures_block_io_off_cpu_and_sched_for_this_process() {
    use perfhawk_capture::etw::EtwProvider;
    use perfhawk_capture::*;
    use perfhawk_model::ProfileKind;
    use std::time::Duration;

    if !enabled() || !is_elevated() {
        eprintln!("skipped: needs PERFHAWK_ETW=1 and an elevated shell");
        return;
    }
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let s2 = stop.clone();
    let worker = std::thread::spawn(move || {
        let p = std::env::temp_dir().join("perfhawk-etw-provider-test.bin");
        while !s2.load(Ordering::Relaxed) {
            std::fs::write(&p, vec![3u8; 1 << 20]).unwrap();
            std::fs::File::open(&p).unwrap().sync_all().ok();
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = std::fs::remove_file(&p);
    });
    let rt = RuntimeInfo {
        provider: "go",
        pid: Some(std::process::id()),
        url: None,
        runtime_version: None,
        command: None,
    };
    let plan = CapturePlan {
        kinds: vec![
            ProfileKind::BlockIo,
            ProfileKind::OffCpu,
            ProfileKind::SchedDelay,
        ],
        duration: Duration::from_secs(3),
        timeline: true,
    };
    let out = EtwProvider::new()
        .capture(&rt, &plan, &CancelToken::new(), &|m| eprintln!("{m}"))
        .expect("capture works when elevated");
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    assert_eq!(out.end, CaptureEnd::Complete);
    assert_eq!(out.artifacts.len(), 3);
    let io = out
        .artifacts
        .iter()
        .find(|a| a.kind == ProfileKind::BlockIo)
        .unwrap();
    // This profiles the test binary itself: system DLL frames have no names
    // yet (PDB support is a follow-up), so an "unresolved frames" note is fine.
    // Any other warning (lost events, event cap) is not.
    if let Some(w) = &io.warning {
        assert!(w.contains("unresolved frames"), "block io warning: {w}");
    }
    assert!(io.timeline.is_some());
    // the profile bytes decode and name our file
    let p = perfhawk_import::import_pprof(&io.bytes, &Default::default()).expect("valid pprof");
    let text = format!("{:?}", p.strings);
    assert!(text.contains("perfhawk-etw-provider-test"), "{text}");
}

#[test]
#[ignore = "needs an elevated shell and PERFHAWK_ETW=1"]
fn dropping_a_trace_stops_the_session() {
    if !enabled() || !is_elevated() {
        eprintln!("skipped: needs PERFHAWK_ETW=1 and an elevated shell");
        return;
    }
    let name = format!("perfhawk-test-drop-{}", std::process::id());
    let first = RawTrace::start(&name, 0x2, &[], |_| {}).expect("first start");
    drop(first); // no stop(): Drop must end the session
    let second = RawTrace::start(&name, 0x2, &[], |_| {})
        .expect("the same session name is free again after the drop");
    second.stop();
}

#[test]
#[ignore = "needs an elevated shell, PERFHAWK_ETW=1 and a built examples/dotnet-demo (Release)"]
fn dotnet_session_reports_method_names_for_a_running_process() {
    use perfhawk_capture::etw::UserTrace;
    use perfhawk_capture::symbolize::{
        decode_method_event, DOTNET_KEYWORDS, DOTNET_RUNDOWN_KEYWORDS, GUID_DOTNET_RUNDOWN,
        GUID_DOTNET_RUNTIME,
    };
    use std::sync::Mutex;

    if !enabled() || !is_elevated() {
        eprintln!("skipped: needs PERFHAWK_ETW=1 and an elevated shell");
        return;
    }
    let dll = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll");
    assert!(
        dll.exists(),
        "build it: dotnet build examples/dotnet-demo -c Release"
    );
    let mut child = std::process::Command::new("dotnet")
        .arg(&dll)
        .args(["all", "8"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("dotnet starts");
    std::thread::sleep(std::time::Duration::from_secs(2)); // let it JIT some methods
    let pid = child.id();
    let names = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = names.clone();
    let trace = UserTrace::start(
        &format!("perfhawk-test-dotnet-{}", std::process::id()),
        &[
            (GUID::from_u128(GUID_DOTNET_RUNTIME), DOTNET_KEYWORDS),
            (
                GUID::from_u128(GUID_DOTNET_RUNDOWN),
                DOTNET_RUNDOWN_KEYWORDS,
            ),
        ],
        pid,
        move |r| {
            if r.pid == pid {
                if let Some(m) = decode_method_event(r.id, r.data) {
                    sink.lock().unwrap().push(m.name);
                }
            }
        },
    )
    .expect("user trace starts when elevated");
    std::thread::sleep(std::time::Duration::from_secs(4));
    trace.stop();
    let _ = child.kill();
    let _ = child.wait();
    let names = names.lock().unwrap();
    assert!(
        names.iter().any(|n| n.starts_with("DotnetDemo.")),
        "rundown did not report the demo's methods; got {} names, e.g. {:?}",
        names.len(),
        names.iter().take(5).collect::<Vec<_>>()
    );
}

/// Diagnostic, not an assertion: prints which event ids the .NET runtime and
/// rundown providers deliver for a running process, so the decoder can match
/// the real ids. Run with --nocapture from an elevated shell.
#[test]
#[ignore = "needs an elevated shell, PERFHAWK_ETW=1 and a built examples/dotnet-demo (Release)"]
fn dotnet_event_id_histogram() {
    use perfhawk_capture::etw::UserTrace;
    use perfhawk_capture::symbolize::{GUID_DOTNET_RUNDOWN, GUID_DOTNET_RUNTIME};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    if !enabled() || !is_elevated() {
        eprintln!("skipped: needs PERFHAWK_ETW=1 and an elevated shell");
        return;
    }
    let dll = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll");
    let mut child = std::process::Command::new("dotnet")
        .arg(&dll)
        .args(["all", "10"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("dotnet starts");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let pid = child.id();
    // (provider, id, opcode, version) -> (count, payload length range, first name-like string)
    type Key = (&'static str, u16, u8, u8);
    let seen = Arc::new(Mutex::new(
        BTreeMap::<Key, (u64, usize, usize, String)>::new(),
    ));
    let sink = seen.clone();
    // Rundown keywords: Loader | JIT | NGen | StartRundown (0x40) | EndRundown (0x100).
    let runtime_kw: u64 = 0x8 | 0x10 | 0x20;
    let rundown_kw: u64 = 0x8 | 0x10 | 0x20 | 0x40 | 0x100;
    let trace = UserTrace::start(
        &format!("perfhawk-test-hist-{}", std::process::id()),
        &[
            (GUID::from_u128(GUID_DOTNET_RUNTIME), runtime_kw),
            (GUID::from_u128(GUID_DOTNET_RUNDOWN), rundown_kw),
        ],
        pid,
        move |r| {
            if r.pid != pid {
                return;
            }
            let prov = if r.provider == GUID::from_u128(GUID_DOTNET_RUNTIME) {
                "runtime"
            } else if r.provider == GUID::from_u128(GUID_DOTNET_RUNDOWN) {
                "rundown"
            } else {
                return;
            };
            // First UTF-16 run of printable characters after byte 36, as a hint.
            let hint: String = r
                .data
                .get(36..)
                .map(|d| {
                    d.as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .take_while(|&u| u != 0)
                        .filter_map(|u| char::from_u32(u as u32))
                        .collect()
                })
                .unwrap_or_default();
            let mut m = sink.lock().unwrap();
            let e = m.entry((prov, r.id, r.opcode, r.version)).or_insert((
                0,
                usize::MAX,
                0,
                hint.clone(),
            ));
            e.0 += 1;
            e.1 = e.1.min(r.data.len());
            e.2 = e.2.max(r.data.len());
        },
    )
    .expect("user trace starts when elevated");
    std::thread::sleep(std::time::Duration::from_secs(6));
    trace.stop();
    let _ = child.kill();
    let _ = child.wait();
    eprintln!("provider id opcode version | count | payload bytes min..max | string at byte 36");
    for ((p, id, op, v), (n, lo, hi, hint)) in seen.lock().unwrap().iter() {
        eprintln!("{p:<8} {id:>4} {op:>3} v{v} | {n:>6} | {lo}..{hi} | {hint:?}");
    }
}
