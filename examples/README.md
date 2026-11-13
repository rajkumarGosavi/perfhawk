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
