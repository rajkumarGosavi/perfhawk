# GC: gcscope-parity additions

Source of the idea: https://pkg.go.dev/github.com/timur-developer/gcscope. Perfhawk
already has per-cycle events, heap, allocation and pause charts (`GcData` schema 1).
This plan closes the remaining gaps that can be filled when the agent *attaches*
to a running process.

## Scope

| # | Item | How | Layer |
|---|------|-----|-------|
| 1 | Heap goal | `/gc/heap/goal:bytes` sampled with live heap | agent, model, UI heap chart |
| 2 | GC CPU % | `/cpu/classes/gc/total:cpu-seconds` / `/cpu/classes/total:cpu-seconds` deltas | agent, model, summary, UI card |
| 3 | GOGC / GOMEMLIMIT context | `debug.SetGCPercent(-1)` read-and-restore is unsafe; use `/gc/gogc:percent` and `/gc/gomemlimit:bytes` metrics (Go 1.19+) | agent, model, UI note |
| 4 | Rate, interval, time since last GC | derived from events | analysis summary, UI card |
| 5 | Bad-STW count and percent | pause >= `long_pause_ms` (existing threshold) | analysis summary, UI card |
| 6 | GC diff A vs B | `perfhawk gc <a> --against <b>` (flag, not a subcommand: `gc` takes a positional ref) CLI table (cycles, heap, pause p50/p99/max, pause share, GC CPU, delta) | CLI |

Already covered, no work: forced count (`induced_count`), p50/p99/max pause, heap
live, per-cycle pauses.

## Out of scope (and why)

- **Sweep-term / mark-term split and pacer signals (assist ratio, workers, pages
  swept):** only exposed by `GODEBUG=gctrace=1` on stderr, which must be set when
  the process starts. Perfhawk attaches to running processes, so it cannot get
  them. Revisit if perfhawk gains a spawn mode for Go targets.
- **Live sliding-window dashboard:** perfhawk is capture-then-view by design.

## Schema (additive, still `schema: 1`)

All new fields are optional, so old gc.json files, the .NET helper and stored
sessions keep working with no migration.

- `GcData.config: Option<GcConfigInfo>` = `{ gogc: Option<i64>, mem_limit_bytes: Option<u64> }`
- `HeapPoint.goal_bytes: Option<u64>`
- `GcData.cpu: Option<GcCpu>` = `{ gc_ns: u64, total_ns: u64 }` (deltas over the capture)

Summary additions (camelCase, analysis crate): `gcsPerMin`, `meanIntervalNs`,
`sinceLastGcNs`, `badPauseCount`, `badPausePct`, `gcCpuPct: Option<f64>`,
`peakHeapGoalBytes: Option<u64>`.

## Steps

1. **Model** (`perfhawk-model/src/gc.rs`): add the optional fields, serde defaults,
   tests for round-trip and for old JSON without them.
2. **Analysis** (`perfhawk-analysis/src/gc.rs`): extend `GcSummary` and
   `summarize`; unit tests with fixed numbers (rate, interval, bad pauses, CPU %).
3. **Go agent** (`agents/go/gc.go`): read the extra metrics each poll; fill
   `goal_bytes`, `cpu`, `config`; skip gracefully when a metric is
   `KindBad` (older Go). Extend `agent_test.go`.
4. **UI** (`ui/src/api.ts`, `GcView.tsx`, `GcCharts.tsx`): new types, cards
   (GC rate, GC CPU, bad pauses), goal line on the heap chart, GOGC/GOMEMLIMIT in
   the notes. Update `GcView.test.tsx`.
5. **CLI** (`perfhawk-cli`): print the new numbers in `perfhawk gc`; add
   `perfhawk gc <a> --against <b>`. Tests in `crates/perfhawk-cli/tests/gc.rs`.
6. **Docs/changelog:** CHANGELOG entry; note the Go agent additions need a new
   `agents/go` tag (v0.1.2) at release time, not now.
7. **Verify:** `cargo test` for touched crates, `go test ./...` in `agents/go`,
   UI `npm test` for GC files, `cargo clippy`.

## Risks

- Agent change ships only with a new `agents/go` tag; until then a capture with
  an old agent simply lacks the new fields (UI hides those cards).
- Windows pause clock is coarse (already noted); bad-pause percent inherits it.
