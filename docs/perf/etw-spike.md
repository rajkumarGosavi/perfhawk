# ETW spike: what a real Windows kernel trace looks like

Findings from running `crates/perfhawk-capture/examples/etw_spike.rs` from an elevated shell on Windows 11 (QPC frequency 10 MHz). They fix the design of `etw/` and `etwdecode.rs`; the decoder's tests use payload bytes captured here.

## Session

- `ferrisetw` 1.2 cannot enable stack walking (no `TraceSetInformation`), so the provider uses the `windows` crate directly.
- A private session (`perfhawk-kernel-<pid>`, `EVENT_TRACE_SYSTEM_LOGGER_MODE`) works, provided its `Wnode.Guid` is its own GUID, **not** `SystemTraceControlGuid` (that returns error 87). It coexists with WPR, PerfView and Visual Studio. If a machine still returns 87, the code falls back to the classic `NT Kernel Logger`, which only one tool can hold.
- Kernel events are selected through `TraceSetInformation(TraceSystemTraceEnableFlagsInfo)`; stacks through `TraceSetInformation(TraceStackTracingInfo)`.
- **Consume with `PROCESS_TRACE_MODE_RAW_TIMESTAMP`.** Without it `ProcessTrace` rewrites header timestamps to system time (about 1.3e17) while the stack-walk payload keeps raw QPC ticks (about 1e10), and nothing matches.
- Rate on an ordinary busy desktop over 5 s: about 118,000 stackable events and 124,000 stack records. The decoder drops events of other processes early.

## Which stack belongs to which event

A stack-walk record (`StackWalk`, opcode 32) carries `EventTimeStamp` (u64, raw QPC), process id (u32 at 8), thread id (u32 at 12), then return addresses, leaf first. It matches an event by `(timestamp, thread)`:

| Event | Stack thread | Notes |
|---|---|---|
| FileIo read/write (67/68) | header thread = `TTid` payload field | issuing thread; this is the useful Block I/O stack |
| CSwitch (Thread 36) | the thread switched **in** | the stack it blocked on: this is the off-CPU stack |
| ReadyThread (Thread 50) | neither the header nor the readied thread | not used; scheduler delay reuses the switch-in's blocked stack |
| DiskIo read/write (10/11) | thread 0 (DPC on an idle thread) | not used for stacks; Block I/O rows come from FileIo |

Stack-walk records can arrive before or after their event. Kernel addresses (at or above `0xffff800000000000`) are dropped.

## Payload layouts (64-bit, little endian, offsets in bytes)

- **FileIo 67/68 read/write (v3, 48 B):** offset u64 @0, IRP u64 @8, file object u64 @16, file key u64 @24, thread u32 @32, size u32 @36, flags u32 @40.
- **FileIo 76 OpEnd (20 B):** IRP u64 @0. Duration is the OpEnd time minus the read/write time, matched by IRP.
- **FileIo 0/32/35/36 name events:** file key u64 @0, UTF-16 name @8. Most arrive in rundown at trace end, so names are resolved after the trace stops. **FileIo 64 create:** file object @8, path @32.
- **DiskIo 10/11 (v3, 52 B):** transfer size u32 @8, file object u64 @24, IRP @32, response time u64 @40 (QPC ticks), issuing thread u32 @48.
- **CSwitch (v5, 28 B):** new thread u32 @0, old thread u32 @4, old thread wait reason u8 @12.
- **ReadyThread (v2, 8 B):** readied thread u32 @0.
- **Thread start/rundown (opcodes 1, 3):** process id @0, thread id @4. **Process end (2):** process id @8.
- **Image load/rundown (opcodes 2, 3, 4, 10):** base u64 @0, size u64 @8, process id u32 @16, UTF-16 path @56.

File paths come as NT device paths (`\Device\HarddiskVolume3\...`); `nt_to_dos` rewrites them with `QueryDosDevice`.

## Symbols

Go 1.27 on Windows writes a COFF symbol table readable with the `object` crate (`main.main` and friends); a binary built with `-ldflags=-s -w` has none and is reported as stripped. No `.gopclntab` parser was needed. .NET JIT methods need ETW rundown events from `Microsoft-Windows-DotNETRuntime`; that part is not implemented yet (see FOLLOWUPS), so managed frames currently show as module+offset or `[unknown]`.

## Go goroutine stacks are not reachable (found in the first e2e run)

Go makes Windows syscalls on the thread's system stack (`g0`) through `runtime.asmcgocall` / `asmstdcall`. The kernel's stack walk therefore ends there: a Block I/O stack for the Go demo reads `ntdll.dll+0x...` / `KernelBase.dll+0x...` / `runtime.asmstdcall` / `runtime.asmcgocall` and never reaches `main.(*service).fileWorker`. File attribution (the `[file: ...]` leaf), timings and bytes are correct; the Go function that issued the I/O is not recoverable from the ETW stack alone. Fixing it needs the Go agent's goroutine-to-thread information (a FOLLOWUP). .NET managed code runs on the thread's own stack, so it does not have this problem once rundown names are added.
