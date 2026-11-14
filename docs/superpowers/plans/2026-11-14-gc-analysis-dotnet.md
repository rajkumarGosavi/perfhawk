# GC analysis (.NET end to end) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A recorded or imported .NET session carries real GC data (counts per generation, pauses, heap, allocation rate) that `perfhawk gc`, the API, Doctor and a GC tab in the UI all present; the .NET demo exercises it.

**Architecture:** A runtime-neutral `GcData` model (`perfhawk-model`) is produced by the .NET helper from the existing EventPipe trace as `gc.json`, stored as a zstd-JSON sidecar next to a session's blobs, summarised by `perfhawk-analysis`, and exposed through one shared `report_for_session` used by the CLI and the HTTP API. No SQLite schema change.

**Tech Stack:** Rust workspace (`perfhawk-model/-store/-analysis/-capture/-server/-cli`), C# helper (`helpers/perfhawk-dotnet`, TraceEvent, xunit), React+TS UI (`ui/`, vitest, oxlint).

**Spec:** [docs/superpowers/specs/2026-11-14-gc-analysis-design.md](../specs/2026-11-14-gc-analysis-design.md). This plan covers milestones M1 to M3 (.NET). The Go spike and provider (M4, M5) get their own plan after the spike result.

## Global Constraints

- GC data is **not** a profile: do not extend `ProfileKind`; no SQLite schema change (spec 7.3, 7.4).
- Sidecar path `blobs/<id[..2]>/<session-id>.gc.zst`, **zstd-compressed JSON** with a `schema` number, written atomically (spec 7.4).
- `pause_ns` (application threads stopped) and `duration_ns` (GC wall time) are always separate fields and labelled separately in CLI and UI (spec 4).
- "Heap keeps growing" is worded as a possible leak/suspicion with the number of gen2s it rests on, never a verdict (spec 4).
- No new EventPipe provider or keyword, no new rundown. If a needed event is missing at the current keywords, stop and ask the user (spec 7.2).
- Analysis, storage, API and UI never name .NET or Go; `runtime` is a display string only.
- Every optional `GcEvent`/`HeapPoint` field may be absent and must be handled (spec 7.1).
- CLI offers everything the UI shows, with `--json` returning the API structure (spec 2).
- UI: no new dependencies, hand-written SVG; every chart has a table alternative; no colour-only encoding (spec 7.8, `docs/A11Y_CHECKLIST.md`).
- Percentiles use **nearest rank**: index `ceil(p/100 * n) - 1` of the ascending values.

## Review Focus

1. A session with **zero GC events** (idle target): all counts 0, no NaN/inf, UI and CLI say "no collections", not an error. (Task 3, Task 10, Task 12)
2. **Unknown or zero capture duration**: pause share is 0, not infinity or NaN; alloc rate series empty rather than dividing by zero. (Task 3)
3. **Very many events** (e.g. 200,000 GCs): series is downsampled to `max_points`, summary numbers still computed from all events, API payload bounded. (Task 3)
4. **Missing optional fields** (no generation, no heap sizes, as a Go-like provider would give): summary, findings and UI render without panic and omit what they cannot know. (Task 3, Task 4, Task 12)
5. **Corrupt, truncated or invalid GC files**: unreadable sidecar loads as `None` with a logged warning; invalid `gc.json` (bad schema, unordered events, `pause_ns > duration_ns`) is rejected with a message naming the problem; importing the same file twice or attaching to a session that already has GC data is refused unless `--replace`. (Task 2, Task 8)

---

### Task 0: Branch and commit the design

**Files:** `docs/superpowers/specs/2026-11-14-gc-analysis-design.md`, this plan.

- [ ] **Step 1:** `git switch main && git switch -c feat/gc-analysis` (untracked docs carry over). Run `git status --short`; expected: only the spec and plan files listed.
- [ ] **Step 2:** `git add docs/superpowers && git commit -m "docs: GC analysis design and plan"`. (The two spec amendments this plan depends on are already in the spec: `--no-gc` instead of a `gc` pseudo-kind, and imported data marked by a `notes` entry instead of an `imported` fidelity.)

### Task 1: `GcData` model

**Files:**
- Create: `crates/perfhawk-model/src/gc.rs`
- Modify: `crates/perfhawk-model/src/lib.rs` (add `pub mod gc; pub use gc::*;`)
- Test: `crates/perfhawk-model/tests/gc.rs`

**Interfaces:**
- Produces (all `Debug, Clone, PartialEq, Serialize, Deserialize`, field names exactly as below, serde snake_case; `Option` fields use `#[serde(default, skip_serializing_if = "Option::is_none")]`):

```rust
pub const GC_SCHEMA: u32 = 1;
pub struct GcData { pub schema: u32, pub runtime: String, pub start_unix_ns: i64, pub duration_ns: u64,
  pub fidelity: Fidelity, pub events: Vec<GcEvent>, pub heap: Vec<HeapPoint>,
  pub alloc: AllocSeries, pub notes: Vec<String> }
pub struct GcEvent { pub id: u32, pub start_ns: u64, pub generation: Option<u8>, pub kind: String,
  pub reason: String, pub reason_raw: Option<String>, pub induced: bool, pub pause_ns: u64, pub duration_ns: u64,
  pub heap_before_bytes: Option<u64>, pub heap_after_bytes: Option<u64>,
  pub gen_after: Option<GenSizes>, pub promoted_bytes: Option<u64> }
pub struct GenSizes { pub gen0: u64, pub gen1: u64, pub gen2: u64, pub loh: u64, pub poh: Option<u64> }
pub struct HeapPoint { pub t_ns: u64, pub total_bytes: u64, pub gen0: Option<u64>, pub gen1: Option<u64>,
  pub gen2: Option<u64>, pub loh: Option<u64>, pub poh: Option<u64> }
pub struct AllocSeries { pub bucket_ns: u64, pub bytes: Vec<u64> }
pub const GC_REASONS: [&str; 5] = ["allocation", "induced", "large-object", "low-memory", "other"];
#[derive(Debug, thiserror::Error)] pub enum GcError { #[error("{0}")] Invalid(String), #[error("{0}")] Json(String) }
impl GcData {
  pub fn validate(&self) -> Result<(), GcError>;
  pub fn from_json(bytes: &[u8]) -> Result<GcData, GcError>;   // parse then validate
  pub fn to_json(&self) -> Vec<u8>;
}
```

- [ ] **Step 1: Write failing tests** in `tests/gc.rs` using a `sample()` helper (two events, two heap points, 1 s alloc bucket):
  `roundtrip_json_is_lossless` (`from_json(&d.to_json()) == d`); `rejects_wrong_schema` (schema 2 → `Err` containing `"schema"`); `rejects_unordered_events` (event 1 `start_ns` < event 0 → `Err` containing `"ordered"`); `rejects_pause_longer_than_duration` (`Err` containing `"pause"`); `rejects_unordered_heap`; `rejects_zero_bucket_with_bytes` (`alloc.bucket_ns == 0` and non-empty `bytes`); `accepts_all_optionals_absent` (an event with every `Option` `None` validates); `rejects_unknown_reason` (reason not in `GC_REASONS` → `Err` containing `"reason"`).
- [ ] **Step 2:** Run `cargo test -p perfhawk-model --test gc`; expected: FAIL (module missing).
- [ ] **Step 3:** Implement the types and `validate`. Confirm `Fidelity` already derives serde (`crates/perfhawk-model/src/lib.rs:108`); add the derives if not. `serde_json` is a dev-dependency there; move it to `[dependencies]` for `from_json`/`to_json`.
- [ ] **Step 4:** Re-run; expected: PASS. Run `cargo clippy -p perfhawk-model -- -D warnings`.
- [ ] **Step 5:** Commit `feat(model): GcData runtime-neutral GC model`.

### Task 2: Store sidecar

**Files:**
- Modify: `crates/perfhawk-store/src/lib.rs` (new methods near `save_timeline`, `remove_session` at ~line 793, `StoreError`, `SessionRecord`)
- Test: `crates/perfhawk-store/tests/gc.rs`

**Interfaces:**
- Consumes: `GcData::{validate,from_json,to_json}`.
- Produces:
  `pub fn save_gc(&self, session_id: &str, g: &GcData) -> Result<(), StoreError>` (validates, atomic temp+rename, zstd level 3, like `save_timeline`; id shorter than 2 chars or non-ASCII → `NotFound`);
  `pub fn load_gc(&self, session_id: &str) -> Result<Option<GcData>, StoreError>` (`Ok(None)` if absent **or** unreadable/invalid, logging `perfhawk-store: ignoring unreadable GC data for {id}`);
  `pub fn has_gc(&self, session_id: &str) -> bool`;
  new `StoreError::InvalidGc(String)`;
  `SessionRecord` gains `#[serde(default)] pub has_gc: bool`, set from `has_gc` wherever a `SessionRecord` is built from the DB.
  `remove_session` also deletes the GC sidecar (ignore not-found).

- [ ] **Step 1: Write failing tests:** `save_then_load_roundtrips`; `load_missing_is_none`; `load_truncated_file_is_none` (write half of the saved bytes over the sidecar, expect `Ok(None)`); `save_invalid_is_rejected` (unordered events → `Err(InvalidGc)`); `remove_session_deletes_sidecar` (`has_gc` false afterwards, file gone); `session_record_reports_has_gc` (`resolve_session` shows `has_gc` true only after `save_gc`).
- [ ] **Step 2:** `cargo test -p perfhawk-store --test gc`; expected FAIL.
- [ ] **Step 3:** Implement as specified, following `timeline_path`/`save_timeline` for path and atomic write.
- [ ] **Step 4:** Re-run plus `cargo test -p perfhawk-store`; expected all PASS (existing tests must not break; update any struct-literal `SessionRecord` constructions).
- [ ] **Step 5:** Commit `feat(store): GC data sidecar per session`.

### Task 3: Summary and series

**Files:**
- Create: `crates/perfhawk-analysis/src/gc.rs`; modify `crates/perfhawk-analysis/src/lib.rs` (`pub mod gc; pub use gc::*;`); add `perfhawk-model` is already a dependency.
- Test: `crates/perfhawk-analysis/tests/gc.rs`

**Interfaces:**
- Produces (all `Serialize`, `#[serde(rename_all = "camelCase")]`, `Debug, Clone, PartialEq`):

```rust
pub struct GcConfig { pub long_pause_ms: f64 /*50*/, pub pause_share_pct: f64 /*5*/, pub gen2_share_pct: f64 /*30*/,
  pub gen2_min_gcs: usize /*10*/, pub growth_min_gen2: usize /*4*/, pub growth_pct: f64 /*20*/,
  pub loh_share_pct: f64 /*20*/, pub max_points: usize /*500*/ }   // impl Default with the values in comments
pub struct PauseStats { pub count: u64, pub total_ns: u64, pub mean_ns: u64, pub p50_ns: u64, pub p95_ns: u64, pub p99_ns: u64, pub max_ns: u64 }
pub struct GenCount { pub generation: Option<u8>, pub count: u64, pub pause: PauseStats }
pub struct GcSummary { pub total_gcs: u64, pub by_generation: Vec<GenCount> /*ascending, None last*/,
  pub by_reason: Vec<(String, u64)> /*desc count*/, pub by_kind: Vec<(String, u64)>, pub induced_count: u64,
  pub pause: PauseStats, pub pause_share_pct: f64, pub peak_heap_bytes: Option<u64>, pub min_post_gc_heap_bytes: Option<u64>,
  pub post_gen2_heap_bytes: Vec<u64>, pub alloc_total_bytes: u64, pub alloc_mean_bytes_per_s: f64, pub alloc_peak_bytes_per_s: f64 }
pub struct RatePoint { pub t_ns: u64, pub bytes_per_s: f64 }
pub struct PausePoint { pub id: u32, pub start_ns: u64, pub pause_ns: u64, pub generation: Option<u8> }
pub struct GcSeries { pub heap: Vec<HeapPoint>, pub alloc_rate: Vec<RatePoint>, pub pauses: Vec<PausePoint> }
pub fn summarize(d: &GcData, cfg: &GcConfig) -> GcSummary;
pub fn series(d: &GcData, cfg: &GcConfig) -> GcSeries;
```

  `HeapPoint` needs `Serialize` camelCase handling at the API boundary: derive `Serialize` on model types with snake_case and let the API return `series.heap` as-is; the UI types follow the API (snake_case for model types, camelCase for analysis types), as other endpoints already mix them.
  Rules: pause share = `total pause / duration_ns * 100`, **0.0 when `duration_ns == 0`**; `peak_heap_bytes` = max over `heap[].total_bytes` and `heap_before_bytes`; `post_gen2_heap_bytes` = `heap_after_bytes` of events with `generation == Some(2)`, in order; alloc mean = `total / (duration_ns/1e9)` (0.0 if duration 0); `alloc_peak` = max bucket bytes / bucket seconds. `series` downsamples each of heap, alloc_rate, pauses to at most `max_points` by keeping, per equal-width time window, the point with the largest value (heap `total_bytes`, rate, `pause_ns`), so peaks survive.

- [ ] **Step 1: Write failing tests** (synthetic `GcData` builder `d(events)` in the test file): `counts_per_generation` (3×gen0, 2×gen1, 1×gen2 → counts 3/2/1, `total_gcs` 6); `pause_percentiles_nearest_rank` (ten pauses 1..=10 ms → p50 5 ms, p95 10 ms, p99 10 ms, max 10 ms, mean 5.5 ms); `pause_share_uses_total_pause_not_gc_duration` (pause 10 ms, duration 100 ms of 1 s capture → 1.0%); `zero_events_gives_zeroes_not_nan` (all numbers 0, `pause_share_pct == 0.0`, `by_generation` empty); `zero_duration_has_zero_share_and_rate`; `missing_generation_counts_under_none`; `missing_heap_sizes_give_none_peak`; `post_gen2_heap_in_order`; `series_downsamples_but_summary_uses_all` (200,000 events, `max_points` 500 → each series ≤ 500, `summary.total_gcs == 200_000`, the largest pause still present in `series.pauses`); `by_reason_sorted_desc`.
- [ ] **Step 2:** `cargo test -p perfhawk-analysis --test gc`; expected FAIL.
- [ ] **Step 3:** Implement `summarize` and `series`.
- [ ] **Step 4:** Re-run; expected PASS.
- [ ] **Step 5:** Commit `feat(analysis): GC summary and chart series`.

### Task 4: GC findings

**Files:** Modify `crates/perfhawk-analysis/src/gc.rs`; test `crates/perfhawk-analysis/tests/gc_findings.rs`.

**Interfaces:**
- Consumes: `GcSummary`, `GcConfig`, `GcData`; reuse `crate::findings::Severity` (check its variants).
- Produces:

```rust
pub struct GcFinding { pub rule: &'static str, pub severity: Severity, pub title: String, pub detail: String, pub value: f64, pub based_on: u64 } // Serialize camelCase
pub struct GcReport { pub runtime: String, pub notes: Vec<String>, pub summary: GcSummary, pub series: GcSeries, pub findings: Vec<GcFinding> }
pub fn gc_findings(d: &GcData, s: &GcSummary, cfg: &GcConfig) -> Vec<GcFinding>;
pub fn report(d: &GcData, cfg: &GcConfig) -> GcReport;
```

Rules (rule id, condition, severity):
`gc_long_pause`: `pause.max_ns >= long_pause_ms` → Warn, value = max ms.
`gc_pause_share`: `pause_share_pct >= pause_share_pct` → Warn, value = share.
`gc_gen2_heavy`: `total_gcs >= gen2_min_gcs` and gen2 count / total ≥ `gen2_share_pct` → Warn, value = share.
`gc_heap_growth`: `post_gen2_heap_bytes.len() >= growth_min_gen2` and mean(second half) ≥ mean(first half) × (1 + growth_pct/100) → Warn, `based_on` = gen2 count, title says "possible leak", detail names the gen2 count and the growth.
`gc_induced`: `induced_count > 0` → Info.
`gc_loh_driven`: GCs with reason `large-object` ≥ `loh_share_pct` of all and `total_gcs >= gen2_min_gcs` → Info.
Findings sorted by severity then `value` descending.

- [ ] **Step 1: Write failing tests:** one "fires" and one "does not fire just below threshold" test per rule (e.g. `long_pause_fires_at_50ms_not_49ms`, `heap_growth_needs_four_gen2s`, `heap_growth_wording_says_possible`: title contains `"possible"`), `no_findings_for_empty_data`, `findings_tolerate_missing_fields`, `report_bundles_summary_series_findings`.
- [ ] **Step 2:** `cargo test -p perfhawk-analysis --test gc_findings`; expected FAIL.
- [ ] **Step 3:** Implement `gc_findings` and `report`.
- [ ] **Step 4:** Re-run both gc test files; expected PASS. `cargo clippy -p perfhawk-analysis -- -D warnings`.
- [ ] **Step 5:** Commit `feat(analysis): GC findings and report`.

### Task 5: Helper parses GC events into `gc.json`

**Files:**
- Create: `helpers/perfhawk-dotnet/Convert/GcCollector.cs`
- Modify: `helpers/perfhawk-dotnet/Convert/TraceConverter.cs` (`AllKinds` gains `"gc"`; hook into `ConvertLog`), `helpers/perfhawk-dotnet/Protocol.cs` unchanged (`protocol.Artifact("gc", path, events)` reuses it)
- Test: `helpers/perfhawk-dotnet/tests/GcCollectorTests.cs`, extend `ConverterTests.cs`

**Interfaces:**
- Produces a pure, trace-independent class so tests can drive it:

```csharp
public sealed class GcCollector {
  public void OnSuspendStart(double ms);                       // GCSuspendEEStart
  public void OnRestartStop(double ms);                        // GCRestartEEStop
  public void OnGcStart(int count, int generation, string reasonRaw, string type, double ms);  // type: NonConcurrentGC|BackgroundGC|ForegroundGC
  public void OnGcEnd(int count, int generation, double ms);
  public void OnHeapStats(double ms, ulong gen0, ulong gen1, ulong gen2, ulong loh, ulong? poh, ulong promoted);
  public void OnAllocTick(double ms, long bytes);
  public byte[] BuildJson(long startUnixNs, long durationNs);  // GcData JSON, snake_case, schema 1, fidelity "native"
  public int EventCount { get; }
}
```
  Rules: a pause interval `[SuspendStart, RestartStop]` belongs to the GC whose `[start,end]` contains the interval start, otherwise to the first `GCStart` inside the interval; a GC's `pause_ns` is the sum of its intervals (background GCs have several short ones, so its `pause_ns` is far below its `duration_ns`). `kind`: NonConcurrentGC→`blocking`, BackgroundGC→`background`, ForegroundGC→`blocking`. Reason normalisation: AllocSmall→`allocation`; AllocLarge, OutOfSpaceLOH→`large-object`; names starting `Induced`→`induced` (and `induced: true`); names containing `LowMemory`→`low-memory`; else `other`; the original name goes in `reason_raw`. `heap_after_bytes` and `gen_after` come from the `GCHeapStats` that follows that GC's `GCEnd`; `heap_before_bytes` is the previous GC's `heap_after_bytes` (absent for the first). Alloc ticks bucket into 1 s buckets by time. `notes` always includes "allocation rate is derived from allocation ticks (about 100 KB apart) and is approximate".

- [ ] **Step 1: Unit tests (no trace needed)** in `GcCollectorTests.cs`: `blocking_gc_pause_spans_suspend_to_restart` (suspend 100 ms, GCStart 101, GCEnd 110, restart-stop 112 → `pause_ns` 12 ms, `duration_ns` 9 ms); `background_gc_sums_its_short_pauses` (two intervals inside a BGC window of 500 ms → `pause_ns` = their sum, `< duration_ns`); `reason_normalisation` (table of the cases above); `induced_flag_set`; `heap_stats_attach_to_gc_that_just_ended`; `alloc_ticks_bucket_per_second`; `empty_collector_builds_valid_json` (zero events; the JSON parses and has `schema` 1).
- [ ] **Step 2:** `dotnet test helpers/perfhawk-dotnet/tests --filter GcCollectorTests`; expected FAIL.
- [ ] **Step 3:** Implement `GcCollector`.
- [ ] **Step 4: Real-trace test** in `ConverterTests.cs`, following the existing in-process pattern there: start `Workload.Start()`-style allocation plus an explicit `GC.Collect(2)` loop, collect with `kinds = {"gc"}`, assert `gc.json` exists, contains ≥ 1 event with `generation` 2, every `pause_ns > 0` and `<= duration_ns`, and `heap` is non-empty. **This test also answers the spec's open question** whether `GCStart/End`, `GCHeapStats` and the suspend/restart events arrive at the current keywords; if any is missing, stop and report to the user (Global Constraints).
- [ ] **Step 5:** Wire `GcCollector` into `ConvertLog` (subscribe `clr.GCStart`, `GCEnd`, `GCHeapStats`, `GCSuspendEEStart`, `GCRestartEEStop`, and the existing `GCAllocationTick` handler also feeds `OnAllocTick` when `gc` is requested), write `gc.json` into the out dir, and return `ArtifactInfo("gc", path, EventCount)`. Property names come from the TraceEvent version in the csproj; check them against its `ClrTraceEventParser`. Update the unknown-kind error text so `gc` is listed.
- [ ] **Step 6:** `dotnet test helpers/perfhawk-dotnet/tests`; expected PASS. Commit `feat(helper): parse GC events into gc.json`.

### Task 6: Capture seam in Rust

**Files:**
- Modify: `crates/perfhawk-capture/src/types.rs` (`CapturePlan.gc: bool`, `CaptureOutcome.gc: Option<GcData>`), `src/dotnet/helper.rs` (`HelperEvent::Gc`), `src/dotnet/mod.rs`, `src/provider.rs` (`fn supports_gc(&self) -> bool { false }` default on `Provider`), every `CapturePlan {` / `CaptureOutcome {` literal (compile-driven: `session.rs`, the CLI `doctor.rs`/`record.rs`, `perfhawk-server/src/api/capture.rs`, and the test files found by `grep -rn "CapturePlan {\|CaptureOutcome {" crates`).
- Test: `crates/perfhawk-capture/tests/dotnet_protocol.rs`, `tests/fake_provider.rs`

**Interfaces:**
- Consumes: `GcData::from_json`.
- Produces: `HelperEvent::Gc { path: PathBuf, events: u64 }` (the `artifact` line with `kind == "gc"` parses to this before `ProfileKind::parse`); `kinds_arg` unchanged for profile kinds, and the `--kinds` value sent to the helper gets `,gc` appended when `plan.gc`; `pub fn convert_nettrace_gc(path: &Path) -> Result<GcData, CaptureError>` in `dotnet/mod.rs` (runs `convert --kinds gc`, reads `gc.json`, errors `CaptureError::Target("no GC events in <file>")` when there are none).
- Compatibility: older helpers never emit `gc` because the new `perfhawk` only requests it with the new helper; an older `perfhawk` never asks, so no change is needed there.

- [ ] **Step 1: Write failing tests:** protocol test `gc_artifact_line_parses_to_gc_event` (using the fake-helper style already in `dotnet_protocol.rs`, helper prints `{"type":"artifact","kind":"gc","path":"<tmp>/gc.json","samples":3}`; outcome `gc.is_some()` with 3 events); `plan_without_gc_does_not_request_it` (fake helper records its argv; no `gc` in `--kinds`); `invalid_gc_json_becomes_a_warning_not_a_failure` (bad file → `outcome.gc == None`, a warning mentioning "GC data"); in `fake_provider.rs` a provider returning `gc: Some(..)` flows through `record` (see Task 7).
- [ ] **Step 2:** `cargo test -p perfhawk-capture`; expected FAIL / compile errors.
- [ ] **Step 3:** Implement; fix all literals with `gc: true` for plans created by `record` and Doctor, `gc: false` in tests that do not care, and `gc: None` on outcomes.
- [ ] **Step 4:** `cargo test -p perfhawk-capture` and `cargo build --workspace`; expected PASS.
- [ ] **Step 5:** Commit `feat(capture): carry GC data from the .NET helper`.

### Task 7: Session records GC data

**Files:** Modify `crates/perfhawk-capture/src/session.rs`, `src/dotnet/mod.rs` (`supports_gc` → true); test `crates/perfhawk-capture/tests/fake_provider.rs`.

**Interfaces:** Consumes `store.save_gc`, `CaptureOutcome.gc`, `Provider::supports_gc`.

- [ ] **Step 1: Write failing tests:** `record_stores_gc_data_with_the_session` (fake provider returns `GcData`; `store.load_gc(&session.id)` is `Some`, `session.has_gc` true via `resolve_session`); `record_warns_when_gc_supported_but_missing` (fake `supports_gc` true, returns `None`, plan.gc true → session `warnings` contains "no GC data"); `record_does_not_warn_when_gc_not_requested`; `record_does_not_warn_when_provider_does_not_support_gc`.
- [ ] **Step 2:** Run `cargo test -p perfhawk-capture --test fake_provider`; expected FAIL.
- [ ] **Step 3:** In `record`, after importing artifacts: save `outcome.gc` (on `save_gc` error push a warning and mark degraded); add the missing-data warning per the tests. Make `session.rs` pass `plan.gc` through to the provider plans.
- [ ] **Step 4:** Re-run; PASS. Commit `feat(capture): store GC data in recorded sessions`.

### Task 8: `perfhawk import --gc`

**Files:** Modify `crates/perfhawk-cli/src/main.rs` (Import args), `crates/perfhawk-cli/src/cmd/import.rs`; test `crates/perfhawk-cli/tests/gc.rs`.

**Interfaces:**
- Consumes: `GcData::from_json`, `convert_nettrace_gc`, `Store::{create_session, finish_session, save_gc, has_gc, resolve_session}` (check their exact signatures at `crates/perfhawk-store/src/lib.rs:653,696`).
- Produces: new `import` flags `--gc` (bool), `--session <REF>` (requires `--gc`), `--replace` (requires `--session`). With `--gc` each file is a `gc.json` (anything not ending `.nettrace`) or a `.nettrace`. Without `--session` each file creates a session named after the file (or `--name`) with `target.runtime` = `GcData.runtime`, status Complete, `duration_ms` from the data, tag `imported`; the data's `notes` gain `"imported from <file name>"`. With `--session REF` the data attaches to that session; if it already has GC data the command fails with `session <short_id> already has GC data; use --replace` unless `--replace` is given. Prints `<short_id>  gc  <name>  (<n> collections)`.

- [ ] **Step 1: Write failing tests** (CLI tests use a temp store as in `cli.rs`): `import_gc_json_creates_session_with_gc` (then `load_gc` is `Some`, `sessions --json` has `has_gc: true`); `import_gc_json_rejects_invalid` (unordered events → exit 1, stderr contains "ordered", no session created); `import_gc_twice_creates_two_sessions_not_corruption`; `attach_to_existing_session`; `attach_refuses_duplicate_without_replace`; `attach_with_replace_overwrites`; `unknown_file_gives_clear_error` (garbage bytes → stderr mentions "gc.json" and ".nettrace"); `session_requires_gc_flag` (clap error).
- [ ] **Step 2:** `cargo test -p perfhawk-cli --test gc`; expected FAIL.
- [ ] **Step 3:** Implement. A `.nettrace` test needs the helper, so gate it like the existing helper-dependent CLI tests (skip when `locate_helper` fails).
- [ ] **Step 4:** Re-run; PASS. Commit `feat(cli): import GC data from gc.json and .nettrace`.

### Task 9: Shared report and `perfhawk gc`

**Files:**
- Create: `crates/perfhawk-server/src/gc.rs`, `crates/perfhawk-cli/src/cmd/gc.rs`
- Modify: `crates/perfhawk-server/src/lib.rs`, `crates/perfhawk-cli/src/main.rs` and `cmd/mod.rs`, `cmd/sessions.rs` (add a `GC` column showing `yes`/`-`), `cmd/record.rs` (headline line)
- Test: `crates/perfhawk-cli/tests/gc.rs` (extend)

**Interfaces:**
- Produces: `pub fn report_for_session(store: &Store, s: &SessionRecord) -> Result<Option<GcReport>, StoreError>` (`None` when no GC data; uses `GcConfig::default()`); `pub fn headline(d: &GcData) -> String` in `cmd/gc.rs` returning exactly `GC: 42 collections (gen0 30, gen1 9, gen2 3), max pause 12.3 ms, peak heap 210.4 MB` (generations without a count omitted; `GC: no collections` for zero events; heap part omitted when unknown); command `perfhawk gc [REFERENCE=last] [--json]`.
- Text report sections in this order: header (session, runtime, duration, notes), Collections (table: generation, count, total pause, p50, p95, p99, max), Pause (total, share of wall time; line "pause = time application threads were stopped; GC duration is shown separately in the event list"), Heap (peak, minimum after GC, after-gen2 trend), Allocation (total, mean and peak rate), Reasons, Findings (title, detail), and the last 10 longest pauses. `--json` prints `GcReport` as in the API. A session with no GC data exits 1: `session <id> has no GC data; record a .NET target or run perfhawk import --gc <file>`.

- [ ] **Step 1: Write failing tests:** `gc_text_report_has_sections_in_order`; `gc_json_matches_report`; `gc_without_data_exits_1_with_hint`; `gc_zero_events_prints_no_collections_not_error`; `headline_exact_format`; `headline_zero_events`; `sessions_json_shows_has_gc`.
- [ ] **Step 2:** `cargo test -p perfhawk-cli --test gc`; expected FAIL.
- [ ] **Step 3:** Implement. Print the headline at the end of `perfhawk record` when the session has GC data.
- [ ] **Step 4:** Re-run; PASS. Commit `feat(cli): perfhawk gc report and record headline`.

### Task 10: HTTP API

**Files:** Modify `crates/perfhawk-server/src/api.rs` (route near `doctor_h`, line ~145); test `crates/perfhawk-server/tests/api_gc.rs` (copy the `start()`/`get()` helpers from `api_doctor.rs`).

**Interfaces:** `GET /api/sessions/{reference}/gc` → `200 {"available": false, "reason": "no GC data for this session"}` or `200 {"available": true, "runtime", "notes", "summary", "series", "findings"}` (the `GcReport` fields flattened under `available`); unknown session → 404 as other session routes.

- [ ] **Step 1: Write failing tests:** `gc_unavailable_for_session_without_data`; `gc_returns_report` (summary counts and a finding present); `gc_unknown_session_is_404`; `gc_zero_events_available_with_zero_counts`; `gc_large_report_is_bounded` (200,000 events → `series.pauses.length <= 500`).
- [ ] **Step 2:** `cargo test -p perfhawk-server --test api_gc`; expected FAIL.
- [ ] **Step 3:** Implement `gc_h` with `blocking(...)` like `doctor_h`, calling `report_for_session`.
- [ ] **Step 4:** PASS. Commit `feat(server): GET /api/sessions/{ref}/gc`.

### Task 11: Doctor uses real pause share

**Files:** Modify `crates/perfhawk-analysis/src/doctor.rs` (`DoctorInput` gains `pub gc: Option<&'a GcSummary>`; every `DoctorInput {` literal gets `gc: None`), `crates/perfhawk-server/src/doctor.rs` (load via `store.load_gc`); tests `crates/perfhawk-analysis/tests/doctor.rs`, `crates/perfhawk-server/tests/api_doctor.rs`.

**Interfaces:** When `input.gc` is `Some`, the sample-based `gc_pressure` finding (around line 476) is skipped and replaced by `gc_pressure` with `share_pct = summary.pause_share_pct`, `fidelity = Native`, title `GC pressure: {x:.0}% of wall time is GC pause`, firing at `cfg.gc_pct`; hints mention the allocation rate. GC remains a finding, never the verdict. With `gc: None` behaviour is identical to today.

- [ ] **Step 1: Write failing tests:** `event_based_gc_pressure_replaces_sample_based` (CPU profile with GC frames plus GC data of low pause share → no `gc_pressure` finding); `event_based_gc_pressure_fires_at_threshold`; `gc_never_becomes_the_verdict`; `no_gc_data_keeps_old_behaviour` (existing tests keep passing); API test: doctor on a session with GC data returns `gc_pressure` with fidelity `native`.
- [ ] **Step 2:** Run both test files; expected FAIL.
- [ ] **Step 3:** Implement; add `gc: None` to existing literals.
- [ ] **Step 4:** `cargo test --workspace`; PASS. Commit `feat(doctor): use real GC pause share when available`.

### Task 12: UI GC view

**Files:**
- Modify: `ui/src/api.ts` (types `GcReport` etc. mirroring Task 10 JSON, `getGc(sessionRef)` beside `getDoctor` at ~line 285), `ui/src/views/SessionList.tsx` (render `<GcView sessionId={s.id} />` after `<Doctor .../>` at ~line 169), `ui/src/theme.css` (styles, tokens only), `ui/src/help/` entry if the help panel lists views
- Create: `ui/src/analysis/gc.ts`, `ui/src/views/GcView.tsx`, `ui/src/views/GcCharts.tsx`
- Test: `ui/src/analysis/gc.test.ts`

**Interfaces:**
- Produces in `analysis/gc.ts` (pure): `generationLabel(g: number | null | undefined): string` (`"gen0"`, `"gen1"`, `"gen2"`, `"cycle"` for null); `formatPause(ns: number): string` (`"0.4 ms"`, `"12.3 ms"`, `"1.20 s"`); `filterPauses(points: PausePoint[], range: [number, number] | null): PausePoint[]`; `rowsInRange(events, range)`; `heapSeries(series)` returning points with absent per-generation values omitted; `generationShape(g)` returning a marker shape name (`circle` gen0, `square` gen1, `triangle` gen2, `diamond` other) so generation is never colour-only. Reuse `ui/src/format.ts` byte/duration helpers and `ui/src/time/range.ts` for range types where they fit.
- `GcView({ sessionId })`: fetches `getGc`; renders nothing while loading, nothing when `available` is false for a session without data (hidden tab), a muted "No collections happened during this capture" when zero events; otherwise summary cards (GCs with gen0/gen1/gen2, max pause, pause % of time, peak heap, mean allocation rate), the findings list (wording as served), heap-over-time and allocation-rate charts, the pause timeline (one mark per GC, height = pause, shape = generation), and a sortable GC table (id, time, generation, kind, reason, pause, duration, heap before → after). Dragging on the pause timeline selects a range that filters the table and the other charts; a "Clear range" button resets it. Heap and rate charts each have a `<details>` "Data table" alternative; charts carry an `aria-label` text summary; everything reachable by keyboard.

- [ ] **Step 1: Write failing vitest tests** in `gc.test.ts`: `generationLabel` cases including null; `formatPause` boundaries (0.4 ms, 12.3 ms, 1.20 s); `filterPauses` inclusive range and null range; `heapSeries_omits_absent_generations`; `generationShape_is_distinct_per_generation`; `empty_report_helpers_return_empty` (zero pauses, zero heap).
- [ ] **Step 2:** `cd ui && npx vitest run src/analysis/gc.test.ts`; expected FAIL.
- [ ] **Step 3:** Implement `analysis/gc.ts`, then the views and `getGc`. Follow `Doctor.tsx` for fetch/failed state handling and `ui/src/time/sparkline.ts` for hand-drawn SVG.
- [ ] **Step 4:** `cd ui && npm run lint && npx tsc -b && npx vitest run`; expected clean and PASS.
- [ ] **Step 5: Manual check** (text-first): `cargo run -p perfhawk-cli -- import --gc <a small hand-written gc.json>` then `perfhawk ui`, open the session, confirm the tab shows; tab through the chart and table by keyboard; walk `docs/A11Y_CHECKLIST.md` for the new view and note results in the commit message.
- [ ] **Step 6:** Commit `feat(ui): GC view`.

### Task 13: .NET demo `gc` mode and end-to-end verification

**Files:** Modify `examples/dotnet-demo/Program.cs` (new `gc` mode, included in `all`), `examples/dotnet-demo/profile.ps1`/`profile.sh` only if they enumerate modes.

**Interfaces:** `dotnet-demo.dll gc <seconds>` runs these workloads for `<seconds>`: (a) bursts of short-lived small allocations (gen0 churn), (b) a retained list that grows steadily so objects are promoted and post-gen2 heap rises, (c) a large-array allocation (`>= 85,000` bytes, LOH) every ~500 ms, (d) three explicit `GC.Collect()` calls at 25%, 50% and 75% of the run (so `gc_induced` can be shown). At exit it prints one stderr line: `[dotnet-demo] GC.CollectionCount gen0=<n> gen1=<n> gen2=<n>`.

- [ ] **Step 1:** Implement the mode, following the existing `alloc`/`HandleRequests` style in `Program.cs`.
- [ ] **Step 2: End-to-end check** (build as in `examples/README.md`; helper via `dotnet publish helpers/perfhawk-dotnet -c Release -o target/helper`, `PERFHAWK_DOTNET` set). In PowerShell: `perfhawk record --duration 25s -- dotnet examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll gc 15`, then `perfhawk gc last --json`. Verify against the printed counts: events with any generation == `CollectionCount(0)`, generation ≥ 1 == `CollectionCount(1)`, generation 2 == `CollectionCount(2)`. The record headline line and `gc_induced` finding must appear, and the long-pause or heap-growth findings should appear or be explainable from the numbers. Report any mismatch to the user instead of adjusting thresholds to hide it.
- [ ] **Step 3:** Commit `feat(demo): .NET gc workload`.

### Task 14: Docs

**Files:** Modify `docs/USER_GUIDE.md` (new "GC analysis" section: what each number means, pause vs duration, reading findings, `perfhawk gc`, `import --gc`, the `gc.json` schema with a small example, `--no-gc`), `docs/QUICKSTART.md` (one GC example using the demo), `docs/LIMITATIONS.md` (allocation rate is approximate; .NET only until Go lands; no GC data without diagnostics enabled), `examples/README.md` (`gc` mode, expected findings), `CHANGELOG.md`.

- [ ] **Step 1:** Write the docs; every command shown must be copied from a command that ran in Task 13 (no invented flags). Check `perfhawk gc --help`, `perfhawk import --help` and `perfhawk record --help` output against the text.
- [ ] **Step 2:** `cargo test --workspace && cd ui && npx vitest run && cd .. && dotnet test helpers/perfhawk-dotnet/tests`; expected all PASS.
- [ ] **Step 3:** Commit `docs: GC analysis`.

---

## Self-review notes

- **Spec coverage:** 7.1 → T1; 7.2 → T5; 7.3 → T6, T7 (flag amendment recorded in T0); 7.4 → T2; 7.5 → T3, T4, T11; 7.7 → T8, T9, T10; 7.8 → T12; 7.10 → T8; 7.9 .NET demo and docs → T13, T14; Go (7.6, M4, M5) deliberately excluded and needs its own plan after the spike.
- **Type consistency:** `GcData`, `GcEvent`, `HeapPoint`, `AllocSeries` (T1) are the names used in T2 to T12; `GcSummary`/`GcSeries`/`GcFinding`/`GcReport` (T3, T4) are what T9 to T12 serve; `report_for_session` (T9) is what T10 calls; `supports_gc` (T6) is what T7 reads.
- **Open items the builder must settle:** exact TraceEvent property names (T5), `Severity` variants (T4), the existing `create_session`/`finish_session` signatures (T8), and whether the pause lane on the session timeline is worth adding (spec section 10; left out of this plan).
