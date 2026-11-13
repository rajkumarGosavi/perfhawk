//! Doctor: a deterministic rule engine over one session's profiles. It says
//! whether the target was CPU-, I/O-, lock- or scheduler-bound (or idle), how
//! sure it is, and what to run next. All shares are thread-seconds over the
//! capture window, with idle runtime threads left out of "busy".

use crate::filter::{for_each_sample, Filter};
use crate::findings::{GC_PATTERN, IDLE_NAMES};
use prism_model::{Fidelity, Profile, ProfileKind, Unit};
use regex::Regex;
use serde::Serialize;

/// Wall-clock stacks that end in file or disk I/O, used when no Block I/O
/// profile exists. An estimate, never better than low confidence.
const IO_PATTERN: &str = r"^(syscall\.(Read|Write|Pread|Pwrite|FlushFileBuffers|Fsync)|internal/poll\.\(\*FD\)\.(Read|Write|Fsync)|os\.\(\*File\)\.(Read|Write|Sync)|System\.IO\.)";

/// A goroutine parked in `runtime.gopark` is idle unless one of its frames says it is
/// waiting on a mutex, which costs the program time. Sleeping and `WaitGroup.Wait` are
/// idle: they are not evidence of a bottleneck.
const WAIT_PATTERN: &str =
    r"^sync\.(\(\*(Mutex|RWMutex)\)\.|runtime_Semacquire(RW)?Mutex)";

/// .NET threads waiting for work (thread-pool workers, the finalizer, event waits). The
/// .NET helper samples every managed thread every tick, so these must not count as busy.
const DOTNET_IDLE_PATTERN: &str = r"(^|\.)(LowLevelLifoSemaphore\.(WaitForSignal|WaitNative)|WaitHandle\.WaitOne\w*|Monitor\.Wait|SemaphoreSlim\.Wait|ManualResetEventSlim\.Wait)$";

/// Leaf frames of a thread that is blocked, not running or waiting for a CPU. A busy
/// (non-idle) wall sample with any other leaf is running or runnable.
const BLOCKING_LEAF_PATTERN: &str = r"^(runtime\.(gopark|goparkunlock|notetsleepg|cgocall|usleep|futex\w*|netpoll\w*|semasleep|selectgo|chanrecv\w*|chansend\w*|timeSleep)|syscall\.|internal/poll\.|os\.|net\.|sync\.|time\.Sleep$|System\.Threading\.|System\.IO\.|System\.Net\.|Interop\.)";

pub struct DoctorProfile<'a> {
    pub kind: ProfileKind,
    pub short_id: &'a str,
    pub profile: &'a Profile,
    /// The stored profile's fidelity (an import override is not in `Profile::meta`).
    pub fidelity: Fidelity,
}

pub struct DoctorInput<'a> {
    pub window_ns: u64,
    pub profiles: Vec<DoctorProfile<'a>>,
    /// How to point `prism record` at the target, for example `--pid 4242`.
    pub record_target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    CpuBound,
    IoBound,
    LockBound,
    SchedulerStarved,
    WaitingUnknown,
    Idle,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowUp {
    pub kinds: Vec<ProfileKind>,
    pub duration_secs: u64,
    pub needs_privilege: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStep {
    pub label: String,
    pub command: Option<String>,
    /// Short id of a profile of this session that shows the evidence.
    pub open_profile: Option<String>,
    pub follow_up: Option<FollowUp>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorFinding {
    pub rule: &'static str,
    pub confidence: Confidence,
    /// The weakest fidelity of the evidence behind this finding.
    pub fidelity: Fidelity,
    pub share_pct: f64,
    pub title: String,
    pub detail: String,
    pub next: Vec<NextStep>,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    pub verdict: Verdict,
    pub summary: String,
    /// Average number of busy (non-idle) threads over the window.
    pub busy_threads: f64,
    pub findings: Vec<DoctorFinding>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DoctorConfig {
    pub idle_busy_threads: f64,
    pub cpu_bound_pct: f64,
    pub wait_pct: f64,
    pub lock_pct: f64,
    pub io_pct: f64,
    pub sched_pct: f64,
    pub gc_pct: f64,
}

impl Default for DoctorConfig {
    fn default() -> Self {
        DoctorConfig {
            idle_busy_threads: 0.05,
            cpu_bound_pct: 60.0,
            wait_pct: 40.0,
            lock_pct: 25.0,
            io_pct: 25.0,
            sched_pct: 10.0,
            gc_pct: 25.0,
        }
    }
}

const FOLLOW_UP_SECS: u64 = 30;

fn pct(v: i64, total: i64) -> f64 {
    if total <= 0 || v <= 0 {
        0.0
    } else {
        (v as f64 / total as f64 * 100.0).min(100.0)
    }
}

/// Total of the profile's default sample type when it is a time in ns.
fn ns_total(p: &Profile) -> Option<i64> {
    let st = p.default_sample_type;
    (p.sample_types.get(st)?.unit == Unit::Nanoseconds).then(|| p.total(st))
}

fn fidelity_rank(f: &Fidelity) -> u8 {
    match f {
        Fidelity::Native => 4,
        Fidelity::SampledExternal => 3,
        Fidelity::Derived => 2,
        Fidelity::Approximate => 1,
        Fidelity::Unknown => 0,
    }
}

fn weakest(fs: &[&Fidelity]) -> Fidelity {
    fs.iter()
        .min_by_key(|f| fidelity_rank(f))
        .map_or(Fidelity::Unknown, |f| (*f).clone())
}

fn confidence(share: f64, threshold: f64, fidelity: &Fidelity) -> Confidence {
    match fidelity {
        Fidelity::Approximate | Fidelity::Unknown => Confidence::Low,
        Fidelity::Native | Fidelity::SampledExternal if share >= threshold * 1.5 => Confidence::High,
        _ => Confidence::Medium,
    }
}

struct Ctx<'a> {
    input: &'a DoctorInput<'a>,
}

impl<'a> Ctx<'a> {
    fn find(&self, kind: ProfileKind) -> Option<&'a DoctorProfile<'a>> {
        self.input.profiles.iter().find(|p| p.kind == kind)
    }

    fn show(&self, id: &str, suffix: &str, label: &str) -> NextStep {
        NextStep {
            label: label.into(),
            command: Some(format!("prism show {id} --top 20{suffix}")),
            open_profile: Some(id.to_string()),
            follow_up: None,
        }
    }

    fn capture_io(&self) -> NextStep {
        let label = "Capture Block I/O and off-CPU time (needs administrator or root)";
        match &self.input.record_target {
            Some(t) => NextStep {
                label: label.into(),
                command: Some(format!(
                    "prism record {t} --kinds block_io,off_cpu --duration {FOLLOW_UP_SECS}s"
                )),
                open_profile: None,
                follow_up: Some(FollowUp {
                    kinds: vec![ProfileKind::BlockIo, ProfileKind::OffCpu],
                    duration_secs: FOLLOW_UP_SECS,
                    needs_privilege: true,
                }),
            },
            None => NextStep {
                label: "Run `prism doctor` against a live target (--pid or --url) to capture Block I/O and off-CPU time".into(),
                command: None,
                open_profile: None,
                follow_up: None,
            },
        }
    }
}

pub fn diagnose(input: &DoctorInput, cfg: &DoctorConfig) -> Diagnosis {
    let ctx = Ctx { input };
    let mut notes: Vec<String> = Vec::new();

    // Time totals; a profile whose default value is not a time is skipped.
    let mut total_of = |kind: ProfileKind| -> Option<(i64, &DoctorProfile)> {
        let dp = ctx.find(kind)?;
        match ns_total(dp.profile) {
            Some(t) => Some((t, dp)),
            None => {
                notes.push(format!(
                    "{kind} profile skipped: its values are not times in nanoseconds"
                ));
                None
            }
        }
    };
    let wall = total_of(ProfileKind::Wall);
    let cpu = total_of(ProfileKind::Cpu);
    // The runtime reports its own locks (scheduler, GC) in the mutex profile; only
    // samples whose leaf is not a `runtime.` function are the program's own locks.
    let lock = total_of(ProfileKind::Lock).map(|(total, dp)| {
        let p = dp.profile;
        let mut internal = 0i64;
        for_each_sample(p, p.default_sample_type, &Filter::default(), |funcs, v| {
            if funcs
                .last()
                .is_some_and(|&l| p.str(p.functions[l.0 as usize].name).starts_with("runtime."))
            {
                internal = internal.saturating_add(v);
            }
        });
        (total - internal, dp)
    });
    let block_io = total_of(ProfileKind::BlockIo);
    let sched = total_of(ProfileKind::SchedDelay);

    let Some((wall_total, wall_dp)) = wall else {
        notes.push("no usable wall-clock profile, so busy time cannot be measured".into());
        return Diagnosis {
            verdict: Verdict::Inconclusive,
            summary: "Not enough data to diagnose: the session has no wall-clock profile.".into(),
            busy_threads: 0.0,
            findings: vec![],
            notes,
        };
    };

    // Busy time: wall time minus samples parked in an idle runtime frame; also the
    // part of it that ends in file I/O, as a fallback estimate.
    let io_re = Regex::new(IO_PATTERN).expect("static regex");
    let wait_re = Regex::new(WAIT_PATTERN).expect("static regex");
    let dotnet_idle_re = Regex::new(DOTNET_IDLE_PATTERN).expect("static regex");
    let blocking_re = Regex::new(BLOCKING_LEAF_PATTERN).expect("static regex");
    let mut runnable_ns = 0i64;
    let st = wall_dp.profile.default_sample_type;
    let mut idle_ns = 0i64;
    let mut io_guess_ns = 0i64;
    let name_of = |f: prism_model::FuncId| wall_dp.profile.str(wall_dp.profile.functions[f.0 as usize].name);
    for_each_sample(wall_dp.profile, st, &Filter::default(), |funcs, v| {
        if let Some(&leaf) = funcs.last() {
            let leaf_name = name_of(leaf);
            let parked = IDLE_NAMES.contains(&leaf_name)
                && !funcs.iter().any(|&f| wait_re.is_match(name_of(f)));
            if parked || dotnet_idle_re.is_match(leaf_name) {
                idle_ns = idle_ns.saturating_add(v);
                return;
            }
            if !blocking_re.is_match(leaf_name) {
                runnable_ns = runnable_ns.saturating_add(v);
            }
        }
        if funcs.iter().any(|&f| io_re.is_match(name_of(f))) {
            io_guess_ns = io_guess_ns.saturating_add(v);
        }
    });
    let busy_ns = (wall_total - idle_ns).max(0);
    let busy_threads = if input.window_ns == 0 {
        0.0
    } else {
        busy_ns as f64 / input.window_ns as f64
    };
    if input.window_ns == 0 {
        notes.push("capture window length unknown; the idle check was skipped".into());
    }
    let idle = busy_ns == 0 || (input.window_ns > 0 && busy_threads < cfg.idle_busy_threads);
    if idle {
        return Diagnosis {
            verdict: Verdict::Idle,
            summary: format!(
                "The target looked idle ({busy_threads:.2} busy threads on average); rerun under load."
            ),
            busy_threads,
            findings: vec![],
            notes,
        };
    }

    let wall_f = &wall_dp.fidelity;
    let mut findings: Vec<DoctorFinding> = Vec::new();
    let mut triggered: Vec<(Verdict, f64)> = Vec::new();

    // Lock evidence is the mutex profile only. Go's block profile is mostly idle channel
    // waits (including the capture agent's own handlers), so it says nothing about lock cost.
    let lock_src = lock;
    let lock_share = lock_src.map_or(0.0, |(t, _)| pct(t, busy_ns));
    let lock_hit = lock_src.is_some() && lock_share >= cfg.lock_pct;

    // I/O: a real Block I/O profile, else an estimate from wall-clock stacks.
    let (io_share, io_fid, io_conf, io_id, io_estimated) = match block_io {
        Some((t, dp)) => {
            let share = pct(t, busy_ns);
            let c = if dp.fidelity == Fidelity::Approximate {
                Confidence::Medium
            } else {
                Confidence::High
            };
            (share, dp.fidelity.clone(), c, Some(dp.short_id), false)
        }
        None => (pct(io_guess_ns, busy_ns), Fidelity::Approximate, Confidence::Low, None, true),
    };
    let io_hit = io_share >= cfg.io_pct;

    if io_hit {
        let mut next = vec![match io_id {
            Some(id) => ctx.show(id, "", "Slowest files and call paths"),
            None => ctx.capture_io(),
        }];
        if io_estimated {
            next.truncate(1);
        }
        findings.push(DoctorFinding {
            rule: "io_bound",
            confidence: io_conf,
            fidelity: io_fid,
            share_pct: io_share,
            title: format!(
                "I/O-bound: {io_share:.0}% of busy time waits on I/O{}",
                if io_estimated { " (estimated from wall-clock stacks)" } else { "" }
            ),
            detail: "Threads spend this share of their busy time in file or disk I/O.".into(),
            next,
            hints: vec!["Cache or batch reads on the top files; check disk or volume latency.".into()],
        });
        triggered.push((Verdict::IoBound, io_share));
    }

    if lock_hit {
        let (_, dp) = lock_src.expect("lock_hit implies a source");
        findings.push(DoctorFinding {
            rule: "lock_bound",
            confidence: confidence(lock_share, cfg.lock_pct, &dp.fidelity),
            fidelity: dp.fidelity.clone(),
            share_pct: lock_share,
            title: format!("Lock contention: {lock_share:.0}% of busy time waits on locks"),
            detail: "Threads queue on contended locks for this share of their busy time.".into(),
            next: vec![ctx.show(dp.short_id, " --sort total", "Longest waits by call site")],
            hints: vec!["Shorten the critical section or shard the lock.".into()],
        });
        triggered.push((Verdict::LockBound, lock_share));
    }

    if let Some((t, dp)) = sched {
        let share = pct(t, busy_ns);
        if share >= cfg.sched_pct {
            findings.push(DoctorFinding {
                rule: "scheduler_starved",
                confidence: confidence(share, cfg.sched_pct, &dp.fidelity),
                fidelity: dp.fidelity.clone(),
                share_pct: share,
                title: format!("Scheduler delay: runnable threads wait for a CPU for {share:.0}% of busy time"),
                detail: "Threads were ready to run but no CPU was free.".into(),
                next: vec![ctx.show(dp.short_id, "", "Where the delay lands")],
                hints: vec!["Check CPU limits and throttling of the container, or reduce runnable threads.".into()],
            });
            triggered.push((Verdict::SchedulerStarved, share));
        }
    }

    match cpu {
        Some((cpu_total, cpu_dp)) => {
            let cpu_share = pct(cpu_total.min(busy_ns), busy_ns);
            // More runnable goroutines than cores: wall time in user code is a multiple of
            // CPU time, but those threads are waiting for a CPU, not for anything else.
            let runnable_share = pct(runnable_ns, busy_ns);
            let saturated = cpu_share < cfg.cpu_bound_pct && runnable_share >= cfg.cpu_bound_pct;
            if cpu_share >= cfg.cpu_bound_pct || saturated {
                let fid = weakest(&[&cpu_dp.fidelity, wall_f]);
                let (share, conf, title, detail) = if saturated {
                    (
                        runnable_share,
                        Confidence::Low,
                        format!("CPU-bound: {runnable_share:.0}% of busy time is running or waiting for a CPU"),
                        "Wall time in program code is far above CPU time: more threads are runnable than there are cores.",
                    )
                } else {
                    (
                        cpu_share,
                        confidence(cpu_share, cfg.cpu_bound_pct, &fid),
                        format!("CPU-bound: {cpu_share:.0}% of busy time is on-CPU"),
                        "Threads are running, not waiting; the top self-time functions are the cost.",
                    )
                };
                findings.push(DoctorFinding {
                    rule: "cpu_bound",
                    confidence: conf,
                    fidelity: fid,
                    share_pct: share,
                    title,
                    detail: detail.into(),
                    next: vec![ctx.show(cpu_dp.short_id, "", "Hottest functions")],
                    hints: vec!["Optimise the top self-time functions; use `prism diff` after a change.".into()],
                });
                triggered.push((Verdict::CpuBound, share));
            }
            let wait_share = 100.0 - cpu_share.max(runnable_share);
            if wait_share >= cfg.wait_pct && !io_hit && !lock_hit {
                let fid = weakest(&[&cpu_dp.fidelity, wall_f]);
                let conf = if fidelity_rank(&fid) <= 1 { Confidence::Low } else { Confidence::Medium };
                findings.push(DoctorFinding {
                    rule: "waiting_unknown",
                    confidence: conf,
                    fidelity: fid,
                    share_pct: wait_share,
                    title: format!("Waiting: {wait_share:.0}% of busy time is off-CPU, cause unknown"),
                    detail: "Threads spend this share of busy time not running, and no lock or I/O data explains it.".into(),
                    next: vec![ctx.capture_io()],
                    hints: vec!["Capture Block I/O and off-CPU time to see what the threads wait for.".into()],
                });
                triggered.push((Verdict::WaitingUnknown, wait_share));
            }
            // GC pressure is a finding, never the verdict.
            if let Ok(gc_re) = Regex::new(GC_PATTERN) {
                let mut gc_ns = 0i64;
                let p = cpu_dp.profile;
                for_each_sample(p, p.default_sample_type, &Filter::default(), |funcs, v| {
                    if funcs
                        .iter()
                        .any(|&f| gc_re.is_match(p.str(p.functions[f.0 as usize].name)))
                    {
                        gc_ns = gc_ns.saturating_add(v);
                    }
                });
                let gc_share = pct(gc_ns, cpu_total);
                if gc_share >= cfg.gc_pct {
                    findings.push(DoctorFinding {
                        rule: "gc_pressure",
                        confidence: confidence(gc_share, cfg.gc_pct, &cpu_dp.fidelity),
                        fidelity: cpu_dp.fidelity.clone(),
                        share_pct: gc_share,
                        title: format!("GC pressure: {gc_share:.0}% of CPU time is garbage collection"),
                        detail: "A large part of the CPU goes to the garbage collector.".into(),
                        next: vec![],
                        hints: vec!["Reduce allocation rate; see the memory profile for the top allocators.".into()],
                    });
                }
            }
        }
        None => notes.push("no usable CPU profile: CPU-bound and waiting rules were skipped".into()),
    }

    findings.sort_by(|a, b| b.share_pct.total_cmp(&a.share_pct));

    // Verdict: the largest triggered share; ties go Io, Lock, Scheduler, Cpu, Waiting.
    let order = |v: &Verdict| match v {
        Verdict::IoBound => 0,
        Verdict::LockBound => 1,
        Verdict::SchedulerStarved => 2,
        Verdict::CpuBound => 3,
        _ => 4,
    };
    triggered.sort_by(|a, b| b.1.total_cmp(&a.1).then(order(&a.0).cmp(&order(&b.0))));
    let verdict = triggered.first().map_or(Verdict::Inconclusive, |t| t.0);
    let summary = match verdict {
        Verdict::Inconclusive => {
            "No single bottleneck stands out; look at the top functions of each profile.".to_string()
        }
        v => {
            let rule = match v {
                Verdict::IoBound => "io_bound",
                Verdict::LockBound => "lock_bound",
                Verdict::SchedulerStarved => "scheduler_starved",
                Verdict::CpuBound => "cpu_bound",
                _ => "waiting_unknown",
            };
            findings
                .iter()
                .find(|f| f.rule == rule)
                .map(|f| f.title.clone())
                .unwrap_or_default()
        }
    };

    Diagnosis {
        verdict,
        summary,
        busy_threads,
        findings,
        notes,
    }
}
