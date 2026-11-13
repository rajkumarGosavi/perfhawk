# Example programs

Two small "order service" simulations with deliberate performance problems, for
trying prism by hand. In Phase 0 prism **imports** profile files; recording
directly from a running process arrives in Phase 1. So each demo produces
profile files, which you then import.

| Demo | What it produces | Problems built in |
|---|---|---|
| [`go-demo`](go-demo/) | Six pprof files: CPU, heap, allocs, mutex, block, goroutine | Wasteful re-hashing and JSON in request handling, a cache that keeps growing, a hot lock held during "DB calls", a slow channel consumer, parked goroutines |
| [`dotnet-demo`](dotnet-demo/) | One speedscope file per workload (via `dotnet-trace`) | Same request handling, string concatenation in a loop, a contended `lock`, flushed 1-byte-buffer file writes |

See the [user guide](../docs/USER_GUIDE.md) for the full walkthrough.

## Go (needs Go 1.22+)

```bash
cd examples/go-demo
go run . -out profiles -duration 10s
```

The program prints a ready-to-run `prism import ...` line for the six files.

## .NET (needs .NET SDK 8+ and dotnet-trace)

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
makes them good for CPU and "where are threads stuck" questions. Phase 1's
native .NET provider adds real allocation, lock-contention and I/O profiles.
