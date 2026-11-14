# GC analysis design

Status: draft for approval. Date: 2026-11-14. Roadmap: [PLAN.md](../../PLAN.md) items 1 (execution timeline) and 9 (runtime counters panel), narrowed to garbage collection.

## 1. Intent

Today Perfhawk can only *infer* GC cost from sampled stack frames (`gc-alloc-heavy` in `findings.rs`, "GC pressure" in `doctor.rs`). It cannot say how many GCs ran, how long the application was paused, how big the heap was, or how fast the program allocated. Those are the first questions a developer asks about a slow or memory-hungry service.

After this work, a recorded session carries real GC data. The user can see, per capture:

- **GC count per generation** (.NET: gen0, gen1, gen2).
- **Pause time**: count, total, mean, p50, p95, p99, max, overall and per generation, and pause as a share of wall time.
- **Heap usage over time**: total and per generation (including large object heap, LOH), and the size after each GC.
- **Allocation rate** over time (bytes per second) and in total.
- **Why** each GC happened (reason) and what kind it was (blocking, background).

It works in the CLI and the browser UI, for .NET first, then Go, with a runtime-neutral data model so other languages can be added by writing a provider, not by changing the core.

## 2. Goals

- A runtime-neutral GC data model, stored with the session, served by the API, rendered in a GC view, and printed by a CLI command (`perfhawk gc`).
- .NET end to end first: parse GC events from the existing EventPipe capture. No new capture overhead.
- Go second, after a short spike picks the data source (section 7.6).
- Deterministic findings from the data: gen2-heavy, long pauses, pause share of wall time, heap growth after gen2, LOH-driven GCs. Each finding says how much evidence it rests on.
- Extensibility: adding a language means mapping its GC events onto the common model and registering a provider output. Nothing in analysis, storage, API or UI should name .NET or Go.
- The `dotnet-demo` and `go-demo` examples get a `gc` workload and README/guide entries so every feature can be tried in one command.
- UI/CLI parity: everything the GC view shows is available from `perfhawk gc` (with `--json`), per [the parity design](2026-11-14-ui-cli-parity-design.md).

## 3. Non-goals (v1)

- No heap snapshots, `dotnet-gcdump`, dominator trees or retained-size analysis.
- No per-type allocation breakdown (the existing `memory` profile already gives allocation by stack). It is a good v2 because `GCAllocationTick` carries the type name.
- No GC tuning advice engine ("set `GCgen0size` to ..."). Findings point at evidence and a next step, not config changes.
- No live/continuous GC monitoring. This is capture-then-analyse, like everything else in Perfhawk.
- No importing of Go `gctrace` logs, JFR or `dotnet-counters` files in v1 (the Go log is a candidate for M5). `gc.json` and `.nettrace` import are in scope (section 7.10).
- No new languages beyond .NET and Go in this work.

## 4. Anti-goals (a failure even if it technically works)

- Reporting GC *duration* as pause time. Concurrent/background GC does most of its work while the app runs; only suspension time is a pause. The UI and CLI must label the two separately.
- Claiming a memory leak. "Heap after gen2 keeps growing" is a *suspicion* with stated evidence (how many gen2s, growth), never a verdict.
- Stating numbers more precisely than the source allows (for example allocation rate derived from ~100 KB allocation ticks is approximate, and says so through the fidelity badge).
- Any added capture overhead the user was not told about. The .NET GC keyword is already enabled at Verbose; no new keyword, provider or rundown should be required.
- Breaking existing sessions or the existing DB schema. Old sessions simply have no GC data and the view says so.

## 5. Constraints

- .NET 8+ (helper targets net8.0). Go 1.22+. Windows is the development machine; Linux/macOS cannot be built or tested here (same constraint as Phase 2). The .NET path is OS-independent (EventPipe), so it is testable here.
- Apache-2.0 core; permissive dependencies only. The helper already uses `Microsoft.Diagnostics.Tracing.TraceEvent`, which has the GC event parsers.
- Existing conventions: stable CLI output is available as `--json`; UI follows the accessibility checklist (charts need a table alternative, no colour-only meaning).
- The helper protocol is JSON lines, version 1. Adding a new artifact `kind` must not break an older `perfhawk` reading a newer helper or vice versa (section 7.3).

## 6. Approaches considered

- **A. Dedicated, runtime-neutral GC data artifact** (chosen by the user, 2026-11-14). A common `GcData` model beside the session's profiles, its own analysis, API route, CLI command and UI tab. Cost: new artifact type and plumbing. Benefit: represents intervals, heap curves and rates faithfully, and is the clean extension point for new languages.
- **B. Encode GC as a pprof profile (`ProfileKind::Gc`).** Reuses storage and diff with almost no plumbing, but cannot hold heap-over-time or intervals, and GC has no real stacks. Rejected.
- **C. General time-series/event store with GC as first consumer.** Most future-proof (would also serve runtime counters and contention events) but the largest piece of work and risks designing a framework before a second consumer exists. Deferred; the model in A is kept small and plain so it can be generalised later.

## 7. Design

### 7.1 Data model (`perfhawk-model`, new `gc.rs`)

```text
GcData {
  schema: 1,
  runtime: "dotnet" | "go" | ...,          // free string, display only
  start_unix_ns, duration_ns,              // capture window; all times below are ns since start
  fidelity,                                // existing Fidelity type
  events:  [GcEvent],                      // one per GC, ordered by start
  heap:    [HeapPoint],                    // size samples, ordered by time
  alloc:   AllocSeries,                    // bytes allocated per bucket
  notes:   [string],                       // provider warnings, e.g. "allocation rate is approximate"
}

GcEvent {
  id, start_ns,
  generation: Option<u8>,                  // .NET 0/1/2; Go: None (one concurrent cycle)
  kind: string,                            // "blocking" | "background" | "concurrent" | "other"
  reason: string,                          // provider's reason, normalised to a small known set + raw text
  induced: bool,                           // GC.Collect() or equivalent
  pause_ns,                                // total time application threads were stopped
  duration_ns,                             // wall time from GC start to end (>= pause_ns)
  heap_before_bytes: Option<u64>,
  heap_after_bytes:  Option<u64>,
  gen_after: Option<[u64; ..]>,            // per-generation sizes after: gen0, gen1, gen2, loh, poh when known
  promoted_bytes: Option<u64>,
}

HeapPoint { t_ns, total_bytes, gen0, gen1, gen2, loh, poh }   // per-generation fields optional
AllocSeries { bucket_ns, bytes: [u64] }                       // allocation rate = bytes / bucket_ns
```

Rules: every optional field is optional because a provider may not know it (Go has no generations; MemStats polling has no reasons). Analysis and UI must handle absence, not assume it. The model carries no language-specific names; `reason` and `kind` are strings with a documented common vocabulary (`allocation`, `induced`, `large-object`, `low-memory`, `other`) plus the raw provider text in the event.

### 7.2 .NET provider (helper `perfhawk-dotnet`)

Parse from the trace the helper already records (`Microsoft-DotNETCore-SampleProfiler` + `Microsoft-Windows-DotNETRuntime` with GC keyword `0x1` at Verbose):

- `GCStart` / `GCEnd`: count, generation, reason, type (blocking/background), induced.
- `GCSuspendEEStart` / `GCRestartEEEnd`: pause time. A non-background GC's pause spans suspend-to-restart; a background GC pauses only during its short suspension windows, so sum those.
- `GCHeapStats`: per-generation sizes and promoted bytes at the end of each GC.
- `GCAllocationTick` (already parsed for the memory profile): bucketed into `AllocSeries`. Ticks fire about every 100 KB, so the series is approximate; mark it in `fidelity` and `notes`.

Output is one `gc.json` per capture, written by `TraceConverter`. **Unverified:** that all of the above events are present in a `.nettrace` at the current keywords/level. The first implementation task is a test that records a small allocating process and asserts each event type appears. If `GCHeapStats` or suspend events need a different keyword, widening it is a decision to bring back, not to make silently (overhead is an anti-goal).

### 7.3 Capture seam and protocol

- The helper emits a new artifact event with `kind: "gc"` and the path of `gc.json`. `ProfileKind` is **not** extended: GC data is not a profile. On the Rust side, `HelperEvent::Artifact` handling splits on kind: profile kinds as today, `"gc"` to the new path.
- Compatibility: an older `perfhawk` that does not know `"gc"` must ignore it with a warning, not fail the capture. A newer `perfhawk` talking to an older helper just gets no GC data and notes it. Protocol stays version 1 (additive change); builder confirms how the Rust parser treats an unknown kind today and fixes it if it errors.
- Provider outcome gains `gc: Option<GcData>`. Providers that do not support GC leave it `None`.
- The session importer stores it (section 7.4) and adds a warning if the user asked for GC and none came back.
- GC capture is on by default for runtimes that support it (the data is already being recorded) and is turned off with `perfhawk record --no-gc`. (Amended 2026-11-14: replaces the earlier idea of a `gc` pseudo-kind in `--kinds`, which does not fit the `ProfileKind`-typed `--kinds` option.)

### 7.4 Storage (`perfhawk-store`)

- A sidecar next to the session's blobs, keyed by session id, written atomically like the existing timeline sidecar: `blobs/<xx>/<session-id>.gc.zst`.
- Format: **zstd-compressed JSON**, not bincode. The existing timeline uses bincode, which is not self-describing; GC data will gain fields as providers learn more, so a self-describing format with a `schema` number avoids the migration problem the timeline comment warns about.
- **No SQLite schema change.** "Has GC data" is "the sidecar exists" (same as `has_timeline`). Old sessions and old stores keep working. Removing a session removes its GC sidecar.
- API: `save_gc(session_id, &GcData)`, `load_gc(session_id) -> Option<GcData>` (None when missing or unreadable, with a logged warning, like timelines).

### 7.5 Analysis (`perfhawk-analysis`, new `gc.rs`)

`summarize(&GcData, &GcConfig) -> GcSummary`:

- Counts: total and per generation; counts by reason and by kind.
- Pause stats: count, total, mean, p50, p95, p99, max; overall and per generation. Pause share of wall time = total pause / capture duration.
- Heap: peak, minimum post-GC size, post-gen2 sizes, and a growth indicator (trend of post-gen2 heap across gen2s, with how many gen2s it is based on).
- Allocation: total bytes, mean rate, peak rate bucket.
- Series for charts: heap over time, allocation rate over time, pause list (id, start, pause_ns, generation), downsampled to a bounded number of points for large captures (builder chooses the cap; the summary numbers are always computed from full data).

Findings (reuse the existing `Finding` shape, with severity and evidence; thresholds in a config struct like the other analysers):

- `gc-long-pause`: max or p99 pause above a threshold (default proposal 50 ms; builder tunes).
- `gc-pause-share`: pause share of wall time above a threshold (default 5%).
- `gc-gen2-heavy`: gen2 share of all GCs high, or gen2 count high per minute.
- `gc-heap-growth`: post-gen2 heap trending up across at least N gen2s (wording: "possible leak", evidence shown, low confidence unless N is large).
- `gc-induced`: GCs triggered by explicit collect calls (often a bug).
- `gc-loh-driven`: GCs whose reason is large-object allocation.

Doctor: when a session has GC data, the existing sample-based "GC pressure" finding is replaced by (or backed with) pause-share from real data, and its evidence is labelled event-based. When there is no GC data the current behaviour is unchanged.

### 7.6 Go (milestone 2, behind a spike)

A time-boxed spike compares data sources for Go, then picks one. Candidates:

- **`runtime.MemStats` polling via the perfhawk agent** (the demo starts one; builder verifies what the agent exposes today). Cheap, works when attaching, gives GC count, `PauseNs` ring (256 entries, so poll often enough not to lose entries), heap sizes and allocated bytes. No reasons, no generations. My lean.
- **`runtime/trace`** (via the agent or `go test`): full GC and STW detail and timestamps, but higher overhead and a larger parser.
- **`GODEBUG=gctrace=1` log parsing**: only possible when Perfhawk launches the process; text format varies by Go version.

The spike's output is a one-page note: overhead measured, what fields each option fills in, recommendation. The Go mapping onto `GcData`: `generation: None`, `kind: "concurrent"`, `pause_ns` from the STW pauses, `heap_*` from MemStats, `reason: "other"` unless the source provides it. Go is blocked on the spike result; nothing in sections 7.1 to 7.5 may depend on Go specifics.

### 7.7 Server and CLI

- `GET /api/sessions/{reference}/gc` returns `{ available, summary, findings, series }`. `available: false` with a reason for sessions without data. Same reference style as `doctor`/`pivot`.
- `perfhawk gc <session>`: a text report (counts per generation, pause table with percentiles, heap, allocation, findings, next steps). `--json` returns the same structure as the API. `perfhawk sessions` and `show` indicate when a session has GC data.
- `perfhawk record` prints GC headline numbers in its end-of-capture summary when GC data was captured (one line: GCs, max pause, peak heap).

### 7.8 UI

A **GC** tab on the session view (next to Doctor/timeline), hidden or showing a plain explanation when the session has no GC data. Layout, top to bottom:

```text
[ Summary cards: GCs  gen0/gen1/gen2 | max pause | pause % of time | peak heap | avg alloc rate ]
[ Findings list (same component as Doctor/Findings) ]
[ Heap over time (stacked by generation) ]      [ Allocation rate over time ]
[ Pause timeline: one mark per GC, height = pause, colour/shape = generation ]
[ Table: GC list (id, time, gen, kind, reason, pause, heap before->after) sortable ]
```

Selecting a time range in any chart filters the others and the table (reusing `ui/src/time` range logic). Every chart has the table alternative and a text summary for screen readers; generation is encoded by shape and label as well as colour. The pause timeline is also offered as a lane on the existing session timeline so pauses can be read against CPU and lock time (builder decides whether this is a follow-up if it adds risk).

### 7.10 Importing GC data from files (added 2026-11-14 at the user's request)

`perfhawk import --gc <file>` accepts:

- **Perfhawk `gc.json`**: the `GcData` model (section 7.1), validated on load (schema number, ordered events, `pause_ns <= duration_ns`, sane sizes). This is the extension point for any tool or language, and the easy way to write test fixtures by hand. The schema is documented in the user guide.
- **`.nettrace`** (from `dotnet-trace collect` or similar): the .NET helper converts it with the same GC parser used for `record`, so imported and recorded data are identical in shape. Needs the helper to be installed, same as `record`; if it is missing, the error says so and how to get it.

Behaviour:

- Imported GC data belongs to a session. By default `import --gc` creates a new session named after the file; `--session <ref>` attaches the data to an existing session instead (and refuses if that session already has GC data, unless `--replace` is given).
- File type is detected by content/extension; an unknown file gets a clear error naming the supported types.
- `.nettrace` import extracts GC data only by default. A trace usually also has CPU, lock and allocation events; extracting them too is a flag (`--also cpu,wall,memory,lock`) and reuses the existing converter kinds. Builder confirms this adds no new conversion code.
- Imported data is marked by a `notes` entry "imported from <file name>" (amended 2026-11-14: `Fidelity` has no `imported` variant, and adding one would touch every profile view). Allocation-rate approximation rules from 7.2 apply to `.nettrace`.
- The server's existing upload route may accept `gc.json` and `.nettrace`; whether the UI gets an upload control for it is left to the builder (CLI is required, UI is optional in v1).
- Go `gctrace` log import is a candidate for M5, after the Go spike decides the Go provider's fidelity.

### 7.9 Demos and docs

- **`examples/dotnet-demo`**: new `gc` mode (also part of `all`) that: allocates short-lived garbage in bursts (steady gen0 churn), keeps a growing retained cache so objects are promoted (gen1/gen2 and visible heap growth), and allocates large arrays (LOH, `>=85,000` bytes) on a timer. A flag or argument lets a run induce a few explicit collects so `gc-induced` can be demonstrated. Runnable as `perfhawk record --duration 15s -- dotnet ... dotnet-demo.dll gc 20`.
- **`examples/go-demo`**: new `-mode gc` with allocation churn plus a retained growing structure, and a `-gogc` knob to show how GC frequency changes.
- Update `examples/README.md`, `docs/USER_GUIDE.md` (new GC section: what each number means, pause vs duration, how to read the findings), `docs/QUICKSTART.md`, `docs/LIMITATIONS.md` (approximate allocation rate; Go has no generations; no GC data when attaching to targets without diagnostics enabled), and `CHANGELOG.md`.
- The Go demo and doc updates land with the Go milestone; the .NET demo and docs land with the .NET milestone.

## 8. Milestones

1. **M1 (.NET data):** verify events exist in a real trace; `GcData` model; converter parses to `gc.json`; protocol/capture/store plumbing; `load_gc`/`save_gc`. Done when a recorded .NET session has a stored `GcData`.
2. **M2 (analysis, CLI, API, import):** `summarize`, findings, doctor hook, `perfhawk gc`, API route, `record` headline line, `perfhawk import --gc` for `gc.json` and `.nettrace`.
3. **M3 (UI + .NET demo + docs):** GC tab, accessibility checks, `dotnet-demo gc`, docs. Done when a user can follow the README to a GC tab with all findings reproducible on the demo.
4. **M4 (Go spike):** one-page note, no product code kept.
5. **M5 (Go provider + Go demo + docs).**

## 9. Testing

- **Model/analysis:** synthetic `GcData` unit tests for percentiles, per-generation counts, pause share, growth indicator, each finding at/below threshold, missing optional fields, empty data.
- **Helper (C#):** converter tests on a real `.nettrace` fixture recorded from a tiny allocating program (kept small); assert counts per generation, pause durations are positive and `<= duration`, heap points exist. A protocol test for the new artifact kind and for an unknown kind being ignored.
- **Capture/store:** fake provider that returns `GcData`; round trip through the importer and `load_gc`; corrupt/truncated sidecar returns `None`; removing a session removes the sidecar.
- **Import:** valid and invalid `gc.json` (bad schema number, unordered events, impossible values); `.nettrace` import yields the same `GcData` as `record` for the same trace; `--session` attach, duplicate refusal and `--replace`; missing helper gives the documented error.
- **Server/CLI:** API test for available and unavailable sessions; CLI snapshot-style tests for text and `--json`.
- **UI:** vitest for series building, downsampling and range filtering; accessibility checklist run for the new tab (keyboard, table alternative, no colour-only encoding).
- **End to end on this machine:** record the real `dotnet-demo gc` workload and check the numbers against `GC.CollectionCount(n)` printed by the demo itself at exit, so counts have an independent reference.
- Linux/macOS: not testable here; the .NET path is OS-independent and the Go path must be checked on CI before release.

## 10. Left to the builder

- Exact thresholds and defaults for findings (proposals above are starting points).
- The downsampling cap and algorithm for chart series.
- How `gc.json` event timestamps are made relative to the same session clock as the existing timelines (so the lanes align); verify with `bucket_ns` handling in `Timeline`.
- Reason-string normalisation table for .NET GC reasons.
- Whether the pause lane on the session timeline is in M3 or a follow-up.

## 11. Calls I made (please check)

- GC data is a sidecar JSON file, not a profile and not a SQLite table. Reason: no schema migration, self-describing, matches how timelines already work.
- `ProfileKind` is not extended; GC capture is on by default where supported and disabled with `--no-gc`.
- Allocation rate is derived from allocation ticks (approximate, labelled) rather than adding the `System.Runtime` EventCounters provider, to keep overhead and complexity unchanged.
- Go source is deliberately undecided until the spike; my lean is `MemStats` polling.
- GC findings and the Doctor change use deterministic thresholds, no AI.
- The doc is not committed yet. Proposal: commit it on its own branch off `main`, not on `feat/ui-cli-parity`, to keep the two pieces of work separate.

## 12. Risks

- The needed .NET events may need a higher keyword/level than is on now (overhead trade-off to bring back to the user).
- Background-GC pause accounting is subtle; getting pause vs duration wrong would violate an anti-goal. Mitigated by the end-to-end check against the demo's own `GC.CollectionCount` and by tests with known synthetic traces.
- Go `MemStats` polling can miss pauses between polls if the ring (256) wraps; the provider must poll fast enough and note any loss.
- Large long captures produce many GC events; API and UI must bound payload size.
