# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/) (while below 1.0, minor versions may change behaviour).

## [Unreleased]

### Added
- Windows x64 release package: `prism.exe` with the UI embedded, a self-contained `prism-dotnet.exe` recording helper, checksums, and a PowerShell installer.
- Third-party notices generated for each release.
- Browser hardening headers on every response (`X-Frame-Options`, `Content-Security-Policy`, `X-Content-Type-Options`, `Referrer-Policy`).
- Host-header check on the Go agent, matching the UI server (DNS-rebinding protection).

### Changed
- The Go agent module path is now `github.com/rajkumarGosavi/prism/agents/go` (was `prism.dev/agent`).
- The ".NET helper not found" message now explains what to do for release-package users.

## [0.1.0] - unreleased

First public release (Windows x64).

### Added
- Import pprof, collapsed stacks, speedscope and `.nettrace` profiles into a local store.
- `prism record` for live Go (agent, `net/http/pprof` URL, `go test`) and .NET 8+ processes (CPU, wall-clock, allocations, lock contention, threads), grouped into sessions.
- Browser UI: flame graph and icicle, Top, sandwich, call tree, hot path, call graph, time heat map, session timeline, history, findings, diff, export.
- CLI: `import`, `ls`, `show`, `open`, `ui`, `record`, `targets`, `sessions`, `diff`, `check`, `rm`.
- `prism check` regression gate for CI.
- Fidelity badges (native, approximate, derived) on every profile.

[Unreleased]: https://github.com/rajkumarGosavi/prism/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/rajkumarGosavi/prism/releases/tag/v0.1.0
