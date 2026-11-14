# Go GC data source spike (2026-11-14)

Question from the GC spec (section 7.6): which source should fill `GcData` for Go?

## Method

One Go 1.27.4 program on Windows 11, 32 logical CPUs. `GOMAXPROCS` goroutines allocate 1-5 KiB slices in a tight loop (about 65 GCs per second, far busier than a real service) and retain a 64 KiB block every 2,000 allocations. Each mode ran 3 times for 8 s; the score is allocations per second. Modes: nothing, `debug.ReadGCStats` plus `runtime/metrics` every 50 ms, `runtime.ReadMemStats` every 50 ms, and `runtime/trace` for the whole run. Extra runs polled every 5 ms.

## Results

| Mode | allocs/s (3 runs) | GCs seen / GCs run | Notes |
|---|---|---|---|
| none | 4.25 M, 4.47 M, 4.23 M | n/a | run to run noise is about 5% |
| `ReadGCStats` + `metrics.Read`, 50 ms | 4.18 M, 4.19 M, 4.29 M | 100% (524/524, 520/520, 513/513) | no measurable cost |
| `ReadMemStats`, 50 ms | 4.23 M, 4.22 M, 4.23 M | 98-100% | no measurable cost |
| `runtime/trace` | 4.31 M, 4.39 M, 4.26 M | needs a parser | 137 MB in 8 s (about 17 MB/s); 15% fewer GCs ran (430-449 vs 520-527) |
| `ReadGCStats` + metrics, 5 ms | 4.26 M | 100% | |
| `ReadMemStats`, 5 ms | 3.97 M | 100% | about 8% slower: it stops the world, `ReadGCStats` does not |

## What each source gives

| Field | `ReadGCStats` + `runtime/metrics` | `ReadMemStats` | `runtime/trace` |
|---|---|---|---|
| GC count | yes | yes | yes |
| Pause per GC | yes (`Pause`, most recent first, 256 ring) | yes (`PauseNs` ring, 256) | yes, each STW phase |
| When the GC ended | yes (`PauseEnd`) | yes (`PauseEnd`) | exact start and end |
| GC wall duration | no | no | yes |
| Heap live after GC | `/gc/heap/live:bytes` (Go 1.21+), sampled | `HeapAlloc` at poll time | yes |
| Heap goal | `/gc/heap/goal:bytes` | `NextGC` | yes |
| Bytes allocated | `/gc/heap/allocs:bytes`, no STW | `TotalAlloc` | per allocation sample |
| Reason (forced vs heap growth) | no (`NumForcedGC` count only) | `NumForcedGC` count | yes |
| Cost | none measured | STW per poll | high volume, parser needed |
| Works with a plain `net/http/pprof` URL | no | no | yes (`/debug/pprof/trace`) |

## Recommendation

Use **agent-side polling with `debug.ReadGCStats` and `runtime/metrics`**, polled every 50 ms, and have the agent assemble `gc.json` itself.

- No measurable overhead, and it does not stop the world (`ReadMemStats` does; at 5 ms it cost about 8%).
- 50 ms polling lost no GCs even at 65 GCs per second; the 256 entry ring only overflows beyond about 5,000 GCs per second.
- It fills pause, end time, heap after GC, heap goal and allocation rate. It cannot give GC duration, reason or generations: set `kind: "concurrent"`, `generation: None`, `duration_ns = pause_ns` (documented as unknown), `reason: "other"` (or `induced` when `NumForcedGC` moved), fidelity `approximate`, and a note saying so.
- Attaching with only a `net/http/pprof` URL gets no GC data (the agent is required). `runtime/trace` stays a later option for true duration and reasons; the agent already exposes `/debug/pprof/trace`, and `golang.org/x/exp/trace` could summarise it agent-side.
- `gctrace` parsing is dropped: it only works when Perfhawk launches the process, and its format varies by Go version.

The new endpoint is an additive change to the agent module, so it ships as `agents/go/v0.1.1`. `v0.1.0` is already on the module proxy and must not move. The agent `Protocol` stays 1; the CLI detects GC support by a 404 on the new endpoint and says to update the agent.

Spike program: not kept (about 100 lines; the numbers above are the output). Re-create from the method if the results need checking.
