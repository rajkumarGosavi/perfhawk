# Phase 1 — MVP Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `prism record` captures CPU, wall-clock, memory, lock/blocking and threads profiles from live Go and .NET processes on Linux, macOS and Windows, either by attaching or by launching the target. Captures are grouped into sessions. Two profiles can be diffed in the CLI and the UI.

**Architecture:**
- A new `prism-capture` crate defines a `Provider` trait plus the session orchestration. Two providers implement it:
  - **Go:** talks HTTP to `net/http/pprof`. A tiny Go agent package adds wall-clock profiling, temporary sampling rates and pid discovery.
  - **.NET:** drives a bundled C# helper, `prism-dotnet`, over a versioned JSON-lines protocol. The helper uses Microsoft's diagnostics libraries and writes pprof files.
- Every capture result is a pprof file imported through the existing Phase 0 pipeline, tagged with a session id and a fidelity.
- Diff and a flame depth cap are added to `prism-analysis`. The store moves to schema v2. The server, UI and CLI gain sessions, diff and delete.

**Tech Stack:**
- Rust (existing workspace, plus `ureq` for blocking HTTP).
- Go 1.22+ agent module (`github.com/felixge/fgprof`, MIT).
- C# .NET 8 helper (`Microsoft.Diagnostics.NETCore.Client` and `Microsoft.Diagnostics.Tracing.TraceEvent`, both MIT), with xunit tests.
- React UI (existing).

**Spec:** [docs/PLAN.md](../../PLAN.md) — §0 decisions, §0.1 MVP capability matrix, §3 normalization/fidelity rules, §4.1 providers, §5 workflow, §7 Phase 1. Follow-ups: [docs/FOLLOWUPS.md](../../FOLLOWUPS.md) "Track for Phase 1".

## Global Constraints

All Phase 0 constraints still apply. See `docs/superpowers/plans/2026-11-13-phase0-foundations.md` § Global Constraints:
- Apache-2.0 licensing; permissive dependencies only; `cargo deny`.
- 3-OS CI.
- One `prism` binary.
- `127.0.0.1`-only server.
- LF line endings.
- Iterative tree code.
- `--home` / `PRISM_HOME`.

Phase 1 adds the following.

**Targets and versions**
- Minimum target versions: **Go 1.22+**, **.NET 8+** (PLAN.md §9).
- All Phase 1 kinds work **without admin/root**: the target runs as the same user, and no eBPF, ETW kernel sessions or perf are needed. Block I/O stays in Phase 2.

**Licensing of the new components**
- Go agent and helper dependencies must be permissive:
  - Go: `fgprof` (MIT) plus the standard library only.
  - .NET: `Microsoft.Diagnostics.NETCore.Client` and `Microsoft.Diagnostics.Tracing.TraceEvent` (MIT). Test-only: `xunit` (Apache-2.0).
  - Nothing else without a recorded ruling.

**Safety and behaviour**
- Capture must leave the target as it found it. Any sampling rate prism raises (Go mutex/block rates) is restored when capture ends, including on error or Ctrl-C.
- Record duration: default **30 s**, minimum **1 s**, maximum **10 min**. Format `<n>ms|s|m` (e.g. `500ms`, `30s`, `2m`).
- Fidelity values follow PLAN.md §0.1 exactly:

  | Kind | Go | .NET |
  |---|---|---|
  | cpu | `native` | `approximate` |
  | wall | `derived` (fgprof) | `native` |
  | memory | `native` | `native` |
  | lock | `native` | `native` |
  | blocking | `native` (Go only) | — |
  | threads | `native` | `native` |

- The kind vocabulary is unchanged. Go capture produces `blocking` from the block profile and `lock` from the mutex profile. .NET produces no `blocking` profile in Phase 1.

**Helper protocol**
- `prism-dotnet` speaks a JSON-lines protocol on stdout. The first line is `{"type":"hello","protocol":1,"helper_version":"<semver>"}`. Prism refuses any other protocol number and says how to update.
- Helper location order:
  1. `PRISM_DOTNET` environment variable (path to an executable, or to a `.dll` run with `dotnet`).
  2. The sibling of `std::env::current_exe()` named `prism-dotnet[.exe]`.
  3. Otherwise, an error that tells the user how to build the helper.

**Go agent discovery**
- The agent writes `<temp>/prism-agents/<pid>.json` containing `{"pid":N,"addr":"127.0.0.1:PORT","go_version":"...","protocol":1}`. `<temp>` is Go's `os.TempDir()` and Rust's `std::env::temp_dir()`.
- The agent removes the file on clean exit. Prism ignores stale files whose pid isn't alive or whose address doesn't answer.

**Exit criteria (PLAN.md §7 Phase 1):** the same `prism record` UX works for Go and .NET on all three OSes, and the §0.1 matrix is green except Block I/O. This is proven by the gated end-to-end tests in Task 12 passing in CI on `ubuntu-latest`, `macos-latest` and `windows-latest`.

## Review Focus

1. **The target exits mid-capture** (a launched program finishes in 2 s of a 30 s record). The session is stored as `partial` with whatever was captured, and the CLI says so. No hang, no lost data. Tests: Tasks 4, 6 and 8.
2. **Ctrl-C during `prism record`.** The .NET EventPipe session stops cleanly, Go rates are reverted, partial artifacts are imported, the session is `cancelled`, and the exit code is 130. Tests: Tasks 4 and 9.
3. **The .NET target isn't reachable** (`DOTNET_EnableDiagnostics=0`, another user, mismatched `TMPDIR` on Linux/macOS, or a pid that isn't .NET). The error names the cause and a fix, never a raw exception. Tests: Tasks 7 and 8.
4. **A Go target without the agent, with mutex/block rates at zero.** That capture produces an empty profile, which is imported with the warning `mutex profiling is disabled in the target (call runtime.SetMutexProfileFraction or use the prism agent)`. It is never silently empty. Test: Task 6.
5. **Diffing incompatible profiles** (no shared sample type name/unit, e.g. CPU vs heap) gives a clear 400 / CLI error. Diffing a profile against itself gives all-zero deltas. Tests: Tasks 3 and 10.

---

## Prerequisite (not a task)

Add a GitHub remote and push `main` so the Phase 0 CI matrix and `cargo deny` actually run (FOLLOWUPS.md). Phase 1 CI changes build on a known-green baseline. **The user does this.** Executors don't push.

## File Structure

```
crates/
  prism-model/src/lib.rs           + Fidelity Display/parse helpers (no schema change)
  prism-store/src/lib.rs           schema v2 (sessions, fidelity, session_id), migration, rm, blob repair
  prism-analysis/src/diff.rs       diff_flame, diff_top
  prism-analysis/src/tree.rs       max_depth cap in flame/inverted_flame
  prism-capture/                   NEW crate
    src/lib.rs                     re-exports
    src/types.rs                   Target, RuntimeInfo, Capability, CapturePlan, Artifact, CaptureOutcome
    src/duration.rs                parse_duration
    src/cancel.rs                  CancelToken (Ctrl-C aware)
    src/provider.rs                Provider trait, ProviderRegistry
    src/session.rs                 record(): orchestration + store import
    src/go/mod.rs                  GoProvider
    src/go/discovery.rs            agent discovery files
    src/go/http.rs                 endpoint capture (ureq)
    src/go/gotest.rs               `go test` launch mode
    src/dotnet/mod.rs              DotnetProvider
    src/dotnet/helper.rs           helper location + JSON-lines client
    tests/                         fake_provider.rs, go_http.rs, dotnet_protocol.rs, e2e.rs (gated)
    src/bin/fake-dotnet-helper.rs  test-only protocol fake (behind feature "test-helpers")
  prism-server/src/api.rs          sessions, diff, delete endpoints; depth param
  prism-cli/src/cmd/{record,targets,sessions,diff,rm}.rs
agents/go/                         NEW Go module  prism.dev/agent (consumed via `replace` directives until published)
  agent.go, wall.go, rates.go, discovery.go, agent_test.go, go.mod, go.sum
helpers/prism-dotnet/              NEW C# solution
  PrismDotnet.csproj, Program.cs, Protocol.cs, Collect.cs, Launch.cs, Convert/*.cs, Pprof/PprofWriter.cs
  tests/PrismDotnet.Tests.csproj, tests/*.cs
ui/src/
  api.ts, url.ts                   sessions, diff, delete, fidelity, depth
  views/SessionList.tsx, views/DiffView.tsx, flame/diffColor.ts (+ tests)
examples/go-demo/main.go           uses the agent
examples/dotnet-demo/              README: `prism record` instead of dotnet-trace
docs/USER_GUIDE.md                 Phase 1 sections
.github/workflows/ci.yml           setup-go, setup-dotnet, helper build/test, agent test, gated e2e
```

---

### Task 1: Store schema v2 — sessions, fidelity, delete, blob repair

**Files:** Modify `crates/prism-store/src/lib.rs`. Test: `crates/prism-store/tests/store.rs`, `tests/migration.rs`.

**Interfaces:**
- Consumes: `prism_model::{Fidelity, ProfileKind}` (Fidelity already serde snake_case).
- Produces:
  - `ImportOptions` gains `pub fidelity_override: Option<Fidelity>` and `pub session_id: Option<String>`.
  - `ProfileRecord` gains `pub fidelity: Fidelity` and `pub session_id: Option<String>`. Serialized names `fidelity`, `session_id`.
  - `pub enum SessionStatus { Running, Complete, Partial, Cancelled, Failed }` (serde snake_case).
  - `pub struct TargetInfo { pub runtime: String /*"go"|"dotnet"*/, pub pid: Option<u32>, pub command: Option<String>, pub url: Option<String>, pub runtime_version: Option<String> }` (Serialize/Deserialize).
  - `pub struct SessionRecord { pub id: String, pub short_id: String, pub name: String, pub target: TargetInfo, pub status: SessionStatus, pub started_at_unix_ms: i64, pub duration_ms: Option<i64>, pub requested_kinds: Vec<ProfileKind>, pub warnings: Vec<String>, pub tags: Vec<String>, pub profile_ids: Vec<String> }`.
  - `Store::create_session(&self, name: &str, target: TargetInfo, kinds: &[ProfileKind], tags: &[String]) -> Result<SessionRecord, StoreError>`. The id is 32 lowercase hex characters built from 16 random bytes (`getrandom`). Status is `Running`.
  - `Store::finish_session(&self, id: &str, status: SessionStatus, duration_ms: i64, warnings: &[String]) -> Result<SessionRecord, StoreError>`.
  - `Store::list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError>` (newest first) and `Store::resolve_session(&self, reference: &str) -> Result<SessionRecord, StoreError>`. Resolution follows the profile rules: `last`, full id, hex prefix of 4+ characters, exact name.
  - `Store::remove_profile(&self, id: &str) -> Result<(), StoreError>`: deletes the row and the blob. A missing blob is OK.
  - `Store::remove_session(&self, id: &str) -> Result<usize /*profiles removed*/, StoreError>`.
- Schema v2 (migration from v1 inside one transaction, or a fresh create):
  - `ALTER TABLE profiles ADD COLUMN fidelity TEXT NOT NULL DEFAULT 'unknown'`
  - `ALTER TABLE profiles ADD COLUMN session_id TEXT NULL`
  - `CREATE TABLE sessions (id TEXT PRIMARY KEY, short_id, name, target TEXT /*json*/, status, started_at_unix_ms, duration_ms NULL, requested_kinds TEXT, warnings TEXT, tags TEXT)`
  - `PRAGMA user_version = 2`
  - Version > 2 → `Corrupt` (same message shape as today).
  - Run the version gate **before** the WAL pragma (deferred Phase 0 minor).
- Fidelity at import: `fidelity_override`, otherwise the profile's `meta.fidelity`.
- Re-importing bytes whose id already exists still returns the existing record and `false`. **Blob repair:** if the existing blob is missing or fails to decode, rewrite it from the new bytes before returning. Also `fsync` (`File::sync_all`) the temp blob before renaming (deferred Phase 0 minor).

- [ ] **Step 1: Write failing tests.**
  - `migrates_v1_db_to_v2`: build a v1 database with the Phase 0 DDL and one row (copy the DDL into the test as a string). Open it with the new `Store`, then check `user_version == 2`, the old row reads back with `fidelity: Unknown` and `session_id: None`, and a new import works.
  - `session_lifecycle`: create a session, import two profiles with that `session_id`, finish it as `Partial` with one warning, then `resolve_session("last")` returns `profile_ids.len() == 2`, `status == Partial`, and the warning.
  - `fidelity_override_persists`.
  - `remove_profile_deletes_row_and_blob`.
  - `remove_session_cascades`: returns 2, and both profiles are gone.
  - `reimport_repairs_corrupt_blob`: corrupt the blob, re-import the same bytes (still `false`), and `load` now succeeds.
  - `newer_schema_rejected_before_wal`: `user_version = 3`, then `Corrupt`, and `PRAGMA journal_mode` on the raw connection is still `delete`.
- [ ] **Step 2:** `cargo test -p prism-store`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`. Expected: PASS. Server and CLI compile because they only read the new fields.
- [ ] **Step 5: Commit:** `feat(store): schema v2 with sessions, fidelity, delete and blob repair`

---

### Task 2: Analysis — flame depth cap

**Files:** Modify `crates/prism-analysis/src/tree.rs`, `lib.rs`. Test: `tests/analysis.rs`.

**Interfaces:**
- Produces:
  - `FlameNode` gains `pub truncated: bool`, serialized as `truncated` and **omitted when false** (`skip_serializing_if`) so Phase 0 JSON is unchanged.
  - `pub struct FlameOptions { pub min_fraction: f64, pub max_depth: Option<usize> }` with `Default` = `{ min_fraction: 0.0005, max_depth: None }`.
  - `pub fn flame_with(p: &Profile, st: usize, filter: &Filter, opts: &FlameOptions) -> FlameNode` and `pub fn inverted_flame_with(...)`.
  - The existing `flame` / `inverted_flame` keep their signatures and delegate with `max_depth: None`.
- Rule: a node at depth `max_depth` (root = 0) keeps its `total` and `self_value`. It loses its children and gets `truncated = true`. Its self value becomes `total` (all cost below the cap folds into it). This applies to both the fast and slow paths.

- [ ] **Step 1: Write failing tests.**
  - `max_depth_truncates_and_folds`: samples `main;a;b;c=4` and `main;d=1` with `max_depth: Some(2)` → `a` has no children, `truncated == true`, `self_value == 4`, `total == 4`; root total is 5.
  - `max_depth_none_unchanged`: the result equals `flame(...)`.
  - `truncated_omitted_in_json_when_false`: `serde_json` output of an untruncated tree contains no `"truncated"`.
  - The fast and slow paths give equal results under `max_depth: Some(1)` (extend the existing equivalence test).
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** `cargo test -p prism-analysis` PASS.
- [ ] **Step 5: Commit:** `feat(analysis): flame depth cap with truncation marker`

---

### Task 3: Analysis — diff

**Files:** Create `crates/prism-analysis/src/diff.rs`; modify `lib.rs`. Test: `tests/diff.rs`.

**Interfaces:**
- Produces:
  - `pub enum DiffNormalize { None, Total }` (serde snake_case). `Total` scales base values by `head_total / base_total` (0 when `base_total == 0`), then rounds to i64.
  - `pub struct DiffNode { pub name: String, pub file: String, pub base_self: i64, pub base_total: i64, pub head_self: i64, pub head_total: i64, pub children: Vec<DiffNode> }`. Serialized camelCase. It has an iterative `Drop` and a stack-safe `Serialize`, like `FlameNode` (R3).
  - `pub struct DiffRow { pub name: String, pub file: String, pub base_self: i64, pub head_self: i64, pub delta_self: i64, pub base_total: i64, pub head_total: i64, pub delta_total: i64 }`.
  - `pub struct DiffInput<'a> { pub profile: &'a Profile, pub sample_type: usize }`.
  - `pub enum DiffError { NoCommonSampleType { base: Vec<String>, head: Vec<String> } }`.
  - `pub fn match_sample_type(base: &Profile, head: &Profile, preferred: Option<&str>) -> Result<(usize, usize), DiffError>`. Matches on equal `(name, unit)`. If `preferred` is given and present in both, use it. Otherwise use the base's default type if head has it. Otherwise the first common type. Otherwise the error.
  - `pub fn diff_flame(base: DiffInput, head: DiffInput, filter: &Filter, normalize: DiffNormalize, min_fraction: f64, max_depth: Option<usize>) -> DiffNode`. The two trees merge by **(function name, file)**, not FuncId. A child is pruned when `max(base_total, head_total) < min_fraction * max(root totals)`. Children are sorted by `max(head_total, base_total)` desc, then name, then file.
  - `pub fn diff_top(base: DiffInput, head: DiffInput, filter: &Filter, normalize: DiffNormalize, limit: usize) -> Vec<DiffRow>`. Sorted by `|delta_self|` desc, then `|delta_total|` desc, then name. Recursion counts once per sample in totals, as in `top`.
  - Both use saturating arithmetic.

- [ ] **Step 1: Write failing tests.**
  - `diff_identical_is_zero`: diff of a profile with itself → every `DiffRow` has zero deltas, and the tree has equal base and head everywhere.
  - `diff_detects_regression`: base `main;a=5`, `main;b=5`; head `main;a=5`, `main;b=15` → the top row is `b` with `delta_self == 10`; `diff_flame` gives root `base_total 10`, `head_total 20`.
  - `diff_merges_by_name_not_id`: head built with functions in a different order (different FuncIds) → same tree shape as base.
  - `diff_function_only_in_head`: base has 0.
  - `normalize_total_scales_base`: base total 10, head total 20 → normalized base values double.
  - `no_common_sample_type_errors`: cpu-only vs `inuse_space`-only → `NoCommonSampleType` listing both.
  - `preferred_sample_type_wins`.
  - `deep_diff_serializes`: 10 000 deep.
- [ ] **Step 2:** FAIL. **Step 3:** implement. Reuse `Filter` per sample and the per-FuncId match cache from `filter.rs`; don't duplicate the filter logic. **Step 4:** PASS.
- [ ] **Step 5: Commit:** `feat(analysis): profile diff (flame and top)`

---

### Task 4: `prism-capture` core — types, provider trait, session orchestration

**Files:** Create `crates/prism-capture/{Cargo.toml, src/lib.rs, types.rs, duration.rs, cancel.rs, provider.rs, session.rs}`; add the crate to the workspace. Test: `crates/prism-capture/tests/fake_provider.rs`.

**Interfaces:**
- Consumes: `prism_store::{Store, ImportOptions, TargetInfo, SessionStatus, SessionRecord}`, `prism_model::{ProfileKind, Fidelity}`.
- Produces:
  - `pub enum Target { Pid(u32), Url(String), Launch { program: String, args: Vec<String>, cwd: Option<PathBuf> }, GoTest { package: String, args: Vec<String> } }`.
  - `pub struct RuntimeInfo { pub provider: &'static str, pub pid: Option<u32>, pub url: Option<String>, pub runtime_version: Option<String>, pub command: Option<String> }`.
  - `pub struct Capability { pub kind: ProfileKind, pub fidelity: Fidelity, pub available: bool, pub note: Option<String> }`.
  - `pub struct CapturePlan { pub kinds: Vec<ProfileKind>, pub duration: Duration }`.
  - `pub struct Artifact { pub kind: ProfileKind, pub fidelity: Fidelity, pub file_name: String, pub bytes: Vec<u8>, pub warning: Option<String> }`.
  - `pub enum CaptureEnd { Complete, TargetExited, Cancelled }`.
  - `pub struct CaptureOutcome { pub artifacts: Vec<Artifact>, pub end: CaptureEnd, pub warnings: Vec<String> }`.
  - `pub enum CaptureError { NotDetected(String), Unsupported { kind: ProfileKind, reason: String }, Target(String), Helper(String), Io(std::io::Error), Store(prism_store::StoreError) }`, with `thiserror` and human messages.
  - `pub fn parse_duration(s: &str) -> Result<Duration, String>`, following the Global Constraints format and limits.
  - `#[derive(Clone)] pub struct CancelToken`, with `new()`, `cancel()`, `is_cancelled()`, `wait_timeout(Duration) -> bool` (Condvar-based; returns true if cancelled), and `CancelToken::install_ctrlc() -> CancelToken` (`ctrlc` crate, MIT/Apache).
  - `pub trait Provider: Send + Sync`:
    - `fn id(&self) -> &'static str`
    - `fn detect(&self, target: &Target) -> Result<Option<RuntimeInfo>, CaptureError>`
    - `fn capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability>`
    - `fn capture(&self, rt: &RuntimeInfo, target: &Target, plan: &CapturePlan, cancel: &CancelToken, progress: &dyn Fn(&str)) -> Result<CaptureOutcome, CaptureError>`
  - `pub struct ProviderRegistry`, with `with_defaults() -> Self` (Go + .NET, added in Tasks 6 and 8; empty until then), `register(Box<dyn Provider>)`, and `detect(&self, target) -> Result<(&dyn Provider, RuntimeInfo), CaptureError>`. Detection goes in registration order and the first `Some` wins; none → `NotDetected` listing what was tried.
  - `pub struct RecordRequest { pub target: Target, pub plan: CapturePlan, pub name: Option<String>, pub tags: Vec<String> }`.
  - `pub fn record(store: &Store, registry: &ProviderRegistry, req: &RecordRequest, cancel: &CancelToken, progress: &dyn Fn(&str)) -> Result<SessionRecord, CaptureError>`. Sequence:
    1. Detect.
    2. Requested kinds that are unavailable → `Unsupported` before anything starts. An empty `plan.kinds` means "all available kinds".
    3. `create_session` (default name `<provider>-<pid or program file stem>`).
    4. `capture`.
    5. Import each artifact with `kind_override`, `fidelity_override`, `session_id` and `file_name`.
    6. Status: Complete → `Complete`; TargetExited → `Partial`; Cancelled → `Cancelled`. Any artifact warning or outcome warning goes into the session warnings.
    7. If capture returns `Err` after the session was created: `finish_session(Failed)` with the error text, then return the error.

- [ ] **Step 1: Write failing tests** with an in-test `FakeProvider` whose behaviour is scripted per test:
  - `parse_duration_cases`: `"500ms"` is ok but below the minimum, so `Err`; `"1s"` → 1 s; `"30s"`; `"2m"`; `"11m"` → `Err`; `"abc"` → `Err`.
  - `record_complete_imports_artifacts`: two pprof artifacts (use `crates/prism-import/tests/fixtures/go-cpu.pb.gz` and `go-heap.pb.gz`) → status `Complete`, 2 profiles with the given kinds and fidelities and the `session_id`.
  - `record_target_exit_is_partial`: the fake returns `TargetExited` with one artifact.
  - `record_cancel_is_cancelled`: the fake waits on the token and returns `Cancelled` with a partial artifact when it fires. The test cancels after 100 ms. The artifact is imported.
  - `unsupported_kind_fails_before_session`: no session is created.
  - `capture_error_marks_failed`.
  - `empty_kinds_means_all_available`.
  - `cancel_token_wait_timeout`.
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** `cargo test -p prism-capture` PASS.
- [ ] **Step 5: Commit:** `feat(capture): provider trait, cancel token, session orchestration`

---

### Task 5: Go agent package

**Files:** Create `agents/go/{go.mod, agent.go, wall.go, rates.go, discovery.go, agent_test.go}`. Module path `prism.dev/agent`, with `go 1.22` and `require github.com/felixge/fgprof`.

**Interfaces (Go):**
- `type Options struct { Addr string /* default "127.0.0.1:0" */; DisableDiscovery bool }`.
- `func Start(opts Options) (*Agent, error)`: starts an `http.Server` on `Addr` (loopback only; a non-loopback host returns an error).
- `func (a *Agent) Addr() string` and `func (a *Agent) Close() error` (Close removes the discovery file).
- Routes, all GET:
  - The standard `/debug/pprof/*`, registered on the agent's own mux (`net/http/pprof` handlers, not `DefaultServeMux`).
  - `/debug/prism/info`: JSON `{"protocol":1,"pid":N,"go_version":"..."}`.
  - `/debug/prism/wall?seconds=N`: fgprof pprof output.
  - `/debug/prism/contention?seconds=N&kind=mutex|block`: raises `SetMutexProfileFraction(5)` or `SetBlockProfileRate(10_000 /*ns*/)` (remembering the previous value), sleeps N seconds or until the request context is cancelled, writes a **delta** profile (Lookup before and after, then subtract), and **restores the previous rate in a `defer`**.
- Discovery file per Global Constraints: written with 0600 permissions, removed on `Close`, and also removed on `SIGINT`/`SIGTERM` via a `signal.NotifyContext` hook that `Start` installs only if `PRISM_AGENT_SIGNALS=1` (the agent must not steal the app's signal handling by default).
- Auto-start: the package's `init()` calls `Start(Options{})` **only** when the env var `PRISM_AGENT=1` is set. This is used by prism's launch mode (Task 6). Importing the package with a blank `_` import and no env var does nothing.
- Extra test: `TestAutoStartOnlyWithEnv` runs a subprocess with and without `PRISM_AGENT=1` and checks that the discovery file appears only with it.

- [ ] **Step 1: Write failing tests (`go test ./...`).**
  - `TestStartWritesDiscoveryAndCloseRemoves`.
  - `TestRejectsNonLoopbackAddr`.
  - `TestInfoEndpoint`.
  - `TestWallReturnsPprof`: `seconds=1`; the body is gzip and decodes with `github.com/google/pprof/profile` (test-only dependency, Apache-2.0).
  - `TestContentionRestoresRates`: set the mutex fraction to 0, call `/debug/prism/contention?seconds=1&kind=mutex`, and `runtime.SetMutexProfileFraction(-1)` reads back 0 afterwards. Run with a goroutine contending a mutex, so the profile has ≥ 1 sample.
  - `TestContentionCancelledRequestRestores`: cancel the client context after 200 ms → the rate is restored.
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** `cd agents/go && go vet ./... && go test ./...` PASS.
- [ ] **Step 5: Commit:** `feat(agent-go): prism Go agent with wall, contention and discovery`

---

### Task 6: Go provider

**Files:** Create `crates/prism-capture/src/go/{mod.rs, discovery.rs, http.rs, gotest.rs}`. Test: `crates/prism-capture/tests/go_http.rs`, plus a test helper program `crates/prism-capture/tests/goapp/main.go` (uses the agent via a `replace prism.dev/agent => ../../../../agents/go` directive).

**Interfaces:**
- Consumes: Task 4 types; Task 5 HTTP contract.
- Produces: `pub struct GoProvider` implementing `Provider` with id `"go"`.
  - **`detect`:**
    - `Url(u)` → GET `u + "/debug/prism/info"`. If that 404s, try `u + "/debug/pprof/"`; success means Go (`runtime_version: None`, "agent: no").
    - `Pid(p)` → discovery file `<temp>/prism-agents/<p>.json` → URL, verified via `/debug/prism/info`. With no discovery file, return `Ok(None)`. If the pid's executable contains the Go buildinfo magic `\xff Go buildinf:`, set the registry's `NotDetected` hint: "Go process without the prism agent: start the agent or pass --url to its net/http/pprof endpoint".
    - `Launch` → `Ok(None)` (Go launch is via `GoTest`, or via `Launch` once the program embeds the agent — see capture).
    - `GoTest` → `Ok(Some)` if `go` is on PATH.
  - **`capabilities`:**
    - Agent present: cpu, wall, memory, lock, blocking and threads, all available with the fidelities from Global Constraints.
    - No agent (plain pprof): wall unavailable (note "needs prism agent").
    - Lock and blocking available but with the note "depends on target's runtime.SetMutexProfileFraction / SetBlockProfileRate".
  - **`capture` over HTTP:** run all kinds **concurrently**, one thread each, with `ureq` default features (rustls), so both `http://` and `https://` URLs work. Endpoints:

    | Kind | Endpoint |
    |---|---|
    | cpu | `/debug/pprof/profile?seconds=D` |
    | wall | `/debug/prism/wall?seconds=D` |
    | lock | `/debug/prism/contention?seconds=D&kind=mutex` (agent), else `/debug/pprof/mutex?seconds=D` |
    | blocking | `/debug/prism/contention?seconds=D&kind=block` (agent), else `/debug/pprof/block?seconds=D` |
    | memory | `/debug/pprof/heap`, fetched at the end of the window |
    | threads | `/debug/pprof/goroutine`, fetched at the end of the window |

    - `D` is whole seconds, rounded up.
    - Request timeout is `D + 15s`.
    - Cancellation: close the in-flight requests (drop the agents — the agent restores rates on context cancel) and return `Cancelled` with the completed artifacts.
    - A connection refused mid-capture → `TargetExited`.
    - An **empty** lock/blocking profile (0 samples) from plain pprof → keep the artifact and attach the Review Focus #4 warning.
  - **`capture` with `GoTest`:** run `go test <package> <args> -run <args or .> -cpuprofile T/cpu.pprof -memprofile T/heap.pprof -mutexprofile T/mutex.pprof -blockprofile T/block.pprof -o T/test.bin` in a temp dir. The requested kinds pick which flags are passed. wall and threads are unavailable for GoTest. Collect the files as artifacts. Duration is not applicable: the test decides; warn if `--duration` was given.
  - **`capture` with `Launch` and an agent-enabled program:** spawn it with `PRISM_AGENT=1` in the env (agent auto-start, Task 5), wait up to 10 s for `<temp>/prism-agents/<childpid>.json`, then capture as with a URL. No discovery file within 10 s → `CaptureError::Target("program did not start the prism agent; import prism.dev/agent")`. Kill the child after capture unless it already exited.

- [ ] **Step 1: Write failing tests.** In-process tests run a mock HTTP server on loopback (`tiny_http`, MIT/Apache, dev-dependency) serving the fixture pprof bytes:
  - `detect_by_url_with_agent`.
  - `detect_plain_pprof_marks_wall_unavailable`.
  - `capture_all_kinds_concurrently`: assert the total wall time is < 2× D, which proves the kinds ran concurrently.
  - `empty_mutex_without_agent_warns`.
  - `connection_refused_midway_is_target_exited`.
  - `cancel_returns_partial`.
  - `discovery_stale_file_ignored`: pid not alive.
  - Gated real-Go tests (`#[ignore]` unless `PRISM_E2E=1` and `go` is on PATH), using `tests/goapp`:
    - `go_attach_by_pid_via_discovery`.
    - `go_launch_with_agent`.
    - `go_test_mode_produces_cpu_and_heap`.
- [ ] **Step 2:** FAIL. **Step 3:** implement. Add `GoProvider` to `ProviderRegistry::with_defaults()`. **Step 4:** `cargo test -p prism-capture` PASS; `PRISM_E2E=1 cargo test -p prism-capture -- --ignored go_` PASS locally.
- [ ] **Step 5: Commit:** `feat(capture): Go provider (agent, net/http/pprof, go test)`

---

### Task 7: `prism-dotnet` helper (C#)

**Files:** Create `helpers/prism-dotnet/{PrismDotnet.csproj, Program.cs, Protocol.cs, ProcessList.cs, Collect.cs, Launch.cs, Convert/TraceConverter.cs, Pprof/PprofWriter.cs}` and `helpers/prism-dotnet/tests/{PrismDotnet.Tests.csproj, PprofWriterTests.cs, ConverterTests.cs, CollectSmokeTests.cs}`.

**Interfaces (CLI + protocol, protocol 1):**
- Project: `net8.0`, `RollForward=Major`, `Nullable=enable`, `InvariantGlobalization=true`. Package refs: `Microsoft.Diagnostics.NETCore.Client` and `Microsoft.Diagnostics.Tracing.TraceEvent`, latest stable versions that support net8.0, pinned exactly. Publish: `-p:PublishSingleFile=true --self-contained -r <rid>`.
- Every command writes the `hello` line first, then events, one JSON object per line:
  - `{"type":"progress","message":...}`
  - `{"type":"artifact","kind":"cpu|wall|memory|lock|threads","path":"<abs>","samples":N}`
  - `{"type":"warning","message":...}`
  - `{"type":"error","code":"not_found|diagnostics_disabled|permission|not_dotnet|launch_failed|internal","message":...}`
  - Final `{"type":"done","end":"complete|target_exited|cancelled"}`.
  - Exit code 0 when `done` was written, 1 after `error`.
- Commands:
  - `list` → one `{"type":"process","pid":N,"name":...,"command_line":...,"runtime_version":...}` per process in `DiagnosticsClient.GetPublishedProcesses()` (best-effort `GetProcessInfo`).
  - `collect --pid N --duration-ms D --kinds cpu,wall,memory,lock,threads --out DIR`:
    - One EventPipe session with providers `Microsoft-DotNETCore-SampleProfiler` and `Microsoft-Windows-DotNETRuntime`. Keywords: GC `0x1` at **Verbose** (for `GCAllocationTick`), Contention `0x4000`, Loader `0x8`, Jit `0x10`, plus rundown — `requestRundown: true`.
    - Write `DIR/trace.nettrace`, convert (below), and emit the artifacts.
    - Stdin closing, or a line `stop` on stdin, ends the session early → `end: cancelled`.
    - The target exiting ends it → `end: target_exited`.
  - `launch --kinds ... --duration-ms D --out DIR -- <program> <args...>`:
    - Start the program with `DOTNET_DiagnosticPorts=<unique port>,suspend` and accept it with `ReversedDiagnosticsServer`.
    - Start the session, then `ResumeRuntime`, then proceed as `collect`.
    - The child is killed at the end if still running.
  - `convert --in FILE.nettrace --out DIR --kinds ...` → artifacts only (used by `prism import x.nettrace`).
- Conversion (`TraceConverter`, using `TraceLog` + `TraceCallStack` for managed frames; unresolved frames `[unknown]`; modules as `module!Method`):

  | Kind | Source | Sample type | Fidelity |
  |---|---|---|---|
  | wall | every SampleProfiler `ThreadSample`, weight = sampling interval (1 ms default) | `wall/nanoseconds` | native |
  | cpu | SampleProfiler samples whose type is `Managed` (running managed code) | `cpu/nanoseconds` | approximate (set by the provider) |
  | memory | `GCAllocationTick` | `alloc_space/bytes` (`AllocationAmount64`) and `alloc_objects/count` (1 per tick); default `alloc_space` | native |
  | lock | `ContentionStop.DurationNs` (.NET 8+), stack from the matching `ContentionStart` on the same thread | `contentions/count` and `delay/nanoseconds` | native |
  | threads | the **last** sample per thread in the window, count = 1, label `thread=<id>` | `threads/count` | native |

  - .NET 8 runtimes without `DurationNs`: pair Start/Stop timestamps instead.
- `PprofWriter`: a hand-written minimal protobuf encoder for the pprof `Profile` message (varints, length-delimited, packed repeated). It writes gzip `.pb.gz`, with string table index 0 = `""`. No protobuf package dependency.

- [ ] **Step 1: Write failing tests (`dotnet test`).**
  - `PprofWriterTests.RoundTripsThroughGooglePprofSchema`: write a 2-sample profile, gunzip, and parse the bytes with a hand-written test decoder → string table, sample values and location ids are correct.
  - `ConverterTests`: run a tiny in-test workload (spin + lock + allocate) for 2 s under `collect` on its **own** process (`DiagnosticsClient(Environment.ProcessId)`), convert, and check: wall > 0 samples; memory has frames containing the test's allocation method; lock has ≥ 1 sample; threads ≥ 2.
  - `CollectSmokeTests.UnknownPidGivesNotFoundError`: the output contains `"code":"not_found"`.
  - `ProtocolTests.FirstLineIsHello`.
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** `cd helpers/prism-dotnet && dotnet test` PASS; `dotnet publish -c Release -r win-x64 --self-contained -p:PublishSingleFile=true` succeeds.
- [ ] **Step 5: Commit:** `feat(dotnet-helper): EventPipe collect/launch/convert to pprof`

---

### Task 8: .NET provider + `.nettrace` import

**Files:** Create `crates/prism-capture/src/dotnet/{mod.rs, helper.rs}` and `crates/prism-capture/src/bin/fake-dotnet-helper.rs` (`required-features = ["test-helpers"]`). Modify `crates/prism-cli/src/cmd/import.rs` (`.nettrace`). Test: `crates/prism-capture/tests/dotnet_protocol.rs`.

**Interfaces:**
- Produces:
  - `pub struct HelperPath { pub program: PathBuf, pub args_prefix: Vec<String> }` (e.g. `dotnet` + `[path.dll]`) and `pub fn locate_helper() -> Result<HelperPath, CaptureError>`, following the Global Constraints order.
  - `pub enum HelperEvent { Hello { protocol: u32, helper_version: String }, Process { pid: u32, name: String, command_line: Option<String>, runtime_version: Option<String> }, Progress(String), Artifact { kind: ProfileKind, path: PathBuf, samples: u64 }, Warning(String), Error { code: String, message: String }, Done { end: CaptureEnd } }`.
  - `pub fn run_helper(h: &HelperPath, args: &[String], cancel: &CancelToken, on_event: &mut dyn FnMut(HelperEvent)) -> Result<(), CaptureError>`:
    - Spawns the helper with piped stdin and stdout and reads lines.
    - The first line must be `Hello` with protocol 1, else `Helper("prism-dotnet protocol X is not supported; rebuild the helper")`.
    - On cancel, write `stop\n` to stdin, then wait up to 10 s, then kill.
    - Malformed lines → `Helper` error that includes the line.
  - `pub fn list_dotnet_processes() -> Result<Vec<RuntimeInfo>, CaptureError>`.
  - `pub struct DotnetProvider` with id `"dotnet"`:
    - **detect:** `Pid` → in `list` output; `Launch` where the program is `dotnet`, ends with `.dll`, or a `.runtimeconfig.json` sits beside it → `Some`.
    - **capabilities:** cpu, wall, memory, lock, threads with the Global Constraints fidelities. blocking is unavailable ("not in Phase 1 for .NET").
    - **capture:** `collect` or `launch` into a temp dir; read the artifact files into `Artifact`s (fidelity from the table; **cpu = approximate**); map helper error codes to messages per Review Focus #3:
      - `diagnostics_disabled` → "the target has DOTNET_EnableDiagnostics=0"
      - `permission` → "the target runs as another user, or TMPDIR differs (Linux/macOS: run prism with the same TMPDIR as the target)"
      - `not_dotnet` → "pid N is not a .NET 8+ process"
  - `prism import <file>.nettrace` runs the helper `convert` with all kinds and imports each artifact (no session; tag `nettrace`). Without a helper → a clear error.
- The `fake-dotnet-helper` reads `FAKE_SCRIPT=<path to a JSON-lines file>` and replays it. It honours `stop` on stdin by emitting `done cancelled`. It copies fixture pprof files to the requested `--out` paths.

- [ ] **Step 1: Write failing tests**, using `PRISM_DOTNET=<fake helper path>` (built by `cargo test --features test-helpers`):
  - `hello_protocol_mismatch_errors`.
  - `collect_imports_artifacts_with_fidelity`: the cpu fidelity is approximate.
  - `error_codes_map_to_messages`: one case per code.
  - `stop_on_cancel`.
  - `target_exited_is_partial`.
  - `malformed_line_errors`.
  - `locate_helper_env_then_sibling`.
  - Gated (`PRISM_E2E=1`, `dotnet` on PATH, `PRISM_DOTNET` pointing at the real helper):
    - `dotnet_launch_demo_all_kinds`: `examples/dotnet-demo all 5`.
    - `dotnet_attach_by_pid`.
- [ ] **Step 2:** FAIL. **Step 3:** implement. Add the provider to `with_defaults()` after Go. **Step 4:** PASS (gated tests locally).
- [ ] **Step 5: Commit:** `feat(capture): .NET provider via prism-dotnet; .nettrace import`

---

### Task 9: CLI — record, targets, sessions, diff, rm

**Files:** Create `crates/prism-cli/src/cmd/{record.rs, targets.rs, sessions.rs, diff.rs, rm.rs}`; modify `main.rs`. Test: `crates/prism-cli/tests/cli_phase1.rs`.

**Interfaces / UX (exact):**

`prism record [--pid N | --url URL | --go-test PKG [-- ARGS] | -- PROGRAM ARGS...] [--kinds k1,k2] [--duration 30s] [--name N] [--tag T]... [--open] [--port P]`
- Exactly one target form is allowed. Zero or more → exit 2: `error: choose one target: --pid, --url, --go-test or -- <program>`.
- It prints progress lines to stderr (`[go] capturing cpu, wall, memory, lock, blocking, threads for 30s (pid 1234)`), then a summary table to stdout:
  ```
  session 3f9a1c2b7d10  go-1234  complete  30.2s
  ID            KIND      FIDELITY     SAMPLES     TOTAL  NOTE
  984dbaaf5c15  cpu       native          4575    87.3 s
  ```
- Warnings print as `warning: <text>` on stderr.
- `--open` starts the UI on `/?session=<short_id>` after recording.
- Exit codes: 0 for complete; 0 for partial, with the stderr line `note: target exited before the window ended; session is partial`; 130 for cancelled; 1 for failed or any capture error.

`prism targets [--json]` → table `RUNTIME  PID  VERSION  AGENT  COMMAND` from the Go discovery files (verified) plus `list_dotnet_processes()`. If the helper is missing, a stderr warning and Go targets only.

`prism sessions [--json]` → `ID  NAME  STATUS  STARTED (UTC)  DURATION  PROFILES  TARGET`.

`prism diff <BASE> <HEAD> [--st NAME] [--normalize none|total] [--top N=20] [--focus/--exclude/--hide RE]` → header `base: <name> (<total>)  head: <name> (<total>)  sample type: <st>`, then rows `DELTA SELF  BASE SELF  HEAD SELF  DELTA TOTAL  FUNCTION`. Positive deltas get a `+` sign and are formatted with the unit formatter. `NoCommonSampleType` → exit 1 `error: profiles share no sample type (base: a, b; head: c)`.

`prism rm <REF>... [--session]` → removes profiles, or sessions with `--session`. Prints `removed <short_id> <name>`. Unknown ref → error, exit 1, and the others are still removed.

`prism ls` gains a `SESSION` column (short session id or `-`). `prism show` header adds `fidelity: <f>`.

- [ ] **Step 1: Write failing tests** (`assert_cmd`; use `PRISM_DOTNET=fake-dotnet-helper` + `FAKE_SCRIPT` for record):
  - `record_requires_one_target`.
  - `record_with_fake_dotnet_prints_summary`.
  - `record_partial_exit_zero_with_note`.
  - `record_unknown_kind_exit_2`.
  - `sessions_lists_recorded`.
  - `diff_identical_zero_deltas`.
  - `diff_incompatible_errors`: go-cpu vs go-heap.
  - `rm_profile_and_session`.
  - `targets_without_helper_warns`.
  - `ls_has_session_column`.
  - `record_ctrl_c_exit_130`: Unix-only test sending SIGINT to the child; on Windows, cover the same path through `CancelToken` in Task 4's tests.
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** `cargo test -p prism-cli` PASS.
- [ ] **Step 5: Commit:** `feat(cli): record, targets, sessions, diff, rm`

---

### Task 10: Server API — sessions, diff, delete, depth

**Files:** Modify `crates/prism-server/src/api.rs` (and split into `api/{profiles,sessions,diff}.rs` if `api.rs` exceeds ~600 lines). Test: `crates/prism-server/tests/api_phase1.rs`.

**Interfaces:**
- `GET /api/sessions` → `[SessionRecord]`; `GET /api/sessions/{ref}` → `SessionRecord`.
- `DELETE /api/profiles/{ref}` → 204; `DELETE /api/sessions/{ref}` → `{removed: N}`. Both go through the Host/Origin guard. **DELETE requires the header `X-Prism-Request: 1`**, else 403 `forbidden`. A custom header forces a CORS preflight that prism never approves.
- `GET /api/profiles/{ref}/flame` gains `depth` (optional usize, 1..=10 000, default **1000**) → `flame_with` / `inverted_flame_with`.
- `GET /api/diff?base=REF&head=REF&st=NAME&normalize=none|total&view=flame|top&min=&depth=&limit=&focus=&exclude=&hide=`:
  - `view=flame` → `{base: ProfileRecord, head: ProfileRecord, sampleType: {name, unit}, root: DiffNode}`.
  - `view=top` → same, with `rows: [DiffRow]`.
  - `NoCommonSampleType` → 400 `bad_request` with a message listing both type sets.
  - Uses the same LRU cache (2 entries are enough for base + head).
- Profile and session records serialize the new `fidelity` and `session_id` fields (already in the store types).

- [ ] **Step 1: Write failing tests:**
  - `sessions_list_and_get`.
  - `delete_requires_custom_header`: 403 without it, 204 with it, then 404 on GET.
  - `delete_session_removes_profiles`.
  - `flame_depth_param_truncates`: the go-cpu fixture with `depth=2` → some node has `truncated: true`.
  - `flame_depth_out_of_range_400`.
  - `diff_flame_and_top`.
  - `diff_incompatible_400`.
  - `diff_unknown_ref_404`.
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** `cargo test -p prism-server` PASS.
- [ ] **Step 5: Commit:** `feat(server): sessions, diff, delete and depth endpoints`

---

### Task 11: UI — sessions, fidelity badges, diff view, delete, depth notice

**Files:** Modify `ui/src/{api.ts, url.ts, App.tsx, views/ProfileList.tsx, views/ProfileView.tsx, flame/FlameGraph.tsx, flame/layout.ts}`. Create `ui/src/views/{SessionList.tsx, DiffView.tsx}` and `ui/src/flame/diffColor.ts`. Tests: `ui/src/flame/diffColor.test.ts`, `ui/src/url.test.ts` additions, `ui/src/api.test.ts` (new; mocks `fetch`).

**Interfaces:**
- `api.ts` adds:
  - `listSessions()`, `getSession(ref)`
  - `deleteProfile(ref)`, `deleteSession(ref)` — both send `X-Prism-Request: 1`
  - `getDiffFlame(base, head, q)`, `getDiffTop(base, head, q)`
  - Types `SessionRecord`, `DiffNode`, `DiffRow` matching the Rust JSON exactly (`SessionRecord` snake_case, `DiffNode`/`DiffRow` camelCase); `ProfileRecord` gains `fidelity` and `session_id`
  - `getFlame` passes `depth`
- `url.ts`: `ViewState` gains `session?: string`, `base?: string`, `normalize?: 'total'` (omitted when none), and `view` gains `'diff'`. The default-omission rules stay as today.
- `diffColor.ts`: `diffColor(base: number, head: number): string`. Computes `ratio = (head - base) / max(base, head, 1)`, clamped to [-1, 1]. Positive → red scale, negative → green scale, 0 → neutral grey. Colors are CSS custom properties `--diff-up-<0..4>`, `--diff-down-<0..4>`, `--diff-flat`, picked from 5 buckets by `|ratio|`.
- **Home page** has two tabs: **Sessions** (default when any sessions exist) and **Profiles** (Phase 0 list).
  - Sessions table: name, status pill (complete / partial / cancelled / failed), target, started, duration, profile count, warnings icon with tooltip. Expanding a row lists its profiles with kind and fidelity badge; click a profile to open it.
  - Both lists get a delete button with a confirm dialog ("Delete <name>? This removes the profile data and cannot be undone.").
- **Fidelity badge** next to the kind everywhere: `native`, `approximate`, `derived`, `sampled external`, `unknown`. A tooltip gives the meaning per PLAN.md §3.
- **ProfileView** gets a "Compare with…" select (other profiles of the same kind, newest first). Choosing one navigates to `view=diff&base=<that>`.
- **DiffView:**
  - Toolbar with a normalize toggle and the sample type (common types only).
  - A diff flame using the existing canvas renderer: width = `max(head_total, base_total)`, color = `diffColor`; tooltip shows base, head and delta with %.
  - Below it, a "Top changes" table from `diff/top`.
  - A "Swap" button flips base and head.
- **Truncation and canvas safety:**
  - Any node with `truncated` renders with a hatched pattern, and its tooltip says "deeper frames folded (depth limit 1000)".
  - The canvas CSS height is clamped so `height × devicePixelRatio ≤ 16 384`. If the tree is taller, show a notice "Graph clipped; use Focus or zoom to see deeper frames".

- [ ] **Step 1: Write failing tests:**
  - `diffColor`: 10→10 is `var(--diff-flat)`; 10→20 is an up bucket; 20→10 is a down bucket; 0→5 is the max up bucket; 5→0 is the max down bucket.
  - `url`: round-trip `{profile, view:'diff', base, normalize:'total'}`; `normalize` none is omitted.
  - `api`: `deleteProfile` sends the header; non-2xx → `ApiError`.
  - `layout`: depth clamp helper `maxCanvasRows(dpr)` returns `floor(16384 / (18 × dpr))`.
- [ ] **Step 2:** `npm test` FAIL. **Step 3:** implement. **Step 4:** `npm test && npm run build` PASS; `cargo test -p prism-server` still PASS.
- [ ] **Step 5: Manual check** (with `cargo run -p prism-cli -- record --go-test ./...` in `examples/go-demo`, or the gated demos): the sessions tab, badges, compare → diff colors, swap, delete with confirm, the hatched truncated node (record go-demo and set `depth=3` in the URL), and 375 px width.
- [ ] **Step 6: Commit:** `feat(ui): sessions, fidelity badges, diff view, delete, depth notice`

---

### Task 12: Examples, docs, CI and end-to-end acceptance

**Files:**
- Modify:
  - `examples/go-demo/{main.go, go.mod}` — add an optional `-agent` flag (default on) that calls `agent.Start`; `replace prism.dev/agent => ../../agents/go`.
  - `examples/README.md`, `examples/dotnet-demo/` — replace `profile.ps1`/`profile.sh` with `prism record -- dotnet run ...` docs; keep the scripts as a "dotnet-trace alternative".
  - `docs/USER_GUIDE.md` — new sections: Recording (Go, .NET), Sessions, Diff, Deleting, Fidelity badges, Troubleshooting capture (Review Focus #3 causes).
  - `docs/FOLLOWUPS.md` — strike done items.
  - `.github/workflows/ci.yml`
- Create: `crates/prism-capture/tests/e2e.rs` (gated).

**CI (exact):**
- Matrix jobs (3 OS) add `actions/setup-go@v5` (go `1.22.x`) and `actions/setup-dotnet@v4` (`8.0.x`).
- Steps, after the Phase 0 steps:
  1. `cd agents/go && go test ./...`
  2. `cd helpers/prism-dotnet && dotnet test`
  3. `dotnet publish helpers/prism-dotnet -c Release -o target/helper`
  4. `PRISM_DOTNET=target/helper/prism-dotnet[.dll] PRISM_E2E=1 cargo test -p prism-capture --features test-helpers -- --include-ignored`
- **Linux only:** the e2e step runs with `TMPDIR` unset so prism and the target agree.
- `cargo deny` keeps its ubuntu-only position.

**`e2e.rs` acceptance tests** (all `#[ignore]`, run by CI with `--include-ignored`):
- `go_record_all_kinds_via_agent`: build and run `examples/go-demo -duration 60s`, then `record --pid` with `--duration 3s` → session complete with 6 profiles: cpu (native), wall (derived), memory, lock, blocking, threads; each has ≥ 1 sample except blocking, which may be empty with no warning (agent present).
- `go_record_url_plain_pprof`: go-demo with `-agent=false`, pprof on 6060 → wall reported unavailable; `--kinds cpu,memory` works.
- `dotnet_record_launch_all_kinds`: `record --kinds cpu,wall,memory,lock,threads --duration 3s -- dotnet examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll all 6` → 5 profiles; cpu fidelity approximate; memory frames contain `BuildReports`; lock frames contain `ReserveInventory`.
- `dotnet_record_attach_pid`.
- `diff_two_go_sessions`: two short go-demo recordings → `prism diff` exits 0 with rows.
- `target_exit_partial`: dotnet-demo `cpu 1` with `--duration 5s` → partial.

- [ ] **Step 1:** Write `e2e.rs`; run it locally on Windows with the real helper and Go installed. Expected: PASS.
- [ ] **Step 2:** Update the examples, guide, follow-ups and CI.
- [ ] **Step 3: Verify:** `cargo test --workspace`, `npm test`, `go test` (agent), `dotnet test` (helper), and the e2e suite locally. All PASS.
- [ ] **Step 4: Commit:** `test(e2e): phase 1 acceptance; docs, examples and CI for capture`

---

## Phase 1 done when

- All unit and integration suites pass on Windows locally, and in CI on all 3 OSes once the remote exists: Rust, UI, Go agent, .NET helper, and the gated e2e.
- `prism record` captures every §0.1 kind except Block I/O for Go and .NET, by attach and by launch, with correct fidelity badges.
- `prism diff` / the Diff view, sessions and delete work as specified. The user guide documents them.

**Scope notes:**
- Moved to Phase 2 with block I/O: extending the trie fast path to filtered, inverted and sandwich queries (FOLLOWUPS.md "Ungated query paths"), and the `%LOCALAPPDATA%` change.
- Release packaging (bundling `prism-dotnet` next to `prism`, notices file) stays in "before the first binary release".
