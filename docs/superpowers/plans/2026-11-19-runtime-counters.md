# Runtime Counters (B1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `perfhawk record` samples .NET `System.Runtime` counters and Go `runtime/metrics` once per second by default, stores them per session, and shows them on the session timeline, in `perfhawk counters` and as findings.

**Architecture:** New runtime-neutral `CounterData` (model) written as `counters.json` by the .NET helper and the Go agent, carried through `CaptureOutcome` like `GcData`, saved as a session sidecar `{id}.counters.zst`. Analysis summarises and raises findings; server adds counter rows to the session timeline; UI renders them under profile rows.

**Tech Stack:** Rust (model, store, capture, analysis, cli, server), C# .NET helper (TraceEvent 4.0.0, NETCore.Client), Go agent (`runtime/metrics`), React + TypeScript UI (vitest).

**Spec:** `docs/superpowers/specs/2026-11-19-runtime-counters-design.md`

## Global Constraints

- Interval 1 s (`interval_ns = 1_000_000_000`); .NET provider arg `EventCounterIntervalSec=1`.
- `CounterData.schema == 1`; caps: ≤ 256 series, ≤ 700 points per series.
- Counters on by default in `record`; opt out with `--no-counters` (mirrors existing `--no-gc`).
- Counter capture failure never fails `record`: it becomes a session warning.
- No second EventPipe session; no extra connection to the target.
- .NET 8+ target support; Windows only (Linux/macOS deferred).
- Old-agent warning text: `counters: update the perfhawk agent to record runtime counters`.
- No-data CLI message: `session has no runtime counters; record again with perfhawk 0.2+ (Go agent v0.1.3+)`.
- Finding thresholds (constants in one place, `perfhawk-analysis/src/counters.rs`): queue > 0 for ≥ 5 consecutive intervals while thread count rises; goroutines last-third mean ≥ 1.5 × first-third mean and grew ≥ 1,000; sched latency p99 > 10 ms; `monitor-lock-contention-count` > 100 per interval.
- Timeline resample: Gauge → max per bucket, Rate → sum.
- Use the PowerShell tool for anything under `%LOCALAPPDATA%`. Verify branch is `feat/runtime-counters` before each commit.

## Review Focus

1. Session recorded before this feature (no sidecar): timeline, `/counters` API and CLI must work without error (CLI prints the no-data message, timeline has `counters: []`). Test in Task 9 and Task 10.
2. Recording shorter than one interval or target exits early: helper writes `counters.json` with empty series; valid, no findings, no crash. Test in Task 6 and Task 8.
3. Runtime reports a non-finite counter value (`cpu-usage` NaN on first tick): collector drops that point; the file stays valid. Test in Task 6.
4. Counters start later or earlier than the profiles: timeline origin is the min over profile and counter starts; rows align. Test in Task 10.
5. Second concurrent `record` against the same Go agent: endpoint returns 409, record continues with a warning. Test in Task 4.

---

### Task 1: Counter data model

**Files:**
- Create: `crates/perfhawk-model/src/counters.rs`
- Modify: `crates/perfhawk-model/src/lib.rs` (add `pub mod counters; pub use counters::*;`, match how `gc` is exported)

**Interfaces:**
- Produces:
  - `pub struct CounterData { schema: u32, runtime: String, start_unix_ns: i64, duration_ns: u64, interval_ns: u64, series: Vec<CounterSeries>, events: Vec<CounterEvent>, notes: Vec<String> }`
  - `pub struct CounterSeries { name: String, display_name: String, unit: String, kind: CounterKind, points: Vec<CounterPoint> }`
  - `pub enum CounterKind { Gauge, Rate }` (serde `rename_all = "lowercase"`)
  - `pub struct CounterPoint { t_ns: u64, value: f64 }`
  - `pub struct CounterEvent { t_ns: u64, kind: String, detail: String }`
  - `impl CounterData { pub fn from_json(&[u8]) -> Result<CounterData, CounterError>; pub fn to_json(&self) -> Vec<u8>; pub fn series(&self, name: &str) -> Option<&CounterSeries> }`
  - `pub enum CounterError { Json(String), Invalid(String) }` (thiserror)
  - Constants `pub const MAX_COUNTER_SERIES: usize = 256; pub const MAX_COUNTER_POINTS: usize = 700;`
  - All fields `pub`, serde snake_case JSON, `Debug, Clone, PartialEq, Serialize, Deserialize`.

- [ ] **Step 1: Write failing tests** in `counters.rs` `#[cfg(test)]`: `round_trip` (to_json → from_json equal), `rejects_schema_2` (Invalid), `rejects_unordered_points`, `rejects_non_finite` (construct via serde_json `Value` with `1e400` → Json or Invalid), `rejects_too_many_series` (257), `rejects_zero_interval`, `accepts_empty_series` (series: [] ok).
- [ ] **Step 2: Run** `cargo test -p perfhawk-model counters` → FAIL (module missing).
- [ ] **Step 3: Implement** the types and `from_json` validation (schema == 1, interval_ns > 0, caps, points strictly ascending by `t_ns`, values finite).
- [ ] **Step 4: Run** `cargo test -p perfhawk-model counters` → PASS.
- [ ] **Step 5: Commit** `feat(model): runtime counter data`.

### Task 2: Store sidecar

**Files:**
- Modify: `crates/perfhawk-store/src/lib.rs` (next to `gc_path`/`save_gc`/`has_gc`/`load_gc` at ~389-440; session removal at ~937)
- Test: existing store test module/file that covers `save_gc` (grep `save_gc` under `crates/perfhawk-store`)

**Interfaces:**
- Consumes: `perfhawk_model::CounterData` (Task 1).
- Produces: `Store::save_counters(&self, session_id: &str, c: &CounterData) -> Result<(), StoreError>`, `Store::has_counters(&self, session_id: &str) -> bool`, `Store::load_counters(&self, session_id: &str) -> Result<Option<CounterData>, StoreError>`. Path `blobs/<id[..2]>/<id>.counters.zst`, zstd JSON, atomic rename — same as GC.

- [ ] **Step 1: Write failing tests** `counters_round_trip` (save → has true → load equals), `load_counters_missing_is_none`, `remove_session_deletes_counters` (sidecar file gone after session removal).
- [ ] **Step 2: Run** `cargo test -p perfhawk-store counters` → FAIL.
- [ ] **Step 3: Implement** by mirroring the GC functions; add `remove_file(counters_path)` beside the GC one at session removal.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(store): counters sidecar`.

### Task 3: Capture plumbing, record wiring, `--no-counters`

**Files:**
- Modify: `crates/perfhawk-capture/src/types.rs` (`CapturePlan`, `CaptureOutcome`)
- Modify: `crates/perfhawk-capture/src/provider.rs` (`supports_counters`)
- Modify: `crates/perfhawk-capture/src/session.rs` (~75, ~183, ~200-260: save + warning)
- Modify: `crates/perfhawk-capture/src/session.rs` `RecordRequest` usages, every `CapturePlan { .. }` literal (grep `gc: true`, `gc: !no_gc`, `gc: None` in `crates/`)
- Modify: `crates/perfhawk-cli/src/main.rs` (Record: `no_counters: bool` next to `no_gc` at ~113, ~504, ~519)
- Modify: `crates/perfhawk-server/src/api/capture.rs` (~239: `counters: true`)
- Test: `crates/perfhawk-capture/tests/fake_provider.rs`

**Interfaces:**
- Consumes: Task 1, Task 2.
- Produces: `CapturePlan.counters: bool`; `CaptureOutcome.counters: Option<CounterData>`; `trait Provider { fn supports_counters(&self) -> bool { false } }`. `session::record` saves `outcome.counters` via `store.save_counters`; if plan asked, provider supports it, and none came back → session warning `counters: no runtime counters were captured`. Save failure → warning, not error.

- [ ] **Step 1: Write failing tests** in `fake_provider.rs`: `record_saves_counters` (fake returns `Some(CounterData)` → `store.has_counters(&session.id)`), `record_warns_when_counters_missing` (supports true, returns None → warning contains `counters:`), `no_counters_plan_skips_warning`.
- [ ] **Step 2: Run** `cargo test -p perfhawk-capture --test fake_provider counters` → FAIL.
- [ ] **Step 3: Implement** fields, trait default, session save/warn logic (mirror GC block at ~249-256), CLI flag `--no-counters` with doc "Skip runtime counters (threadpool, goroutines, ...)", server `counters: true`. Fix all struct literals so the workspace compiles.
- [ ] **Step 4: Run** `cargo test -p perfhawk-capture --test fake_provider` and `cargo build --workspace` → PASS.
- [ ] **Step 5: Commit** `feat(capture): carry runtime counters through record`.

### Task 4: Go agent `/debug/perfhawk/counters`

**Files:**
- Create: `agents/go/counters.go`
- Modify: `agents/go/agent.go:124` (register handler)
- Test: `agents/go/agent_test.go` (beside `TestGCEndpoint*`)

**Interfaces:**
- Produces: HTTP `GET /debug/perfhawk/counters?seconds=N` → `application/json` CounterData (schema 1, runtime `"go"`, interval 1 s). Series names are the `runtime/metrics` names:
  - Gauge: `/sched/goroutines:goroutines` (display "Goroutines", unit "count"), `/sched/goroutines/runnable:goroutines` ("Runnable goroutines"), `/sched/threads/total:threads` ("OS threads"), `/sched/gomaxprocs:threads` ("GOMAXPROCS").
  - Rate: `/sync/mutex/wait/total:seconds` → name `mutex-wait-ms`, display "Mutex wait", unit "ms", value = delta seconds × 1000.
  - Gauge: `sched-latency-p99-ms` (display "Scheduler latency p99", unit "ms") from `/sched/latencies:seconds` histogram delta between polls; skip the point when the delta has no counts.
  - Metric absent (`KindBad`) → omitted, note `"<name> not available in this Go version"`.
  - Errors: seconds outside 1..600 → 400; already running → 409 (`tryLock("counters")`); request cancel ends early and returns what was collected.

- [ ] **Step 1: Write failing tests** `TestCountersEndpointReturnsSeries` (seconds=2: schema 1, `/sched/goroutines:goroutines` present with ≥ 1 point, points ascending), `TestCountersEndpointBadSeconds` (0 and 601 → 400), `TestCountersEndpointConflict` (second concurrent call → 409), `TestCountersP99FromHistogramDelta` (unit test of a helper `p99Delta(prev, cur *metrics.Float64Histogram) (float64, bool)` with hand-built histograms).
- [ ] **Step 2: Run** `cd agents/go; go test -run Counters ./...` → FAIL.
- [ ] **Step 3: Implement** `countersHandler` (poll on a 1 s ticker, first sample at t=0), `p99Delta`. Reuse `tryLock`/`unlock` and the doc-writing style of `gcHandler`.
- [ ] **Step 4: Run** `go test ./...` and `go vet ./...` → PASS.
- [ ] **Step 5: Commit** `feat(agent): runtime counters endpoint`.

### Task 5: Go capture fetch

**Files:**
- Modify: `crates/perfhawk-capture/src/go/http.rs` (`Msg` ~96, `fetch_gc` ~112 pattern, plan dispatch ~241, receive ~281)
- Modify: `crates/perfhawk-capture/src/go/mod.rs:168` (`supports_counters` → true)
- Test: `crates/perfhawk-capture/tests/go_http.rs`

**Interfaces:**
- Consumes: Task 3 fields; agent endpoint from Task 4.
- Produces: when `plan.counters`, fetch `{base}/debug/perfhawk/counters?seconds={secs}` in parallel; `Msg::Counters { data: Option<CounterData>, gone: bool, warning: Option<String> }`. 404 → warning `counters: update the perfhawk agent to record runtime counters`; invalid JSON → `counters: ignoring invalid counter data from the agent (<e>)`; non-agent target → `counters: runtime counters need the perfhawk agent in the target`.

- [ ] **Step 1: Write failing tests** in `go_http.rs` with the existing fake server: `agent_counters_land_in_outcome`, `old_agent_counters_404_warns`, `counters_409_warns_and_record_continues`.
- [ ] **Step 2: Run** `cargo test -p perfhawk-capture --test go_http counters` → FAIL.
- [ ] **Step 3: Implement** `fetch_counters(url, timeout) -> Msg` mirroring `fetch_gc`.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(capture): fetch Go runtime counters`.

### Task 6: .NET helper counter collection

**Files:**
- Modify: `helpers/perfhawk-dotnet/Collect.cs:11-18` (providers)
- Create: `helpers/perfhawk-dotnet/Convert/CounterCollector.cs`
- Modify: `helpers/perfhawk-dotnet/Convert/TraceConverter.cs` (`AllKinds` line 15; hook near GC block ~86; write artifact near ~143)
- Test: `helpers/perfhawk-dotnet/tests/CounterCollectorTests.cs`

**Interfaces:**
- Produces: helper kind `counters`; writes `counters.json` (Task 1 JSON shape, runtime `"dotnet"`) and emits artifact `counters` with path and event count, same message shape as `gc`.
- `CounterCollector`:
  - `void OnCounterPayload(double ms, IDictionary<string, object> payload)` — reads `Name`, `DisplayName`, `DisplayUnits`, `CounterType`; `"Sum"` → kind rate, value `Increment`; otherwise gauge, value `Mean`. Drops non-finite values. Keeps first-seen order of series.
  - `void OnStarvation(double ms, uint newWorkerCount)` → event `{ kind: "threadpool_starvation", detail: "worker threads -> <n>" }`.
  - `int EventCount { get; }`, `byte[] BuildJson(long startUnixNs, long durationNs)`.
- Providers: add `new EventPipeProvider("System.Runtime", EventLevel.Informational, 0, new Dictionary<string,string>{{"EventCounterIntervalSec","1"}})`; add Threading keyword to `RuntimeKeywords` (verify value against `ClrTraceEventParser.Keywords.Threading`; expected `0x10000`).
- Converter: subscribe `source.Dynamic.All` filtered to `ProviderName == "System.Runtime" && EventName == "EventCounters"`, payload at `PayloadByName("Payload")`; starvation via `clr.ThreadPoolWorkerThreadAdjustmentAdjustment` where reason is Starvation (confirm the TraceEvent enum name).

- [ ] **Step 1: Write failing tests**: `MeanCounterBecomesGauge`, `SumCounterBecomesRate` (value = Increment), `NonFiniteValueIsDropped`, `StarvationAddsEvent`, `EmptyCollectorWritesValidJson` (series `[]`, schema 1, interval 1e9).
- [ ] **Step 2: Run** `dotnet test helpers/perfhawk-dotnet/tests --filter CounterCollector` → FAIL.
- [ ] **Step 3: Implement** collector, provider changes, converter wiring.
- [ ] **Step 4: Run** `dotnet test helpers/perfhawk-dotnet/tests` → PASS. Note in the commit body whether .NET 9/10 still emit `System.Runtime` EventCounters (check with an installed runtime if available, else say unverified).
- [ ] **Step 5: Commit** `feat(dotnet): record System.Runtime counters`.

### Task 7: Rust side of the .NET helper

**Files:**
- Modify: `crates/perfhawk-capture/src/dotnet/helper.rs` (`HelperEvent` ~92, parse ~157)
- Modify: `crates/perfhawk-capture/src/dotnet/mod.rs` (`kinds_arg` ~54, `read_gc` ~63 pattern, `supports_counters`, outcome assembly ~386)
- Test: `crates/perfhawk-capture/tests/dotnet_protocol.rs`

**Interfaces:**
- Consumes: Task 3, helper output from Task 6.
- Produces: `HelperEvent::Counters { path: PathBuf, events: u64 }`; `kinds_arg(kinds, gc, counters: bool)` appends `counters`; `read_counters(path, &mut warnings) -> Option<CounterData>` (bad file → warning `counters: ignoring invalid counter data from the helper (<e>)`).

- [ ] **Step 1: Write failing tests**: `parses_counters_artifact_event`, `kinds_arg_includes_counters`, `bad_counters_json_warns`.
- [ ] **Step 2: Run** `cargo test -p perfhawk-capture --test dotnet_protocol counters` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p perfhawk-capture` → PASS.
- [ ] **Step 5: Commit** `feat(capture): read .NET counters from the helper`.

### Task 8: Analysis — summary and findings

**Files:**
- Create: `crates/perfhawk-analysis/src/counters.rs`
- Modify: `crates/perfhawk-analysis/src/lib.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - `pub struct CounterStat { name, display_name, unit: String, kind: CounterKind, min: f64, mean: f64, max: f64, last: f64, points: usize }`
  - `pub struct CounterFinding { id: &'static str, title: String, detail: String, threshold: String, at_ns: Option<u64> }` (serde Serialize, camelCase)
  - `pub struct CounterReport { stats: Vec<CounterStat>, findings: Vec<CounterFinding> }`
  - `pub fn summarize(d: &CounterData) -> CounterReport`
  - Finding ids: `threadpool_starvation`, `goroutine_growth`, `sched_latency`, `lock_contention`. Threshold constants per Global Constraints; `threshold` field states them in words (e.g. `"queue > 0 for ≥ 5 s while threads rise, or a runtime starvation event"`).
  - Goroutine growth needs ≥ 6 points; otherwise no finding.

- [ ] **Step 1: Write failing tests**: `stats_min_mean_max_last`, `starvation_event_fires`, `queue_backlog_with_rising_threads_fires`, `queue_backlog_flat_threads_does_not_fire`, `goroutine_growth_fires` (100 → 2,000), `goroutine_small_growth_does_not_fire` (100 → 400), `sched_latency_over_10ms_fires`, `lock_contention_over_100_fires`, `empty_data_has_no_findings`.
- [ ] **Step 2: Run** `cargo test -p perfhawk-analysis counters` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(analysis): counter summary and findings`.

### Task 9: CLI `perfhawk counters`

**Files:**
- Create: `crates/perfhawk-cli/src/cmd/counters.rs`
- Modify: `crates/perfhawk-cli/src/cmd/mod.rs`, `crates/perfhawk-cli/src/main.rs` (subcommand beside `Gc` ~166 and dispatch ~586)
- Modify: `crates/perfhawk-cli/src/cmd/record.rs:118` (after the GC headline, print one counters line: `counters: <n> series` plus each finding title)
- Test: `crates/perfhawk-cli/tests/cli_phase1.rs` (or the test file that covers `perfhawk gc`)

**Interfaces:**
- Consumes: Task 2, Task 8.
- Produces: `Counters { #[arg(default_value = "last")] reference: String, #[arg(long)] json: bool }`; `cmd::counters::run(store: &Store, reference: &str, json: bool) -> Result<(), String>`. Text output: table columns `counter | unit | min | mean | max | last`, then `Findings:` with `- <title>: <detail> (threshold: <threshold>)`, or `No findings.`. `--json` prints `CounterReport`. No sidecar → `Err` with the no-data message (exit code non-zero, as `gc` does).

- [ ] **Step 1: Write failing tests**: `counters_prints_table_and_findings` (seed a session + sidecar in a temp store), `counters_json_has_stats`, `counters_missing_prints_message`.
- [ ] **Step 2: Run** `cargo test -p perfhawk-cli counters` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(cli): perfhawk counters`.

### Task 10: Server API

**Files:**
- Modify: `crates/perfhawk-server/src/api.rs` (`session_timeline_h` ~1010, `SessionTimelineOut`, route table ~146)
- Test: `crates/perfhawk-server/tests/api.rs`

**Interfaces:**
- Consumes: Task 2, Task 8.
- Produces:
  - `SessionTimelineOut` gains `counters: Vec<CounterRowOut>`; `CounterRowOut { name, display_name, unit: String, kind: CounterKind, values: Vec<Option<f64>>, events: Vec<CounterEventOut> }` (camelCase); `CounterEventOut { index: usize, kind: String, detail: String }`, index into `values`.
  - Origin = min of profile timeline starts and counter `start_unix_ns`; span covers both. Gauge → max per bucket, Rate → sum. `values: Vec<Option<f64>>`; `None` (JSON `null`) for buckets with no point.
  - Session with only counters (no profile timelines) still returns rows with `durationMs` from counters.
  - `GET /api/sessions/{reference}/counters` → `CounterReport` JSON; no sidecar → 404 with code `no_counters`.

- [ ] **Step 1: Write failing tests**: `timeline_includes_counter_rows`, `timeline_gauge_max_rate_sum`, `timeline_aligns_counters_starting_later` (counter start = profile start + 2 s → first 2 s of counter values are `null`), `timeline_without_counters_has_empty_list`, `counters_endpoint_report`, `counters_endpoint_404`.
- [ ] **Step 2: Run** `cargo test -p perfhawk-server --test api counters` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p perfhawk-server` → PASS.
- [ ] **Step 5: Commit** `feat(server): counters on the session timeline`.

### Task 11: UI counter rows and findings

**Files:**
- Modify: `ui/src/api.ts` (`SessionTimeline` type ~172; add `getSessionCounters`)
- Modify: `ui/src/views/SessionTimeline.tsx`
- Modify: `ui/src/time/sparkline.ts` if `toPath`/`peak` need `null` gaps
- Test: `ui/src/views/SessionTimeline.test.tsx` (new), sparkline test if changed

**Interfaces:**
- Consumes: Task 10 JSON.
- Produces: `type CounterRow = { name: string; displayName: string; unit: string; kind: 'gauge' | 'rate'; values: (number | null)[]; events: { index: number; kind: string; detail: string }[] }`; `SessionTimeline` type gains `counters: CounterRow[]`. `getSessionCounters(ref)` → `{ stats, findings }`.
- Default-visible counter names: `threadpool-queue-length`, `threadpool-thread-count`, `cpu-usage`, `gc-heap-size`, `monitor-lock-contention-count`, `exception-count`, `/sched/goroutines:goroutines`, `/sched/goroutines/runnable:goroutines`, `sched-latency-p99-ms`. Toggle button text `Show all ${n} counters` / `Show fewer counters`. Counter rows: label `displayName`, `peak <value> <unit>`; same click/keyboard selection as profile rows; no "Open window" button; events drawn as small markers (`className="timeline-marker"`, `<title>` = detail). Findings list above rows (`role="list"`, title + threshold). Render the section when either profile series or counters exist.

- [ ] **Step 1: Write failing tests**: renders default counter rows only; toggle shows all; selecting on a counter row enables profile rows' "Open window" button; starvation marker rendered with title; component renders with only counters (no profile series).
- [ ] **Step 2: Run** `cd ui; npm test -- SessionTimeline` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `npm test` and `npm run build` (or the project's typecheck script; check `ui/package.json`) → PASS; axe test still passes.
- [ ] **Step 5: Commit** `feat(ui): runtime counter rows on the session timeline`.

### Task 12: Docs and manual check

**Files:**
- Modify: `CHANGELOG.md` (`## [Unreleased]`), `README.md` command list, `docs/LOAD_TESTING.md` (workflow step 4 add `perfhawk counters`; remove "No lag, queue depth" gap only where now covered), `docs/USER_STORIES.md` (mark US-B1 done), `docs/PLAN.md` §7.1 status line — only the B1 lines; leave the user's other uncommitted edits in `docs/PLAN.md` unstaged unless they ask.

- [ ] **Step 1: Update docs** as listed.
- [ ] **Step 2: Manual check**: build, run `examples/go-demo` with the agent (local `replace` to this checkout) and a .NET 8 demo under load; `perfhawk record --pid <pid> --duration 20s`, then `perfhawk counters`; `perfhawk ui` shows counter rows. Record what was and was not checked in the PR body.
- [ ] **Step 3: Full test run**: `cargo test --workspace`, `go test ./...` in `agents/go`, `dotnet test helpers/perfhawk-dotnet/tests`, `npm test` in `ui` → all PASS.
- [ ] **Step 4: Commit** `docs: runtime counters`.
- [ ] **Step 5:** After merge, ask the owner before tagging `agents/go/v0.1.3`.
