# GC analysis for Go Implementation Plan

> Use superpowers:executing-plans (native, no subagents; see no-worktrees: branch in the main checkout).

**Goal:** `perfhawk record` on a Go target with the perfhawk agent stores `GcData` that the existing `perfhawk gc`, API, Doctor and UI GC section already show.

**Source decision:** [docs/perf/go-gc-spike.md](../../perf/go-gc-spike.md). The agent polls `debug.ReadGCStats` + `runtime/metrics` every 50 ms for the capture window and returns a `gc.json` (schema 1). Rust only fetches and validates it. No analysis, storage, API or UI changes: those never name a runtime.

**Spec:** [2026-11-14-gc-analysis-design.md](../specs/2026-11-14-gc-analysis-design.md) section 7.6. Model: `crates/perfhawk-model/src/gc.rs`.

## Global Constraints

- Agent is a nested module and `agents/go/v0.1.0` is published: never move that tag. Ship as `agents/go/v0.1.1`.
- `Protocol` stays 1. Older agent (404 on `/debug/perfhawk/gc`) -> warning "update the perfhawk agent to record GC data", not a failure.
- Go fills only what it knows: `generation: None`, `kind: "concurrent"`, `reason: "other"` (`"induced"` + `induced: true` when `NumForcedGC` advanced), `pause_ns` from `Pause`, `duration_ns = pause_ns` with a note that Go GC duration is not measured, fidelity `Approximate`. Never invent `heap_before_bytes`, `gen_after` or `promoted_bytes`.
- `GcData` JSON must pass `GcData::validate` (events ordered, `pause_ns <= duration_ns`, known reason, ordered heap points).
- Poll interval 50 ms; do not use `runtime.ReadMemStats` (stops the world).
- Capture window ends with the same duration/cancel handling as the other agent endpoints.

## Tasks

1. **Agent endpoint** (`agents/go/gc.go`, wire in `agent.go` next to `/debug/perfhawk/contention`). `GET /debug/perfhawk/gc?seconds=N` polls for N s (cap like the other endpoints), returns `gc.json`. Dedupe GCs by `NumGC`; handle ring overflow by recording a note ("N GCs lost between polls"). Heap points from `/gc/heap/live:bytes` at each poll, alloc series from `/gc/heap/allocs:bytes` deltas (`bucket_ns` = 50 ms). Test in `agent_test.go`: allocating goroutine + `runtime.GC()` calls -> events >= forced count, ordered, valid; zero-GC idle run -> empty events, no error.
2. **Rust fetch** (`crates/perfhawk-capture/src/go/http.rs`). When `plan.gc` and `has_agent`, fetch `/debug/perfhawk/gc` concurrently with the profiles, parse with `GcData::from_json`, set `CaptureOutcome.gc`. 404 or invalid -> warning, no gc. Remove `gc: None` hard-code. Make `Go` provider return `supports_gc() = true` only for agent targets (check `provider.rs` / `session.rs:254` to see how it is asked; if it is per provider, not per target, warn at capture time instead).
3. **Rust tests** with a fake HTTP agent (pattern in `capture_http` tests): gc fetched and stored; 404 downgrades to a warning; garbage body downgrades to a warning; cancel mid-window returns partial or none without hanging.
4. **Go demo** `examples/go-demo` `-mode gc`: allocation churn + a growing retained slice, `-gogc` flag (sets `debug.SetGCPercent`). Update the unknown-mode message at `main.go:110`.
5. **E2E** (gated like the other Go e2e tests): record go-demo `-mode gc` for 5 s with the agent, assert stored `GcData` has events, `perfhawk gc --json` pause share > 0, and `runtime = "go"`.
6. **Docs:** `docs/USER_GUIDE.md` GC section (Go: what is and is not measured), `docs/LIMITATIONS.md` (no GC duration, reasons or generations for Go; agent required; older agent), `examples/README.md`, `docs/QUICKSTART.md` if it lists GC, `CHANGELOG.md`, `docs/FOLLOWUPS.md` (trace-based fidelity upgrade; `gctrace` import).
7. **Release the agent:** after merge, tag `agents/go/v0.1.1` and verify with `go get ...@v0.1.1` from an empty module (RELEASING.md step 8). Needs the user's go-ahead to push the tag.

## Review Focus

1. Agent ring overflow and the first poll (GCs that happened before the window starts must not appear).
2. Zero GCs, one GC, and 200 GCs per second.
3. UI/CLI show "not measured" rather than 0 for Go duration and reason; check `perfhawk gc` text for Go.
4. Agent without the endpoint: capture still succeeds with a clear warning.

## Out of scope

`runtime/trace` parsing (true duration, reasons), per-type allocation, GC data when attaching with only a pprof URL.
