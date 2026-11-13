# Prism User Guide

**Applies to:** Phase 1 (Capture) · updated 2026-11-13

Prism is a local-first profiling workbench. It brings profiles from different
languages and tools into one model, then lets you explore them through one CLI and
one browser UI. Nothing leaves your machine: data is stored locally, and the UI
server only listens on `127.0.0.1`.

> **Scope.** Prism **imports** profile files (Go pprof, collapsed/folded stacks,
> speedscope JSON, `.nettrace`) and **records** directly from live Go and .NET
> processes (`prism record`) into *sessions*. Two profiles can be diffed in the
> CLI and the UI. See [PLAN.md](PLAN.md) §7.

---

## 1. Install / build

Prerequisites: Rust (stable) and Node 22.12+ to build. Go 1.22+ and .NET 8+ are
only needed for the example programs.

```bash
cd ui
npm ci
npm run build
```

```bash
cd ..
cargo build --release -p prism-cli
```

The binary is `target/release/prism` (`prism.exe` on Windows), with the UI
embedded. Put it on your `PATH` or call it by path. Build the UI before the
binary; otherwise the server shows a "UI not built" page.

## 2. Five-minute tour

```bash
cd examples/go-demo
go run . -out profiles -duration 10s
```

The demo prints a full `prism import ...` line for the files it wrote. Then run:

```bash
prism ls
```

```bash
prism show cpu --top 10
```

```bash
prism ui
```

`prism ui` prints `prism UI: http://127.0.0.1:7878/` and opens your browser.
Click a profile to explore it. Stop the server with **Ctrl+C**.

For .NET, see [examples/README.md](../examples/README.md).

To record instead of importing, run the demo live and attach (see §4a):

```bash
cd examples/go-demo
go run . -files=false -duration 60s &      # prints its pid
prism record --pid <pid> --duration 10s
prism ui
```

## 3. Concepts

| Term | Meaning |
|---|---|
| **Profile** | One imported file. It is identified by the SHA-256 of its bytes, so importing the same file twice is a no-op. The short id is the first 12 hex characters. |
| **Kind** | What the profile measures: `cpu`, `wall`, `memory`, `lock`, `blocking`, `block_io`, `threads`, or `unknown`. Kind is detected from the data and can be overridden with `--kind`. |
| **Sample type** | One measured value per sample. A Go heap profile has four: `alloc_objects`, `alloc_space`, `inuse_objects` and `inuse_space`. Each has a unit (`nanoseconds`, `bytes` or `count`). |
| **Self / Total** | *Self* is the cost spent in the function itself. *Total* also includes everything it calls. A recursive function is counted once per sample in Total. |
| **Session** | One `prism record` run. It groups the profiles captured together (one per kind), plus the target, status (`complete`, `partial`, `cancelled`, `failed`), duration and any warnings. |
| **Fidelity** | How a profile's numbers were obtained: `native` (reported by the runtime's own profiler), `approximate` (reconstructed, treat absolute values as estimates), `derived` (computed from other data, for example wall-clock from stack sampling), `sampled external` (sampled from outside the process), or `unknown` (an imported file). Shown as a badge next to the kind everywhere. |
| **Reference** | How you name a profile in commands: `last` (most recent), a full id, a unique id prefix of at least 4 hex characters, or the exact display name. If a name matches several profiles (for example after re-running a demo), prism lists the candidates; use the short id from `prism ls`. |

### How kinds are detected

| File | Detected kind |
|---|---|
| Go CPU profile (`cpu` sample type) | `cpu` |
| Go heap / allocs | `memory` |
| Go goroutine dump | `threads` |
| Go mutex profile | `lock` |
| Go block profile | `blocking`, **only if the file name contains `block`** (Go writes mutex and block profiles with identical sample types) |
| Collapsed stacks, speedscope | `unknown`: pass `--kind` (e.g. `--kind wall` for `dotnet-trace` exports) |

"Blocking" means waiting on channels, `select` or sync primitives. It is not
disk I/O; that is `block_io`, which arrives with Phase 2.

## 4. CLI reference

All commands accept `--home <DIR>` to choose the data directory. Otherwise
`$PRISM_HOME` is used, and failing that the platform default:

- Windows: `%APPDATA%\prism`
- macOS: `~/Library/Application Support/prism`
- Linux: `$XDG_DATA_HOME/prism` or `~/.local/share/prism`

### `prism import <FILES>... [--name N] [--kind K] [--tag T]...`

Imports one or more files. Each file prints one line:

```
984dbaaf5c15  cpu  cpu  (4575 samples)
```

Re-importing an existing file prints the same line with `[already imported]`.
If some files fail, the others are still imported, the errors go to stderr, and
the exit code is 1. `--name` works with a single file only. `--tag` can be
repeated.

Supported formats are detected from content, not file extension:

- **pprof** (`.pb.gz` or raw protobuf), from Go `runtime/pprof`, `net/http/pprof`, `go test -cpuprofile`, async-profiler and others.
- **Collapsed / folded stacks** (`main;a;b 42` per line), from `stackcollapse-*`, `py-spy --format raw`, `perf script | stackcollapse-perf.pl` and others.
- **speedscope JSON**, from `dotnet-trace --format Speedscope`, `py-spy --format speedscope`, `rbspy` and others. Every profile in the file (one per thread for `dotnet-trace`) is merged, and each sample is labelled with its thread (thread filtering in the UI arrives later).

Windows tip: PowerShell does not expand `*.pb.gz`. List the files, or run the
command from Git Bash.

### `prism ls [--json]`

Lists profiles, newest first:

```
ID            KIND     FORMAT  SAMPLES     TOTAL  NAME    IMPORTED (UTC)
984dbaaf5c15  cpu      pprof      4575    87.3 s  cpu     2026-11-13 06:05
```

TOTAL is the default sample type's total. IMPORTED is shown in UTC. `--json` prints full records.

### `prism show [REF] [--top N] [--st NAME] [--focus RE] [--exclude RE] [--hide RE] [--sort self|total]`

Prints the hottest functions, sorted by self value by default, or by total with `--sort total` (REF defaults to `last`). When a filter is set, the header total reads `total: <filtered> (of <unfiltered>)`:

```
prism show heap --st alloc_space --top 5
```

```
prism show cpu --focus 'main\.' --hide 'runtime\.'
```

The filters are regular expressions matched against function names (see §6).

### `prism import trace.nettrace`

A `.nettrace` file (from `dotnet-trace` or the EventPipe) is converted by the
.NET helper into one profile per kind (`cpu`, `wall`, `memory`, `lock`,
`threads`), named `<file>-<kind>` and tagged `nettrace`. It needs the helper
(see §4a).

### `prism open [REF_OR_FILE] [--port P] [--no-browser]`

Starts the UI on a profile. If the argument is an existing file, it is imported
first, so `prism open trace.speedscope.json` is the one-step "just show me"
command. The default port is 7878; `--port 0` picks a free port.

### `prism ui [--port P] [--no-browser]`

Starts the UI on the profile list.

## 4a. Recording

### `prism record [TARGET] [--kinds K,...] [--duration 30s] [--name N] [--tag T]... [--open] [--port P]`

Captures profiles from a live process and stores them as one session. Choose
**exactly one** target:

| Target | Meaning |
|---|---|
| `--pid N` | Attach to a running Go (with the prism agent) or .NET 8+ process. |
| `--url http://127.0.0.1:6060` | Attach to a Go `net/http/pprof` endpoint (with or without the agent). |
| `--go-test PKG [-- ARGS]` | Run `go test` for one package and profile it. |
| `-- PROGRAM ARGS...` | Launch a program and profile it: a .NET program (`dotnet app.dll`, or an apphost with a `.runtimeconfig.json`) or a Go program that embeds the agent. |

`--kinds` is a comma list of `cpu,wall,memory,lock,blocking,threads`; the default
is every kind the target supports. `--duration` is `<n>ms|s|m` between `1s` and
`10m` (default `30s`). Progress goes to stderr, a summary table to stdout:

```
session 3f9a1c2b7d10  go-1234  complete  30.2s
ID            KIND      FIDELITY     SAMPLES     TOTAL  NOTE
984dbaaf5c15  cpu       native          4575    87.3 s
```

Exit codes: `0` complete (also *partial*: the target exited before the window
ended, and prism keeps what it captured), `130` cancelled with Ctrl-C (partial
data is still imported), `1` failed, `2` usage error.

What each runtime supports (no admin/root needed for any of it):

| Kind | Go | .NET |
|---|---|---|
| cpu | native | approximate (managed-thread samples) |
| wall | derived (needs the agent) | native |
| memory | native | native (allocation ticks) |
| lock | native | native |
| blocking | native | not available in Phase 1 |
| threads | native | native |

**Go targets.** Add the agent to your program (`go get prism.dev/agent`, or use a
`replace` directive while the module is unpublished):

```go
import "prism.dev/agent"

func main() {
    a, _ := agent.Start(agent.Options{}) // loopback only
    defer a.Close()
    ...
}
```

or import it with `_ "prism.dev/agent"` and start the program with
`PRISM_AGENT=1` (this is what `prism record -- ./myprog` does). The agent serves
`net/http/pprof`, adds wall-clock profiling, raises the mutex/block sampling
rates only for the capture window and restores them afterwards, and publishes
`<temp>/prism-agents/<pid>.json` so `--pid` can find it. If your program enables
block profiling itself, call `agent.SetBlockProfileRate` instead of
`runtime.SetBlockProfileRate` so the agent can put your rate back. Without the
agent, use `--url` against your own `net/http/pprof` endpoint; `wall` is then
unavailable, and `lock`/`blocking` are empty unless your program sets
`runtime.SetMutexProfileFraction` / `SetBlockProfileRate` (prism warns when
that is the case). Go 1.22+.

**.NET targets** (.NET 8+, same user, diagnostics enabled) are recorded through
the `prism-dotnet` helper, which uses EventPipe. Prism looks for it in the
`PRISM_DOTNET` environment variable (an executable, or a `.dll` run with
`dotnet`), then next to the `prism` binary. Build it with:

```bash
dotnet publish helpers/prism-dotnet -c Release -o <dir>
```

### `prism targets [--json]`

Lists what can be attached to: Go processes running the agent and .NET 8+
processes, with runtime version when known.

### `prism sessions [--json]`

Lists sessions: id, name, status, start time (UTC), duration, profile count and
target. Refer to a session by `last`, its id, a unique id prefix, or its name.

### `prism diff <BASE> <HEAD> [--st NAME] [--normalize none|total] [--top N] [--focus RE] [--exclude RE] [--hide RE]`

Compares two profiles that share a sample type (same name and unit) and prints
the functions whose self value changed most. Positive deltas (a `+` sign) mean
the head spends more. `--normalize total` scales the base to the head's total
first, which separates "this function got slower" from "the whole run was
longer". Profiles with no common sample type (for example CPU vs heap) give an
error listing both sets. Diffing a profile against itself gives all-zero deltas.

### `prism rm <REF>... [--session]`

Deletes profiles, or whole sessions (and their profiles) with `--session`. It
prints `removed <id> <name>` for each; an unknown reference is reported and the
rest are still removed. This cannot be undone.

## 5. Using the UI

### Home: Sessions and Profiles
The home page has two tabs. **Sessions** (the default when any exist) lists
recordings with a status pill, target, start time, duration and profile count; a
warning icon lists any warnings. Expand a row to see its profiles, each with a
fidelity badge; click one to open it. **Profiles** is the flat list of every
profile with its kind, fidelity, format, sample count, total and import time.
You can **drag and drop** files onto the page (or use *Choose file*) to import
without the CLI. Both lists have a **Delete** button that asks for confirmation.
`prism record --open` jumps straight to the new session (`/?session=<id>`).

### Fidelity badges
Next to every kind the UI shows `native`, `approximate`, `derived`,
`sampled external` or `unknown`; hover for what it means (see §3).

### Diff
In any profile, **Compare with…** lists other profiles of the same kind (newest
first) and opens the diff view with the current profile as the *head*. The flame
graph is colored by change: red grew, green shrank, grey is unchanged, and the
width of a frame is the larger of its base and head values. Hover for base, head
and delta. Below it, **Top changes** lists the functions that moved most. You can
**Swap** base and head, choose the sample type (only types both profiles have),
and **Normalize to head total**.

Very deep stacks are capped at 1000 frames by default (`depth=N` in the URL
changes it): the frame at the cap is hatched and its tooltip says deeper frames
were folded into it. If a graph is taller than the browser's canvas limit, the
deepest rows are clipped with a notice; use Focus or zoom to see them.

### Toolbar
- **Sample type**: choose which value to analyze, for example `inuse_space` or `alloc_space` for a Go heap profile.
- **Focus / Exclude / Hide**: regex filters, updated as you type (see §6).
- **Search**: highlights matching frames in the graph without filtering anything.
- **Views**: Flame, Icicle, Top and Sandwich.

### Flame and Icicle
Each box is a function in a call path. Its width is proportional to its total
cost. The flame view has the root at the bottom; the icicle view has the root
at the top.

- **Hover** a box to see its name, file, self, total and share of the whole.
- **Click** a box to zoom into it. **Esc** (or a double-click) resets the zoom.
- **Right-click** a box to open it in the Sandwich view.
- Very small boxes (below 0.05% of the total) are pruned to keep large profiles fast.

### Top
A table of functions with Self, Self %, Total and Total %. Click a column header
to sort by Self or Total; the server does the sorting, so Total always shows the functions with the highest total. Click a function to open it in Sandwich.

### Sandwich
This view is centered on one function. **Callers** (who calls it) are drawn
above, and **callees** (what it calls) below. Right-click any frame to re-center
on it. Use it to answer "who is calling this expensive thing?" and "where does
this function spend its time?".

### Shareable state
The URL holds everything: profile, view, sample type, filters, search and the
selected function. Reloading or bookmarking the page reproduces the exact view.

## 6. Filters

All three filters are regular expressions matched against function names, the
same semantics as `go tool pprof`:

| Filter | Effect | Example |
|---|---|---|
| **Focus** | Keeps only samples whose stack contains a match | `encodeOrder` shows only the cost under JSON encoding |
| **Exclude** | Drops samples whose stack contains a match | `runtime\.gcBgMarkWorker` removes GC worker time |
| **Hide** | Removes matching frames but keeps the sample; their cost moves to the caller | `runtime\.` flattens runtime internals |

An invalid regex shows an error banner instead of a graph.

## 7. Recipes

**"Why is my Go service using CPU?"** Run `prism show cpu`, then switch to the
Flame view. Look for wide plateaus. In the demo, `checksum` (repeated SHA-256)
and `encoding/json` dominate.

**"What is growing in memory?"** Open the heap profile and set the sample type
to `inuse_space` (live) or `alloc_space` (churn). In the demo,
`(*service).remember` keeps growing.

**"Where is lock contention?"** Open the mutex profile and use Sandwich on
`sync.(*Mutex).Unlock` to see which callers hold the lock. In the demo it is
`reserveInventory`.

**"What are my goroutines doing?"** Open the goroutine profile. Wide boxes are
many goroutines parked at the same place, such as `runtime.gopark` under
`parkedListener`.

**".NET: where is my time going?"** `dotnet-trace` speedscope exports add pseudo-frames:
- the leaves `CPU_TIME` and `UNMANAGED_CODE_TIME` (running vs. waiting/native)
- roots such as `Process64 dotnet (…)` and `(Non-Activities)`

To put self time back on real methods, set **Hide** to
`^(CPU_TIME|UNMANAGED_CODE_TIME|\(Non-Activities\)|Process64 .*)$`. To look only
at running code, set **Exclude** to `UNMANAGED_CODE_TIME`. In the demo,
`JsonSerializer.WriteBytes` dominates `cpu`, `String.Concat` dominates `alloc`,
and `ReserveInventory` (spinning while holding the lock) dominates `lock`.

**"Did my change help?"** Import the before and after profiles, tag them
(`--tag before`, `--tag after`) and compare them with `prism diff before after`
or the UI's diff view.

## 8. Troubleshooting

| Symptom | Fix |
|---|---|
| `error: cannot bind 127.0.0.1:7878` | Another prism (or another app) is using the port. Use `--port 0` or stop the other process. |
| Browser shows "UI not built" | Run `npm run build` in `ui/`, then rebuild the binary. |
| `403 forbidden` in the browser | Open the exact printed URL (`127.0.0.1:<port>`). Prism rejects other host names to block DNS-rebinding attacks. |
| `unknown format` on import | The file isn't pprof, collapsed or speedscope. Check that it isn't truncated, or convert it to one of these. |
| A speedscope / collapsed profile shows kind `unknown` | Re-import with `--kind cpu` or `--kind wall` (identical bytes are deduplicated, so delete and re-import, or use a fresh `--home`). |
| `is ambiguous (2 matches: ...)` | Several profiles share that name (for example after re-running a demo). Use the short id from `prism ls` instead of the name. |
| Mutex profile is empty (Go) | Call `runtime.SetMutexProfileFraction(n)`. For block profiles, call `runtime.SetBlockProfileRate(n)`. When recording with the prism agent this is handled for you. |

### Troubleshooting capture

| Symptom | Fix |
|---|---|
| `Go process without the prism agent: start the agent or pass --url ...` | The pid is a Go program with no prism agent. Add `prism.dev/agent`, or expose `net/http/pprof` and use `--url`. (Detected on Linux only.) |
| `pid N: not found among reachable .NET 8+ processes ...` | The process is not .NET, has `DOTNET_EnableDiagnostics=0`, runs as another user, or (Linux/macOS) uses a different `TMPDIR` than prism. Run prism as the same user with the same `TMPDIR`. |
| `the target has DOTNET_EnableDiagnostics=0` | Restart the target without that variable (or set it to 1). |
| `the target runs as another user, or TMPDIR differs` | Run prism as the target's user and with the target's `TMPDIR` (Linux/macOS). |
| `pid N is not a .NET 8+ process` | Only .NET 8 and newer are supported. |
| `the .NET helper (prism-dotnet) was not found` | Build it (`dotnet publish helpers/prism-dotnet -c Release -o <dir>`) and set `PRISM_DOTNET`, or put it next to `prism`. |
| `prism-dotnet protocol X is not supported; rebuild the helper` | The helper and prism are from different versions; rebuild the helper. |
| `program did not start the prism agent; import prism.dev/agent` | The launched Go program does not import the agent. |
| `mutex profiling is disabled in the target ...` | A warning, not a failure: the profile is empty because the target has not enabled mutex/block sampling and no agent is present. |
| Session is `partial` | The target exited before the window ended. Prism keeps what it captured. |

## 9. Where data lives

`<home>/prism.db` (SQLite metadata) and `<home>/blobs/` (compressed profiles).
Deleting the directory removes everything; `prism rm` removes individual
profiles or sessions. Temporary agent discovery files live in
`<temp>/prism-agents/`.

## 10. Coming next

- **Phase 2:** block I/O and off-CPU via eBPF (Linux) and ETW (Windows), and a timeline view.

See [PLAN.md](PLAN.md) for the full roadmap.
