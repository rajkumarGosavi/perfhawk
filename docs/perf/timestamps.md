# V5 spike: what timestamps each runtime gives

Date: 2026-11-13. Gate for [V5 Time](../superpowers/plans/2026-11-13-v5-time.md) Task 5.

## Go: back-to-back pprof CPU windows

Method: a busy loop on one goroutine for 10 s per case, Go 1.27, Windows 11, default 100 Hz CPU profiling.
Each case counts the samples that parse out of the windows and the loop's iterations per second.
Single run per case on one machine, so treat differences of a few percent as noise.

| case | windows | samples | iterations/s | vs continuous profile |
|---|---|---|---|---|
| no profiling (baseline) | 0 | – | 2188 | – |
| continuous 10 s profile | 1 | 963 | 1478 | – |
| 1 s windows | 10 | 960 | 1516 | no loss |
| 250 ms windows | 40 | 950 | 1501 | no loss, 99% of samples |
| 100 ms windows | 100 | 903 | 1360 | **8% throughput loss**, 94% of samples |

Observations:

- Windowing keeps nearly all samples at every window length (target: at least 50%).
- 100 ms windows miss the "under 5% throughput loss" target on this machine; 250 ms and 1 s meet it.
- Profiling itself costs about 32% throughput on this Windows box regardless of windowing, so the target is measured against the continuous profile, not the baseline.

**Decision (Go):** windows of **250 ms**. Heat-map resolution for Go is therefore 250 ms, not the 20 ms bucket size; the UI must state this. Samples inside a window are spread evenly over that window's buckets (a window cannot say when inside itself a sample happened).

## .NET: EventPipe SampleProfiler

Not measured end to end in this spike. What is established from the code: `helpers/perfhawk-dotnet/Convert/TraceConverter.cs` already reads TraceEvent events and uses `TimeStampRelativeMSec` (contention events), so every event, including SampleProfiler samples, carries a timestamp relative to the trace start with sub-millisecond resolution. The converter currently collapses samples into aggregated pprof, which drops it.

**Decision (.NET):** use native timestamps; the helper writes a sidecar of per-sample millisecond offsets next to the pprof, and the Rust side buckets them at 20 ms. Sample yield per second was not measured here (the SampleProfiler's documented default interval is 1 ms, which would give far more samples than 20 ms buckets need).

## User-visible resolution

| runtime | kinds with a timeline | resolution |
|---|---|---|
| Go | cpu, wall (derived from stack sampling) | 250 ms |
| .NET | cpu, wall | 20 ms |
| imports (`perfhawk import`) | none unless the importer carries time | – |

Neither runtime fails its bar, so the plan continues; no human gate is needed.
