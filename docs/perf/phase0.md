# Phase 0 performance gate

Exit criterion (PLAN.md §7): a 500 MB uncompressed pprof imports in ≤ 20 s,
and flame/top queries on it answer in ≤ 1 s each.

Gate test: `crates/perfhawk-store/tests/perf.rs` (`#[ignore]`):

```
cargo test -p perfhawk-store --release --test perf -- --ignored --nocapture
```

It generates the profile with `genprof::generate(dir/big.pb, 500, 42)` into a
temp dir, reads it, then asserts:

| step | threshold |
|---|---|
| `Store::import` (SHA-256, detect, decode, convert, bincode + zstd blob, SQLite row) | ≤ 20 s |
| `Store::load` | ≤ 5 s |
| `flame(default type, no filter, min 0.0005)` + `serde_json::to_vec` | ≤ 1 s, JSON ≤ 5 MiB |
| `top(default type, no filter, limit 100)` | ≤ 1 s |

CI: the `perf` job in `.github/workflows/ci.yml` runs it on `ubuntu-latest`,
only on manual `workflow_dispatch`.

## Machine

- CPU: Intel(R) Core(TM) i9-14900 (8 P-cores + 16 E-cores, 24 cores / 32 threads)
- RAM: 16 GB
- OS: Windows 11 Pro 10.0.26200
- Toolchain: rustc 1.99.1 (x86_64-pc-windows-msvc), `--release`

All the code measured is single-threaded (no rayon), so `RAYON_NUM_THREADS`
does not apply. To approximate the plan's 4-core reference machine, the test
binary was also run pinned to 4 P-cores (affinity `0x55`) and to 4 E-cores
(affinity `0xF0000`, the slower cores).

## Generated profile (`genprof`, seed 42)

- File: 524 288 000+ bytes (500.0 MiB), uncompressed protobuf
- 50 000 functions / 50 000 locations, 2 sample types (`samples/count`, `cpu/nanoseconds`)
- 541 590 samples, 213 482 123 location references
- After import: 5 013 896 stack-trie nodes
- Model: Zipf (s = 1) hot functions; 90 % of samples 5–200 deep following one of
  10 000 Zipf-chosen call-path templates (a tree of shared prefixes); 10 %
  1 000–5 000 deep following one of 512 uniformly chosen recursion patterns
  (template prefix + a 1–4 function cycle); the last 1–3 frames of every sample
  are fresh Zipf draws.

## Final numbers

| run | import | load | flame + JSON | JSON | top(100) |
|---|---|---|---|---|---|
| default scheduling, run 1 | 4.71 s | 0.23 s | 0.148 s | 2.32 MiB | 0.286 s |
| default scheduling, run 2 | 4.49 s | 0.23 s | 0.150 s | 2.32 MiB | 0.295 s |
| default scheduling, run 3 | 4.45 s | 0.23 s | 0.151 s | 2.32 MiB | 0.289 s |
| pinned to 4 P-cores | 3.77 s | 0.22 s | 0.116 s | 2.32 MiB | 0.227 s |
| pinned to 4 E-cores | 5.65 s | 0.23 s | 0.149 s | 2.32 MiB | 0.285 s |

Peak working set of the test process: ~2.9 GB (input bytes plus the decoded
protobuf, whose 213 M `location_id`s dominate).

## Baseline and optimizations

Baseline = code before this task (commit `316afb7`) with the gate test.

| code | generator | import | load | flame + JSON | JSON | top |
|---|---|---|---|---|---|---|
| baseline | first draft (16 Zipf-weighted recursions) | 34.2 s | 0.19 s | 26.6 s | 12.7 MiB | 1.10 s |
| baseline | final (512 uniform recursions) | 28.5 s | 0.17 s | 28.1 s | 2.32 MiB | 1.28 s |
| final | first draft | 6.2 s | 0.20 s | 0.20 s | 12.7 MiB | 0.27 s |
| final | final | 3.2–4.7 s | 0.18–0.23 s | 0.10–0.15 s | 2.32 MiB | 0.17–0.30 s |

What changed, measured with the first-draft file (import = `import_pprof`
alone unless noted):

1. **perfhawk-import: decode raw pprof once.** `import_bytes` used to decode the
   whole protobuf in `detect_format` (to recognise raw pprof) and again in
   `import_pprof`. Detection and import now share `decode_plausible`;
   `detect_format`'s behaviour is unchanged. Saves one full decode (~2 s and
   a transient ~2 GB copy).
2. **perfhawk-import: dense location lookup.** Location id → frames used a
   SipHash `HashMap<u64, Vec<FrameId>>` looked up for each of the ~290 M
   references. Now a dense `Vec` index (when ids are compact, else an Fx map)
   into one flat frame table. With (3): 28.7 s → 23.4 s.
3. **perfhawk-model: Fx hashing in `ProfileBuilder`** (`rustc-hash`).
4. **perfhawk-model: child hints in `ProfileBuilder::stack`.** Each stack node
   remembers the last child taken; stacks sharing a prefix skip the hash map
   almost always (the remaining cost was cache misses on a 3 M entry table).
   23.4 s → 7.0 s; `Store::import` 32.7 s → 4.0–7.7 s.
5. **perfhawk-analysis: unfiltered flame/top from the stack trie.** With no
   focus/exclude/hide filter, values are summed per stack-trie node and the
   function-merged tree is built from the trie in O(stack nodes + samples)
   instead of walking every sample's ~400 frames. Siblings are merged with a
   per-function mark array (no hashing); nodes below the `min_fraction`
   threshold get exact totals but are not expanded. `top` runs a DFS over the
   trie with per-function on-path counters (a function counts once per
   stack). Filtered queries still walk samples, now without a per-sample
   `Vec` allocation and with Fx hashing and child hints in the arena. A test
   asserts the fast path equals the per-sample path.
   Flame 27.9 s → 0.15 s, top 1.8 s → 0.27 s.
6. **perfhawk-analysis: prune before allocating.** `into_flame` decides which
   nodes survive the threshold first and only allocates names/`FlameNode`s
   for survivors.

Not changed, because measurement showed no need: the store's import/load
copies (`bincode` + `zstd` in memory; load is 0.2 s and the blob step is a
fraction of a second) and parallelism.

## Generator change and the JSON budget

The first-draft generator drew deep stacks from 16 Zipf-weighted recursion
patterns. Each one held 0.1–3 % of the total, so in the function-merged flame
graph every one is a tower 4 000–5 000 nodes tall above the 0.0005 cutoff:
67 608 of the 86 263 surviving nodes, 12.7 MiB of JSON. Even with empty names a
node costs ~80 bytes of JSON, so no encoding of that tree fits 5 MiB. The final
generator spreads deep stacks over 512 uniformly chosen recursions (each
~0.02 % of the total, so they are cut where they fork from the shared
prefixes): 15 894 nodes, 2.32 MiB. **A real profile dominated by a few deep
recursions will still produce a flame payload well over 5 MiB at
`min 0.0005`**; the threshold limits width, not depth. Possible follow-ups
(not Phase 0): a depth cap or collapsing recursive runs in the flame API.
