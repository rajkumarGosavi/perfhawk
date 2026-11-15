# Design: .NET heap snapshots, retention paths, leak analysis

Status: proposal (not started). Goal: close the gap vs dotMemory: "what keeps this object alive", retained size, leak suspects.

## Today

- Have: Go heap profiles, .NET `GCAllocationTick` (approximate), GC analysis (pauses, live heap, reasons).
- Missing: object graph. No retained size, no root paths, no snapshot diff by type.
- Helper `helpers/perfhawk-dotnet` already references `Microsoft.Diagnostics.NETCore.Client` and `Microsoft.Diagnostics.Tracing.TraceEvent` 4.0.0.

## Research findings

Source: Microsoft docs for `dotnet-gcdump`.

- `gcdump` triggers a gen2 GC in the target, enables GC heap-walk events over EventPipe, and rebuilds the object graph from the event stream. Output `.gcdump`. Viewable only on Windows (PerfView / Visual Studio).
- Answers "what still references this type" and type counts. Does not give per-field edges or thread stacks.
- Pause: gen2 GC can suspend the runtime long on big heaps. Not for latency-sensitive prod.
- Lossy below .NET 11: event buffers can fill, large heaps give incomplete dumps (error "start of a heap dump but not its completion"). .NET 11+ has blocking buffering. Event buffer lives in the target, up to 256 MB.
- Full process dump + ClrMD is the fallback Microsoft recommends for incomplete dumps. ClrMD gives `ClrHeap` object enumeration, like SOS.

## Spike results (2026-11-15)

Checked by reflection and a live run against a demo process (200k `Leaky` objects, each holding a `byte[]`).

- TraceEvent 4.0.0 has **no public** `MemoryGraph` / `GCHeapDump`. Decision: parse the raw events ourselves. They are public: `GCBulkNode` (address, size, typeId, edgeCount), `GCBulkEdge` (target, referencingField), `GCBulkRootEdge` (rooted address, kind, flags), `TypeBulkType` (typeId to name). Event name for types is `TypeBulkType`, not `GCBulkType`.
- One keyword does it: `ClrTraceEventParser.Keywords.GCHeapSnapshot` = `0x1980001` (GC + GCHeapDump + GCHeapCollect + GCHeapAndTypeNames + Type), Verbose level, no rundown.
- Live capture worked via `DiagnosticsClient.StartEventPipeSession` on the helper's existing packages: 1 induced GC, 403,360 nodes, 408,236 edges, 42 roots, 149 type records, 33.6 MB heap. Top types matched the planted graph exactly (200,003 `Byte[]`, 200,000 `Leaky`). Full graph recovered, nothing dropped at this size.
- `ReferencingField` was 0 on every edge (runtime 8.0.34), so **no field names** in option A. Matches the docs; retention paths show type chains only.
- Not checked: `.gcdump` file layout (importing existing files), completion marker for the snapshot (spike used a fixed 10 s window), behaviour on a large heap (lossy buffering), runtimes other than 8.0.34.

Spike code is a throwaway in the session scratchpad, not in the repo.

## Options

| | A. EventPipe gcdump | B. Process dump + ClrMD |
|---|---|---|
| Fidelity | type, size, edges; no field names | full: addresses, fields, strings, stacks |
| Pause | gen2 GC + event stream | dump write, longer, large file |
| Size | small | GBs, may contain secrets |
| Risk | lossy on <.NET 11 | fits big heaps, needs `createdump` perms |
| Fit | on-demand, local | later, "deep" mode |

Recommend A for v0.2, B as an opt-in "deep" mode later. Both normalize to the same snapshot model, so the UI and engine are shared.

## Snapshot model

New profile kind `heap_snapshot`, fidelity badge `native` (A) / `native-full` (B).

- `types`: id, name, module.
- `nodes`: id, type_id, shallow_size (and address if known).
- `edges`: CSR arrays (`offsets: u32[n+1]`, `targets: u32[m]`); optional `field_id` in B.
- `roots`: node_id, kind (static, stack, handle, finalizer, pinned).
- Store zstd-compressed columnar blob next to the profile row; do not put nodes in SQLite rows.
- Metadata: runtime version, GC mode, timestamp, complete flag, dropped-event hint.

## Engine (Rust analysis crate)

1. Type aggregate: count, shallow bytes per type. Cheap, always.
2. Dominator tree: Lengauer-Tarjan over CSR from a virtual root. Retained size per node, top-N by retained.
3. Retention path: shortest root-to-object path (BFS from roots), shown as type chain. Return up to k distinct paths.
4. Diff: type-level count and bytes between snapshots A and B. Leak suspect = type growing across 3+ snapshots, or top-retained node growing.
5. Memory budget: graph of 20M+ objects x (4B offset + edges) is GBs. Mitigate: u32 ids, drop nodes unreachable from roots, cap analysis size with a clear "truncated" notice, compute dominators lazily and only for top types.

## Surfaces

- CLI: `perfhawk snapshot --pid N` (warn about the gen2 pause, require `--yes` or confirm), `perfhawk show heap --top N`, `perfhawk diff snap1 snap2`, `perfhawk check` gains a type-growth threshold.
- Import: `.gcdump` files (parsed in the .NET helper, emitted over the existing helper protocol in `Protocol.cs`).
- UI: Heap tab: Types table (count, shallow, retained), dominator tree / treemap, retention-path panel for the selected type, diff view of growth between snapshots.
- Findings: `heap_growth_suspect`, `large_retained_cache`, `finalizer_backlog`, `snapshot_incomplete` (warns that data was dropped).

## Milestone 2 status (branch `feat/heap-snapshots`)

Done: `HeapSnapshot` model with binary format (`perfhawk-model/src/heap.rs`), `.heap.zst` store sidecar, per-type summary (`perfhawk-analysis/src/heap.rs`), helper `snapshot` command (`Snapshot.cs`), `perfhawk snapshot --pid` and `perfhawk heap`. Verified live on the demo process: 403,360 objects, same counts as the spike.

Findings from the build:
- Calling `session.Stop()` from inside an event callback deadlocks (Stop waits for the runtime while we must drain the stream). The helper stops the session from a separate task.
- The helper treats a closed stdin as "stop" (same as `collect`), so a manual test must keep stdin open.
- Completion is detected by the `GCStop` of the induced GC, with a timeout; a snapshot cut short is stored with `complete = false`.
- Not done: `.gcdump` import, session `has_heap` in the API/UI, large-heap behaviour, a Rust-side e2e test with the fake helper.

## Milestones

1. Spike (1-2 days): from the helper, capture a gcdump from the demo app; confirm TraceEvent graph API; dump node/edge counts.
2. Model + importer: `heap_snapshot` kind, helper emits graph, Rust stores blob, `show heap` by type.
3. Dominators + retention paths + CLI.
4. UI Heap tab + diff + findings.
5. Optional: ClrMD deep mode, `perfhawk check` gate, Go heap analysis (no object graph in pprof, so type-level only).

## Risks and open questions

- Stop-the-world pause on big heaps; mitigate with a confirm prompt and a timeout (gcdump default is 30 s).
- Incomplete graphs on .NET 8/10 with large heaps; must show "incomplete" loudly and offer deep mode.
- Windows-only viewing in Microsoft tools is our opening: a cross-platform `.gcdump` viewer is itself a feature.
- Decide: ship A without field names, or wait for B? Recommend ship A.
- Sampling/privacy: snapshots hold type names only in A (no string contents); B dumps may hold secrets, so keep local and warn.
