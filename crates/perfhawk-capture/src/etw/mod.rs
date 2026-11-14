//! Windows OS-level capture through Event Tracing for Windows (ETW).

mod os;
pub mod raw;

pub use raw::{is_elevated, EtwError, RawRecord, RawTrace, TraceCounters, UserTrace};

use crate::symbolize::{
    decode_method_event, DOTNET_KEYWORDS, DOTNET_RUNDOWN_KEYWORDS, GUID_DOTNET_RUNDOWN,
    GUID_DOTNET_RUNTIME,
};
use crate::sysevents::MethodInfo;

use crate::cancel::CancelToken;
use crate::etwdecode::{nt_to_dos, Decoder, Record, GUID_FILE_IO, GUID_THREAD};
use crate::symbolize::Symbolizer;
use crate::sysevents::{
    aggregate, build_artifacts, AggregateInput, LossStats, QpcClock, RawEvent, RawKind, WindowSpec,
};
use crate::system::{is_system_kind, SystemProvider, SYSTEM_KINDS};
use crate::types::*;
use perfhawk_model::{Fidelity, ProfileKind};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::GUID;

/// PROCESS | THREAD | IMAGE_LOAD | CSWITCH | DISK_IO | DISK_FILE_IO | DISPATCHER | FILE_IO | FILE_IO_INIT
const KERNEL_FLAGS: u32 =
    0x1 | 0x2 | 0x4 | 0x10 | 0x100 | 0x200 | 0x800 | 0x0200_0000 | 0x0400_0000;
/// Events whose call stacks are recorded: file read/write and context switch.
/// (Disk completions run on an idle thread and ready-thread stacks do not
/// belong to the woken thread, so neither is useful.)
const STACK_EVENTS: [(u128, u8); 3] = [(GUID_FILE_IO, 67), (GUID_FILE_IO, 68), (GUID_THREAD, 36)];
const BUCKET_NS: u64 = 100_000_000;

const NOT_ELEVATED_NOTE: &str =
    "needs administrator rights; run the same command from an elevated shell";

/// Block I/O, off-CPU time and scheduler delay for one process, from a
/// private kernel trace session.
pub struct EtwProvider;

impl EtwProvider {
    pub fn new() -> Self {
        EtwProvider
    }
}

impl Default for EtwProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProvider for EtwProvider {
    fn id(&self) -> &'static str {
        "etw"
    }

    fn capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability> {
        let problem = if rt.pid.is_none() {
            Some("needs a local process id; ETW cannot attach to a URL target")
        } else if !is_elevated() {
            Some(NOT_ELEVATED_NOTE)
        } else {
            None
        };
        SYSTEM_KINDS
            .iter()
            .map(|k| Capability {
                kind: *k,
                fidelity: Fidelity::Native,
                available: problem.is_none(),
                note: problem.map(str::to_string),
            })
            .collect()
    }

    fn overhead(&self) -> &'static str {
        "about 2-8% CPU on the target while tracing (kernel events with stacks)"
    }

    fn capture(
        &self,
        rt: &RuntimeInfo,
        plan: &CapturePlan,
        cancel: &CancelToken,
        progress: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError> {
        let kinds: Vec<ProfileKind> = plan
            .kinds
            .iter()
            .copied()
            .filter(|k| is_system_kind(*k))
            .collect();
        let first = *kinds.first().unwrap_or(&ProfileKind::BlockIo);
        let unsupported = |reason: String| CaptureError::Unsupported {
            kind: first,
            reason,
        };
        let pid = rt.pid.ok_or_else(|| {
            unsupported("needs a local process id; ETW cannot attach to a URL target".into())
        })?;
        let tids = os::thread_ids(pid);
        if tids.is_empty() && is_elevated() {
            return Err(CaptureError::Target(format!("no process with id {pid}")));
        }

        progress("starting kernel trace");
        let decoder = Arc::new(Mutex::new(Decoder::new(pid, tids)));
        let sink = decoder.clone();
        let stacks: Vec<(GUID, u8)> = STACK_EVENTS
            .iter()
            .map(|(g, t)| (GUID::from_u128(*g), *t))
            .collect();
        let trace = RawTrace::start(
            &format!("perfhawk-kernel-{}", std::process::id()),
            KERNEL_FLAGS,
            &stacks,
            move |r: &RawRecord| {
                if let Ok(mut d) = sink.lock() {
                    d.feed(&Record {
                        provider: r.provider.to_u128(),
                        opcode: r.opcode,
                        version: r.version,
                        pid: r.pid,
                        tid: r.tid,
                        qpc: r.qpc,
                        data: r.data,
                    });
                }
            },
        )
        .map_err(|e| match e {
            EtwError::NotElevated => unsupported(NOT_ELEVATED_NOTE.into()),
            EtwError::LoggerBusy => unsupported(e.to_string()),
            EtwError::Other(m) => CaptureError::Helper(m),
        })?;

        // .NET method names: JIT events for new methods plus a rundown of the
        // ones that already exist. Best effort; frames stay module+offset if
        // this session cannot start.
        let mut warnings = Vec::new();
        let methods: Arc<Mutex<Vec<MethodInfo>>> = Arc::default();
        let dotnet_trace = if rt.provider == "dotnet" {
            let sink = methods.clone();
            let started = UserTrace::start(
                &format!("perfhawk-dotnet-{}", std::process::id()),
                &[
                    (GUID::from_u128(GUID_DOTNET_RUNTIME), DOTNET_KEYWORDS),
                    (
                        GUID::from_u128(GUID_DOTNET_RUNDOWN),
                        DOTNET_RUNDOWN_KEYWORDS,
                    ),
                ],
                pid,
                move |r: &RawRecord| {
                    if r.pid != pid {
                        return;
                    }
                    if let Some(m) = decode_method_event(r.id, r.data) {
                        if let Ok(mut v) = sink.lock() {
                            v.push(m);
                        }
                    }
                },
            );
            match started {
                Ok(t) => Some(t),
                Err(e) => {
                    warnings.push(format!(
                        ".NET method names are unavailable ({e}); managed frames show as module+offset"
                    ));
                    None
                }
            }
        } else {
            None
        };

        let started = Instant::now();
        let clock_start = os::qpc_now();
        let unix_start_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        progress(&format!("tracing pid {pid}"));
        let mut end = CaptureEnd::Complete;
        while started.elapsed() < plan.duration {
            std::thread::sleep(Duration::from_millis(100));
            if cancel.is_cancelled() {
                end = CaptureEnd::Cancelled;
                break;
            }
            if !os::process_alive(pid) {
                end = CaptureEnd::TargetExited;
                break;
            }
        }
        let window_ns = started.elapsed().as_nanos() as u64;
        let counters = trace.stop();
        if let Some(t) = dotnet_trace {
            t.stop();
        }

        progress("resolving symbols");
        let decoder = Arc::try_unwrap(decoder)
            .map_err(|_| CaptureError::Helper("trace consumer did not shut down".into()))?
            .into_inner()
            .map_err(|_| CaptureError::Helper("trace decoder failed".into()))?;
        let dropped = decoder.dropped();
        let kernel_only = decoder.kernel_only_stacks();
        let mut events = decoder.finish();
        let devices = os::dos_devices();
        for e in &mut events {
            if let Some(f) = &mut e.file {
                *f = nt_to_dos(f, &devices);
            }
        }
        let mut symbolizer = Symbolizer::new();
        if let Ok(ms) = methods.lock() {
            for m in ms.iter() {
                symbolizer.add_method(m.start, m.size, m.name.clone());
            }
        }
        for e in &events {
            if let (RawKind::ImageLoad, Some(im)) = (e.kind, &e.image) {
                symbolizer.add_image(im.base, im.size, Path::new(&nt_to_dos(&im.path, &devices)));
            }
        }
        let names = |e: &RawEvent| -> Vec<String> {
            e.stack
                .iter()
                .rev()
                .map(|a| symbolizer.resolve(*a).display())
                .collect()
        };
        let mut agg = aggregate(&AggregateInput {
            events: &events,
            target_pid: pid,
            clock: QpcClock {
                freq: counters.qpc_freq.max(1),
                qpc_start: clock_start,
                unix_start_ns,
            },
            window_ns: window_ns.max(1),
            bucket_ns: BUCKET_NS,
            symbolize: &names,
        });
        agg.unresolved_frames = symbolizer.unresolved();
        let artifacts = build_artifacts(
            &agg,
            &WindowSpec {
                start_unix_ns: unix_start_ns,
                duration_ns: window_ns.max(1),
                bucket_ns: BUCKET_NS,
            },
            &LossStats {
                events_lost: counters.events_lost,
                buffers_lost: counters.buffers_lost,
            },
            &kinds,
        );
        let waits: Vec<&RawEvent> = events
            .iter()
            .filter(|e| e.kind == RawKind::CSwitch && e.pid == pid)
            .collect();
        let bare = waits.iter().filter(|e| e.stack.is_empty()).count();
        if !waits.is_empty() && bare * 2 > waits.len() {
            warnings.push(format!(
                "{bare} of {} off-CPU waits have no user-mode stack ({kernel_only} stack walks held only kernel frames), so those rows show only the wait reason",
                waits.len()
            ));
        }
        if dropped > 0 {
            warnings.push(format!(
                "{dropped} events were not recorded because the capture hit its event limit; numbers undercount. Use a shorter --duration"
            ));
        }
        Ok(CaptureOutcome {
            artifacts,
            end,
            warnings,
        })
    }
}
