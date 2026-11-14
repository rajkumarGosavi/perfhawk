# Known limitations (v0.1)

Perfhawk v0.1 is a first release. This page lists what it does not do, so you are not surprised.

## Platform

- **Windows 10/11 x64 only.** Linux and macOS support is planned next; the code builds there but is not released or fully tested (see [FOLLOWUPS.md](FOLLOWUPS.md)).
- Release binaries are **not yet code-signed** unless the release notes say so, so SmartScreen may warn. See [Troubleshooting](TROUBLESHOOTING.md).
- `perfhawk doctor` exists; its I/O verdict is firm only where OS-level tracing is available (Windows with an administrator shell).

## Scope

- Perfhawk is **on-demand**: you start a recording, it runs for a window of 1 second to 10 minutes. It is not a continuous production profiler.
- **Local only.** No remote targets, Kubernetes or SSH, and no team sharing. The UI has no authentication and must not be exposed on a network.
- Languages: **Go 1.22+** and **.NET 8+**. Java, Python and Node can be *imported* from collapsed or speedscope files but cannot be recorded.
- Block I/O, off-CPU and scheduler delay are available on **Windows only**, through ETW, and need an administrator shell. Linux (eBPF) and macOS are planned.

## OS-level tracing (Windows ETW)

- .NET frames in these profiles are named for JIT-compiled code and for precompiled (ReadyToRun) framework code such as `System.Private.CoreLib` (x64 only; composite and ARM64 images, and the cold halves of split methods, stay unnamed). Native frames in `coreclr.dll`, `ntdll.dll`, `kernel32.dll` and `KernelBase.dll` still show as module+offset until PDB support lands. Go frames are named unless the binary is stripped.
- System DLL frames have no names (no PDB lookup): `module+0xoffset`.
- A thread that is blocked for the whole capture and never wakes has no off-CPU row, because Windows records the blocked stack when the thread resumes.
- Under heavy system load Windows drops trace events; the profile carries a warning with the counts and numbers undercount.
- If a stale Perfhawk trace session is left after a crash, stop it with `logman stop perfhawk-kernel-<pid> -ets`.

## Go

- You need the Perfhawk agent in your program, a `net/http/pprof` endpoint, or `--go-test`. There is no zero-change attach to an arbitrary Go process.
- **Wall-clock profiles have no time axis**, so the time heat map does not apply to them.
- Time resolution for Go heat maps is 250 ms.
- With `go test` mode on Windows, Ctrl+C stops the `go` tool but may leave the test binary running; end it from Task Manager.
- `-- go run .` records the `go run` wrapper rather than your program; build first.
- Block profile rate: the Go runtime has no getter, so the agent restores only rates it set itself.

## .NET

- **CPU is approximate.** It is reconstructed from sampling events, so treat absolute values as estimates and compare *within* a capture. The fidelity badge shows this.
- `perfhawk targets` does not show runtime version or command line.
- Processes with diagnostics disabled, or running as another user, cannot be attached.
- The recording helper is a self-contained 35 MB executable. It must stay next to `perfhawk.exe`.

## Analysis and UI

- The flame graph canvas is not keyboard navigable; use the **Top** table for a keyboard path.
- Very deep call stacks are depth-capped (1000) with a visible marker, so extremely deep recursions are truncated in the flame view.
- Filtered flame, Top and sandwich queries on very large profiles (hundreds of MB) can take several seconds.
- Diffing compares functions by exact name; a rename appears as one regression and one improvement.
- `perfhawk check` compares raw values when profiles record no duration.
- Session timelines align by start time only.
- Export: collapsed-stack export does not escape `;` in function names.

## Data

- Very large profiles are loaded into memory for analysis, so a profile of hundreds of MB needs a few GB of RAM while it is open.
- If Perfhawk is killed during a recording, the session can remain marked `running`. Remove it with `perfhawk rm --session <id>`.
- The store format may change between 0.x releases; Perfhawk migrates automatically and refuses to open a database from a newer version rather than corrupting it.

## Reporting gaps

Missing something that blocks you? Open an issue and tell us what you were trying to find. The [roadmap](PLAN.md#7-roadmap) is driven by real use.
- **Go:** Block I/O and off-CPU stacks name the file and the OS/runtime frames but not the Go function that made the call (Go makes syscalls on its system stack). Use the Go wall-clock or CPU profile to find the caller.
