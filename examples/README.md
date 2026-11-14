# Example programs

Two small "order service" simulations with deliberate performance problems, for
trying perfhawk by hand. You can **record** them live with `perfhawk record`, or have
the Go demo write profile files that you import.

| Demo | What it produces | Problems built in |
|---|---|---|
| [`go-demo`](go-demo/) | Live recording via the perfhawk agent, or six pprof files: CPU, heap, allocs, mutex, block, goroutine | Wasteful re-hashing and JSON in request handling, a cache that keeps growing, a hot lock held during "DB calls", a slow channel consumer, parked goroutines |
| [`dotnet-demo`](dotnet-demo/) | Live recording with `perfhawk record` (all five kinds), or one speedscope file per workload via `dotnet-trace` | Same request handling, string concatenation in a loop, a contended `lock`, flushed 1-byte-buffer file writes |

See the [user guide](../docs/USER_GUIDE.md) for the full walkthrough.

## Go (needs Go 1.22+)

```bash
cd examples/go-demo
go run . -out profiles -duration 10s
```

The program prints a ready-to-run `perfhawk import ...` line for the six files.

To record it live instead (it starts the perfhawk agent by default; `-files=false`
skips the demo's own CPU profile so perfhawk can capture CPU):

```bash
go run . -files=false -duration 60s &
perfhawk record --pid <pid printed in the log> --duration 10s
perfhawk ui
```

Or let perfhawk launch a built binary: `go build -o go-demo . && perfhawk record -- ./go-demo -files=false`.
Profile its tests with `perfhawk record --go-test . --kinds cpu,memory`.

For `perfhawk doctor`, run a single-purpose workload with a known answer:
`go run . -files=false -duration 90s -mode cpu` (or `lock`, or `disk`, a stand-in for a
slow disk that blocks in file reads), then `perfhawk doctor --pid <pid>`. The default,
`-mode mixed`, is everything at once.

## .NET with `perfhawk record` (needs .NET SDK 8+)

Build the demo and the perfhawk helper once, then record. No `dotnet-trace` needed:

```bash
dotnet build examples/dotnet-demo -c Release
dotnet publish helpers/perfhawk-dotnet -c Release -o target/helper
export PERFHAWK_DOTNET=$PWD/target/helper/perfhawk-dotnet.dll    # or put perfhawk-dotnet next to perfhawk

perfhawk record --duration 10s -- dotnet examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll all 15
```

This captures CPU (approximate), wall-clock, allocations, lock contention and
threads, grouped as one session. To attach to a running demo instead, use
`perfhawk record --pid <pid>`.

### GC analysis with the `gc` workload

`dotnet-demo gc <seconds>` adds a garbage-collection workload: steady short-lived
allocations (gen0 collections), a list that keeps growing so objects are promoted and the
heap after full GCs rises, a large-object allocation every half second (LOH), and three
explicit `GC.Collect()` calls at 25%, 50% and 75% of the run. (`all` includes the
allocation workload but not the explicit collects.)

```bash
perfhawk record --duration 15s -- dotnet examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll gc 20
perfhawk gc last
```

Expected: collections at gen0, gen1 and gen2, a `gc_heap_growth` finding ("possible leak")
and a `gc_induced` finding (3 explicit collections). The demo prints
`[dotnet-demo] GC.CollectionCount gen0=... gen1=... gen2=...` when it finishes if you want
to compare: `gen0` is every collection, `gen1` counts gen1 and gen2 collections, `gen2` only
gen2, while `perfhawk gc` lists collections by exactly their own generation. Open the session
in `perfhawk ui` to see the charts.

### GC analysis for Go

`go-demo -mode gc` allocates short-lived garbage continuously and keeps a growing retained
slice (up to about 128 MiB, then dropped). `-gogc N` sets the GC percent.

```bash
go run ./examples/go-demo -files=false -mode gc -duration 60s     # note the pid
perfhawk record --pid <pid> --duration 15s
perfhawk gc last
```

Expected: many collections, a rising and falling heap, and a high allocation rate. Go does
not report GC duration, reasons or generations, so those show as pause-only or `other`.

## .NET with dotnet-trace (alternative)

The `profile.ps1` / `profile.sh` scripts still work if you prefer `dotnet-trace`.
Install the trace tool once (it downloads from NuGet):

```bash
dotnet tool install --global dotnet-trace
```

Then:

```powershell
cd examples/dotnet-demo
./profile.ps1            # Windows PowerShell
```

```bash
./profile.sh             # Linux / macOS
```

If PowerShell blocks the script, run it with
`powershell -ExecutionPolicy Bypass -File ./profile.ps1`.

Each workload (`cpu`, `alloc`, `lock`, `io`) is captured separately and imported
with `--kind wall`. Speedscope files exported by `dotnet-trace` contain sampled
thread stacks, which include waiting threads, so they are wall-clock-like. That
makes them good for CPU and "where are threads stuck" questions. `perfhawk record`
adds real allocation and lock-contention profiles.
