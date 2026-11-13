# Unified Profiling Workflow App — Research & Plan

**Status:** Draft v0.1 · 2026-11-13
**Working name:** `prism` (placeholder; one CLI binary `prism`, one UI)

**Goal:** One consistent, *on-demand* (not continuous) profiling workflow across languages for six profile kinds: **CPU, wall-clock, memory, mutex/lock, block I/O, goroutine/thread**.

---

## 0. Decisions (2026-11-13)

| Topic | Decision |
|---|---|
| UI surface | **CLI + local browser UI** (`prism ui`); Tauri desktop wrapper later, same code |
| Primary users | **Developers, local machines / local containers**; remote, Kubernetes, SSH deferred to Phase 3 |
| MVP languages | **Go and C# / .NET** |
| MVP operating systems | **Linux, macOS and Windows** (full cross-platform from the start) |
| License | **Open core** — see §0.2 |
| AI | Deferred until Phases 0–3 ship — [AI.md](AI.md) |

### 0.1 MVP capability matrix (Go + .NET × 3 OS)

| Kind | Go (all OS) | .NET (all OS) | OS-specific notes |
|---|---|---|---|
| CPU | pprof `cpu` — native | EventPipe `SampleProfiler`, filtered to running managed threads — *approximate*; Windows: ETW kernel CPU sampling — native; Linux: `perf` optional for native frames | — |
| Wall-clock | fgprof / execution-trace derived — derived | EventPipe `SampleProfiler` (samples all managed threads in any state) — native | — |
| Memory | pprof `heap`/`allocs` — native | `GCAllocationTick` (alloc sampling) + `gcdump` (live heap) — native | — |
| Lock / mutex | pprof `mutex` (needs `SetMutexProfileFraction`) — native | `ContentionStart/Stop` events with durations — native | — |
| Blocking / wait | pprof `block` — native | wait-state stacks from SampleProfiler — derived | — |
| Block I/O | execution-trace syscall events — derived | Windows: ETW `FileIO`/`DiskIO` — native | **Linux:** eBPF (`biolatency`/`fileslower`-style) — native, needs `CAP_BPF`. **Windows:** ETW, needs admin. **macOS:** no eBPF; `fs_usage`/dtrace need root + SIP limits → *approximate*, ship as best-effort |
| Threads & tasks | pprof `goroutine` debug=2 — native | Diagnostics IPC stack snapshot (`dotnet-stack` equivalent) + thread-pool counters — native | — |

**.NET integration approach.** .NET exposes everything through the cross-platform **Diagnostics IPC** protocol (Unix domain socket on Linux/macOS, named pipe on Windows), which streams EventPipe `.nettrace` data. Plan:
- MVP: a small bundled **.NET helper (`prism-dotnet`)** built on `Microsoft.Diagnostics.NETCore.Client` + `TraceEvent` that starts sessions, parses `.nettrace` and emits our unified model. Fastest path to correct parsing (TraceEvent handles rundown, method maps, R2R/JIT symbols).
- Later: native Rust Diagnostics IPC client + nettrace parser to drop the .NET dependency.
- Requires the target's runtime to have diagnostics enabled (default on; `DOTNET_EnableDiagnostics=0` disables it — detect and explain).

**Go integration approach.** No helper needed for attach-style capture when the app exposes `net/http/pprof`; otherwise provide `prism/agent` (a tiny Go package: pprof + exec trace + flight recorder + fgprof behind one endpoint) and a launch mode for `go test`/`go run` that injects profiling flags.

### 0.2 Open-core boundary

| Open source (Apache-2.0) | Commercial |
|---|---|
| CLI, local UI, all providers, unified model, importers/exporters, analysis engine, local store, diff, eBPF/ETW providers, provider SDK | Team share server (hosted or self-hosted), workspaces, comments/annotations sync, SSO/RBAC, retention policies, org policy & audit, CI regression dashboard across repos, priority support; AI features later |

Rules that follow from open core: everything bundled must be permissively licensed (Apache/MIT/BSD); GPL tools (e.g. `perf`) only invoked as external processes; a **CLA or DCO** for contributions so the commercial tier stays possible; keep the commercial code in a separate repository talking to the core only through public APIs.

---

## 1. Positioning: what we are and aren't

| | Continuous profilers (Pyroscope, Parca/Polar Signals, Datadog, OTel eBPF profiler) | Native per-language tools (pprof, async-profiler, py-spy, dotnet-trace…) | **This app** |
|---|---|---|---|
| Mode | Always on, fleet-wide, low-rate sampling, time-series storage | Ad hoc, one process, one language | **Ad hoc sessions**, one or a few targets, any language |
| Depth | Mostly CPU + alloc; shallow | Deep, language-specific | Deep (we drive the native tools) |
| UX | Consistent, but web/server-heavy | Inconsistent: every tool has its own flags, formats, viewers | **Consistent CLI + UI + format across languages** |
| Infra | Server, DB, agents | None | **None required** — single binary, local-first; optional share server |

The gap: developers today learn 4–6 different tools, flags, permission models, output formats and viewers per language. Continuous profilers solve the consistency problem but require infrastructure and give shallow data. We sit in between: **a local-first "profiling workbench" that orchestrates best-of-breed native profilers and normalizes everything into one model and one UI.**

---

## 2. Landscape research

### 2.1 Language-native profilers (what we'll orchestrate)

**Go** — the reference model for our feature set.
- `runtime/pprof`, `net/http/pprof`, `go test -cpuprofile/-memprofile/-blockprofile/-mutexprofile`.
- Profiles: `cpu`, `heap` (inuse/alloc space & objects), `allocs`, `goroutine` (with full stacks, `debug=2`), `mutex` (contention, requires `SetMutexProfileFraction`), `block` (time blocked on channels/select/sync — **not disk I/O**), `threadcreate`.
- `runtime/trace` + `go tool trace`: execution timeline, scheduler latency, GC, syscalls, network blocking. Flight recorder (Go 1.25+) gives "last N seconds" traces on demand — great fit for triggered captures.
- Wall-clock is not built in; available via `fgprof` (samples all goroutines) or derived from execution traces.
- PGO uses CPU pprof — we can export PGO-ready profiles.

**Java / JVM**
- **async-profiler** (Apache-2.0): CPU (perf_events / itimer / ctimer), `wall`, `alloc` (TLAB-based), `lock` (contended monitors + j.u.c locks), `nativemem` (malloc tracking, 4.x), native lock profiling (4.3), hardware counters, heatmaps; outputs collapsed, flame HTML, JFR, pprof (via converter), OTLP. Attaches to a running JVM.
- **JFR / JMC**: built into JDK; CPU sampling, allocation, monitor-enter/park (lock), file/socket I/O events with durations, GC, thread dumps. Low overhead, production safe.
- Thread dumps (`jcmd Thread.print`, virtual-thread dumps `jcmd Thread.dump_to_file`) = our "goroutine/thread" profile.
- Heap dumps (`.hprof`) + Eclipse MAT for retained-size/leak analysis.

**Python**
- **py-spy** (MIT): sampling, attach without code changes, `--idle` gives wall-clock, `--gil`, `--native`, `dump` (= thread snapshot). Outputs speedscope/flamegraph/raw.
- **Austin**: frame-stack sampler, CPU + wall + memory modes, very low overhead.
- **Scalene**: CPU (Python vs native split), GPU, memory, copy volume; line-level.
- **memray** (Bloomberg, Apache-2.0): every allocation incl. native, leak mode, flamegraph/table/tree reporters; attach supported.
- **pyinstrument**: wall-clock statistical, great for async code.
- `tracemalloc` (stdlib), `cProfile` (deterministic). Python 3.12+ `sys.monitoring`; 3.14 adds remote debugging attach (PEP 768) which simplifies safe attach.
- Locks: no first-class tool; approximate via wall-clock samples filtered on `threading.Lock.acquire`/GIL wait (py-spy `--gil`), or eBPF uprobes.

**Node.js / JS**
- V8 CPU profiler: `--cpu-prof`, Inspector protocol (`Profiler.start`), `.cpuprofile` format.
- Heap: `--heap-prof` (sampling heap profiler), heap snapshots (`.heapsnapshot`) via Inspector, `--heapsnapshot-signal`.
- `0x`, Clinic.js (doctor/flame/bubbleprof — bubbleprof for async waits, now unmaintained), `@datadog/pprof` (native pprof output for Node), `perf` with `--perf-basic-prof`.
- Event-loop delay (`perf_hooks.monitorEventLoopDelay`) and async-hooks ≈ our "blocking" signal. Locks mostly N/A (Atomics.wait in workers).

**.NET**
- `dotnet-trace` (EventPipe → `.nettrace`, speedscope/chromium export), `dotnet-counters`, `dotnet-gcdump`, `dotnet-dump` (thread stacks, heap), `dotnet-stack` (thread snapshot). Profiles: CPU sampling, allocations (GC AllocationTick), contention events (lock), thread-pool starvation, file I/O via events.
- Windows: PerfView, ETW/WPR/WPA.

**Ruby** — `rbspy` (attach sampler, CPU/wall), `stackprof` (cpu/wall/object modes), `vernier` (multi-thread, GVL and GC aware, outputs Firefox Profiler format), `memory_profiler`, `heap-profiler`.

**PHP** — Excimer (sampling, used by Wikimedia), SPX, Xdebug profiler, `phpspy`.

**Rust / C / C++ (native)**
- Linux `perf` (+ `perf record -g`, `--call-graph dwarf`, off-CPU via `sched:` tracepoints), `samply` (perf/ETW/macOS, Firefox Profiler UI), `pprof-rs`, `cargo flamegraph`.
- Memory: `heaptrack`, `bytehound`, jemalloc/tcmalloc heap profiling (pprof output), `dhat`, Valgrind massif.
- Locks: `perf lock`, `mutrace`, eBPF `klockstat`/uprobes on `pthread_mutex_lock`.
- Tracy (frame/instrumentation profiler, games), Intel VTune, AMD uProf.

**OS level (language-agnostic) — essential for block I/O and off-CPU**
- **Linux eBPF**: BCC/bpftrace tools `offcputime`, `biolatency`, `biosnoop`, `biotop`, `fileslower`, `ext4slower`, `runqlat`, `profile`, `memleak`, `klockstat`, `wakeuptime`. Libraries: libbpf / `libbpf-rs` / Aya (Rust).
- **OTel eBPF profiler** (donated by Elastic): whole-system CPU stacks with interpreter unwinding for Python, Ruby, PHP, Node, Java, .NET, Go, BEAM — no instrumentation. We can embed/reuse its unwinders for a "profile any process" mode.
- **Windows**: ETW (CPU sampling, CSwitch for wall/off-CPU, DiskIO, FileIO events) via WPR, `samply`, or `ferrisetw`/`krabsetw`.
- **macOS**: Instruments/`xctrace` (Time Profiler, Allocations, System Trace, File Activity), `sample`, dtrace (limited by SIP).

### 2.2 Viewers / analysis UIs (UX inspiration, some embeddable)

| Tool | Strengths to borrow |
|---|---|
| **pprof web UI** (`go tool pprof -http`) | Graph view, flame, source/disasm, `focus/ignore/hide`, `-diff_base`, sample-type switch |
| **speedscope** | Fast WebGL, "Time Order / Left Heavy / Sandwich" views; universal importer |
| **Firefox Profiler** | Multi-thread timeline + markers + call tree; URL-based sharing; transforms (merge, focus, collapse recursion) |
| **Perfetto UI** | Trace timelines at scale, SQL over traces (trace processor), Chrome trace format |
| **JMC / Eclipse MAT / VisualVM** | Automated rule-based "findings"; retained-size/dominator tree for heap |
| **Pyroscope / Grafana flame graph** | Diff flame graphs, label-based slicing, tag explorer |
| **Clinic.js Doctor** | Guided diagnosis ("you have an I/O issue → run bubbleprof") — great onboarding model |
| **Polar Signals / Parca UI** | Icicle graph + table + source, pprof query language |

### 2.3 Formats we must read/write

`pprof` (protobuf, de-facto interchange) · **OTLP Profiles** (pprof-derived; public alpha since Mar 2026, still breaking) · JFR · `.hprof` · Chrome `.cpuprofile` / `.heapsnapshot` / Trace Event JSON · speedscope JSON · Gecko/Firefox Profiler JSON · collapsed/folded stacks (Brendan Gregg) · `perf.data` / `perf script` · `.nettrace` / `.gcdump` · memray `.bin` · Go execution trace · ETL (Windows).

**Decision:** internal model = pprof-compatible + OTLP-Profiles-aligned superset (see §4.2), so we can export to any continuous profiler later without being one.

---

## 3. The hard part: normalizing six profile kinds across languages

The same word means different things per runtime. The UI must present **one semantic** per kind and clearly label the source signal.

| Kind (our definition) | Go | Java | Python | Node | .NET | Ruby | Native (C/C++/Rust) | OS fallback (any lang) |
|---|---|---|---|---|---|---|---|---|
| **CPU** — on-CPU time per stack | pprof cpu | async-profiler cpu / JFR ExecutionSample | py-spy / Austin | V8 cpu-prof | dotnet-trace sampling | rbspy / stackprof cpu | perf / samply | eBPF `profile`, OTel eBPF profiler, ETW |
| **Wall-clock** — on+off CPU time per thread | fgprof / exec trace | async-profiler `wall` | py-spy `--idle`, pyinstrument | derived (cpu + event-loop idle) | thread-time sampling | rbspy, stackprof wall, vernier | perf + sched (off-CPU merge) | eBPF `offcputime` + `profile` merged |
| **Memory** — alloc rate, live heap, leaks | pprof heap/allocs | async-profiler alloc/nativemem, JFR, hprof | memray, tracemalloc, Scalene | heap-prof, heap snapshot | GC AllocationTick, gcdump | memory_profiler, stackprof object | heaptrack, jemalloc prof | eBPF `memleak` |
| **Lock / mutex** — contention wait per stack | pprof mutex | async-profiler lock, JFR monitor/park | py-spy `--gil` + uprobes (approx) | N/A (Atomics) — hide | ContentionStart/Stop | vernier GVL | perf lock, uprobes pthread | eBPF `klockstat`, futex tracing |
| **Block I/O** — time/bytes in disk & file I/O per stack | exec trace syscalls + eBPF | JFR FileRead/Write, eBPF | eBPF uprobes/syscalls | eBPF / async_hooks fs | FileIO events, ETW | eBPF | eBPF | **eBPF `biolatency`/`biosnoop`/`fileslower`, ETW DiskIO** |
| **Goroutine / thread** — snapshot of all execution units + state + stacks | pprof goroutine debug=2 | jcmd Thread.print / virtual-thread dump | py-spy dump | Inspector (main + workers), async stack | dotnet-stack | rbspy snapshot | gdb/eu-stack, `/proc/*/task` | `/proc`, ETW thread events |

**Key product decisions from this table**
1. **"Block" ≠ "Block I/O".** Go's `block` profile measures channel/select/sync waits. We expose it as **"Blocking / wait"** (a sub-type of wall-clock/off-CPU), and keep **"Block I/O"** strictly for disk/file I/O — implemented mostly via **eBPF (Linux) / ETW (Windows)** because few runtimes provide it.
2. **Rename "goroutine profile" → "Threads & tasks"** in the cross-language UI (goroutines, OS threads, Java virtual threads, asyncio tasks, Node workers, .NET threads), with a language-specific label.
3. Every profile carries a **fidelity badge**: `native` (runtime-provided), `sampled-external` (py-spy/eBPF), `derived` (computed), `approximate`. Unsupported combos are shown greyed out with "why" + alternative.
4. One **capability registry** (data, not code) drives the CLI help, UI menus and docs.

---

## 4. Architecture

```
            ┌──────────────── UI (React + TS, WebGL flame graphs) ───────────────┐
            │  Desktop (Tauri)   │   Browser (`prism ui`)   │   IDE (VS Code/JB) │
            └───────────────┬────────────────────────────────────────────────────┘
                            │ JSON-RPC / gRPC (local)        Analysis also runs as WASM in UI
┌───────────────────────────┴─────────────────────────────────────────────────────┐
│ prism core (Rust, single static binary)                                         │
│  ┌──────────────┐ ┌────────────────┐ ┌───────────────┐ ┌─────────────────────┐  │
│  │ Session      │ │ Analysis engine│ │ Symbolizer    │ │ Store               │  │
│  │ orchestrator │ │ merge/diff/    │ │ DWARF, PDB,   │ │ SQLite metadata +   │  │
│  │ (workflows,  │ │ filter/top/    │ │ ELF, Mach-O,  │ │ content-addressed   │  │
│  │ triggers)    │ │ tree/timeline  │ │ source maps,  │ │ blobs (zstd)        │  │
│  └──────┬───────┘ └────────────────┘ │ debuginfod    │ └─────────────────────┘  │
│         │                            └───────────────┘                          │
│  ┌──────┴──────────────────────── Provider SDK ─────────────────────────────┐   │
│  │ go │ jvm │ python │ node │ dotnet │ ruby │ native │ ebpf │ etw │ import  │   │
│  └──────┬───────────────────────────────────────────────────────────────────┘   │
└─────────┼───────────────────────────────────────────────────────────────────────┘
          │ launch / attach / HTTP endpoint / signal / remote
   Targets: local process · container · Kubernetes pod (ephemeral container) · SSH host
```

### 4.1 Providers (adapters)
Each provider implements one interface:
```
detect(target) -> RuntimeInfo            // language, version, pid, permissions
capabilities(runtime) -> [ProfileKind × Fidelity × Options]
prepare(target, kinds)  -> Plan          // e.g. install async-profiler, enable mutex fraction
capture(plan, duration|until|trigger) -> RawArtifacts
convert(raw) -> [Profile | Timeline | Snapshot]
```
- Providers **drive native tools** (bundled or auto-downloaded, checksum-verified) instead of re-implementing them. Run GPL tools (perf) as subprocesses only.
- Providers are plugins (WASM or out-of-process over stdio) so the community can add languages.
- **Auto-detect** runtime from the PID (`/proc/pid/exe`, loaded libs: `libjvm.so`, `libpython*`, `node`, Go buildinfo, `coreclr`).

### 4.2 Unified data model (3 shapes)
1. **Profile** (aggregated stacks) — pprof-compatible: `sample_types[] (type, unit)`, `samples[] {stack, values[], labels}`, `locations/functions/mappings`, plus extensions: `thread/task id`, `fidelity`, `span_id/trace_id` (OTel link), `source_lang` per frame (mixed Python+C frames).
2. **Timeline** (events over time) — for wall-clock, Go exec traces, JFR events, I/O events, GC; Perfetto/Chrome-trace-compatible; queryable.
3. **Snapshot** — thread/goroutine dumps and heap snapshots (object graph → dominator tree, retained size).

Export: pprof, OTLP Profiles, speedscope, Firefox Profiler, collapsed, Chrome trace, JFR (where possible).

### 4.3 Analysis engine (Rust, also compiled to WASM)
- Views: **flame / icicle**, **sandwich** (callers+callees of a function), **call tree**, **top table** (self/total), **source & disasm annotate**, **timeline**, **thread/task browser** (group goroutines by identical stack — like `goroutine` dedup, with state counts).
- Transforms (Firefox-Profiler style, composable, recorded in URL): focus, exclude, merge function, collapse recursion, collapse library, drop idle, group by label/thread.
- **Diff**: two profiles or two groups (A/B), normalized by total or by time; diff flame graph; automatic "top regressions" table.
- **Query language**: small filter DSL (`kind=cpu fn~"json" thread="worker-*"`) + optional SQL over timelines (DuckDB embedded).

### 4.4 Store & sharing
- Local workspace: `~/.prism` or per-repo `.prism/` — SQLite (sessions, tags, git commit, env, host) + compressed blobs.
- Share: export a self-contained `.prism` bundle (profile + symbols + metadata), or push to an **optional** tiny share server (single container, S3 backend). This is *not* a continuous profiling backend — just artifact hosting with links and comments.

### 4.5 Tech stack (recommended)
- **Core/CLI:** Rust (single binary, fast parsing of large profiles, WASM target, `aya`/`libbpf-rs` for eBPF, `gimli`/`object`/`pdb` crates for symbolization).
- **UI:** TypeScript + React (or Solid) + canvas/WebGL flame graph renderer; virtualized tables; same bundle for desktop/web/IDE webview.
- **Desktop:** Tauri (small, native, reuses Rust core in-process).
- **Language helper agents (optional, tiny):** Go package `prism/agent` (exposes pprof + trace + fgprof over one endpoint), Python/Node/Java helpers only where attach is impossible.

---

## 5. The workflow (the "consistent interface")

Same five verbs in every language, CLI and UI mirror each other:

```bash
prism targets                          # list processes/pods with detected runtime + supported kinds
prism record --pid 4123 --kinds cpu,wall,lock --duration 30s
prism record -- python app.py          # launch mode
prism record --k8s pod/api-7f9 -c app --kinds cpu,io
prism record --pid 4123 --trigger "cpu>80% for 10s" --kinds cpu,threads
prism open last                        # UI on the latest session
prism diff main.prism feature.prism    # A/B
prism report last --format md|html     # shareable findings
prism export last --to pprof|otlp|speedscope
prism import trace.jfr heap.hprof profile.cpuprofile   # analyze anything
```

**Session = unit of work:** target metadata, environment, git SHA, multiple kinds captured *concurrently over the same window* (so CPU, lock and I/O line up on one timeline), notes, tags.

**Guided mode ("Doctor"):** a 60-second triage capture (CPU + wall + threads + basic I/O + runtime counters) → rule engine says "62% of wall time is off-CPU, mostly in `read()` on /data → open Block I/O view" (inspired by Clinic Doctor & JMC rules).

---

## 6. Additional features I recommend

**High value, fits the non-continuous scope**
1. **Execution timeline view** — correlate CPU, GC pauses, lock waits, I/O and scheduler latency on one time axis (Go trace, JFR, ETW, eBPF). This is where most "why is it slow" answers come from.
2. **Diff & regression gating in CI** — `prism bench`/`prism ci`: profile benchmarks (`go test -bench`, JMH, pytest-benchmark, criterion), compare against baseline artifact, fail/comment on PR when a function regresses > X%.
3. **Heap snapshot analysis & leak finder** — dominator tree, retained size, "growth between snapshot 1 → 2 → 3" (3-snapshot technique), duplicated strings/collections.
4. **Off-CPU & scheduler analysis** (runqueue latency, CPU throttling in containers — cgroup `cpu.stat` throttled time) — huge real-world win for Kubernetes users.
5. **Triggered / flight-recorder captures** — capture on threshold, signal, HTTP call, or "last 30s" ring buffer (Go flight recorder, JFR, perf ring buffer). Gives the benefits of continuous profiling for incidents without running a system.
6. **AI analysis assistant ("Prism Ask")** — natural-language questions, auto-insights, explain-this, compare narratives, capture copilot; expose an **MCP server** (`prism mcp`) so coding agents can record/analyze profiles directly. Opt-in and local-model-capable for privacy. **Deferred until the basic app is ready** — design parked in [AI.md](AI.md).
7. **IDE integration** — VS Code/JetBrains: "Profile this test/run config", inline gutter annotations with self/total time per line, jump from flame frame to source.
8. **PGO export** — produce Go PGO `default.pgo`, LLVM/`.profdata`-ready, .NET/JVM hints.
9. **Runtime counters panel** — GC stats, heap size, goroutine/thread counts, event-loop lag, GIL contention, sampled alongside the capture.
10. **Trace ↔ profile linking** — attach OTel `trace_id/span_id` labels (Go pprof labels, async-profiler context, Python contextvars) so users can filter a profile to one slow request.

**Platform / quality**
11. **Safety guardrails** — overhead budget per kind, "production-safe" preset, dry-run showing exactly what will be attached/enabled, auto-revert of runtime settings (e.g. reset mutex fraction).
12. **Permissions helper** — explains & fixes `perf_event_paranoid`, `ptrace_scope`, `CAP_SYS_PTRACE/CAP_BPF`, SIP on macOS, admin on Windows.
13. **Symbol management** — debuginfod client, PDB symbol servers, source-map upload, split DWARF, stripped-binary warnings.
14. **Redaction** — scrub paths, hostnames, env vars, string contents of heap snapshots before sharing.
15. **Scripting API / notebooks** — Python & TS bindings over the analysis engine (`prism.load("x.prism").top(10)`), Jupyter integration.
16. **Collaboration** — comments pinned to frames, saved views (URL encodes transforms), report templates.

**Later / optional**
17. GPU profiling (CUDA CUPTI, PyTorch profiler import), energy/power (RAPL), hardware counters (cache misses, IPC), WebAssembly and browser (Chrome trace import), mobile (Android simpleperf, iOS xctrace import).

---

## 7. Roadmap

| Phase | Scope | Exit criteria |
|---|---|---|
| **0. Foundations** (4–6 wk) | Rust core, unified model, pprof/collapsed/speedscope importers (cpuprofile/JFR → Phase 3), flame/icicle/top/sandwich UI, local store, `prism import/ls/show/open/ui` — [implementation plan](superpowers/plans/2026-11-13-phase0-foundations.md) | 500 MB pprof imports ≤ 20 s; flame/top queries ≤ 1 s |
| **1. MVP capture** (8–10 wk) | Providers: **Go, .NET** (via `prism-dotnet` helper) · kinds: CPU, wall, memory, lock, threads · launch + attach · sessions · diff · **Linux, macOS, Windows** | Same `prism record` UX works for Go and .NET on all three OSes; matrix in §0.1 green except Block I/O — [implementation plan](superpowers/plans/2026-11-13-phase1-capture.md) |
| **2. Contention & I/O** (6–8 wk) | Block I/O: Linux **eBPF provider**, Windows **ETW provider**, macOS best-effort; off-CPU / scheduler analysis; timeline view; Doctor mode | Block I/O per stack for Go and .NET on Linux & Windows. [Design](superpowers/specs/2026-11-13-phase2-contention-io-design.md). **Status:** M1 shared core and M2 `prism doctor` done ([plan 2a](superpowers/plans/2026-11-13-phase2a-core-doctor.md)); M3 ETW, M4 eBPF and M5 macOS pending |
| **3. Breadth** (8 wk) | Java, Python, Node, then Ruby, Rust/C/C++ (perf/samply); Docker/Kubernetes/SSH targets; heap snapshot analysis; native Rust nettrace parser | Full languages × 3 OS capability matrix published |
| **4. Workflow** (ongoing) | CI regression gating, IDE plugins, triggers/flight recorder, share server, plugin SDK | Teams use it in PR review |
| **5. AI (deferred)** | Prism Ask per [AI.md](AI.md) — picked up only after Phases 0–3 ship | — |

---

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Semantic mismatch between languages misleads users | Fidelity badges, per-kind definitions in UI, "how this was measured" panel |
| Attach/permission failures (ptrace, eBPF, SIP, containers) | Permissions doctor; launch-mode fallback; ephemeral debug containers in k8s |
| Symbolization (stripped binaries, JIT, source maps) | Debuginfod/symbol servers, perf-map files for JITs (`--perf-basic-prof`, async-profiler handles JVM) |
| Large artifacts (heap snapshots, long traces) | Streaming parsers in Rust, columnar storage, on-demand loading, WASM workers |
| Overhead on production targets | Conservative defaults, duration caps, overhead estimates shown before capture |
| Dependency on third-party tools / licenses | Bundle only permissive (Apache/MIT) tools; GPL tools (perf) invoked externally; pin versions + checksums |
| OTLP Profiles still alpha & changing | Own internal model; OTLP as export adapter only, versioned |
| Scope explosion (7 langs × 6 kinds × 3 OS) | Capability registry + strict phase gating; community providers via SDK |

---

## 9. Open questions

All questions are answered — see §0. Follow-ups decided 2026-11-13:
1. Minimum supported target versions: **Go 1.22+, .NET 8+**.
2. The self-contained `prism-dotnet` helper (+30–60 MB) is **acceptable for the MVP**; the native Rust nettrace parser stays in Phase 3.

---

## Sources
- [OpenTelemetry Profiles enters public alpha (Mar 2026)](https://opentelemetry.io/blog/2026/profiles-alpha)
- [OTel Profiles signal concept docs](https://opentelemetry.io/es/docs/concepts/signals/profiles/)
- [ClickHouse: OTel profiles signal explainer](https://clickhouse.com/resources/engineering/otel-news-profiles-signal)
- [Datadog OTel news, Mar 2026 (eBPF profiler language support)](https://opensource.datadoghq.com/otel-news/2026/03/)
- [InfoQ: Pyroscope 2.0](https://infoq.com/news/2026/05/pyroscope-2-profiling/)
- [Better Stack: Parca alternatives 2026](https://betterstack.com/community/comparisons/parca-alternatives/)
- [async-profiler releases](https://www.github.com/async-profiler/async-profiler/releases?page=2)