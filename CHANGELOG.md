# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/) (while below 1.0, minor versions may change behaviour).

## [Unreleased]

### Added
- Web UI: a **Record** tab records a running Go or .NET process (pick it from a list, or enter a pid or a local pprof url), with live progress, cancel, and **Record and diagnose** to open the session with its Doctor verdict. A **Regression check** panel in the Diff view is the UI form of `perfhawk check` (PASS or FAIL with the offending functions). Browser recording attaches only; it cannot launch programs or reach non-local urls.
- Web UI: a first-run welcome on the home page explaining sessions and profiles, an in-app Help panel (`?`), hover/focus definitions for terms such as Self and Total, and a System/Light/Dark theme switch.
- Windows OS-level tracing (ETW): `perfhawk record --kinds block_io,off_cpu,sched_delay` records Block I/O (by file), off-CPU time and scheduler delay for one process, with Go frames named from symbols. Needs an administrator shell; Perfhawk never elevates itself and prints the exact command to rerun.
- Windows x64 release package: `perfhawk.exe` with the UI embedded, a self-contained `perfhawk-dotnet.exe` recording helper, checksums, and a PowerShell installer.
- Third-party notices generated for each release.
- Browser hardening headers on every response (`X-Frame-Options`, `Content-Security-Policy`, `X-Content-Type-Options`, `Referrer-Policy`).
- Host-header check on the Go agent, matching the UI server (DNS-rebinding protection).

### Changed
- Web UI errors now say what happened and what to try (for example when the server has stopped) instead of showing raw messages such as `Failed to fetch`; the original text is under "Technical details".
- The Go agent module path is now `github.com/rajkumarGosavi/perfhawk/agents/go` (was `perfhawk.dev/agent`).
- The ".NET helper not found" message now explains what to do for release-package users.

## [0.1.0] - unreleased

First public release (Windows x64).

### Added
- Import pprof, collapsed stacks, speedscope and `.nettrace` profiles into a local store.
- `perfhawk record` for live Go (agent, `net/http/pprof` URL, `go test`) and .NET 8+ processes (CPU, wall-clock, allocations, lock contention, threads), grouped into sessions.
- Browser UI: flame graph and icicle, Top, sandwich, call tree, hot path, call graph, time heat map, session timeline, history, findings, diff, export.
- CLI: `import`, `ls`, `show`, `open`, `ui`, `record`, `targets`, `sessions`, `diff`, `check`, `rm`.
- `perfhawk check` regression gate for CI.
- Fidelity badges (native, approximate, derived) on every profile.

[Unreleased]: https://github.com/rajkumarGosavi/perfhawk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/rajkumarGosavi/perfhawk/releases/tag/v0.1.0
