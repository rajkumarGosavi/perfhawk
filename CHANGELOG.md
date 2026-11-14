# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/) (while below 1.0, minor versions may change behaviour).

## [Unreleased]

### Added
- GC analysis for Go: `perfhawk record` on a Go process running the perfhawk agent now captures GC data (collections, pauses, live heap, allocation rate), shown by `perfhawk gc`, `perfhawk doctor` and the web UI. Go reports no GC duration, reasons or generations, so the data is marked approximate. Needs the agent module `agents/go` v0.1.1 or later; an older agent gives a warning and no GC data.
- `examples/go-demo -mode gc` workload and `-gogc` flag.

## [0.1.0] - 2026-11-15

First public release (Windows x64).

### Added
- Web UI: Inter and JetBrains Mono are bundled (Latin subset, about 90 kB, no network); the UI falls back to system fonts if they fail to load.
- Web UI revamp (workbench): app shell with left rail, breadcrumb and Ctrl K command palette; profile workbench with a timeline range strip, filter chips, flame graph with inspector and Top / Call tree / Findings tabs; findings-first Summary; Home, History, Diff and Call graph restyled on shared Button, Tabs, Chip primitives; GC and Diff are now rail views; Comfortable/Compact density (Ctrl K). **Breaking:** keyboard map is now 1 Summary, 2 Flame, 3 Call tree, 4 Timeline, 5 Call graph, 6 GC, 7 Diff, 8 History; Icicle, Both and Sandwich are folded into Flame, Top and Groups moved out of the rail.
- GC analysis for .NET: `perfhawk record` now captures garbage-collection events (use `--no-gc` to skip). `perfhawk gc [REF] [--json]` reports collections per generation (gen0, gen1, gen2), pause times (p50, p95, p99, max, share of wall time), heap size and allocation rate, with findings for long pauses, gen2-heavy collection, a heap that keeps growing after full GCs, explicit `GC.Collect` calls and large-object collections. The web UI shows the same in a **Garbage collection** section of a session (pause timeline, heap and allocation charts, collections table). `perfhawk doctor` uses the measured pause share when a session has GC data.
- `perfhawk import --gc <file>` imports GC data from a Perfhawk `gc.json` (an open format any tool can write) or a `.nettrace` file, as a new session or attached to an existing one (`--session`, `--replace`).
- `examples/dotnet-demo gc` workload (gen0 churn, a growing retained list, large objects, explicit collects) for trying the GC features.
- Web UI: a **Record** tab records a running Go or .NET process (pick it from a list, or enter a pid or a local pprof url), with live progress, cancel, and **Record and diagnose** to open the session with its Doctor verdict. A **Regression check** panel in the Diff view is the UI form of `perfhawk check` (PASS or FAIL with the offending functions). Browser recording attaches only; it cannot launch programs or reach non-local urls.
- Web UI: a first-run welcome on the home page explaining sessions and profiles, an in-app Help panel (`?`), hover/focus definitions for terms such as Self and Total, and a System/Light/Dark theme switch.
- Windows OS-level tracing (ETW): `perfhawk record --kinds block_io,off_cpu,sched_delay` records Block I/O (by file), off-CPU time and scheduler delay for one process, with Go frames named from symbols and .NET frames named from the runtime's JIT/rundown events and ReadyToRun images (native frames in coreclr/ntdll/kernel32 stay `module+0xoffset`). Needs an administrator shell; Perfhawk never elevates itself and prints the exact command to rerun.
- Windows x64 release package: `perfhawk.exe` with the UI embedded, a self-contained `perfhawk-dotnet.exe` recording helper, checksums, and a PowerShell installer.
- Third-party notices generated for each release.
- Browser hardening headers on every response (`X-Frame-Options`, `Content-Security-Policy`, `X-Content-Type-Options`, `Referrer-Policy`).
- Host-header check on the Go agent, matching the UI server (DNS-rebinding protection).
- Import pprof, collapsed stacks, speedscope and `.nettrace` profiles into a local store.
- `perfhawk record` for live Go (agent, `net/http/pprof` URL, `go test`) and .NET 8+ processes (CPU, wall-clock, allocations, lock contention, threads), grouped into sessions.
- Browser UI: flame graph and icicle, Top, sandwich, call tree, hot path, call graph, time heat map, session timeline, history, findings, diff, export.
- CLI: `import`, `ls`, `show`, `open`, `ui`, `record`, `targets`, `sessions`, `diff`, `check`, `rm`.
- `perfhawk check` regression gate for CI.
- Fidelity badges (native, approximate, derived) on every profile.

### Changed
- Web UI errors now say what happened and what to try (for example when the server has stopped) instead of showing raw messages such as `Failed to fetch`; the original text is under "Technical details".
- The Go agent module path is now `github.com/rajkumarGosavi/perfhawk/agents/go` (was `perfhawk.dev/agent`).
- The ".NET helper not found" message now explains what to do for release-package users.

[0.1.0]: https://github.com/rajkumarGosavi/perfhawk/releases/tag/v0.1.0
