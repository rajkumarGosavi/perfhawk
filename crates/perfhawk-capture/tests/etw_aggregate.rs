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
