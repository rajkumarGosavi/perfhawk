//! Real ETW tests. Windows only. The elevated ones are `#[ignore]`d and need
//! `PERFHAWK_ETW=1` plus an administrator shell:
//!
//!     PERFHAWK_ETW=1 cargo test -p perfhawk-capture --test etw_real -- --ignored --nocapture
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
    assert!(io.warning.is_none(), "block io warning: {:?}", io.warning);
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
