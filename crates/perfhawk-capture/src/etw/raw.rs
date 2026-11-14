//! The only file that talks to the Windows ETW APIs: start a private kernel
//! trace session with stack walking, and hand every record to a callback.

use std::ffi::c_void;
use std::thread::JoinHandle;
use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Diagnostics::Etw::*;
use windows::Win32::System::Performance::QueryPerformanceFrequency;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// `EVENT_TRACE_SYSTEM_LOGGER_MODE`: a kernel session with its own name, so it
/// coexists with other tools' kernel traces.
const SYSTEM_LOGGER_MODE: u32 = 0x0200_0000;
/// `WNODE_FLAG_TRACED_GUID`.
const WNODE_FLAG_TRACED_GUID: u32 = 0x0002_0000;

#[derive(Debug, thiserror::Error)]
pub enum EtwError {
    #[error("ETW needs administrator rights; run this from an elevated shell")]
    NotElevated,
    #[error("another tool is using the Windows kernel trace session (for example Windows Performance Recorder, PerfView or Visual Studio); Perfhawk did not stop it. Stop that trace and retry")]
    LoggerBusy,
    #[error("{0}")]
    Other(String),
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elev = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elev as *mut _ as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elev.TokenIsElevated != 0
    }
}

/// One ETW record, borrowed for the duration of the callback.
pub struct RawRecord<'a> {
    pub provider: GUID,
    pub opcode: u8,
    pub version: u8,
    pub pid: u32,
    pub tid: u32,
    /// QPC ticks (the session asks for the QPC clock).
    pub qpc: i64,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TraceCounters {
    pub events_lost: u64,
    pub buffers_lost: u64,
    pub qpc_freq: i64,
}

type Callback = Box<dyn FnMut(&RawRecord) + Send>;

pub struct RawTrace {
    handle: CONTROLTRACE_HANDLE,
    props: Vec<u64>,
    name: Vec<u16>,
    thread: Option<JoinHandle<()>>,
}

unsafe extern "system" fn record_cb(rec: *mut EVENT_RECORD) {
    let rec = &*rec;
    let cb = rec.UserContext as *mut Callback;
    if cb.is_null() {
        return;
    }
    let h = &rec.EventHeader;
    let data: &[u8] = if rec.UserData.is_null() || rec.UserDataLength == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(rec.UserData as *const u8, rec.UserDataLength as usize)
    };
    (*cb)(&RawRecord {
        provider: h.ProviderId,
        opcode: h.EventDescriptor.Opcode,
        version: h.EventDescriptor.Version,
        pid: h.ProcessId,
        tid: h.ThreadId,
        qpc: h.TimeStamp,
        data,
    });
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl RawTrace {
    /// Start a kernel session named `name` with `flags` (`EVENT_TRACE_FLAG_*`)
    /// and stack walking for `stack_events` (provider GUID, event type), then
    /// deliver every record to `on_record` on a background thread.
    pub fn start(
        name: &str,
        flags: u32,
        stack_events: &[(GUID, u8)],
        on_record: impl FnMut(&RawRecord) + Send + 'static,
    ) -> Result<RawTrace, EtwError> {
        if !is_elevated() {
            return Err(EtwError::NotElevated);
        }
        // First a private, uniquely named system-logger session; some Windows
        // builds reject that (error 87), so fall back to the classic
        // "NT Kernel Logger", which only one tool can hold at a time.
        let mut classic = false;
        let mut name_w = wide(name);
        let header = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
        let (mut props, handle) = loop {
            let total = header + name_w.len() * 2;
            // u64 storage keeps the properties block 8-byte aligned.
            let mut props = vec![0u64; total.div_ceil(8)];
            let p = props.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
            let mut handle = CONTROLTRACE_HANDLE::default();
            let err = unsafe {
                (*p).Wnode.BufferSize = total as u32;
                (*p).Wnode.Guid = if classic {
                    SystemTraceControlGuid
                } else {
                    // A system-logger session needs its own GUID, not the kernel's.
                    GUID::from_u128(
                        0x7065_7266_6861_776b_0000_0000_0000_0000u128 | std::process::id() as u128,
                    )
                };
                (*p).Wnode.ClientContext = 1; // QPC timestamps
                (*p).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
                (*p).BufferSize = 256; // KB
                (*p).MinimumBuffers = 64;
                (*p).MaximumBuffers = 256;
                (*p).LogFileMode = if classic {
                    EVENT_TRACE_REAL_TIME_MODE
                } else {
                    EVENT_TRACE_REAL_TIME_MODE | SYSTEM_LOGGER_MODE
                };
                if classic {
                    (*p).EnableFlags = EVENT_TRACE_FLAG(flags);
                }
                (*p).LoggerNameOffset = header as u32;
                let name_dst = (props.as_mut_ptr() as *mut u8).add(header) as *mut u16;
                std::ptr::copy_nonoverlapping(name_w.as_ptr(), name_dst, name_w.len());
                StartTraceW(&mut handle, PCWSTR(name_w.as_ptr()), p)
            };
            if err.0 == 87 && !classic {
                classic = true;
                name_w = wide("NT Kernel Logger");
                continue;
            }
            if err == ERROR_ACCESS_DENIED {
                return Err(EtwError::NotElevated);
            }
            if err == ERROR_ALREADY_EXISTS {
                return Err(EtwError::LoggerBusy);
            }
            if err.0 != 0 {
                return Err(EtwError::Other(format!(
                    "StartTrace failed: error {}",
                    err.0
                )));
            }
            break (props, handle);
        };
        unsafe {
            let stop_on_error = |what: &str, code: u32, props: &mut Vec<u64>| {
                let p = props.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
                let _ = ControlTraceW(handle, PCWSTR::null(), p, EVENT_TRACE_CONTROL_STOP);
                EtwError::Other(format!("{what} failed: error {code}"))
            };

            // Which kernel events to log: the legacy flag word is element 0 of
            // the group mask array.
            if !classic {
                let mut masks = [0u32; 8];
                masks[0] = flags;
                let err = TraceSetInformation(
                    handle,
                    TraceSystemTraceEnableFlagsInfo,
                    masks.as_ptr() as *const c_void,
                    std::mem::size_of_val(&masks) as u32,
                );
                if err.0 != 0 {
                    return Err(stop_on_error("enabling kernel events", err.0, &mut props));
                }
            }

            if !stack_events.is_empty() {
                let ids: Vec<CLASSIC_EVENT_ID> = stack_events
                    .iter()
                    .map(|(g, t)| CLASSIC_EVENT_ID {
                        EventGuid: *g,
                        Type: *t,
                        Reserved: [0; 7],
                    })
                    .collect();
                let err = TraceSetInformation(
                    handle,
                    TraceStackTracingInfo,
                    ids.as_ptr() as *const c_void,
                    (ids.len() * std::mem::size_of::<CLASSIC_EVENT_ID>()) as u32,
                );
                if err.0 != 0 {
                    return Err(stop_on_error("enabling stack walking", err.0, &mut props));
                }
            }

            // Consumer side.
            let cb: Box<Callback> = Box::new(Box::new(on_record));
            let cb_ptr = Box::into_raw(cb) as usize;
            let mut consumer_name = name_w.clone();
            let thread = std::thread::spawn(move || {
                let mut lf = EVENT_TRACE_LOGFILEW {
                    LoggerName: PWSTR(consumer_name.as_mut_ptr()),
                    Context: cb_ptr as *mut c_void,
                    ..Default::default()
                };
                lf.Anonymous1.ProcessTraceMode =
                    PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
                lf.Anonymous2.EventRecordCallback = Some(record_cb);
                let h = OpenTraceW(&mut lf);
                if h.Value != u64::MAX {
                    let _ = ProcessTrace(&[h], None, None);
                    let _ = CloseTrace(h);
                }
                drop(Box::from_raw(cb_ptr as *mut Callback));
            });
            Ok(RawTrace {
                handle,
                props,
                name: name_w,
                thread: Some(thread),
            })
        }
    }

    /// Stop the session, wait for the consumer to drain, and report losses.
    pub fn stop(mut self) -> TraceCounters {
        let mut out = TraceCounters::default();
        unsafe {
            let p = self.props.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
            let _ = ControlTraceW(
                self.handle,
                PCWSTR(self.name.as_ptr()),
                p,
                EVENT_TRACE_CONTROL_STOP,
            );
            out.events_lost = (*p).EventsLost as u64;
            out.buffers_lost = ((*p).LogBuffersLost + (*p).RealTimeBuffersLost) as u64;
            let mut f = 0i64;
            let _ = QueryPerformanceFrequency(&mut f);
            out.qpc_freq = f;
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        out
    }
}
