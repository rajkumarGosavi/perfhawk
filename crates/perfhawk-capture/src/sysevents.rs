//! OS trac events in a platform-neutral form, and the pure step that turns
//! them into Block I/O, off-CPU and scheduler-delay rows. The Windows ETW
//! session (`etw`) only has to produce [`RawEvent`]s; everything here is
//! testable without admin rights and reusable by a Linux provider.

use crate::events::{event_artifact, EventRow, EventSpec};
use crate::types::Artifact;
use perfhawk_model::file_leaf;
use perfhawk_model::ProfileKind;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    FileRead,
    FileWrite,
    DiskRead,
    DiskWrite,
    /// A context switch. `pid`/`tid` and `stack` describe the thread being
    /// switched IN (`new_tid`): Windows records the stack it blocked on. `old_tid`
    /// is the thread switched out. `pid` is 0 when the thread's process is unknown.
    CSwitch,
    /// A thread became runnable: `tid`/`new_tid` is that thread, `pid` its
    /// process. No stack (the waker's stack is not used).
    ReadyThread,
    ImageLoad,
    ProcessStart,
    ProcessEnd,
    MethodLoad,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageInfo {
    pub base: u64,
    pub size: u64,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MethodInfo {
    pub start: u64,
    pub size: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawEvent {
    pub kind: RawKind,
    /// Raw QPC timestamp.
    pub qpc: i64,
    pub pid: u32,
    pub tid: u32,
    /// Return addresses, leaf first (the order ETW delivers them in).
    pub stack: Vec<u64>,
    pub bytes: u64,
    /// Length of the operation in QPC ticks (file and disk events).
    pub duration_qpc: i64,
    pub file: Option<String>,
    pub new_tid: u32,
    pub old_tid: u32,
    pub wait_reason: u8,
    pub image: Option<ImageInfo>,
    pub method: Option<MethodInfo>,
}

impl RawEvent {
    pub fn new(kind: RawKind, qpc: i64, pid: u32, tid: u32) -> Self {
        RawEvent {
            kind,
            qpc,
            pid,
            tid,
            stack: vec![],
            bytes: 0,
            duration_qpc: 0,
            file: None,
            new_tid: 0,
            old_tid: 0,
            wait_reason: 0,
            image: None,
            method: None,
        }
    }
}

/// Converts QPC ticks to wall-clock time, anchored at capture start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QpcClock {
    pub freq: i64,
    pub qpc_start: i64,
    pub unix_start_ns: i64,
}

impl QpcClock {
    pub fn qpc_to_ns(&self, delta: i64) -> u64 {
        if self.freq <= 0 || delta <= 0 {
            return 0;
        }
        (delta as i128 * 1_000_000_000 / self.freq as i128) as u64
    }

    pub fn to_unix_ns(&self, qpc: i64) -> i64 {
        let d = (qpc - self.qpc_start) as i128 * 1_000_000_000 / self.freq.max(1) as i128;
        self.unix_start_ns + d as i64
    }

    fn ns_to_qpc(&self, ns: u64) -> i64 {
        (ns as i128 * self.freq as i128 / 1_000_000_000) as i64
    }
}

pub struct AggregateInput<'a> {
    pub events: &'a [RawEvent],
    pub target_pid: u32,
    pub clock: QpcClock,
    pub window_ns: u64,
    pub bucket_ns: u64,
    /// Root-first frame names for the event's stack.
    pub symbolize: &'a dyn Fn(&RawEvent) -> Vec<String>,
}

#[derive(Default)]
pub struct Aggregated {
    pub block_io: Vec<EventRow>,
    pub off_cpu: Vec<EventRow>,
    pub sched_delay: Vec<EventRow>,
    /// Filled in by the provider from the symbolizer; `aggregate` leaves it 0.
    pub unresolved_frames: u64,
}

/// Off-CPU or scheduler spans shorter than this are noise.
const MIN_SPAN_NS: u64 = 1_000;

pub fn aggregate(input: &AggregateInput) -> Aggregated {
    let c = &input.clock;
    let window_end = c.qpc_start + c.ns_to_qpc(input.window_ns);
    let bucket_ns = input.bucket_ns.max(1);
    let total_buckets = input.window_ns.div_ceil(bucket_ns).max(1);
    let bucket_of = |qpc: i64| -> u32 {
        let ns = c.qpc_to_ns(qpc - c.qpc_start);
        (ns / bucket_ns).min(total_buckets - 1) as u32
    };
    let in_window = |qpc: i64| qpc >= c.qpc_start && qpc <= window_end;

    let mut events: Vec<&RawEvent> = input.events.iter().collect();
    events.sort_by_key(|e| e.qpc);

    // File events by thread, to tell whether a disk event is already counted:
    // (start, latest end among spans starting at or before it). Events are
    // sorted by time, so starts are non-decreasing and coverage is a binary search.
    let mut file_spans: HashMap<u32, Vec<(i64, i64)>> = HashMap::new();
    for e in &events {
        if e.pid == input.target_pid && matches!(e.kind, RawKind::FileRead | RawKind::FileWrite) {
            let v = file_spans.entry(e.tid).or_default();
            let end = e.qpc + e.duration_qpc;
            let max_end = v.last().map_or(end, |&(_, m)| m.max(end));
            v.push((e.qpc, max_end));
        }
    }

    let mut out = Aggregated::default();
    // When each thread last left the CPU / was made runnable.
    let mut last_out: HashMap<u32, (i64, u8)> = HashMap::new();
    let mut ready: HashMap<u32, i64> = HashMap::new();

    let span_row = |start_qpc: i64, end_qpc: i64, stack: Vec<String>, dest: &mut Vec<EventRow>| {
        let ns = c.qpc_to_ns(end_qpc - start_qpc);
        if ns >= MIN_SPAN_NS {
            dest.push(EventRow {
                stack,
                values: vec![ns as i64],
                bucket: bucket_of(start_qpc),
            });
        }
    };

    for e in events {
        if !in_window(e.qpc) {
            continue;
        }
        match e.kind {
            RawKind::FileRead | RawKind::FileWrite if e.pid == input.target_pid => {
                let mut stack = (input.symbolize)(e);
                stack.push(file_leaf(e.file.as_deref().unwrap_or("[unknown]")));
                out.block_io.push(EventRow {
                    stack,
                    values: vec![c.qpc_to_ns(e.duration_qpc) as i64, e.bytes as i64],
                    bucket: bucket_of(e.qpc),
                });
            }
            RawKind::DiskRead | RawKind::DiskWrite if e.pid == input.target_pid => {
                let covered = file_spans.get(&e.tid).is_some_and(|v| {
                    let i = v.partition_point(|&(start, _)| start <= e.qpc);
                    i > 0 && v[i - 1].1 >= e.qpc
                });
                if !covered {
                    let mut stack = (input.symbolize)(e);
                    stack.push(file_leaf("[disk]"));
                    out.block_io.push(EventRow {
                        stack,
                        values: vec![c.qpc_to_ns(e.duration_qpc) as i64, e.bytes as i64],
                        bucket: bucket_of(e.qpc),
                    });
                }
            }
            RawKind::CSwitch => {
                // Remember when the outgoing thread left the CPU (any process:
                // its pid is not on the event).
                last_out.insert(e.old_tid, (e.qpc, e.wait_reason));
                if e.pid == input.target_pid {
                    // The stack is where the incoming thread had been blocked.
                    let stack = (input.symbolize)(e);
                    // A thread first seen switching in has been waiting at least
                    // since the window opened.
                    let (left, reason) = match last_out.remove(&e.new_tid) {
                        Some((t, r)) => (t.max(c.qpc_start), wait_reason_name(r)),
                        None => (c.qpc_start, "unknown".to_string()),
                    };
                    let mut wait_stack = stack.clone();
                    wait_stack.push(format!("[wait: {reason}]"));
                    span_row(left, e.qpc, wait_stack, &mut out.off_cpu);
                    if let Some(r) = ready.remove(&e.new_tid) {
                        let mut s = stack;
                        if s.is_empty() {
                            s.push("[scheduler queue]".to_string());
                        }
                        span_row(r, e.qpc, s, &mut out.sched_delay);
                    }
                }
            }
            RawKind::ReadyThread if e.pid == input.target_pid => {
                ready.insert(e.new_tid, e.qpc);
            }
            _ => {}
        }
    }
    out
}

/// Which process a capture is about. `start_qpc` guards against pid reuse.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetIdentity {
    pub pid: u32,
    pub start_qpc: Option<i64>,
}

/// QPC time of the target's `ProcessEnd`, if the trace saw it.
pub fn target_exited(events: &[RawEvent], id: &TargetIdentity) -> Option<i64> {
    events
        .iter()
        .find(|e| e.kind == RawKind::ProcessEnd && e.pid == id.pid)
        .map(|e| e.qpc)
}

/// True when `ev` belongs to the target process and not to an earlier process
/// that had the same pid.
pub fn same_process(ev: &RawEvent, id: &TargetIdentity) -> bool {
    ev.pid == id.pid && id.start_qpc.is_none_or(|s| ev.qpc >= s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LossStats {
    pub events_lost: u64,
    pub buffers_lost: u64,
}

pub fn loss_warning(l: &LossStats) -> Option<String> {
    if l.events_lost == 0 && l.buffers_lost == 0 {
        return None;
    }
    Some(format!(
        "ETW dropped {} events and {} buffers under load; the numbers undercount",
        l.events_lost, l.buffers_lost
    ))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowSpec {
    pub start_unix_ns: i64,
    pub duration_ns: u64,
    pub bucket_ns: u64,
}

/// One artifact per requested kind. A kind with no rows still yields an empty
/// artifact so the user sees "nothing happened" rather than a missing profile.
pub fn build_artifacts(
    agg: &Aggregated,
    spec: &WindowSpec,
    loss: &LossStats,
    kinds: &[ProfileKind],
) -> Vec<Artifact> {
    use perfhawk_model::{sample_types as st, Fidelity, Unit};
    let mut out = Vec::new();
    for &kind in kinds {
        let (rows, types): (&[EventRow], Vec<(&str, Unit)>) = match kind {
            ProfileKind::BlockIo => (
                &agg.block_io,
                vec![
                    (st::IO_TIME, Unit::Nanoseconds),
                    (st::IO_BYTES, Unit::Bytes),
                ],
            ),
            ProfileKind::OffCpu => (&agg.off_cpu, vec![(st::OFFCPU, Unit::Nanoseconds)]),
            ProfileKind::SchedDelay => {
                (&agg.sched_delay, vec![(st::SCHED_DELAY, Unit::Nanoseconds)])
            }
            _ => continue,
        };
        let mut a = event_artifact(
            &EventSpec {
                kind,
                fidelity: Fidelity::Native,
                sample_types: &types,
                start_unix_ns: spec.start_unix_ns,
                duration_ns: spec.duration_ns,
                bucket_ns: spec.bucket_ns,
            },
            rows,
        );
        let mut warnings = Vec::new();
        if rows.is_empty() {
            warnings.push(format!("no {kind} events were seen for the target"));
        }
        if let Some(w) = loss_warning(loss) {
            warnings.push(w);
        }
        if agg.unresolved_frames > 0 {
            warnings.push(format!(
                "{} unresolved frames were shown as module+offset or [unknown]",
                agg.unresolved_frames
            ));
        }
        if !warnings.is_empty() {
            a.warning = Some(warnings.join("; "));
        }
        out.push(a);
    }
    out
}

/// Windows `KWAIT_REASON` names, for the frame that ends an off-CPU stack. The
/// kernel often cannot walk user frames of a thread that is resuming, so this
/// frame also keeps those rows from becoming empty stacks.
fn wait_reason_name(code: u8) -> String {
    const NAMES: [&str; 39] = [
        "Executive",
        "FreePage",
        "PageIn",
        "PoolAllocation",
        "DelayExecution",
        "Suspended",
        "UserRequest",
        "WrExecutive",
        "WrFreePage",
        "WrPageIn",
        "WrPoolAllocation",
        "WrDelayExecution",
        "WrSuspended",
        "WrUserRequest",
        "WrEventPair",
        "WrQueue",
        "WrLpcReceive",
        "WrLpcReply",
        "WrVirtualMemory",
        "WrPageOut",
        "WrRendezvous",
        "WrKeyedEvent",
        "WrTerminated",
        "WrProcessInSwap",
        "WrCpuRateControl",
        "WrCalloutStack",
        "WrKernel",
        "WrResource",
        "WrPushLock",
        "WrMutex",
        "WrQuantumEnd",
        "WrDispatchInt",
        "WrPreempted",
        "WrYieldExecution",
        "WrFastMutex",
        "WrGuardedMutex",
        "WrRundown",
        "WrAlertByThreadId",
        "WrDeferredPreempt",
    ];
    match NAMES.get(code as usize) {
        Some(n) => (*n).to_string(),
        None => format!("reason {code}"),
    }
}
