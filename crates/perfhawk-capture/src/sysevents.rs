//! OS trace events in a platform-neutral form, and the pure step that turns
//! them into Block I/O, off-CPU and scheduler-delay rows. The Windows ETW
//! session (`etw`) only has to produce [`RawEvent`]s; everything here is
//! testable without admin rights and reusable by a Linux provider.

use crate::events::EventRow;
use perfhawk_model::file_leaf;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    FileRead,
    FileWrite,
    DiskRead,
    DiskWrite,
    /// `pid`/`tid`/`stack` describe the thread being switched out (`old_tid`);
    /// `new_tid` is the thread switched in.
    CSwitch,
    /// `pid`/`tid`/`stack` describe the thread that made another thread
    /// runnable; `new_tid` is the thread that became runnable.
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

struct Open {
    start_qpc: i64,
    stack: Vec<String>,
}

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

    // Threads known to belong to the target: a CSwitch switch-in or a
    // ReadyThread names a thread without saying which process it is in.
    let target_tids: HashSet<u32> = events
        .iter()
        .filter(|e| e.pid == input.target_pid)
        .map(|e| e.tid)
        .collect();

    // File events by thread, to tell whether a disk event is already counted.
    let mut file_spans: HashMap<u32, Vec<(i64, i64)>> = HashMap::new();
    for e in &events {
        if e.pid == input.target_pid && matches!(e.kind, RawKind::FileRead | RawKind::FileWrite) {
            file_spans
                .entry(e.tid)
                .or_default()
                .push((e.qpc - e.duration_qpc, e.qpc));
        }
    }

    let mut out = Aggregated::default();
    let mut off: HashMap<u32, Open> = HashMap::new();
    let mut ready: HashMap<u32, Open> = HashMap::new();

    let span_row = |open: Open, end_qpc: i64, dest: &mut Vec<EventRow>| {
        let ns = c.qpc_to_ns(end_qpc - open.start_qpc);
        if ns >= MIN_SPAN_NS {
            dest.push(EventRow {
                stack: open.stack,
                values: vec![ns as i64],
                bucket: bucket_of(open.start_qpc),
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
                let covered = file_spans
                    .get(&e.tid)
                    .is_some_and(|v| v.iter().any(|(s, f)| e.qpc >= *s && e.qpc <= *f));
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
                if e.pid == input.target_pid {
                    off.insert(
                        e.old_tid,
                        Open {
                            start_qpc: e.qpc,
                            stack: (input.symbolize)(e),
                        },
                    );
                }
                if target_tids.contains(&e.new_tid) {
                    if let Some(o) = off.remove(&e.new_tid) {
                        span_row(o, e.qpc, &mut out.off_cpu);
                    }
                    if let Some(r) = ready.remove(&e.new_tid) {
                        span_row(r, e.qpc, &mut out.sched_delay);
                    }
                }
            }
            RawKind::ReadyThread if target_tids.contains(&e.new_tid) => {
                ready.insert(
                    e.new_tid,
                    Open {
                        start_qpc: e.qpc,
                        stack: (input.symbolize)(e),
                    },
                );
            }
            _ => {}
        }
    }
    // Threads still off-CPU when the window closed.
    for (_, o) in off {
        span_row(o, window_end, &mut out.off_cpu);
    }
    out
}
