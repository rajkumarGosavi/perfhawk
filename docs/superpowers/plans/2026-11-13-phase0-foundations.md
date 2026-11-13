# Phase 0 — Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A single `prism` binary that imports existing profile files (pprof, collapsed stacks, speedscope), stores them locally, and lets the user explore them in a local browser UI (flame/icicle graph, top table, sandwich view) and from the terminal.

**Architecture:** Rust Cargo workspace with one crate per responsibility: unified data model → importers → analysis → local store → HTTP API → CLI. A React + TypeScript UI is built with Vite and embedded into the binary; the server binds to localhost only. No capture yet — capture providers (Go, .NET) start in Phase 1 and plug into the same model and store.

**Tech Stack:** Rust (stable, edition 2021), `prost` (pprof protobuf), `flate2`, `serde`/`serde_json`, `bincode` 1.3, `zstd`, `rusqlite` (bundled), `sha2`, `regex`, `axum` + `tokio`, `rust-embed`, `clap` 4, `open`, `dirs`; tests with `assert_cmd`, `tempfile`, `reqwest`. UI: Node 20+, Vite, React 18, TypeScript, Vitest.

**Spec:** [docs/PLAN.md](../../PLAN.md) — §0 (decisions), §4.2 (unified model), §4.3 (analysis engine), §4.4 (store), §5 (workflow), §7 Phase 0.

## Global Constraints

- License: Apache-2.0 for every crate and the UI; dependencies must be permissive (Apache-2.0, MIT, BSD, ISC, Zlib, Unicode-3.0) — enforced with `cargo deny check licenses`. No GPL/LGPL/AGPL dependencies.
- Platforms: Linux x86_64 + arm64, macOS arm64 + x86_64, Windows x86_64. CI runs the full test suite on `ubuntu-latest`, `macos-latest`, `windows-latest`.
- One binary: `prism`. UI assets embedded; no runtime dependency on Node.
- The HTTP server binds `127.0.0.1` only. No remote access in Phase 0.
- Data directory: `--home <dir>` flag, else `PRISM_HOME` env var, else `dirs::data_dir()/prism` (Linux `~/.local/share/prism`, macOS `~/Library/Application Support/prism`, Windows `%APPDATA%\prism`).
- Profile identity = lowercase hex SHA-256 of the original file bytes. Short form = first 12 chars.
- Profile kinds use the product vocabulary from PLAN.md §3: `cpu`, `wall`, `memory`, `lock`, `blocking`, `block_io`, `threads`, `unknown`.
- Every value carries a unit: `nanoseconds`, `bytes`, `count`, or the raw unit string.
- Tree-building and import code must be iterative (no recursion proportional to stack depth).
- Phase 0 exit criteria (PLAN.md §7, made measurable in Task 10): a 500 MB uncompressed pprof imports in ≤ 20 s, and flame/top queries on it answer in ≤ 1 s each on a 4-core developer machine.

## Review Focus

1. **Go mutex vs block profiles look identical** (both `contentions/count` + `delay/nanoseconds`) → kind is `lock` by default, `blocking` when the file name contains `block`, and `--kind` always overrides. Test in Task 3.
2. **Very deep stacks** (10 000 frames, deep recursion) → no stack overflow, and recursion is not double-counted in `total` for the top table. Tests in Tasks 5 and 3.
3. **Garbage, empty or truncated files** → clear `ImportError` naming the detected format, never a panic; a 0-sample profile imports and renders an empty flame graph. Tests in Tasks 4 and 5.
4. **Re-importing the same file** → returns the existing record (same id), no duplicate row or blob. Test in Task 6.
5. **Paths with spaces (incl. Windows backslash paths) and non-UTF-8 function names** → import and display work; names are lossily UTF-8 converted, not rejected. Tests in Tasks 4, 6 and 9.

**Scope note vs PLAN.md §7:** Phase 0 originally listed Chrome `.cpuprofile` and JFR importers. With the MVP narrowed to Go + .NET (§0), they move to Phase 3 alongside Node and Java. .NET's `dotnet-trace` speedscope export is covered by the speedscope importer; native `.nettrace` arrives in Phase 1 with `prism-dotnet`.

---

## File Structure

```
Cargo.toml                       workspace, shared deps, [workspace.package] license
deny.toml                        cargo-deny license allowlist
LICENSE                          Apache-2.0
.github/workflows/ci.yml         3-OS matrix: ui build, cargo test, cargo deny
crates/
  prism-model/src/lib.rs         Profile, ids, enums, ProfileBuilder, ModelError
  prism-import/src/lib.rs        import_bytes, detect_format, ImportError
  prism-import/src/pprof.rs      pprof (gz/raw) → Profile
  prism-import/src/pprof.proto   vendored google/pprof profile.proto (Apache-2.0)
  prism-import/src/collapsed.rs  folded stacks → Profile
  prism-import/src/speedscope.rs speedscope JSON (sampled + evented) → Profile
  prism-import/src/kind.rs       infer_kind
  prism-import/build.rs          prost-build for pprof.proto
  prism-import/tests/fixtures/   go-cpu.pb.gz, go-heap.pb.gz, go-mutex.pb.gz, sample.folded, sample.speedscope.json
  prism-analysis/src/lib.rs      Filter, CallTree, flame, top, sandwich
  prism-store/src/lib.rs         Store, ProfileRecord, ImportOptions, StoreError
  prism-server/src/lib.rs        router(), serve(), API handlers, LRU cache, embedded UI
  prism-cli/src/main.rs          clap commands: import, ls, show, open, ui
tools/genprof/src/main.rs        synthetic large-pprof generator (perf test input)
ui/
  package.json, vite.config.ts, tsconfig.json, index.html
  src/api.ts                     typed fetch client for /api
  src/format.ts                  formatValue(value, unit)
  src/url.ts                     readViewState / writeViewState
  src/flame/layout.ts            layoutFlame(root, width, rowHeight, inverted)
  src/flame/FlameGraph.tsx       canvas renderer, zoom, hover, search highlight
  src/views/ProfileList.tsx, ProfileView.tsx, TopTable.tsx, Sandwich.tsx
  src/App.tsx, src/main.tsx, src/theme.css
  src/**/*.test.ts               Vitest tests
```

---

### Task 1: Workspace scaffold, licensing and CI

**Files:**
- Create: `Cargo.toml`, `deny.toml`, `LICENSE`, `.gitignore`, `.github/workflows/ci.yml`, `crates/*/Cargo.toml` + empty `src/lib.rs` (`prism-cli`: `src/main.rs`), `ui/` Vite React-TS skeleton

**Interfaces:**
- Produces: crate names `prism-model`, `prism-import`, `prism-analysis`, `prism-store`, `prism-server`, `prism-cli` (binary name `prism`); `ui/dist/` as the UI build output.

- [ ] **Step 1:** `git init`, create the workspace with the six crates; `[workspace.package]` sets `license = "Apache-2.0"`, `edition = "2021"`, `rust-version = "1.80"`. `prism-cli`'s `[[bin]] name = "prism"`.
- [ ] **Step 2:** `npm create vite@latest ui -- --template react-ts`; add `vitest` dev dependency and `"test": "vitest run"` script.
- [ ] **Step 3:** `deny.toml` allowlist: `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `MIT`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib`, `Unicode-3.0`, `Unicode-DFS-2016`.
- [ ] **Step 4:** CI workflow: matrix `ubuntu-latest, macos-latest, windows-latest`; steps: setup Node 20 → `npm ci && npm test && npm run build` in `ui/` → `cargo test --workspace` → `cargo deny check licenses` (ubuntu only).
- [ ] **Step 5: Verify**
  Run: `cargo test --workspace` and `cd ui && npm test -- --passWithNoTests && npm run build`
  Expected: both succeed; `ui/dist/index.html` exists.
- [ ] **Step 6: Commit** — `git commit -m "chore: workspace scaffold, license, CI"`

---

### Task 2: Unified profile model (`prism-model`)

**Files:**
- Create: `crates/prism-model/src/lib.rs`
- Test: `crates/prism-model/src/lib.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces (all `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`):
  - Ids: `pub struct StrId(pub u32)`, `FuncId(pub u32)`, `FrameId(pub u32)`, `StackId(pub u32)` (all `Copy, Eq, Hash`). `StrId(0)` is always `""`.
  - `pub enum ProfileKind { Cpu, Wall, Memory, Lock, Blocking, BlockIo, Threads, Unknown }` — serde `snake_case`.
  - `pub enum Unit { Nanoseconds, Bytes, Count, Other(String) }` with `Unit::parse(&str) -> Unit` (`"nanoseconds"|"ns"` → Nanoseconds, `"bytes"` → Bytes, `"count"|"samples"|""` → Count; `"microseconds"`/`"milliseconds"`/`"seconds"` → Other — importers convert values to ns where they know the unit).
  - `pub enum Fidelity { Native, SampledExternal, Derived, Approximate, Unknown }`
  - `pub enum SourceFormat { Pprof, Collapsed, Speedscope }`
  - `pub struct SampleType { pub name: String, pub unit: Unit }`
  - `pub struct Function { pub name: StrId, pub file: StrId, pub start_line: u32 }`
  - `pub struct Frame { pub function: FuncId, pub line: u32, pub address: u64 }`
  - `pub struct StackNode { pub parent: Option<StackId>, pub frame: FrameId }` — prefix tree, root-first.
  - `pub struct Label { pub key: StrId, pub value: LabelValue }`, `pub enum LabelValue { Str(StrId), Num { value: i64, unit: Option<StrId> } }`
  - `pub struct Sample { pub stack: Option<StackId>, pub labels: Vec<Label> }` (`None` = empty stack)
  - `pub struct ProfileMeta { pub kind: ProfileKind, pub source_format: SourceFormat, pub fidelity: Fidelity, pub duration_ns: Option<u64>, pub captured_at_unix_ns: Option<i64>, pub period: Option<(SampleType, i64)> }`
  - `pub struct Profile { pub strings: Vec<String>, pub functions: Vec<Function>, pub frames: Vec<Frame>, pub stacks: Vec<StackNode>, pub sample_types: Vec<SampleType>, pub default_sample_type: usize, pub samples: Vec<Sample>, pub values: Vec<i64>, pub meta: ProfileMeta }` — `values` is row-major, `values.len() == samples.len() * sample_types.len()`.
  - `impl Profile { pub fn str(&self, id: StrId) -> &str; pub fn value(&self, sample: usize, sample_type: usize) -> i64; pub fn stack_frames(&self, stack: Option<StackId>) -> Vec<FrameId> /* root-first, iterative */; pub fn total(&self, sample_type: usize) -> i64; pub fn sample_type_index(&self, name: &str) -> Option<usize> }`
  - `pub struct ProfileBuilder` with: `new(sample_types: Vec<SampleType>, meta: ProfileMeta) -> Self`, `intern(&mut self, s: &str) -> StrId`, `function(&mut self, name: &str, file: &str, start_line: u32) -> FuncId`, `frame(&mut self, f: FuncId, line: u32, address: u64) -> FrameId`, `stack(&mut self, root_first: &[FrameId]) -> Option<StackId>`, `add_sample(&mut self, stack: Option<StackId>, values: &[i64], labels: Vec<Label>) -> Result<(), ModelError>`, `set_default_sample_type(&mut self, idx: usize)`, `finish(self) -> Profile`. Functions, frames and stack nodes are deduplicated via `HashMap`s.
  - `pub enum ModelError { ValueCountMismatch { expected: usize, got: usize } }` (`thiserror`).

- [ ] **Step 1: Write failing tests**
  - `builder_dedupes_strings_functions_frames_and_stack_prefixes`: two samples `[main, a, b]` and `[main, a, c]` → `functions.len()==4`, `stacks.len()==4` (main, main/a, main/a/b, main/a/c), `strings[0]==""`.
  - `stack_frames_returns_root_first`: frames for sample 0 resolve to names `["main","a","b"]`.
  - `values_are_row_major_and_total_sums`: 2 sample types, samples `[1,10]`,`[2,20]` → `value(1,1)==20`, `total(0)==3`.
  - `add_sample_rejects_wrong_value_count`: 2 types, pass 1 value → `Err(ValueCountMismatch{expected:2,got:1})`.
  - `deep_stack_10000_frames_does_not_overflow`: build 10 000-deep stack, `stack_frames` returns 10 000 entries.
  - `unit_parse`: `"nanoseconds"`→Nanoseconds, `"ns"`→Nanoseconds, `"bytes"`→Bytes, `""`→Count, `"furlongs"`→`Other("furlongs")`.
- [ ] **Step 2:** `cargo test -p prism-model` → FAIL (types missing).
- [ ] **Step 3:** Implement the types and builder in `crates/prism-model/src/lib.rs`.
- [ ] **Step 4:** `cargo test -p prism-model` → PASS.
- [ ] **Step 5: Commit** — `feat(model): unified profile model and builder`

---

### Task 3: pprof importer and kind inference

**Files:**
- Create: `crates/prism-import/src/lib.rs`, `src/pprof.rs`, `src/pprof.proto` (vendored from github.com/google/pprof `proto/profile.proto`), `src/kind.rs`, `build.rs`
- Create fixtures: `crates/prism-import/tests/fixtures/go-cpu.pb.gz`, `go-heap.pb.gz`, `go-mutex.pb.gz` — produced once by `tools/fixtures/gen.go` (a 40-line Go 1.22 program that burns CPU, allocates, and contends a mutex, then writes the three profiles via `runtime/pprof`); commit both the generator and the outputs.
- Test: `crates/prism-import/tests/pprof.rs`

**Interfaces:**
- Consumes: `prism_model::{Profile, ProfileBuilder, ProfileKind, SampleType, Unit, Fidelity, SourceFormat, ProfileMeta}`
- Produces:
  - `pub struct ImportHints<'a> { pub file_name: Option<&'a str>, pub kind_override: Option<ProfileKind> }` (`Default`)
  - `pub fn import_pprof(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError>` — gunzip when bytes start with `1f 8b`.
  - `pub mod pprof { pub mod proto { /* prost-generated types: Profile, Sample, Location, Line, Function, ValueType, Label, Mapping */ } }` — public so tests (this task) and `tools/genprof` (Task 10) can build pprof files.
  - `pub fn infer_kind(sample_types: &[SampleType], file_name: Option<&str>) -> ProfileKind` in `kind.rs`.
  - `pub enum ImportError { UnknownFormat, Malformed { format: SourceFormat, reason: String }, Io(std::io::Error) }` (`thiserror`), Display e.g. `"malformed pprof: <reason>"`.

Mapping rules (decisions the implementer must follow):
- pprof `Location` with multiple `Line` entries = inlined frames: emit one frame per line, ordered caller-first (pprof lists `line[0]` as the innermost).
- pprof `sample.location_id` is leaf-first → reverse to root-first.
- `default_sample_type` string → index; if absent use the **last** sample type (pprof convention).
- Labels: `label.str` → `LabelValue::Str`, `label.num` → `LabelValue::Num` with `num_unit`.
- Function names: `String::from_utf8_lossy`.
- `duration_nanos` → `duration_ns`; `time_nanos` → `captured_at_unix_ns`; `period_type`/`period` → `period`.
- Fidelity = `Native` for pprof.
- `infer_kind` table, first match wins: kind_override → any type named `cpu` with unit ns, or `samples` alongside `cpu` → `Cpu`; `alloc_space|alloc_objects|inuse_space|inuse_objects|space|objects` → `Memory`; `goroutine` → `Threads`; `contentions` + `delay` → `Blocking` if `file_name` (lowercased) contains `"block"`, else `Lock`; `wall` → `Wall`; otherwise `Unknown`.

- [ ] **Step 1: Write failing tests** in `tests/pprof.rs`:
  - `go_cpu_fixture_imports`: kind `Cpu`, sample types include `("cpu", Nanoseconds)`, `total(cpu) > 0`, some function name contains `"main.burn"`.
  - `go_heap_fixture_default_type_is_inuse_space`: `sample_types[default_sample_type].name == "inuse_space"`, kind `Memory`.
  - `go_mutex_fixture_is_lock_by_default`: kind `Lock`.
  - `block_in_filename_makes_blocking`: same bytes with `file_name: Some("svc-block.pb.gz")` → `Blocking`.
  - `kind_override_wins`: override `Wall` → `Wall`.
  - `uncompressed_pprof_also_imports`: gunzip fixture in test, import raw → same `samples.len()`.
  - `inlined_lines_become_caller_first_frames`: hand-built pprof (via the generated prost types) with one location having lines `[inner, outer]` → stack names `["outer","inner"]`.
  - `recursion_preserved_in_stack`: hand-built sample `main→f→f→f` → stack length 4.
  - `truncated_pprof_is_malformed_error`: first 100 bytes of gunzipped fixture → `Err(Malformed{format: Pprof, ..})`.
- [ ] **Step 2:** `cargo test -p prism-import --test pprof` → FAIL.
- [ ] **Step 3:** Implement `build.rs` (prost-build), `pprof.rs`, `kind.rs`.
- [ ] **Step 4:** `cargo test -p prism-import --test pprof` → PASS.
- [ ] **Step 5: Commit** — `feat(import): pprof importer and kind inference`

---

### Task 4: Collapsed + speedscope importers and format detection

**Files:**
- Create: `crates/prism-import/src/collapsed.rs`, `src/speedscope.rs`; Modify: `src/lib.rs`
- Fixtures: `tests/fixtures/sample.folded`, `tests/fixtures/sample.speedscope.json` (hand-written, one sampled + one evented profile)
- Test: `crates/prism-import/tests/formats.rs`

**Interfaces:**
- Consumes: Task 3 `ImportHints`, `ImportError`, `infer_kind`.
- Produces:
  - `pub fn detect_format(bytes: &[u8], file_name: Option<&str>) -> Option<SourceFormat>` — gzip magic, or a protobuf that decodes as pprof **and** has ≥ 1 `sample_type` **and** `string_table[0] == ""` → Pprof; JSON whose `$schema` contains `speedscope` → Speedscope; text where every non-empty line among the first 100 matches `^.+ \d+$` → Collapsed; empty (or whitespace-only) input with extension `.folded`, `.collapsed` or `.txt` → Collapsed; anything else → `None`.
  - `pub fn import_bytes(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError>` — detect then dispatch; `UnknownFormat` if no match.
  - `pub fn import_collapsed(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError>` — frames split on `;`, count is the last space-separated token; one sample type `("samples", Count)`; fidelity `Unknown`; kind `infer_kind` → `Unknown` unless overridden.
  - `pub fn import_speedscope(bytes: &[u8], hints: &ImportHints) -> Result<Profile, ImportError>` — uses the **first** profile in the file (multi-profile files: later phases); `sampled` → weights per sample; `evented` → convert O/C events into self-time samples (one sample per contiguous interval with weight = interval length); unit from `unit` field (`nanoseconds|microseconds|milliseconds|seconds` → converted to ns; `bytes`; `none` → Count).

- [ ] **Step 1: Write failing tests** in `tests/formats.rs`:
  - `collapsed_parses_stacks_and_counts`: `"main;a;b 3\nmain;a 2\n"` → 2 samples, `total(0)==5`, stack 0 names `["main","a","b"]`.
  - `collapsed_frame_names_with_spaces_ok`: `"main;do work (x) 4"` → leaf name `"do work (x)"`.
  - `speedscope_sampled_fixture`: totals equal the sum of fixture weights, unit converted per fixture.
  - `speedscope_evented_self_time`: `O a@0, O b@10, C b@30, C a@40` (ns) → samples `a:10`, `a/b:20`, `a:10` → `total == 40`.
  - `detect_format_each_fixture`: each fixture detects as its format; `b"hello world"` → `None`.
  - `import_bytes_unknown_is_error`: random 1 KB bytes → `Err(UnknownFormat)`.
  - `empty_collapsed_file_gives_zero_sample_profile`: `""` with hint `file_name: Some("x.folded")` → `Ok`, `samples.len()==0` (empty text with `.folded`/`.collapsed` extension is accepted as collapsed).
  - `non_utf8_function_name_is_lossy`: collapsed bytes containing `0xff` in a frame → imports, name contains `'\u{FFFD}'`.
- [ ] **Step 2:** `cargo test -p prism-import --test formats` → FAIL.
- [ ] **Step 3:** Implement both importers, `detect_format`, `import_bytes`.
- [ ] **Step 4:** `cargo test -p prism-import` → PASS (all import tests).
- [ ] **Step 5: Commit** — `feat(import): collapsed and speedscope importers, format detection`

---

### Task 5: Analysis engine (`prism-analysis`)

**Files:**
- Create: `crates/prism-analysis/src/lib.rs` (split into `filter.rs`, `tree.rs`, `top.rs`, `sandwich.rs` if > 300 lines)
- Test: `crates/prism-analysis/tests/analysis.rs`

**Interfaces:**
- Consumes: `prism_model::{Profile, FuncId}`
- Produces (serde `Serialize` on output types, `camelCase`):
  - `pub struct Filter { pub focus: Option<Regex>, pub exclude: Option<Regex>, pub hide: Option<Regex> }` with `Filter::parse(focus: Option<&str>, exclude: Option<&str>, hide: Option<&str>) -> Result<Filter, regex::Error>`. Semantics (pprof-compatible): `focus` keeps only samples whose stack has a matching function name; `exclude` drops samples whose stack has a match; `hide` removes matching frames from stacks but keeps the sample.
  - `pub struct FlameNode { pub func_id: Option<u32>, pub name: String, pub file: String, pub self_value: i64, pub total: i64, pub children: Vec<FlameNode> }` — root has `func_id: None`, `name: "root"`. Nodes merge by **function** (not line). Children sorted by `total` desc, ties by name.
  - `pub fn flame(p: &Profile, sample_type: usize, filter: &Filter, min_fraction: f64) -> FlameNode` — children with `total < min_fraction * root.total` are dropped (parent `total` unchanged). Built with an arena + iterative traversal; serialization depth safety: `FlameNode` must serialize without recursion overflow for depth 10 000 (use `serde_stacker` or an explicit-stack `Serialize` impl).
  - `pub fn inverted_flame(p: &Profile, sample_type: usize, filter: &Filter, min_fraction: f64) -> FlameNode` — leaf-first (bottom-up) tree.
  - `pub struct TopRow { pub func_id: u32, pub name: String, pub file: String, pub self_value: i64, pub total: i64, pub self_pct: f64, pub total_pct: f64 }`
  - `pub fn top(p: &Profile, sample_type: usize, filter: &Filter, limit: usize) -> Vec<TopRow>` — sorted by `self_value` desc; a function appearing several times in one stack counts **once** toward `total` for that sample.
  - `pub struct Sandwich { pub callers: FlameNode, pub callees: FlameNode }`
  - `pub fn sandwich(p: &Profile, sample_type: usize, filter: &Filter, func: FuncId, min_fraction: f64) -> Sandwich` — callees: tree rooted at `func` from every occurrence (outermost occurrence per sample); callers: inverted tree from `func` toward roots.

- [ ] **Step 1: Write failing tests** (helper builds profiles via `ProfileBuilder`; samples `main;a;b=3`, `main;a=2`, `main;c=5`):
  - `flame_totals_and_self`: root total 10; `main` total 10 self 0; `a` total 5 self 2; `b` self 3; children of `main` ordered `[a(5), c(5)]` by name tie-break.
  - `min_fraction_prunes_small_children`: with `min_fraction=0.4`, `b` (3/10) dropped, `a.total` still 5.
  - `focus_exclude_hide`: focus `^b$` → root total 3; exclude `^c$` → 5; hide `^a$` → `main` has children `[b, c]` with `main.self==2`.
  - `top_recursion_counted_once`: sample `main;f;f;f=7` → row `f`: self 7, total 7 (not 21).
  - `sandwich_callers_and_callees`: for `a`: callees `a→b(3)`, callers `a←main(5)`.
  - `inverted_flame_leaf_first`: root children `[c(5), b(3), a(2)]`.
  - `empty_profile_flame_is_root_only`: 0 samples → root total 0, no children; `top` returns `[]`.
  - `deep_10000_stack_flame_and_serialize`: build, then `serde_json::to_string(&flame(..))` succeeds.
  - `invalid_regex_is_error`: `Filter::parse(Some("("),None,None)` → `Err`.
- [ ] **Step 2:** `cargo test -p prism-analysis` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p prism-analysis` → PASS.
- [ ] **Step 5: Commit** — `feat(analysis): flame, inverted, top, sandwich, filters`

---

### Task 6: Local store (`prism-store`)

**Files:**
- Create: `crates/prism-store/src/lib.rs`
- Test: `crates/prism-store/tests/store.rs`

**Interfaces:**
- Consumes: `prism_import::{import_bytes, ImportHints, ImportError}`, `prism_model::{Profile, ProfileKind, SourceFormat}`
- Produces:
  - `pub struct ImportOptions { pub name: Option<String>, pub file_name: Option<String>, pub kind_override: Option<ProfileKind>, pub tags: Vec<String> }` (`Default`)
  - `#[derive(Serialize)] pub struct ProfileRecord { pub id: String, pub short_id: String, pub name: String, pub kind: ProfileKind, pub source_format: SourceFormat, pub sample_types: Vec<SampleTypeInfo>, pub default_sample_type: usize, pub sample_count: u64, pub default_total: i64, pub imported_at_unix_ms: i64, pub tags: Vec<String> }`, `pub struct SampleTypeInfo { pub name: String, pub unit: String }`
  - `pub struct Store` with `open(root: &Path) -> Result<Store, StoreError>`, `import(&self, bytes: &[u8], opts: ImportOptions) -> Result<(ProfileRecord, bool /*newly_created*/), StoreError>`, `list(&self) -> Result<Vec<ProfileRecord>, StoreError>` (newest first), `resolve(&self, reference: &str) -> Result<ProfileRecord, StoreError>`, `load(&self, id: &str) -> Result<Profile, StoreError>`.
  - `pub fn default_home() -> PathBuf` (Global Constraints rule; reads `PRISM_HOME`).
  - `pub enum StoreError { Import(ImportError), NotFound(String), Ambiguous { reference: String, matches: usize }, Db(rusqlite::Error), Io(std::io::Error), Corrupt(String) }`

Layout and rules:
- `<root>/prism.db` (SQLite, `PRAGMA user_version = 1`, table `profiles` with the record columns, `tags` as JSON text) and `<root>/blobs/<id[0..2]>/<id>.bin.zst` = `zstd(bincode(Profile))`, level 3. Write blob to a temp file then rename (atomic).
- `id` = SHA-256 of original bytes; importing existing id returns the stored record and `false`.
- `name` default: `file_name` stem, else `short_id`.
- `resolve`: `"last"` → newest; exact id; else unique prefix ≥ 4 chars; else exact name; errors `NotFound` / `Ambiguous`.

- [ ] **Step 1: Write failing tests** (`tempfile::TempDir`, fixtures from `prism-import/tests/fixtures` via `include_bytes!`):
  - `import_then_load_roundtrip`: loaded `Profile == ` freshly imported one.
  - `reimport_same_bytes_dedupes`: second import → same id, `false`, `list().len()==1`, one blob file.
  - `resolve_last_prefix_and_name`: import two files; `"last"` → second; 6-char prefix of first → first; name of first → first.
  - `resolve_ambiguous_and_missing`: import two different files both with `name: Some("svc")` → `resolve("svc")` → `Err(Ambiguous{matches: 2, ..})`; 3-char prefix → `NotFound` (too short); unknown → `NotFound`.
  - `kind_override_and_tags_persist`: opts `kind_override: Some(Wall), tags: ["baseline"]` → record reflects both after reopening the store.
  - `store_path_with_spaces`: root `tmp/"my profiles"` works.
  - `corrupt_blob_reports_corrupt`: overwrite blob with junk → `load` → `Err(Corrupt(_))`.
- [ ] **Step 2:** `cargo test -p prism-store` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p prism-store` → PASS.
- [ ] **Step 5: Commit** — `feat(store): sqlite + zstd blob store with dedupe`

---

### Task 7: HTTP API server (`prism-server`)

**Files:**
- Create: `crates/prism-server/src/lib.rs`, `src/api.rs`, `src/assets.rs`
- Test: `crates/prism-server/tests/api.rs`

**Interfaces:**
- Consumes: `Store`, `ProfileRecord`, `prism_analysis::{Filter, flame, inverted_flame, top, sandwich}`, `prism_model::FuncId`
- Produces:
  - `pub fn router(store: Arc<Store>) -> axum::Router`
  - `pub async fn serve(store: Arc<Store>, port: u16 /*0 = random*/) -> Result<(SocketAddr, JoinHandle<()>), std::io::Error>` — binds `127.0.0.1:port`.
  - Endpoints (JSON; query params shared: `st` = sample type index, default = profile default; `focus`, `exclude`, `hide` regex strings; `min` = min_fraction, default `0.0005`):
    - `GET /api/profiles` → `[ProfileRecord]`
    - `GET /api/profiles/{ref}` → `ProfileRecord` (`ref` passes through `Store::resolve`)
    - `GET /api/profiles/{ref}/flame?inverted=false|true` → `FlameNode`
    - `GET /api/profiles/{ref}/top?limit=100` → `[TopRow]`
    - `GET /api/profiles/{ref}/sandwich?func=<u32>` → `Sandwich`
    - `POST /api/profiles?name=&file_name=&kind=` with raw body → `{ record: ProfileRecord, created: bool }` (body limit 2 GiB)
    - Errors: `{"error":{"code":"not_found"|"ambiguous"|"bad_request"|"import_failed"|"internal","message":"..."}}` with 404/409/400/422/500.
    - Everything else → embedded `ui/dist` via `rust-embed` (`#[folder = "../../ui/dist"]`), SPA fallback to `index.html`; if `index.html` is not embedded, serve a plain page "UI not built — run `npm run build` in ui/".
  - Loaded `Profile`s cached in an LRU of 2 entries keyed by id (`Mutex<lru::LruCache>`); analysis runs in `tokio::task::spawn_blocking`.

- [ ] **Step 1: Write failing tests** (spin up `serve(store, 0)`, use `reqwest`):
  - `list_and_get_profile`: import fixture into store first; `/api/profiles` has 1 entry; `/api/profiles/last` returns it.
  - `flame_respects_sample_type_and_filters`: heap fixture `st` for `alloc_space` vs default give different `total`s; `focus=nomatch` → total 0.
  - `top_and_sandwich`: top `limit=5` returns ≤ 5 rows; sandwich with `func` of row 0 returns 200.
  - `upload_raw_body_creates_then_dedupes`: POST fixture twice → `created: true` then `false`.
  - `errors_are_json`: unknown ref → 404 `code:"not_found"`; `focus=(` → 400 `bad_request`; POST `b"junk"` → 422 `import_failed`.
  - `binds_localhost_only`: returned `SocketAddr.ip()` is `127.0.0.1`.
  - `spa_fallback_serves_html`: `GET /profiles/abc` → 200 `text/html`.
- [ ] **Step 2:** `cargo test -p prism-server` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p prism-server` → PASS.
- [ ] **Step 5: Commit** — `feat(server): local JSON API and embedded UI`

---

### Task 8: Browser UI

**Files:**
- Create: files under `ui/src/` listed in File Structure
- Test: `ui/src/flame/layout.test.ts`, `ui/src/format.test.ts`, `ui/src/url.test.ts`

**Interfaces:**
- Consumes: Task 7 endpoints and JSON shapes (camelCase).
- Produces:
  - `api.ts`: `listProfiles(): Promise<ProfileRecord[]>`, `getProfile(ref)`, `getFlame(ref, q: AnalysisQuery & {inverted?: boolean})`, `getTop(ref, q & {limit?: number})`, `getSandwich(ref, func: number, q)`, `uploadProfile(file: File)`; `type AnalysisQuery = { st?: number; focus?: string; exclude?: string; hide?: string; min?: number }`; non-2xx throws `ApiError { code, message }`.
  - `format.ts`: `formatValue(v: number, unit: string): string` — ns → `"1.23 s" | "45.6 ms" | "789 µs" | "12 ns"` (3 significant digits); bytes → `"1.5 GiB" | "512 KiB" | "10 B"` (binary units); count → thousands separators `"12,345"`.
  - `url.ts`: `type ViewState = { profile?: string; view: 'flame'|'icicle'|'top'|'sandwich'; st?: number; focus?: string; exclude?: string; hide?: string; func?: number; search?: string }`; `readViewState(search: string): ViewState`, `writeViewState(s: ViewState): string` (omits defaults; `view` default `'flame'`).
  - `flame/layout.ts`: `type Rect = { x: number; y: number; w: number; depth: number; node: FlameNode }`; `layoutFlame(root: FlameNode, width: number, rowHeight: number, inverted: boolean, zoom?: FlameNode): Rect[]` — x/w proportional to `total` within the zoomed node; rects narrower than 0.5 px omitted; `inverted=false` is a classic flame graph (root at bottom, `y = (maxDepth - depth) * rowHeight`), `true` is an icicle (root at top, `y = depth * rowHeight`).
  - Views: `ProfileList` (table: name, kind, format, samples, default total via `formatValue`, imported time; drag-and-drop upload area); `ProfileView` (toolbar: sample-type `<select>`, focus/exclude/hide inputs with 300 ms debounce, view tabs, search box; error banner showing `ApiError.message`); `FlameGraph` (canvas, `devicePixelRatio` aware, click = zoom, Esc/double-click root = reset, hover tooltip with name, file, self, total, % of total; search match highlight); `TopTable` (sortable by self/total, row click → sandwich view for `funcId`); `Sandwich` (callers icicle above, callees flame below, selected function in the middle).
  - All view state lives in the URL (`writeViewState`), so links reproduce a view.
  - `theme.css`: colors as CSS variables with light and dark (`prefers-color-scheme`) values; frame color hashed from function name into a warm palette; search matches use an accent color.

- [ ] **Step 1: Write failing tests**
  - `layout.test.ts`: root total 10 with children a=5, c=5 at width 1000 → a `{x:0,w:500}`, c `{x:500,w:500}`; child with total 0.004 of 1000 px root is omitted; zoom to `a` makes `a.w==1000`; icicle `y` of depth 1 == `rowHeight`.
  - `format.test.ts`: `formatValue(1_234_000_000,'nanoseconds')=='1.23 s'`, `(45_600_000,'nanoseconds')=='45.6 ms'`, `(1536,'bytes')=='1.5 KiB'`, `(12345,'count')=='12,345'`.
  - `url.test.ts`: roundtrip `{profile:'ab12',view:'top',st:1,focus:'json'}`; default state writes `''`.
- [ ] **Step 2:** `cd ui && npm test` → FAIL.
- [ ] **Step 3:** Implement modules and views.
- [ ] **Step 4:** `npm test` → PASS; `npm run build` → succeeds with no TypeScript errors.
- [ ] **Step 5: Commit** — `feat(ui): profile list, flame/icicle, top, sandwich` (end-to-end manual UI check happens in Task 9 Step 5, once `prism ui` exists)

---

### Task 9: CLI (`prism`)

**Files:**
- Create: `crates/prism-cli/src/main.rs` (+ `src/cmd/*.rs` per subcommand)
- Test: `crates/prism-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `Store`, `ImportOptions`, `default_home`, `prism_analysis::{Filter, top}`, `prism_server::serve`
- Produces commands (global `--home <DIR>`):
  - `prism import <FILES>... [--name <N>] [--kind <cpu|wall|memory|lock|blocking|block_io|threads>] [--tag <T>]...` → one line per file: `<short_id>  <kind>  <name>  (<sample_count> samples)` plus ` [already imported]` when deduped. Exit 1 if any file failed, still importing the others; errors printed to stderr as `error: <path>: <message>`. `--name` only allowed with a single file.
  - `prism ls [--json]` → aligned table `ID  KIND  FORMAT  SAMPLES  TOTAL  NAME  IMPORTED` (TOTAL formatted with the same unit rules as `format.ts`), or JSON array of `ProfileRecord`.
  - `prism show [REF] [--top N=20] [--st <NAME>] [--focus/--exclude/--hide <RE>]` → header (name, kind, sample type, total) + top table `SELF  SELF%  TOTAL  TOTAL%  FUNCTION`. `REF` default `last`.
  - `prism open [REF_OR_FILE] [--port P] [--no-browser]` → if the argument is an existing file path, import it first; start server; print `prism UI: http://127.0.0.1:<port>/?profile=<short_id>`; open the browser unless `--no-browser`; run until Ctrl-C.
  - `prism ui [--port P] [--no-browser]` → same, profile list page.
  - Exit codes: 0 ok, 1 runtime error, 2 usage error (clap default).

- [ ] **Step 1: Write failing tests** (`assert_cmd`, `--home` temp dir):
  - `import_prints_summary_and_dedupes`: import `go-cpu.pb.gz` → stdout matches `^[0-9a-f]{12}  cpu  go-cpu`; again → contains `[already imported]`.
  - `import_partial_failure_exit_1`: valid fixture + junk file → exit 1, stdout has the valid one, stderr `error: ` with the junk path.
  - `ls_json_is_parseable`: after one import, `ls --json` parses to array len 1.
  - `show_top_lists_hot_function`: `show last --top 5` stdout contains `main.burn`.
  - `kind_flag_overrides`: `import go-mutex.pb.gz --kind blocking` → line contains `blocking`.
  - `path_with_spaces`: copy fixture to `tmp/"my dir"/cpu profile.pb.gz` and import → exit 0.
  - `open_no_browser_serves`: spawn `open <fixture> --no-browser --port 0`, read the printed URL from stdout, `GET /api/profiles` → 200, then kill the process.
- [ ] **Step 2:** `cargo test -p prism-cli` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p prism-cli` → PASS; `cargo test --workspace` → PASS.
- [ ] **Step 5: Manual UI check:** `cd ui && npm run build`, then `cargo run -p prism-cli -- open crates/prism-import/tests/fixtures/go-heap.pb.gz`. Import the other fixtures by drag-and-drop; for each: switch sample types, zoom and reset, search, top → sandwich, reload keeps the view. Check light and dark mode and a 375 px wide window (toolbar wraps, no horizontal page scroll).
- [ ] **Step 6: Commit** — `feat(cli): import, ls, show, open, ui`

---

### Task 10: Large-profile performance gate

**Files:**
- Create: `tools/genprof/Cargo.toml`, `tools/genprof/src/main.rs` (add to workspace)
- Test: `crates/prism-store/tests/perf.rs` (`#[ignore]`, run explicitly)
- Modify: `.github/workflows/ci.yml` (separate manual `workflow_dispatch` job `perf` on ubuntu)

**Interfaces:**
- Consumes: pprof prost types from `prism-import` (expose them as `prism_import::pprof::proto` — `pub mod`), `Store`, analysis functions.
- Produces: `genprof --out <file> --target-mb <N> --seed <S>` writing an **uncompressed** pprof with 50 000 functions, stack depth uniformly 5–200 (10 % of samples 1 000–5 000 deep), 2 sample types (`samples/count`, `cpu/nanoseconds`), Zipf-distributed hot functions, until the file reaches `N` MB. Deterministic for a given seed.

- [ ] **Step 1: Write the ignored test** `perf_500mb_gate`: generate 500 MB with seed 42 into a temp dir; assert `Store::import` ≤ 20 s; `load` ≤ 5 s; `flame(min 0.0005)` ≤ 1 s and its JSON ≤ 5 MB; `top(limit 100)` ≤ 1 s. Print all timings.
- [ ] **Step 2:** `cargo test -p prism-store --release --test perf -- --ignored --nocapture` → FAIL or measure baseline.
- [ ] **Step 3:** Optimize until it passes — likely levers: stream-decode with `prost` from a `BufReader` instead of reading twice, pre-size `Vec`s from protobuf counts, use `ahash` maps in `ProfileBuilder`, parallel per-sample-type aggregation with `rayon` (MIT/Apache — allowed).
- [ ] **Step 4:** Re-run → PASS; record timings and machine spec in `docs/perf/phase0.md`.
- [ ] **Step 5: Commit** — `perf: large-profile generator and phase-0 performance gate`

---

## Phase 0 done when

- `cargo test --workspace` and `npm test` pass on Linux, macOS and Windows in CI; `cargo deny check licenses` passes.
- `prism import`, `ls`, `show`, `open`, `ui` work as specified with Go pprof, collapsed and speedscope files.
- The perf gate in Task 10 passes and `docs/perf/phase0.md` records the numbers.
