# Follow-ups

Items found during reviews and deliberately deferred. ~~Struck through~~ items were done in Phase 1.

## Track for Phase 1
- ~~**Flame payload depth.** `min_fraction` limits the tree's width, not its depth. A few heavy, deep recursions produce flame JSON over 5 MiB, and deep trees can also exceed the browser's canvas size limit, so the graph renders blank. Add a depth cap or recursion collapsing to the flame API, plus a "too deep" notice in the UI.~~ Done: depth cap (default 1000) with a truncation marker, and a canvas clip notice.
- **Ungated query paths** (moved to Phase 2 with block I/O). Filtered flame/top, `inverted_flame` and `sandwich` still walk every sample (about 213 M frames on the 500 MB benchmark) and are not covered by the perf gate. Extend the trie fast path to them, or gate them.
- **CI.** (Still open: push a remote and get a green run; Phase 1 added Go, .NET and gated e2e steps that have only run locally on Windows.) The 3-OS matrix and `cargo deny` have only run locally on Windows. Push the branch and get a green run (this also checks that `stacker` builds on Linux and macOS).
- ~~**`perfhawk rm`.** Pair it with repairing a corrupt or partial blob on re-import, and add an fsync before the blob rename.~~ Done.
- **Data directory** (moved to Phase 2). Consider `%LOCALAPPDATA%` instead of Roaming `%APPDATA%` on Windows for multi-hundred-MB blobs.

## Before the first binary release
- Add a third-party notices file covering the Rust crates (e.g. cargo-about) and the npm production dependencies. The embedded UI bundle currently drops React's MIT notice.
- Add `X-Frame-Options: DENY` / `frame-ancestors 'none'` to server responses.

## Hardening / polish
- **Importer validation:** speedscope negative weights, non-monotonic `at` values and weights-length mismatches are accepted silently. Collapsed lines with a trailing space are rejected.
- **Gzip decompression is unbounded** (decompression-bomb risk). Fine under the local-only trust model; revisit before any remote access or share server.
- **Fx hashing is not DoS-resistant** against adversarial ids. Same local-trust caveat.
- **Server:** no single-flight on cache misses (two loads of the same id can double peak memory). Mutex-poison `unwrap`. Path rejections and 405 responses are plain text. The 2 GiB upload is buffered in memory. The 413 path is untested.
- **UI:** no `AbortController` for superseded requests. The canvas is not keyboard accessible (the Top table gives a keyboard path). The tooltip can clip near edges. No component or interaction tests.
- **CLI:** `to_sig3` rounds ties half-to-even, unlike JS `toPrecision` (cosmetic). Ctrl-C exits 0. The open-test timeout message omits the child's stderr.
- **Store:** ~~the WAL pragma runs before the schema-version gate~~ (fixed in Phase 1). The platform branches of `default_home` are untested.
- **Misc:** `$profile` loop variable in `examples/dotnet-demo/profile.ps1` shadows PowerShell's `$PROFILE`. Clippy `needless_lifetimes` in `crates/perfhawk-import/tests/pprof.rs`. Recursive derived `Debug`/`Clone`/`PartialEq` on `FlameNode` overflow the stack on very deep trees (tests only).

## Found in Phase 1
- **Licences:** `cargo deny check licenses` passes locally (ureq's `webpki-roots` needed `CDLA-Permissive-2.0` in `deny.toml`). Still record the new .NET (MIT) and Go (`fgprof` MIT, `google/pprof` Apache-2.0) licences in the notices file before release.
- **Ctrl-C while recording** abandons in-flight Go HTTP requests (ureq cannot abort); the agent restores sampling rates when the connection closes or the window ends. `go test` mode kills only the `go` tool, not the test binary, on Windows. The Unix `record_ctrl_c_exit_130` test has not been run on a Unix machine yet.
- **Go block rate:** the runtime has no getter, so the agent restores only rates set through `agent.SetBlockProfileRate` (otherwise 0).
- **.NET `perfhawk targets`** shows no runtime version or command line (the client library does not expose them publicly).
- **"Go process without the perfhawk agent" hint** works on Linux only (`/proc/<pid>/exe`).
- **Packaging:** bundle `perfhawk-dotnet` next to `perfhawk` in release archives (see "Before the first binary release").
- **UI:** no component tests for the new views (Swap, delete confirm, normalize, compare were not exercised by automated tests).
- **Review minors deferred:** `-- go run .` / `-- dotnet run` wrappers are mis-detected and can orphan the real app; `.nettrace` import writes its `.etlx` beside the input; ureq timeouts are classed as target-exited; a re-imported identical profile is not attached to the new session; a session stays `running` if perfhawk is killed; blob repair uses the new call's kind override; two processes migrating a v1 database at once can collide; `perfhawk record` prints the partial note for import-failure downgrades too; helper maps unreachable macOS .NET to `not_dotnet` and `UnauthorizedAccessException` to `diagnostics_disabled`; the agent has no Host check (DNS rebinding) and fgprof ignores client disconnects; per-user discovery directory and file-ownership check on shared Unix `/tmp`; the fake .NET helper bin should move to a test-only crate before packaging; folded diff nodes carry no truncated marker.

## From V2-V6 (visualization roadmap)

- **Deferred review minors:** session timeline offsets per profile (alignment is by start time only); the heat map reloads on every window change; typing in the window boxes can reorder start and end; the Go windowed endpoint truncates silently if the CPU profiler is taken mid-run; `has_timeline` in lists only checks that the file exists while the record validates it; the server's timeline cache keeps "no timeline" after a later import attaches one; memory profiles can show two findings for one function; collapsed-stack export does not escape `;`.
- **Not verified live:** drag and Shift-drag selection, graph pan/zoom and slider, window-vs-window diff in the UI, Export downloads and clipboard, a full .NET capture with timelines, the 1 h heat map and 1 M-sample graph timing budgets, and the GitHub Actions example in CI.md.
- **Known limits:** Go wall-clock has no time data; Go time resolution is 250 ms; `perfhawk check` compares raw values when profiles record no duration; the UI still has no component test harness.
