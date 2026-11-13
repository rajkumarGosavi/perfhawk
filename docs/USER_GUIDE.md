# Prism User Guide

**Applies to:** Phase 0 (Foundations) · updated 2026-11-13

Prism is a local-first profiling workbench. It brings profiles from different
languages and tools into one model, then lets you explore them through one CLI and
one browser UI. Nothing leaves your machine: data is stored locally, and the UI
server only listens on `127.0.0.1`.

> **Phase 0 scope.** Prism **imports** existing profile files: Go pprof,
> collapsed/folded stacks, and speedscope JSON (including `dotnet-trace`
> exports). Recording directly from a running Go or .NET process comes in
> Phase 1. See [PLAN.md](PLAN.md) §7.

---

## 1. Install / build

Prerequisites: Rust (stable) and Node 20+ to build. Go 1.22+ and .NET 8+ are
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

## 3. Concepts

| Term | Meaning |
|---|---|
| **Profile** | One imported file. It is identified by the SHA-256 of its bytes, so importing the same file twice is a no-op. The short id is the first 12 hex characters. |
| **Kind** | What the profile measures: `cpu`, `wall`, `memory`, `lock`, `blocking`, `block_io`, `threads`, or `unknown`. Kind is detected from the data and can be overridden with `--kind`. |
| **Sample type** | One measured value per sample. A Go heap profile has four: `alloc_objects`, `alloc_space`, `inuse_objects` and `inuse_space`. Each has a unit (`nanoseconds`, `bytes` or `count`). |
| **Self / Total** | *Self* is the cost spent in the function itself. *Total* also includes everything it calls. A recursive function is counted once per sample in Total. |
| **Reference** | How you name a profile in commands: `last` (most recent), a full id, a unique id prefix of at least 4 hex characters, or the exact display name. |

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
- **speedscope JSON**, from `dotnet-trace --format Speedscope`, `py-spy --format speedscope`, `rbspy` and others. Every profile in the file (one per thread for `dotnet-trace`) is merged, and each sample is labelled with its thread.

Windows tip: PowerShell does not expand `*.pb.gz`. List the files, or run the
command from Git Bash.

### `prism ls [--json]`

Lists profiles, newest first:

```
ID            KIND     FORMAT  SAMPLES     TOTAL  NAME    IMPORTED
984dbaaf5c15  cpu      pprof      4575    87.3 s  cpu     2026-11-13 06:05
```

TOTAL is the default sample type's total. `--json` prints full records.

### `prism show [REF] [--top N] [--st NAME] [--focus RE] [--exclude RE] [--hide RE]`

Prints the hottest functions, sorted by self value (REF defaults to `last`):

```
prism show heap --st alloc_space --top 5
```

```
prism show cpu --focus 'main\.' --hide 'runtime\.'
```

The filters are regular expressions matched against function names (see §6).

### `prism open [REF_OR_FILE] [--port P] [--no-browser]`

Starts the UI on a profile. If the argument is an existing file, it is imported
first, so `prism open trace.speedscope.json` is the one-step "just show me"
command. The default port is 7878; `--port 0` picks a free port.

### `prism ui [--port P] [--no-browser]`

Starts the UI on the profile list.

## 5. Using the UI

### Profile list
This view shows every imported profile with its kind, format, sample count,
total and import time. You can **drag and drop** files onto the page (or use
*Choose file*) to import without the CLI. Click a name to open it.

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
to sort. Click a function to open it in Sandwich.

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
(`--tag before`, `--tag after`), and compare them in two browser tabs. A
dedicated diff view arrives in Phase 1.

## 8. Troubleshooting

| Symptom | Fix |
|---|---|
| `error: cannot bind 127.0.0.1:7878` | Another prism (or another app) is using the port. Use `--port 0` or stop the other process. |
| Browser shows "UI not built" | Run `npm run build` in `ui/`, then rebuild the binary. |
| `403 forbidden` in the browser | Open the exact printed URL (`127.0.0.1:<port>`). Prism rejects other host names to block DNS-rebinding attacks. |
| `unknown format` on import | The file isn't pprof, collapsed or speedscope. Check that it isn't truncated, or convert it to one of these. |
| A speedscope / collapsed profile shows kind `unknown` | Re-import with `--kind cpu` or `--kind wall` (identical bytes are deduplicated, so delete and re-import, or use a fresh `--home`). |
| Mutex profile is empty (Go) | Call `runtime.SetMutexProfileFraction(n)`. For block profiles, call `runtime.SetBlockProfileRate(n)`. |

## 9. Where data lives

`<home>/prism.db` (SQLite metadata) and `<home>/blobs/` (compressed profiles).
Deleting the directory removes everything. There is no `prism rm` yet.

## 10. Coming next

- **Phase 1:** recording from live Go and .NET processes (`prism record`), profile diff, `.nettrace` import.
- **Phase 2:** block I/O and off-CPU via eBPF (Linux) and ETW (Windows), and a timeline view.

See [PLAN.md](PLAN.md) for the full roadmap.
