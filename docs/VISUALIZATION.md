# Visualization & "find it faster" plan

**Status:** V1–V6 delivered (see §7), 2026-11-13 · **Scope:** the analysis UI/CLI for every profile kind perfhawk captures · **Relates to:** [PLAN.md](PLAN.md) §3 (views), §7 (phases), [FOLLOWUPS.md](FOLLOWUPS.md)

Goal: a user opens any profile (CPU, wall, memory, lock, blocking, threads, later block I/O) and gets to *"this function, this call path, this moment is the problem"* in seconds, without knowing the language's own tooling.

## 1. Where we are

Delivered in Phases 0 and 1: flame, icicle, top table (self/total), sandwich (callers/callees), regex focus/exclude/hide, search highlight, sample-type switch, **diff** (red/green diff flame + top-changes table, normalize), sessions, fidelity badges, depth cap, shareable URL state.

Listed in PLAN.md §3 but **not scheduled** (no phase item): call tree, call graph (the `go tool pprof -http` "Graph" view), thread/goroutine browser, source/disasm annotate, query DSL. A **timeline** is Phase 2. There is no item for "guided" features (insights, hot-path, per-kind starting views). This document adds those.

## 2. What other tools do well (research)

| Tool | Pattern worth copying | Source |
|---|---|---|
| **pprof web UI** (`go tool pprof -http`) | **Graph view**: node font/box size and colour encode flat/cum, edge thickness = resource flow, dashed edge = elided nodes (`nodecount`), solid = direct call, `(inline)` edge labels. Shows *shape* of the call structure that a flame graph hides (fan-in, shared callees). Diff colours: red = grew, green = shrank, grey ≈ 0. | [pprof docs](https://github.com/google/pprof/blob/9665fa269a30/doc/README.md) · [dotgraph.go](https://golang.org/src/cmd/vendor/github.com/google/pprof/internal/graph/dotgraph.go) |
| **Brendan Gregg: flame graph family** | Flame graph (merged, alphabetical), **icicle** (inverted), **flame chart** (time-ordered, stacks not merged), **differential** (red = more samples, blue = fewer, intensity = size of change). Known gap: a path that vanishes can't be coloured, so pair the diff graph with a table. | [differential flame graphs](https://brendangregg.com/blog/2014-11-09/differential-flame-graphs.html) · [USENIX 2017 slides](https://brendangregg.com/Slides/USENIX_ATC2017_flamegraphs) |
| **Speedscope** | Three modes on one dataset: **Time Order**, **Left Heavy** (identical stacks grouped, heaviest left), **Sandwich** (function table → callers/callees). Minimap range select, `r` collapses recursion, keyboard-first (`1/2/3`, `+/-/0`, `n/p` thread switch, `Ctrl+F` cycles matches). | [speedscope](https://github.com/jlfwong/speedscope) |
| **Firefox Profiler** | **Transforms** applied to the call tree and stored in the URL: focus function / focus subtree, merge function / merge node, collapse function subtree, collapse (direct) recursion, drop function, invert stack. Multi-thread track timeline; a range selection filters every view. | [Firefox Profiler docs](https://profiler.firefox.com/docs/) · transform semantics described by [Palette](https://docs.palette.dev/docs/profile-aggregates-transforms) (similar model; not authoritative for Firefox) |
| **FlameScope** (Netflix) | **Subsecond-offset heat map**: columns = seconds, rows = offset within the second, colour = activity. Select a pattern (stall, periodic burst) → flame graph of only that range. Finds variance and perturbations an aggregate flame graph averages away. | [FlameScope](https://github.com/Netflix/flamescope) |
| **async-profiler heatmap** | Time heat map from one recording (20 ms – 5 s squares, up to ~24 h on a page). Click/drag a range → flame graph; **Ctrl + second range = baseline → differential flame graph between two time windows**; `Ctrl+F` regex highlights matching intervals *and* frames; `Ctrl+Shift+F` filters. | [Heatmap docs](https://github.com/async-profiler/async-profiler/blob/master/docs/Heatmap.md) |
| **Grafana Pyroscope / Profiles Drilldown** | Label-sliced time series → pick baseline and comparison ranges **on the chart** → diff flame graph; side-by-side vs diff toggle (diff needs a common profile type); top table + flame graph shown together; favourites. | [Investigate trends and spikes](https://grafana.com/docs/plugins/grafana-pyroscope-app/latest/investigate/) · [Choose a view](https://grafana.com/docs/plugins/grafana-pyroscope-app/latest/choose-a-view/) |
| **Parca** | Icicle + table **side by side ("Both")**, compare two points in time, table `Diff` column, metrics-over-time graph where clicking a point opens that profile. | [Parca intro](https://www.polarsignals.com/blog/posts/2022/07/12/introducing-parca-sequel) · [icicle graph docs](https://parca.dev/docs/icicle-graph-interpretation) |
| **Perfetto UI** | Per-thread/CPU **tracks**, thread-state slices (running / runnable / blocked), flow arrows between slices, area selection → pivot table, **SQL query tab** over the same data. | [Perfetto UI](https://flutter.googlesource.com/third_party/perfetto/+/refs/heads/master/docs/visualization/perfetto-ui.md) · [PerfettoSQL](https://flutter.googlesource.com/third_party/perfetto/+show/refs/heads/main/docs/analysis/perfetto-sql-getting-started.md) |
| **Visual Studio profiler** | Call tree with **Expand / Show Hot Path** buttons (one click to the heaviest path); flame graph → "View in call tree"; a Summary page with hot path and top functions. | [CPU usage](https://learn.microsoft.com/visualstudio/profiling/cpu-usage) · [feature tour](https://learn.microsoft.com/en-us/visualstudio/profiling/profiling-feature-tour) |
| **JDK Mission Control** | **Automated analysis**: opening a JFR recording lands on a page of rule results, scored and colour-coded (yellow/red draw attention), highest score first, individually enable/disable-able. | [JFR guide](https://docs.oracle.com/en/java/java-components/jdk-mission-control/9/user-guide/using-jdk-flight-recorder.html) · [rules (JMC 6)](https://www.oracle.com/java/technologies/javase/jmc6-release-notes.html) |

Caveats on the research: the exact pprof node-size encoding differs between doc revisions (check the current UI before copying it); Firefox's own doc page was not machine-readable, so its transform list above is cross-checked against Palette's docs and my own knowledge of the product; JMC's score thresholds were not confirmed.

## 3. Design principles (what "faster" means)

1. **One screen, one answer first.** Lead with a summary ("what's hot"), not an empty graph. (JMC, Visual Studio Summary.)
2. **Same data, several lenses, one click apart.** Graph shape (flame/icicle/call graph), ranked list (top/sandwich), time (heat map/timeline). Selection and filters follow you between lenses. (Speedscope, Parca "Both".)
3. **Reductions are reversible and shareable.** Every filter/transform is a visible chip in the URL, stackable, removable. (Firefox transforms.)
4. **Comparison is first-class.** Any two profiles, any two time ranges, any two sessions. Pair the diff graph with a table so vanished frames are not lost. (Gregg, async-profiler, Pyroscope.)
5. **Kind-aware defaults.** A lock profile should open on "which lock/site waits longest", a heap profile on "who allocates most / what grew", not all on a CPU flame graph. Label the signal's fidelity (already shipped).
6. **Keyboard-first and fast.** Everything reachable by keys; every interaction under ~100 ms on large profiles.
7. **Honest about elision.** Dashed edges, truncation hatching (shipped), "N nodes hidden" counts.

## 4. Proposed features

Effort: S ≈ days, M ≈ 1–2 weeks, L ≈ 3+ weeks. "Data" = what the model needs.

### Tier A — quick wins on data we already store (no capture changes)

| # | Feature | Why / example | Data | Effort |
|---|---|---|---|---|
| A1 | **Summary landing page** per profile and per session: top 5 self, top 5 total, hot path (single heaviest root→leaf chain, expandable), biggest diff movers when a baseline exists, fidelity + warnings. | Visual Studio Summary / JMC results page. Answers "where do I look" before any graph. | aggregates | M |
| A2 | **Hot path button** in flame, icicle and a new call tree: expand/highlight the heaviest path (and "next heaviest" on repeat). | VS "Show Hot Path". | tree | S |
| A3 | **Call tree** (expandable, self/total/%, keyboard nav) linked with flame selection. Already listed in PLAN §3. | The precise, searchable companion to the graph. | tree | M |
| A4 | **Call graph view** (pprof "Graph"): nodes sized/coloured by flat/cum, edges by flow, dashed = elided, node-count slider, click = focus. Layout via a small layered-graph algorithm in the UI (no Graphviz dependency). | Shows fan-in and shared callees (e.g. five handlers all spending time in one JSON encoder) that flame graphs split across towers. | caller→callee edges from stacks | L |
| A5 | **Transforms as URL chips**: focus function, focus subtree, merge function, collapse recursion, drop function, collapse by package/module (".NET namespace", "Go package"). Replaces most regex usage for casual users; regex stays for power users. | Firefox transforms, speedscope `r`. | tree ops in `perfhawk-analysis` | M |
| A6 | **Left-heavy / merged ordering toggle** and side-by-side **flame + top table** layout. | Speedscope Left Heavy; Parca "Both". | none | S |
| A7 | **Diff improvements:** diff call tree + "regressions only / improvements only" filter, sort top-changes by %, a "new / gone" badge column, red-blue-colour-blind-safe palette toggle, side-by-side (base | head) mode. | Gregg limitation (vanished paths), Pyroscope side-by-side. | none | M |
| A8 | **Keyboard model + command palette** (`/` search, `1–5` views, `h` hot path, `[` `]` zoom history, `?` help). | Speedscope. | none | S |
| A9 | **Group-by lens for threads/goroutines**: identical-stack grouping with counts and state (PLAN §3 "thread/task browser"), and for lock/blocking profiles a "by site" table (who waits where, total wait, count, mean). | Go `goroutine?debug=1` dedup; JMC lock views. | existing `threads/lock/blocking` samples | M |

### Tier B — "insights": say what to look at (rules, not AI)

| # | Feature | Detail | Effort |
|---|---|---|---|
| B1 | **Findings panel** (local rule engine, scored, enable/disable): e.g. "one function owns >30 % self time", "recursion deeper than N", "allocation rate dominated by `X`", "lock wait > 20 % of wall in one lock site", "GC/alloc frames > 25 % of CPU", "profile is empty because mutex profiling is disabled" (we already warn), "sampling too sparse (< 100 samples)". Each finding links to the exact node/filter that shows it. | JMC automated analysis. Deterministic and explainable, so it also feeds the parked "Perfhawk Ask" later. | M |
| B2 | **Per-kind starting view** (table in §5). | Principle 5. | S |
| B3 | **Cross-kind pivot**: from a hot CPU function jump to the same function in the session's wall, lock and memory profiles ("this function also appears at 12 % of lock wait"). | Sessions already group kinds captured together. | S–M |

### Tier C — time-aware views (need timestamps; align with Phase 2's timeline)

| # | Feature | Detail | Data | Effort |
|---|---|---|---|---|
| C1 | **Time heat map** with range selection → flame graph for the range; Ctrl/second range → baseline → diff between two windows. | FlameScope + async-profiler heatmap + Pyroscope range-pick. Finds stalls, periodic bursts, warm-up vs steady state. | **per-sample timestamps** | L |
| C2 | **Timeline with tracks** (per thread/goroutine, thread state: running / runnable / blocked, GC, lock waits), range select drives every other view. | Perfetto, Firefox track timeline; PLAN Phase 2. | timeline kind (PLAN §196) | L |
| C3 | **Session timeline**: sparkline per kind across the capture window; click a spike to zoom all kinds. | Parca metrics graph. | timestamps | M |
| C4 | **History view** (multiple sessions of one target over days): a trend line of a function's self time, click a point to open or diff that session. | Pyroscope/Parca time-series → profile. Fits `perfhawk diff` and CI regression gating (PLAN §5.2). | sessions already stored | M |

### Tier D — power tools

D1 **Query tab** (filter DSL from PLAN §205, then SQL over timelines via embedded DuckDB, Perfetto-style) · D2 **Source/disasm annotate** (PLAN §3; needs DWARF/PDB/source maps) · D3 **Treemap / sunburst** (skip unless users ask; the research shows the flame/icicle + graph + table set covers the real workflows) · D4 **Export**: PNG/SVG of any view, "copy as pprof/collapsed", share link with state.

## 5. Per-kind starting view and questions answered

| Kind | Open on | Key questions the UI should answer in one click |
|---|---|---|
| **cpu** | Summary → icicle with hot path | Which function burns CPU? Which call path leads there? What changed since the baseline? |
| **wall** | Summary → icicle, **thread/state filter** (idle vs running) | Where do threads *wait*? Is wall ≫ cpu for this path? (cross-kind pivot A/B3) |
| **memory** | Summary → flame by `alloc_space`, toggle `alloc_objects` / `inuse_*` | Who allocates the most? What grew between two sessions (diff on inuse)? Which types? |
| **lock** | "by site" table (A9) → call graph of waiters | Which lock/site has the longest total and mean wait? Who holds it? |
| **blocking** (Go) | by-site table + flame | What are goroutines blocked on (channel/select/sync)? |
| **threads** | Grouped-by-identical-stack table with counts | Are 5 000 goroutines parked in the same place? Which stacks are growing? |
| **block I/O** (Phase 2) | Timeline + latency heat map + by-file/device table | Which stack issues slow I/O, and when? |

## 6. Data-model prerequisites

- **Timestamps.** Phase 1 stores aggregated samples. Tier C needs per-sample (or per-bucket) time. .NET EventPipe traces and Go execution traces carry timestamps; the pprof path does not. Decide in Phase 2: a `Timeline` kind (PLAN §196) holding bucketed counts per stack-id (e.g. 20 ms buckets) rather than raw events keeps size bounded.
- **Edges for the call graph** are derivable from stacks at query time; cache per (profile, sample type, filter) like the flame trie.
- **Per-thread / per-goroutine ids** are already captured as labels for the threads kind; extend to wall and lock samples so tracks and grouping work.
- **Performance budgets** (extend Phase 0's): call graph ≤ 1 s for 1 M samples at default node count; transforms re-run ≤ 300 ms; heat map ≤ 1 s for a 1 h capture.

## 7. Roadmap

| Step | Contents | Fits into |
|---|---|---|
| **V1 "Orient"** (≈ 3–4 wk) **— delivered** | A1 summary, A2 hot path, A3 call tree, A6, A8 keyboard, B2 per-kind defaults | before/alongside Phase 2 (no capture work) |
| **V2 "Reduce"** (≈ 3–4 wk) **— delivered** | A5 transforms-as-chips, A7 diff upgrades, A9 groupings, B3 cross-kind pivot | Phase 2 |
| **V3 "Explain"** (≈ 3 wk) **— delivered** | B1 findings engine (start with 8–10 rules), export (D4) | Phase 2 |
| **V4 "Graph"** (≈ 3 wk) **— delivered** | A4 call graph | Phase 2/3 |
| **V5 "Time"** (≈ 6–8 wk) **— delivered** | timestamps in the model, C1 heat map, C2 timeline, C3 session timeline | Phase 2 (timeline already scheduled) |
| **V6 "Trend"** (≈ 3 wk) **— delivered** | C4 history, CI regression views | Phase 3 / CI gating |
| later | D1 query tab, D2 source annotate, IDE hooks | Phase 3+ |

**Implementation plans:** [V1 Orient](superpowers/plans/2026-11-13-v1-orient.md) (delivered) · [V2 Reduce](superpowers/plans/2026-11-13-v2-reduce.md) · [V3 Explain](superpowers/plans/2026-11-13-v3-explain.md) · [V4 Graph](superpowers/plans/2026-11-13-v4-graph.md) · [V5 Time](superpowers/plans/2026-11-13-v5-time.md) · [V6 Trend](superpowers/plans/2026-11-13-v6-trend.md).

**Not planned yet (Tier D, later):** D1 query tab (filter DSL, then SQL over timelines) waits for the Phase 2 event timeline; D2 source/disasm annotate needs DWARF/PDB/source-map work in the importers; D3 treemap/sunburst is skipped unless users ask. C2 (per-thread timeline tracks) is deferred out of V5 until the Phase 2 `Timeline` kind carries thread and state events. Open questions closed by the plans: §9 Q1 (in-house graph layout, V4) and Q2 (20 ms buckets at capture time, V5).

**V1 rulings (deviations from the items above):**

- A6 left-heavy toggle dropped: the server already orders flame children by total, so the flame is left-heavy. Only the flame + top *Both* layout shipped.
- A8 trimmed: `/`, `1–6`, `h`, `?` and call-tree arrows shipped; the command palette and `[` `]` zoom history are deferred to V2.
- A1 trimmed: "biggest diff movers" on the summary needs a baseline-selection design; deferred to V2 with A7.
- Hot path "next heaviest on repeat" dropped; one deterministic path.
- B2: every kind opens on Summary; the per-kind difference is the guide card. Lock/blocking by-site tables arrive with A9 in V2.
- Default view change: links with no `view` now open Summary; `view=flame` is written explicitly.

**V2 rulings (deviations from the items above):**

- A5: "collapse by package" keeps the entry frame of each same-package run instead of synthesising a package node; `package_of` is a heuristic; indirect recursion is not collapsed; matching is by exact function name. Chips are added from the call tree (`f m x`), an add row and two global toggles; Top-table and flame-click entry points are deferred.
- A7: the regressions/improvements filter uses self delta for tables and total delta for trees; percent sort is server-side (before the row limit); side-by-side shares one diff request and does not link zoom.
- A9: no goroutine state column (the model has none); "site" is the innermost non-`runtime.`/`sync.`/`internal/` frame (Go-centric); "who holds it" is out of scope.
- B3: pivot compares each profile's default sample type by function name; shown in Sandwich and on the Summary.

**V3 rulings:** thresholds are fixed defaults in `RuleConfig` (no per-user tuning yet); rules target Go and .NET runtime frame names, other runtimes get only the language-neutral rules; disabled rules are per-browser; findings are computed per request; PNG captures the current canvas only; diff export is the flame only; the "mutex profiling disabled" finding is not repeated because capture warnings already show it.

**V4 rulings:** in-house layered layout instead of a library (closes §9 Q1; the layout module is isolated so a library could replace it); nodes are the top N by total, so a small-total high-self function can be hidden (the slider is the escape hatch); no differential graph; no node dragging or clustering.

**V5 rulings:** C2 (per-thread tracks) deferred to the Phase 2 event timeline; time data is a sidecar file beside the profile blob (no model or store migration); buckets are decided at capture time (Q2): 20 ms for .NET, one pprof window (250 ms, from the spike in [perf/timestamps.md](perf/timestamps.md)) for Go; samples carry a `perfhawk.bucket` label that the capture client turns into the sidecar; Go wall-clock (fgprof) has no windowed route so it has no time data; windows are in ms from capture start; the heat map ignores the current window; the session timeline aligns profiles by start time.

**V6 rulings:** series come from `series:<name>` tags or a key derived from the target (program basename, or URL), with no schema change; a function is identified by its exact name (a rename is one improvement and one regression); History points for sessions where the function is absent are gaps, not zeros; Enter or Open from a History point opens the profile's flame with the function highlighted (history has no per-profile function id); `perfhawk check` reads the local store only and does no PR commenting (a later phase); see [CI.md](CI.md).

Order rationale: V1–V3 need no new capture data and give the largest "time to first insight" win; V4 is the one "pprof web" gap users will notice; V5 is the largest build and unlocks the transient-problem class that aggregate graphs cannot show.

## 8. Success measures

- **Time to first insight** on the bundled demos: a new user finds the planted bug (`checksum` re-hashing, `ReserveInventory` lock, the growing cache) in ≤ 60 s using only the summary and one click. Add this as a manual acceptance script per release.
- **Clicks to answer**: hot function ≤ 1 click from home; "what changed" ≤ 2 clicks from a session.
- **No-regex path**: the common filters (focus subtree, merge, collapse recursion, hide package) are reachable without typing a regex.
- Interaction latency budgets above; keyboard coverage of every view.

## 9. Open questions

1. Is a **call graph** worth building in-house (layout engine) or should we embed an existing layered-graph library? (Licence: permissive only.)
2. Heat-map **bucket size** and where it is computed (capture time vs query time) once timestamps exist.
3. How much **rule-based insight** is enough before the AI assistant (parked in AI.md) takes over; keep rules deterministic so AI can cite them.
4. Should **side-by-side diff** replace the single diff graph by default for small profiles?
5. Colour palette policy: red/green is familiar (pprof) but red/blue (Gregg) is safer for colour-blindness; ship a toggle (A7) and pick the default after a quick user check.

## 10. Sources

- pprof: [README / web UI docs](https://github.com/google/pprof/blob/9665fa269a30/doc/README.md), [dotgraph.go (graph rendering)](https://golang.org/src/cmd/vendor/github.com/google/pprof/internal/graph/dotgraph.go)
- Brendan Gregg: [Differential flame graphs](https://brendangregg.com/blog/2014-11-09/differential-flame-graphs.html), [Visualizing Performance with Flame Graphs (USENIX ATC 2017)](https://brendangregg.com/Slides/USENIX_ATC2017_flamegraphs)
- [Speedscope](https://github.com/jlfwong/speedscope) · [FlameScope](https://github.com/Netflix/flamescope) · [async-profiler heatmap](https://github.com/async-profiler/async-profiler/blob/master/docs/Heatmap.md)
- [Firefox Profiler docs](https://profiler.firefox.com/docs/) · [Palette: profile transforms](https://docs.palette.dev/docs/profile-aggregates-transforms)
- [Perfetto UI](https://flutter.googlesource.com/third_party/perfetto/+/refs/heads/master/docs/visualization/perfetto-ui.md) · [PerfettoSQL](https://flutter.googlesource.com/third_party/perfetto/+show/refs/heads/main/docs/analysis/perfetto-sql-getting-started.md)
- Grafana: [Profiles Drilldown: investigate](https://grafana.com/docs/plugins/grafana-pyroscope-app/latest/investigate/), [choose a view](https://grafana.com/docs/plugins/grafana-pyroscope-app/latest/choose-a-view/)
- Parca / Polar Signals: [Introduction](https://www.polarsignals.com/blog/posts/2022/07/12/introducing-parca-sequel), [icicle graph interpretation](https://parca.dev/docs/icicle-graph-interpretation)
- Visual Studio: [CPU usage](https://learn.microsoft.com/visualstudio/profiling/cpu-usage), [profiling feature tour](https://learn.microsoft.com/en-us/visualstudio/profiling/profiling-feature-tour)
- JDK Mission Control: [Using JDK Flight Recorder](https://docs.oracle.com/en/java/java-components/jdk-mission-control/9/user-guide/using-jdk-flight-recorder.html), [JMC 6 release notes (rules)](https://www.oracle.com/java/technologies/javase/jmc6-release-notes.html)
