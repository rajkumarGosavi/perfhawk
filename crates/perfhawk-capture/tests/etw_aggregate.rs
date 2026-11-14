use perfhawk_capture::sysevents::*;
use perfhawk_model::file_leaf;

const TARGET: u32 = 100;
const FREQ: i64 = 10_000_000; // 10 MHz QPC: 1 tick = 100 ns

fn clock() -> QpcClock {
    QpcClock {
        freq: FREQ,
        qpc_start: 1_000_000,
        unix_start_ns: 5_000_000_000,
    }
}

/// QPC ticks for `ms` milliseconds after the window start.
fn at_ms(ms: i64) -> i64 {
    clock().qpc_start + ms * FREQ / 1000
}

fn ticks_ms(ms: i64) -> i64 {
    ms * FREQ / 1000
}

fn names(ev: &RawEvent) -> Vec<String> {
    // stacks arrive leaf-first from ETW; the closure returns root-first names
    ev.stack.iter().rev().map(|a| format!("fn{a}")).collect()
}

fn run(events: &[RawEvent]) -> Aggregated {
    aggregate(&AggregateInput {
        events,
        target_pid: TARGET,
        clock: clock(),
        window_ns: 10_000_000_000,
        bucket_ns: 1_000_000_000,
        symbolize: &names,
    })
}

fn file_write(pid: u32, tid: u32, ms: i64, dur_ms: i64, bytes: u64, path: &str) -> RawEvent {
    let mut e = RawEvent::new(RawKind::FileWrite, at_ms(ms), pid, tid);
    e.duration_qpc = ticks_ms(dur_ms);
    e.bytes = bytes;
    e.file = Some(path.to_string());
    e.stack = vec![2, 1]; // leaf-first: fn1 is the root
    e
}

fn switch_out(tid: u32, ms: i64, stack: Vec<u64>) -> RawEvent {
    let mut e = RawEvent::new(RawKind::CSwitch, at_ms(ms), TARGET, tid);
    e.old_tid = tid;
    e.new_tid = 999;
    e.stack = stack;
    e
}

fn switch_in(tid: u32, ms: i64) -> RawEvent {
    let mut e = RawEvent::new(RawKind::CSwitch, at_ms(ms), 4, 999);
    e.old_tid = 999;
    e.new_tid = tid;
    e
}

#[test]
fn qpc_converts_to_unix_ns() {
    let c = clock();
    assert_eq!(
        c.to_unix_ns(c.qpc_start + FREQ),
        5_000_000_000 + 1_000_000_000
    );
    assert_eq!(c.qpc_to_ns(FREQ), 1_000_000_000);
}

#[test]
fn file_write_becomes_block_io_row_with_file_leaf() {
    let a = run(&[file_write(TARGET, 7, 100, 2, 4096, "C:\\data\\x.db")]);
    assert_eq!(a.block_io.len(), 1);
    let r = &a.block_io[0];
    assert_eq!(r.stack, vec!["fn1", "fn2", &file_leaf("C:\\data\\x.db")]);
    assert_eq!(r.values, vec![2_000_000, 4096]);
    assert_eq!(r.bucket, 0);
}

#[test]
fn off_cpu_span_runs_from_switch_out_to_switch_in() {
    let a = run(&[switch_out(7, 100, vec![3, 2, 1]), switch_in(7, 101)]);
    // 1 ms span; the switch-in alone is not a row
    assert_eq!(a.off_cpu.len(), 1);
    assert_eq!(a.off_cpu[0].values, vec![1_000_000]);
    assert_eq!(a.off_cpu[0].stack, vec!["fn1", "fn2", "fn3"]);
    assert!(a.block_io.is_empty() && a.sched_delay.is_empty());
}

#[test]
fn open_off_cpu_span_is_closed_at_window_end() {
    let a = run(&[switch_out(7, 9_000, vec![1])]);
    assert_eq!(a.off_cpu.len(), 1);
    assert_eq!(a.off_cpu[0].values, vec![1_000_000_000]); // 9 s .. 10 s
}

#[test]
fn short_spans_are_dropped() {
    // 0.5 us apart: 5 ticks is 500 ns at 10 MHz
    let mut out = switch_out(7, 100, vec![1]);
    let mut back = switch_in(7, 100);
    out.qpc = at_ms(100);
    back.qpc = at_ms(100) + 5;
    assert!(run(&[out, back]).off_cpu.is_empty());
}

#[test]
fn ready_thread_to_switch_in_is_sched_delay() {
    // thread 7 first goes off-CPU (so it is known to belong to the target)
    let mut ready = RawEvent::new(RawKind::ReadyThread, at_ms(200), 55, 56);
    ready.new_tid = 7;
    ready.stack = vec![9];
    let a = run(&[switch_out(7, 100, vec![1]), ready, switch_in(7, 203)]);
    assert_eq!(a.sched_delay.len(), 1);
    assert_eq!(a.sched_delay[0].values, vec![3_000_000]);
    assert_eq!(a.sched_delay[0].stack, vec!["fn9"]);
}

#[test]
fn events_of_other_pids_are_ignored() {
    let a = run(&[file_write(77, 8, 100, 2, 4096, "C:\\other")]);
    assert!(a.block_io.is_empty());
}

#[test]
fn events_outside_window_are_dropped() {
    let before = file_write(TARGET, 7, -1, 1, 1, "C:\\a");
    let after = file_write(TARGET, 7, 10_001, 1, 1, "C:\\a");
    assert!(run(&[before, after]).block_io.is_empty());
}

#[test]
fn bucket_clamps_to_last_bucket() {
    // exactly at the window end: bucket 10 would be out of range for 10 buckets
    let a = run(&[file_write(TARGET, 7, 10_000, 1, 1, "C:\\a")]);
    assert_eq!(a.block_io[0].bucket, 9);
}

#[test]
fn bucket_follows_event_time() {
    let a = run(&[file_write(TARGET, 7, 2_500, 1, 1, "C:\\a")]);
    assert_eq!(a.block_io[0].bucket, 2);
}

#[test]
fn disk_event_without_file_gets_disk_leaf() {
    let mut d = RawEvent::new(RawKind::DiskRead, at_ms(100), TARGET, 7);
    d.duration_qpc = ticks_ms(4);
    d.bytes = 8192;
    d.stack = vec![1];
    let a = run(&[d]);
    assert_eq!(a.block_io.len(), 1);
    assert_eq!(a.block_io[0].stack.last().unwrap(), &file_leaf("[disk]"));
    assert_eq!(a.block_io[0].values, vec![4_000_000, 8192]);
}

#[test]
fn disk_event_covered_by_a_file_event_is_not_counted_twice() {
    let w = file_write(TARGET, 7, 100, 5, 4096, "C:\\a");
    let mut d = RawEvent::new(RawKind::DiskWrite, at_ms(98), TARGET, 7);
    d.duration_qpc = ticks_ms(1);
    d.bytes = 4096;
    let a = run(&[w, d]);
    assert_eq!(a.block_io.len(), 1);
}

// ---- Task 3: lifecycle, loss and artifacts ----

use perfhawk_capture::EventRow;
use perfhawk_model::ProfileKind;

fn window() -> WindowSpec {
    WindowSpec {
        start_unix_ns: 5_000_000_000,
        duration_ns: 10_000_000_000,
        bucket_ns: 1_000_000_000,
    }
}

fn no_loss() -> LossStats {
    LossStats {
        events_lost: 0,
        buffers_lost: 0,
    }
}

fn agg_with_io() -> Aggregated {
    Aggregated {
        block_io: vec![EventRow {
            stack: vec!["main".into(), file_leaf("C:/a")],
            values: vec![1_000, 10],
            bucket: 0,
        }],
        ..Aggregated::default()
    }
}

#[test]
fn process_end_is_reported() {
    let id = TargetIdentity {
        pid: TARGET,
        start_qpc: None,
    };
    let end = RawEvent::new(RawKind::ProcessEnd, at_ms(300), TARGET, 1);
    let other = RawEvent::new(RawKind::ProcessEnd, at_ms(100), 77, 1);
    assert_eq!(target_exited(&[other, end], &id), Some(at_ms(300)));
    assert_eq!(target_exited(&[], &id), None);
}

#[test]
fn pid_reuse_events_before_start_are_ignored() {
    let id = TargetIdentity {
        pid: TARGET,
        start_qpc: Some(at_ms(500)),
    };
    let old = RawEvent::new(RawKind::FileWrite, at_ms(100), TARGET, 1);
    let new = RawEvent::new(RawKind::FileWrite, at_ms(600), TARGET, 1);
    let wrong = RawEvent::new(RawKind::FileWrite, at_ms(600), 77, 1);
    assert!(!same_process(&old, &id));
    assert!(same_process(&new, &id));
    assert!(!same_process(&wrong, &id));
}

#[test]
fn loss_warning_none_when_zero() {
    assert_eq!(loss_warning(&no_loss()), None);
}

#[test]
fn loss_warning_mentions_counts() {
    let w = loss_warning(&LossStats {
        events_lost: 12,
        buffers_lost: 3,
    })
    .unwrap();
    assert!(w.starts_with("ETW dropped"), "{w}");
    assert!(w.contains("12") && w.contains('3'), "{w}");
}

#[test]
fn build_artifacts_only_requested_kinds() {
    let a = build_artifacts(
        &agg_with_io(),
        &window(),
        &no_loss(),
        &[ProfileKind::BlockIo],
    );
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].kind, ProfileKind::BlockIo);
    assert!(a[0].timeline.is_some());
    assert_eq!(a[0].warning, None);
}

#[test]
fn empty_kind_still_yields_artifact_with_warning() {
    let a = build_artifacts(
        &agg_with_io(),
        &window(),
        &no_loss(),
        &[ProfileKind::BlockIo, ProfileKind::OffCpu],
    );
    let off = a.iter().find(|x| x.kind == ProfileKind::OffCpu).unwrap();
    assert_eq!(
        off.warning.as_deref(),
        Some("no off_cpu events were seen for the target")
    );
}

#[test]
fn loss_warning_attached_to_every_artifact() {
    let loss = LossStats {
        events_lost: 5,
        buffers_lost: 1,
    };
    let a = build_artifacts(
        &agg_with_io(),
        &window(),
        &loss,
        &[ProfileKind::BlockIo, ProfileKind::SchedDelay],
    );
    assert_eq!(a.len(), 2);
    for x in &a {
        assert!(
            x.warning.as_deref().unwrap().contains("ETW dropped"),
            "{:?}",
            x.warning
        );
    }
}

#[test]
fn unresolved_frames_warning_has_count() {
    let mut agg = agg_with_io();
    agg.unresolved_frames = 42;
    let a = build_artifacts(&agg, &window(), &no_loss(), &[ProfileKind::BlockIo]);
    let w = a[0].warning.as_deref().unwrap();
    assert!(w.contains("42") && w.contains("unresolved"), "{w}");
}
