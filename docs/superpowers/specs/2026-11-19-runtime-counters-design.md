# B1 Runtime counters beside the profile: design

Date: 2026-11-19. Story: US-B1 in [USER_STORIES.md](../../USER_STORIES.md). Roadmap: PLAN §7.2 v0.2.

## Intent

During a load test, sampling profiles miss queue build-up: a starved .NET threadpool or a pile of parked goroutines uses little CPU, so the flame graph looks calm while latency climbs. `perfhawk record` should sample the runtime's own counters at a fixed interval, with no extra flags, and show them as rows on the session timeline. A short findings list should call out threadpool starvation and runaway goroutine growth, and print the thresholds it used.

## Goals

- .NET: record **every counter the `System.Runtime` provider emits**, the same set `dotnet-counters monitor` shows by default. Capture is generic (name, display name, unit, type come from the payload), so nothing is hard-coded and new runtime versions add counters for free.
- .NET: record the runtime's threadpool starvation signal (worker-thread adjustment events with reason Starvation).
- Go: goroutines, runnable goroutines, OS threads, scheduler latency p99 per interval, mutex wait time per second, GOMAXPROCS.
- Counters are on by default in `record` for both runtimes; 1 s interval.
- Session timeline shows counter rows on the same axis as profile rows; the shared window selection still opens a profile window.
- `perfhawk counters [SESSION] [--json]` prints a per-counter summary (min / mean / max / last) and findings.
- Findings list their thresholds in the output.

## Non-goals

- Custom providers or meters (`Microsoft.AspNetCore.Hosting`, user `Meter`s). Later, behind a flag.
- Importing counters from a `.nettrace` or a `dotnet-counters` CSV file.
- Counter diff between sessions (`--against`). Later, like `gc --against`.
- Linux/macOS (deferred by owner).
- Duplicating GC data: heap size and GC events already live in the GC sidecar. Counters record what `System.Runtime` emits (it includes `gc-heap-size`); no extra GC capture.

## Anti-goals

- Noticeable extra overhead or a second connection to the target.
- A flag the user must remember to get counters.
- A timeline drowned in 25 rows by default.
- A failed counter capture failing the whole `record`. It must degrade to a warning.

## Approaches considered

| | Approach | Trade-off |
|---|---|---|
| **A (picked)** | Add the `System.Runtime` EventCounters provider (`EventCounterIntervalSec=1`) and the Threading keyword to the helper's existing EventPipe session. | One session, one trace file, no new connection. Works on .NET 8+. Counter names are the classic `dotnet-counters` names. |
| B | Open a second EventPipe session just for counters. | Isolation, but a second connection, a second buffer, and two clocks to align. No gain. |
| C | Read the .NET 9+ `System.Runtime` Meter through `System.Diagnostics.Metrics`. | Newer names, but no .NET 8 support, which perfhawk targets. |

Go has one sensible way: a new agent endpoint polling `runtime/metrics`, the same pattern as `/debug/perfhawk/gc`.

## Design

### Data model (`perfhawk-model`, new `counters.rs`)

```rust
pub struct CounterData {
    pub schema: u32,               // 1
    pub runtime: String,           // "dotnet" | "go"
    pub start_unix_ns: i64,
    pub duration_ns: u64,
    pub interval_ns: u64,          // 1_000_000_000
    pub series: Vec<CounterSeries>,
    pub events: Vec<CounterEvent>, // e.g. threadpool starvation
    pub notes: Vec<String>,
}
pub struct CounterSeries {
    pub name: String,         // "threadpool-queue-length", "/sched/goroutines:goroutines"
    pub display_name: String, // "ThreadPool Queue Length"
    pub unit: String,         // as reported: "", "MB", "%", "B", "ms", "count"
    pub kind: CounterKind,    // Gauge (mean over interval) | Rate (increment per interval)
    pub points: Vec<CounterPoint>, // { t_ns: u64 since start, value: f64 }, ordered
}
pub struct CounterEvent { pub t_ns: u64, pub kind: String, pub detail: String }
```

JSON on the wire (helper and agent write it), validated by `CounterData::from_json` like `GcData`: schema check, points ordered, finite values, a cap on series (256) and points per series (one per interval up to 10 min plus slack).

### .NET capture (helper)

- `Collect.Providers()`: add `System.Runtime` at Informational with argument `EventCounterIntervalSec=1`, and add the Threading keyword (`0x10000`) to the runtime provider. Builder to confirm the keyword value against TraceEvent's `ClrTraceEventParser.Keywords.Threading`.
- `TraceConverter`: new kind `counters` (default on, in `AllKinds`). A new `CounterCollector`:
  - handles `EventCounters` events from `System.Runtime`, reads the `Payload` dictionary (`Name`, `DisplayName`, `DisplayUnits`, `CounterType` = `Mean`|`Sum`, `Mean`, `Increment`), and appends a point per counter. `Sum` → Rate (value = `Increment`), `Mean` → Gauge (value = `Mean`).
  - handles `ThreadPoolWorkerThreadAdjustmentAdjustment` with reason Starvation → `CounterEvent { kind: "threadpool_starvation" }`, detail = new worker count.
  - writes `counters.json`, emits artifact `counters` like `gc`.
- Rust `dotnet/mod.rs`: pass `counters` in `--kinds` unless disabled, read `counters.json` with a warning on bad data (same as `read_gc`).

### Go capture (agent)

- New `counters.go`: `/debug/perfhawk/counters?seconds=N` (1..600, one capture at a time via `tryLock`). Polls `runtime/metrics` every 1 s:
  - Gauge: `/sched/goroutines:goroutines`, `/sched/goroutines/runnable:goroutines`, `/sched/threads/total:threads`, `/sched/gomaxprocs:threads`.
  - Rate: `/sync/mutex/wait/total:seconds` (as ms waited per second).
  - Gauge from histogram: `/sched/latencies:seconds` delta between polls → p99 in ms.
  - Metrics the running Go version lacks (`KindBad`) are skipped with a note.
- Rust `go/http.rs`: fetch in parallel with the other kinds, same as `fetch_gc`. A 404 means an old agent → warning "update the perfhawk agent to record runtime counters".
- Needs agent tag `agents/go/v0.1.3` after merge (owner go-ahead).

### Record and store

- `record` captures counters by default when the provider supports it; `--no-counters` turns it off (matches the anti-goal of no flag needed to get them).
- `Store::save_counters / load_counters / has_counters`, sidecar `blobs/xx/{session_id}.counters.zst`, atomic rename, like `save_gc`.
- `rm` removes the sidecar with the session.

### Analysis (`perfhawk-analysis`, new `counters.rs`)

Summary per series: min, mean, max, last. Findings with thresholds stated in the output:

| Finding | Rule |
|---|---|
| .NET threadpool starvation | any `threadpool_starvation` event, or `threadpool-queue-length` > 0 for ≥ 5 consecutive intervals while `threadpool-thread-count` rises |
| Go runaway goroutines | last-third mean of goroutines ≥ 1.5 × first-third mean and grew by ≥ 1,000 |
| Go scheduler latency | scheduler latency p99 > 10 ms in any interval |
| Lock pressure (.NET) | `monitor-lock-contention-count` > 100 / s in any interval |

Thresholds are constants in one place; builder may tune them with a short note.

### CLI

`perfhawk counters [REF=last] [--json]`: table of series (name, unit, min, mean, max, last), then findings. Missing data → clear message ("session has no runtime counters; record again with perfhawk ≥ 0.2 / agent ≥ v0.1.3").

### Server and UI

- `GET /api/sessions/{ref}/timeline` adds `counters: [{ name, displayName, unit, kind, values }]` resampled to the same `points` and the same origin as profile rows (counter start aligned via `start_unix_ns`). Resample: Gauge → max per bucket (spikes stay visible), Rate → sum.
- `GET /api/sessions/{ref}/counters` returns summary + findings for the UI.
- `SessionTimeline.tsx`: counter rows below profile rows, label shows peak with unit. Default rows: .NET `threadpool-queue-length`, `threadpool-thread-count`, `cpu-usage`, `gc-heap-size`, `monitor-lock-contention-count`, `exception-count`; Go goroutines, runnable, sched latency p99. A "Show all N counters" toggle reveals the rest. Counter rows share the window selection but have no "Open window" button of their own. Starvation events draw as markers on the threadpool rows.
- Findings show above the timeline as a short list.

## Error handling

- Counter capture failure never fails `record`: it becomes a session warning.
- Invalid JSON from helper or agent → warning, no sidecar.
- A session without counters: timeline omits counter rows; CLI prints the message above.

## Testing

- Model: `from_json` round-trip and rejection cases.
- Helper: `CounterCollector` unit test fed synthetic payload dictionaries; converter test on a small checked-in `.nettrace` if one with counters can be made, else unit only.
- Go agent: handler test with `seconds=1`, schema check, skipping unknown metrics.
- Rust capture: fake agent server returning counters; old-agent 404 warning.
- Analysis: each finding fires and does not fire on synthetic series.
- Server: timeline includes counters, resampling Gauge max / Rate sum.
- UI: row rendering, toggle, shared selection (existing test style).
- Manual: `examples/go-demo` and a .NET demo under load.

## Left to the builder

- Exact TraceEvent API for the `EventCounters` payload and the starvation reason enum.
- Whether .NET 9/10 still emit the classic `System.Runtime` EventCounters next to the Meter. Expected yes; verify on an installed runtime and note it.
- UI colours and marker style, following the existing timeline CSS.
- Docs: CHANGELOG, README command list, LOAD_TESTING.md "Limits" update.
