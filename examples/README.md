# Example programs

Two small "order service" simulations with deliberate performance problems, for
trying prism by hand. You can **record** them live with `prism record`, or have
the Go demo write profile files that you import.

| Demo | What it produces | Problems built in |
|---|---|---|
| [`go-demo`](go-demo/) | Live recording via the prism agent, or six pprof files: CPU, heap, allocs, mutex, block, goroutine | Wasteful re-hashing and JSON in request handling, a cache that keeps growing, a hot lock held during "DB calls", a slow channel consumer, parked goroutines |
| [`dotnet-demo`](dotnet-demo/) | Live recording with `prism record` (all five kinds), or one speedscope file per workload via `dotnet-trace` | Same request handling, string concatenation in a loop, a contended `lock`, flushed 1-byte-buffer file writes |

See the [user guide](../docs/USER_GUIDE.md) for the full walkthrough.

## Go (needs Go 1.22+)

```bash
cd examples/go-demo
go run . -out profiles -duration 10s
```

The program prints a ready-to-run `prism import ...` line for the six files.

To record it live instead (it starts the prism agent by default; `-files=false`
skips the demo's own CPU profile so prism can capture CPU):

```bash
go run . -files=false -duration 60s &
prism record --pid <pid printed in the log> --duration 10s
prism ui
```

Or let prism launch a built binary: `go build -o go-demo . && prism record -- ./go-demo -files=false`.
Profile its tests with `prism record --go-test . --kinds cpu,memory`.

For `prism doctor`, run a single-purpose workload with a known answer:
`go run . -files=false -duration 90s -mode cpu` (or `lock`, or `disk`, a stand-in for a
slow disk that blocks in file reads), then `prism doctor --pid <pid>`. The default,
`-mode mixed`, is everything at once.

## .NET with `prism record` (needs .NET SDK 8+)

Build the demo and the prism helper once, then record. No `dotnet-trace` needed:

```bash
dotnet build examples/dotnet-demo -c Release
dotnet publish helpers/prism-dotnet -c Release -o target/helper
export PRISM_DOTNET=$PWD/target/helper/prism-dotnet.dll    # or put prism-dotnet next to prism

prism record --duration 10s -- dotnet examples/dotnet-demo/bin/Release/net8.0/dotnet-demo.dll all 15
```

This captures CPU (approximate), wall-clock, allocations, lock contention and
threads, grouped as one session. To attach to a running demo instead, use
`prism record --pid <pid>`.

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
makes them good for CPU and "where are threads stuck" questions. `prism record`
adds real allocation and lock-contention profiles.
