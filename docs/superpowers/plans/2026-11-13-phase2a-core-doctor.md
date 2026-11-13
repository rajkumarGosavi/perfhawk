# Phase 2a: shared core and Doctor v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `prism doctor`: a 60 s triage that says whether a Go or .NET service is CPU-, lock-, I/O- or scheduler-bound (or idle), prints the exact next command, and runs it with `--follow-up`. This plan covers milestones M1 and M2 of the design.

**Architecture:** Add the `OffCpu` and `SchedDelay` kinds next to the existing `BlockIo`, a `SystemProvider` seam in `prism-capture` (no OS provider yet, a fake one for tests), a pure rule engine in `prism-analysis`, one server endpoint, one CLI command and one UI card. OS providers (ETW, eBPF) are separate later plans (2b, 2c) that only implement the seam.

**Tech Stack:** Rust workspace (`prism-model`, `-import`, `-store`, `-capture`, `-analysis`, `-server`, `-cli`), React/TypeScript UI with vitest, Go demo app.

**Spec:** [2026-11-13-phase2-contention-io-design.md](../specs/2026-11-13-phase2-contention-io-design.md). Not in this plan: M3 ETW, M4 eBPF, M5 macOS (each gets its own plan, because their event contract is proven by this one).

## Global Constraints

- Targets: Go 1.22+, .NET 8+; Linux, macOS, Windows. Apache-2.0 core; new dependencies must be permissively licensed (`cargo deny check licenses` stays green).
- Prism never self-elevates. `--follow-up` refuses and prints the exact elevated command when privilege is missing.
- Doctor default window 60 s; follow-up capture default 30 s and is a new session tagged `doctor-follow-up-of:<short id of the Doctor session>`.
- Doctor shares are in **thread-seconds** over the window; idle runtime threads (the `IDLE_NAMES` leaf frames in `findings.rs`) are excluded from "busy". The target is not made busy by Doctor: if it was idle, the verdict is `Idle`.
- Every finding carries a confidence (`low|medium|high`) and the weakest fidelity of its evidence. No finding may claim more than its data supports (heuristic I/O from wall-clock stacks is `low`).
- Overhead of OS tracing is a static per-provider text plus the duration cap, printed before capture.
- Block I/O profiles: sample types `io_time` (ns, default) and `io_bytes` (bytes); the file is the synthetic leaf frame `[file: <path>]`. Off-CPU: `offcpu` (ns). Scheduler: `sched_delay` (ns).
- Windows data directory is `%LOCALAPPDATA%\prism` for new homes; an existing `%APPDATA%\prism` keeps being used. (Changed from the spec's "copy on first write": copying hundreds of MB is riskier than keeping the old directory; recorded in the spec by Task 6.)

## Review Focus

1. A session with no wall profile, or with empty profiles → `Inconclusive` / `Idle` with a note, never a panic, `NaN` or `inf` in the JSON (Task 8).
2. A mostly idle target must read `Idle`, not `CpuBound` from a few busy samples (Task 8).
3. A system-provider failure (missing privilege) while the language capture succeeds → session `Partial` with a warning, and Doctor still runs on what exists (Task 7, Task 10).
4. Mutex and block profiles overlap in Go, so lock evidence uses `max(lock, blocking)`, never the sum (Task 8).
5. Both `%LOCALAPPDATA%\prism` and `%APPDATA%\prism` present → the new one wins and nothing is deleted (Task 6).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/prism-model/src/lib.rs` | new kinds, `sample_types` names, `file_leaf` |
| `crates/prism-import/src/kind.rs` | infer the new kinds |
| `crates/prism-import/src/pprof/encode.rs` (new) | `Profile` to gzipped pprof |
| `crates/prism-capture/src/events.rs` (new) | event rows to `Artifact` for system providers |
| `crates/prism-capture/src/system.rs` (new) | `SystemProvider` trait, static texts |
| `crates/prism-capture/src/provider.rs`, `session.rs` | registry and `record()` integration |
| `crates/prism-analysis/src/doctor.rs` (new) | rule engine |
| `crates/prism-server/src/doctor.rs` (new) | `diagnose_session` (store + analysis), reused by the CLI |
| `crates/prism-server/src/api.rs` | `GET /api/sessions/{reference}/doctor` |
| `crates/prism-cli/src/cmd/doctor.rs` (new), `main.rs` | `prism doctor` |
| `ui/src/analysis/doctor.ts`, `ui/src/views/Doctor.tsx` (new) | card |
| `examples/go-demo/main.go` | `-mode` flag for known-answer workloads |

---

### Task 1: New kinds and sample-type names

**Files:**
- Modify: `crates/prism-model/src/lib.rs`, `crates/prism-model/tests/names.rs`
- Modify: `crates/prism-import/src/kind.rs`
- Modify: `crates/prism-cli/src/main.rs` (the `--kind`/`--kinds` help text and the `parse_kind` error text)

**Interfaces:**
- Produces: `ProfileKind::OffCpu` (`"off_cpu"`), `ProfileKind::SchedDelay` (`"sched_delay"`); `ProfileKind::ALL: [ProfileKind; 10]`; `pub mod sample_types { pub const IO_TIME: &str = "io_time"; pub const IO_BYTES: &str = "io_bytes"; pub const OFFCPU: &str = "offcpu"; pub const SCHED_DELAY: &str = "sched_delay"; }`; `pub fn file_leaf(path: &str) -> String` returning `[file: {path}]`.

- [ ] **Step 1: Write failing tests.** In `names.rs`: `new_kinds_round_trip` asserts `ProfileKind::parse("off_cpu") == Some(ProfileKind::OffCpu)`, `parse("sched_delay") == Some(SchedDelay)`, `ALL.len() == 10`, and that every kind's `as_str` parses back. Add `file_leaf_wraps_path`: `file_leaf("/data/x.db") == "[file: /data/x.db]"`. In `kind.rs` tests add rows to `table`: `[io_time ns, io_bytes bytes]` → `BlockIo`; `[offcpu ns]` → `OffCpu`; `[sched_delay ns]` → `SchedDelay`.
- [ ] **Step 2: Run** `cargo test -p prism-model -p prism-import` — expect FAIL (unknown variants).
- [ ] **Step 3: Implement** the variants, `ALL`, `as_str`, the `sample_types` module and `file_leaf` in `lib.rs`; in `infer_kind` add the three `has(...)` checks directly after the cpu check. Update the two CLI strings to list `off_cpu|sched_delay`.
- [ ] **Step 4: Run** `cargo build --workspace` and fix every non-exhaustive match the compiler reports (add the new kinds next to `Lock | Blocking` or a `_` arm that keeps current behaviour); then `cargo test --workspace` — expect PASS.
- [ ] **Step 5: Commit** `git add crates && git commit -m "feat: off_cpu and sched_delay profile kinds"`

---

### Task 2: Encode profiles as pprof and build event artifacts

**Files:**
- Create: `crates/prism-import/src/pprof/encode.rs`; Modify `crates/prism-import/src/pprof/mod.rs` (`pub mod encode;` plus `pub use encode::encode_pprof;`) and `lib.rs`
- Create: `crates/prism-capture/src/events.rs`; Modify `crates/prism-capture/src/lib.rs`
- Test: `crates/prism-import/tests/pprof_encode.rs`, `crates/prism-capture/tests/events.rs`

**Interfaces:**
- Produces: `prism_import::encode_pprof(p: &Profile) -> Result<Vec<u8>, ImportError>` (gzip-compressed pprof).
- Produces (in `prism_capture`):
  ```rust
  pub struct EventRow { pub stack: Vec<String> /* root first */, pub values: Vec<i64>, pub bucket: u32 }
  pub struct EventSpec<'a> { pub kind: ProfileKind, pub fidelity: Fidelity,
      pub sample_types: &'a [(&'a str, Unit)], pub start_unix_ns: i64,
      pub duration_ns: u64, pub bucket_ns: u64 }
  pub fn event_artifact(spec: &EventSpec, rows: &[EventRow]) -> Artifact
  ```
  `Artifact.file_name` is `"{kind}.pb.gz"`, `timeline` is `Some(Timeline { bucket_ns, start_unix_ns, sample_bucket, total_buckets: ceil(duration_ns / bucket_ns) })`.
- Consumes: `ProfileBuilder` (model), `Timeline` (model).

- [ ] **Step 1: Write failing tests.** `pprof_encode.rs::round_trip_preserves_stacks_and_totals`: import `tests/fixtures/go-cpu.pb.gz`, encode, import the result; assert equal `sample_types`, equal `total(i)` for each type, and equal sets of function names. `events.rs::event_artifact_imports_with_timeline`: two rows (`["main","load","[file: /a]"]`, `[5_000_000, 4096]`, bucket 0; same stack, bucket 3), spec kind `BlockIo`, types `[("io_time", Nanoseconds), ("io_bytes", Bytes)]`; import via `Store::import` with `kind_override`, `timeline: artifact.timeline`; assert record kind `block_io`, `default_total == 10_000_000`, and `load_timeline` is `Some` with `bucket_count() == 4` when `duration_ns` is 4 buckets long.
- [ ] **Step 2: Run** `cargo test -p prism-import --test pprof_encode` and `cargo test -p prism-capture --test events` — expect FAIL.
- [ ] **Step 3: Implement** `encode_pprof` (one pprof `Location`/`Function` per model frame/function, samples from stacks leaf-first, `default_sample_type` set, gzip with the crate already used for decoding). Implement `event_artifact` with `ProfileBuilder`, default sample type 0.
- [ ] **Step 4: Run** both tests — expect PASS.
- [ ] **Step 5: Commit** `git add crates && git commit -m "feat: encode pprof and build event artifacts for system providers"`

---

### Task 3: Timelines for the new kinds

**Files:**
- Modify: `crates/prism-capture/src/session.rs:122` (`wants_time`)
- Test: `crates/prism-capture/tests/fake_provider.rs`

**Interfaces:** Consumes `event_artifact` (Task 2). Produces: a recorded session keeps timelines of `BlockIo | OffCpu | SchedDelay` artifacts, and warns "no time resolution" when a requested timeline is missing for them.

- [ ] **Step 1: Write failing test** `new_kind_timelines_are_kept`: a fake provider (existing `FakeProvider` in the test file) returns a `BlockIo` artifact built with `event_artifact` (capability `BlockIo` available); record with `timeline: true`; assert `store.load_timeline(id)` is `Some`.
- [ ] **Step 2: Run** `cargo test -p prism-capture --test fake_provider new_kind_timelines_are_kept` — expect FAIL. The test also asserts that an artifact with `timeline: None` produces the warning `no time resolution for block_io`.
- [ ] **Step 3: Implement** by extending the `matches!` in `wants_time` to the three kinds.
- [ ] **Step 4: Run** `cargo test -p prism-capture` — expect PASS.
- [ ] **Step 5: Commit** `git commit -am "feat: keep timelines for I/O, off-CPU and scheduler profiles"`

---

### Task 4: UI guides for the new kinds

**Files:**
- Modify: `ui/src/analysis/guide.ts`, `ui/src/analysis/guide.test.ts`
- Modify: `ui/src/views/Summary.tsx` (render `guide.measured` under the questions)

**Interfaces:**
- Produces: `Guide` gains `measured: string` ("how this was measured"); `kindGuide('block_io' | 'off_cpu' | 'sched_delay')` return real guides; every existing guide gets a `measured` sentence; `GENERIC.measured` is `'Measured by the capture tool that produced this profile.'`.

- [ ] **Step 1: Write failing tests** in `guide.test.ts`: `kindGuide('block_io').title === 'Block I/O'` with actions containing the label `'Slowest files'` (patch `{ view: 'top' }`) and `'Top by total'`; `off_cpu` title `'Off-CPU time'`; `sched_delay` title `'Scheduler delay'`; for every kind in `['cpu','wall','memory','lock','blocking','threads','block_io','off_cpu','sched_delay']` `measured` is a non-empty string.
- [ ] **Step 2: Run** `cd ui && npx vitest run src/analysis/guide.test.ts` — expect FAIL.
- [ ] **Step 3: Implement** the guides. Block I/O questions: "Which files cost the most I/O time?", "Which call paths wait on them?"; Off-CPU: "What are threads waiting for when they are not running?"; Scheduler: "Are runnable threads waiting for a CPU?". `measured` strings follow the matrix in PLAN.md §1 (for block I/O: "Time between an I/O request and its completion, per stack, from OS tracing (ETW on Windows, eBPF on Linux)").
- [ ] **Step 4: Run** `cd ui && npx vitest run && npx tsc -b && npx oxlint` — expect PASS.
- [ ] **Step 5: Commit** `git add ui && git commit -m "feat(ui): guides for block I/O, off-CPU and scheduler profiles"`

---

### Task 5: Gate the filtered query paths

**Files:**
- Modify: `crates/prism-store/tests/perf.rs`
- Modify: `docs/FOLLOWUPS.md`

**Interfaces:** Consumes `prism_analysis::{flame, inverted_flame, sandwich, Filter}` (existing). Produces: the ignored perf gate also asserts that a focus-filtered flame, an `inverted_flame` and a `sandwich` each finish in at most 2 s on the 500 MiB profile.

- [ ] **Step 1: Extend** `perf_500mb_gate` with three `timed(...)` calls (focus regex = the name of the hottest function from `top`; `sandwich` on that function id) and `assert!(t <= Duration::from_secs(2), ...)` for each.
- [ ] **Step 2: Run** `cargo test -p prism-store --release --test perf -- --ignored --nocapture` and record the three timings.
- [ ] **Step 3: If all pass,** mark the FOLLOWUPS item "Ungated query paths" as done (struck through, with the timings). **If any fail,** keep the gate asserting the measured time rounded up to the next second, and rewrite the FOLLOWUPS item with the measured numbers so the optimisation is a separately tracked task; do not change `tree.rs` in this plan.
- [ ] **Step 4: Commit** `git commit -am "test: gate filtered flame, inverted flame and sandwich on the 500 MiB profile"`

---

### Task 6: Windows data directory

**Files:**
- Modify: `crates/prism-store/src/lib.rs:141` (`default_home`)
- Test: `crates/prism-store/tests/store.rs`
- Modify: `docs/superpowers/specs/2026-11-13-phase2-contention-io-design.md` (§10 last bullet), `docs/FOLLOWUPS.md`

**Interfaces:**
- Produces: `pub fn pick_home(os: HomeOs, env: &dyn Fn(&str) -> Option<std::ffi::OsString>, exists: &dyn Fn(&std::path::Path) -> bool) -> PathBuf` with `pub enum HomeOs { Windows, MacOs, Other }`; `default_home()` calls it with the real OS, `std::env::var_os` (non-empty only) and `Path::exists`.

- [ ] **Step 1: Write failing tests** `pick_home_windows_*`: (a) with `LOCALAPPDATA=L`, `APPDATA=A` and nothing existing → `L\prism`; (b) `A\prism` exists, `L\prism` does not → `A\prism`; (c) both exist → `L\prism`; (d) `LOCALAPPDATA` unset, `APPDATA=A` → `A\prism`; (e) `PRISM_HOME=/x` wins on every OS; (f) macOS and Other behave as before (`~/Library/Application Support/prism`, `$XDG_DATA_HOME/prism` then `~/.local/share/prism`, else `.prism`).
- [ ] **Step 2: Run** `cargo test -p prism-store --test store pick_home` — expect FAIL.
- [ ] **Step 3: Implement** `pick_home` by moving the current `default_home` body into it and adding the Windows preference order above. Keep `default_home_respects_prism_home` passing.
- [ ] **Step 4: Run** `cargo test -p prism-store` — PASS. Replace the spec §10 bullet with "Windows data directory: new homes use `%LOCALAPPDATA%\prism`; an existing `%APPDATA%\prism` keeps being used (no copy). Changed during planning." and strike the FOLLOWUPS "Data directory" item.
- [ ] **Step 5: Commit** `git commit -am "feat: Windows data dir moves to LOCALAPPDATA for new homes"`

---

### Task 7: SystemProvider seam and `record()` integration

**Files:**
- Create: `crates/prism-capture/src/system.rs`; Modify `crates/prism-capture/src/lib.rs`, `provider.rs`, `session.rs`
- Test: `crates/prism-capture/tests/system_provider.rs` (new)

**Interfaces:**
- Produces:
  ```rust
  pub const SYSTEM_KINDS: [ProfileKind; 3] = [ProfileKind::BlockIo, ProfileKind::OffCpu, ProfileKind::SchedDelay];
  pub trait SystemProvider: Send + Sync {
      fn id(&self) -> &'static str;
      fn capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability>;   // one per SYSTEM_KIND
      fn overhead(&self) -> &'static str;                            // static text, shown before capture
      fn capture(&self, rt: &RuntimeInfo, plan: &CapturePlan, cancel: &CancelToken,
                 progress: &dyn Fn(&str)) -> Result<CaptureOutcome, CaptureError>;
  }
  impl ProviderRegistry {
      pub fn register_system(&mut self, p: Box<dyn SystemProvider>);
      /// Capabilities for SYSTEM_KINDS: the provider's, or `available: false` with the
      /// note "needs an OS-level provider (ETW on Windows, eBPF on Linux); none is available in this build".
      pub fn system_capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability>;
      pub fn language_capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability>; // existing provider's
  }
  ```
- Behaviour of `record()`: an empty `plan.kinds` still means "every available **language** kind" (system kinds are never captured by default). Requested system kinds must be available or `CaptureError::Unsupported` (reason = the capability note). Language and system captures run concurrently over the same window (`std::thread::scope`); outcomes merge (artifacts appended, warnings appended, `end` is the most severe of `Cancelled > TargetExited > Complete`). A system capture that returns `Err` does not fail the session: its text becomes a warning `"{kind list}: {error}"` and the session ends `Partial`. When only system kinds were requested, the language provider's `capture` is not called. Before starting, `progress` receives `"[{id}] overhead: {overhead()}; capped at {n}s"`.

- [ ] **Step 1: Write failing tests** in `system_provider.rs`, with a `FakeSystem` returning `event_artifact` rows and a `FakeProvider` copy as in `fake_provider.rs`:
  `system_kinds_not_default` (empty kinds → no BlockIo artifact), `system_kind_unavailable_is_unsupported` (registry without a system provider, request `BlockIo` → `Unsupported` whose message contains `ETW on Windows`), `language_and_system_merge_into_one_session` (cpu + block_io profiles share the session id; status `Complete`), `system_failure_makes_partial_with_warning` (system returns `Err(Helper("access denied"))` → status `Partial`, a warning containing `access denied`, the cpu profile still imported), `system_only_does_not_call_language_capture` (language script panics if called), `overhead_is_reported_before_capture` (collect `progress` lines; the overhead line comes before the capturing line).
- [ ] **Step 2: Run** `cargo test -p prism-capture --test system_provider` — expect FAIL.
- [ ] **Step 3: Implement** per the interface section. Keep `ProviderRegistry::with_defaults()` registering no system provider.
- [ ] **Step 4: Run** `cargo test -p prism-capture` — PASS (all existing capture tests unchanged).
- [ ] **Step 5: Commit** `git add crates && git commit -m "feat: SystemProvider seam for OS-level captures"`

---

### Task 8: Doctor rule engine

**Files:**
- Create: `crates/prism-analysis/src/doctor.rs`; Modify `crates/prism-analysis/src/lib.rs`, `findings.rs` (make `IDLE_NAMES`, `GC_PATTERN` `pub(crate)`)
- Test: `crates/prism-analysis/tests/doctor.rs`

**Interfaces:**
- Produces (re-exported from `prism_analysis`, serialisable with `serde(rename_all = "camelCase")`; kinds as snake_case strings):
  ```rust
  pub struct DoctorProfile<'a> { pub kind: ProfileKind, pub short_id: &'a str, pub profile: &'a Profile }
  pub struct DoctorInput<'a> { pub window_ns: u64, pub profiles: Vec<DoctorProfile<'a>>,
      pub record_target: Option<String> /* e.g. "--pid 4242" */ }
  pub enum Verdict { CpuBound, IoBound, LockBound, SchedulerStarved, WaitingUnknown, Idle, Inconclusive }
  pub enum Confidence { Low, Medium, High }
  pub struct FollowUp { pub kinds: Vec<ProfileKind>, pub duration_secs: u64 /* 30 */, pub needs_privilege: bool }
  pub struct NextStep { pub label: String, pub command: Option<String>,
      pub open_profile: Option<String> /* short id */, pub follow_up: Option<FollowUp> }
  pub struct DoctorFinding { pub rule: &'static str, pub confidence: Confidence, pub fidelity: Fidelity,
      pub share_pct: f64, pub title: String, pub detail: String,
      pub next: Vec<NextStep>, pub hints: Vec<String> }
  pub struct Diagnosis { pub verdict: Verdict, pub summary: String, pub busy_threads: f64,
      pub findings: Vec<DoctorFinding>, pub notes: Vec<String> }
  pub struct DoctorConfig { pub idle_busy_threads: f64 /*0.05*/, pub cpu_bound_pct: f64 /*60*/,
      pub wait_pct: f64 /*40*/, pub lock_pct: f64 /*25*/, pub io_pct: f64 /*25*/,
      pub sched_pct: f64 /*10*/, pub gc_pct: f64 /*25*/ }
  pub fn diagnose(input: &DoctorInput, cfg: &DoctorConfig) -> Diagnosis
  ```
  Rules (all on each profile's `default_sample_type`, only when its unit is `Nanoseconds`; otherwise a `notes` entry says which profile was skipped):
  - `busy_ns` = wall total minus wall samples whose **leaf** function is in `IDLE_NAMES`; `busy_threads = busy_ns / window_ns` (0 when `window_ns == 0`). No wall profile → `Inconclusive`, note contains `wall-clock`. `busy_threads < idle_busy_threads` → `Idle` (summary says to rerun under load).
  - `cpu_share = min(cpu_total, busy_ns) / busy_ns * 100`; `wait_share = 100 - cpu_share`.
  - `lock_share = max(lock_total, blocking_total) / busy_ns * 100`, capped at 100.
  - I/O: with a `BlockIo` profile, `io_share = io_total / busy_ns * 100` (confidence High if the profile fidelity is not `Approximate`, else Medium). Without it, `io_share` = share of busy wall time in samples with a frame matching `^(syscall\.(Read|Write|Pread|Pwrite)|internal/poll\.\(\*FD\)\.(Read|Write)|os\.\(\*File\)\.(Read|Write)|System\.IO\.)` and confidence Low, fidelity Approximate.
  - Scheduler: `sched_share = sched_total / busy_ns * 100` from a `SchedDelay` profile only.
  - GC: share of cpu samples with a stack frame matching `GC_PATTERN`; finding only, never the verdict.
  - Triggered when `share >= threshold` (cpu: `cpu_bound_pct`, io: `io_pct`, lock: `lock_pct`, sched: `sched_pct`, waiting: `wait_share >= wait_pct` **and** neither IoBound nor LockBound triggered). Verdict = triggered rule with the largest share; ties break in the order Io, Lock, Scheduler, Cpu, Waiting; nothing triggered → `Inconclusive`. Findings are sorted by `share_pct` descending. All shares are finite (guard every division).
  - Next steps (`record_target` interpolated, e.g. `prism record --pid 4242 --kinds block_io,off_cpu --duration 30s`; when `record_target` is `None` the `command` is `None` and the label says to re-run Doctor with a target):
    - CpuBound: `open_profile` = cpu short id, `command` = `prism show {cpu id} --top 20`.
    - IoBound with a block_io profile: `open_profile` = block_io id, `command` = `prism show {id} --top 20`; without it: `follow_up` = `{kinds: [BlockIo, OffCpu], duration_secs: 30, needs_privilege: true}`.
    - LockBound: `open_profile` = the larger of lock/blocking, `command` = `prism show {id} --top 20 --sort total`.
    - SchedulerStarved: `open_profile` = sched id, command `prism show {id} --top 20`.
    - WaitingUnknown: `follow_up` = `{kinds: [BlockIo, OffCpu], duration_secs: 30, needs_privilege: true}`.
  - Hints (one or two plain sentences each, in `hints`): CPU "Optimise the top self-time functions; use `prism diff` after a change."; I/O "Cache or batch reads on the top files; check disk or volume latency."; lock "Shorten the critical section or shard the lock."; scheduler "Check CPU limits and throttling of the container, or reduce runnable threads."

- [ ] **Step 1: Write failing tests** in `doctor.rs` using `ProfileBuilder` helpers (`profile(kind, &[("time", Nanoseconds)], &[(&["main","work"], &[8_000_000_000])])`). Window 10 s. Scenarios and assertions:
  - `cpu_bound`: wall `[main,work]=8e9`, `[main,runtime.gopark]=1e9`; cpu `[main,work]=7.5e9` → `CpuBound`, `findings[0].share_pct` within 0.1 of 93.75, `busy_threads` within 0.01 of 0.8.
  - `lock_bound`: wall `[main,reserve,sync.(*Mutex).Lock]=6e9`, `[main,work]=2e9`; cpu `[main,work]=2e9`; lock `[main,reserve,sync.(*Mutex).Lock]=5.5e9` → `LockBound`; with an additional blocking profile of `5.5e9` the share is unchanged (max, not sum).
  - `io_likely_without_block_io`: wall `[main,load,os.(*File).Read,syscall.Read]=6e9`, `[main,work]=2e9`; cpu `[main,work]=2e9` → `IoBound`, finding confidence `Low`, `next[0].follow_up` kinds `[BlockIo, OffCpu]` and `needs_privilege == true`, command contains `--kinds block_io,off_cpu` and `--pid 4242`.
  - `io_firm_with_block_io`: the above plus a `BlockIo` profile (`io_time`, `[main,load,os.(*File).Read,[file: /data/x.db]]=5.5e9`, fidelity Native) → `IoBound`, confidence `High`, `next[0].open_profile` is the block_io short id.
  - `idle_target`: wall `[runtime.gopark]=9.9e9`, `[main,work]=1e7` → `Idle`, summary contains `under load`.
  - `waiting_unknown`: wall `[main,handle,time.Sleep]=6e9`, `[main,work]=2e9`; cpu `[main,work]=2e9` → `WaitingUnknown`.
  - `scheduler_finding`: cpu-bound data plus a `SchedDelay` profile of `3e9` → verdict stays `CpuBound`, findings contain rule `scheduler_starved` with share within 0.1 of 37.5.
  - `no_wall_profile`: only a cpu profile → `Inconclusive`, a note contains `wall-clock`.
  - `empty_profiles_are_finite`: every profile has zero samples → `Idle`; `serde_json::to_string(&diagnosis)` succeeds and contains neither `NaN` nor `inf`; `window_ns = 0` also yields a finite JSON.
  - `non_time_unit_is_skipped`: a cpu profile with `Count` units → a note names it and the verdict is computed without it.
- [ ] **Step 2: Run** `cargo test -p prism-analysis --test doctor` — expect FAIL.
- [ ] **Step 3: Implement** `doctor.rs` per the interface section; reuse `top`/`filtered_total` helpers where they fit, otherwise iterate samples and `stack_frames` directly. `DoctorConfig: Default` with the values above.
- [ ] **Step 4: Run** `cargo test -p prism-analysis` — PASS; `cargo clippy -p prism-analysis -- -D warnings`.
- [ ] **Step 5: Commit** `git add crates && git commit -m "feat: Doctor rule engine"`

---

### Task 9: `diagnose_session` and the server endpoint

**Files:**
- Create: `crates/prism-server/src/doctor.rs`; Modify `crates/prism-server/src/lib.rs` (`pub mod doctor;`), `crates/prism-server/src/api.rs` (route near `:127`)
- Test: `crates/prism-server/tests/api_doctor.rs` (new)

**Interfaces:**
- Produces: `pub fn diagnose_session(store: &Store, session: &SessionRecord) -> Result<Diagnosis, StoreError>`: loads each profile of the session, builds `DoctorInput` (`window_ns` = `session.duration_ms * 1_000_000`, falling back to the longest profile `duration_ns`; `record_target` = `--pid {pid}` when the session target has a pid, else `--url {url}` when it has a url, else `None`) and calls `diagnose` with `DoctorConfig::default()`.
- Produces: `GET /api/sessions/{reference}/doctor` returning the `Diagnosis` JSON (camelCase); unknown session → 404 with the existing error shape.
- Consumes: `prism_analysis::{diagnose, DoctorInput, DoctorProfile, DoctorConfig, Diagnosis}`.

- [ ] **Step 1: Write failing tests** (pattern of `tests/api_phase1.rs`): seed a store with a session holding a wall and a cpu profile (built with `ProfileBuilder` + `encode_pprof`, imported with `session_id`); `doctor_endpoint_returns_verdict` (status 200, `verdict == "cpuBound"` or the serde name chosen in Task 8, `findings` non-empty); `doctor_unknown_session_is_404`.
- [ ] **Step 2: Run** `cargo test -p prism-server --test api_doctor` — FAIL.
- [ ] **Step 3: Implement** the function and the handler (`blocking(...)` helper as in `pivot_h`).
- [ ] **Step 4: Run** `cargo test -p prism-server` — PASS.
- [ ] **Step 5: Commit** `git add crates && git commit -m "feat: Doctor endpoint"`

---

### Task 10: `prism doctor`

**Files:**
- Create: `crates/prism-cli/src/cmd/doctor.rs`; Modify `crates/prism-cli/src/cmd/mod.rs`, `crates/prism-cli/src/main.rs`
- Test: `crates/prism-cli/tests/doctor.rs` (new; uses a fixture session seeded in a temp `--home`, no live target)

**Interfaces:**
- Produces the command (same target flags as `record`: `--pid`, `--url`, `--go-test`, `-- <program>`):
  ```
  prism doctor [target] [--duration 60s] [--name N] [--json] [--check] [--follow-up]
  prism doctor --session <REF> [--json]      # diagnose an existing session without capturing
  ```
  Capture kinds: language kinds `Cpu, Wall, Lock, Blocking, Threads` that are available, plus `SYSTEM_KINDS` that are available. Unavailable system kinds do not fail the run; the report notes what was not seen and how to enable it (the capability note).
  Report layout (stdout): first line `Verdict: {summary}`; then for each finding `  {title}  [{confidence}, {share}%]`, `  next: {command}` per next step, and `  hint: {hint}`; then `note:` lines. `--json` prints the `Diagnosis` JSON only.
  `--check` prints a table `KIND AVAILABLE FIDELITY NOTE` for language and system capabilities of the target and exits 0 without capturing.
  `--follow-up`: after the report, take the first finding's first `NextStep` with `follow_up`. If its kinds are not all available, print `error: cannot run the follow-up: {capability note}` plus the `command`, and exit 1 (never elevate). Otherwise record it for `follow_up.duration_secs` as a new session with tag `doctor-follow-up-of:{doctor session short id}` and print its short id. With no follow-up step, print `no follow-up capture to run` and exit 0.
  Exit codes: 0 report produced; 1 capture error or refused follow-up; 2 usage error.
- Consumes: `prism_capture::{record, RecordRequest, ProviderRegistry}`, `prism_server::doctor::diagnose_session`.

- [ ] **Step 1: Write failing tests** (assert_cmd, as in `tests/check.rs`): `doctor_session_report_names_verdict_and_command` (seeded cpu-bound session; stdout contains `Verdict:` and `prism show `); `doctor_session_json_is_valid` (`--json` parses and has `verdict`); `doctor_session_with_idle_data_says_rerun_under_load`; `doctor_requires_a_target_or_session` (exit 2, stderr names the options); `follow_up_without_system_provider_refuses` (seeded waiting-unknown session with a `--pid`-bearing target, `--session REF --follow-up`: exit 1, stderr contains `ETW on Windows`, stdout contains `--kinds block_io,off_cpu`).
- [ ] **Step 2: Run** `cargo test -p prism-cli --test doctor` — FAIL.
- [ ] **Step 3: Implement** `cmd/doctor.rs` (reuse `record::build_target`, `ProviderRegistry::with_defaults`, `print_table`); wire the subcommand in `main.rs` including the clap conflicts (`--session` conflicts with every target flag and with `--check`).
- [ ] **Step 4: Run** `cargo test -p prism-cli` — PASS; `cargo clippy --workspace -- -D warnings`.
- [ ] **Step 5: Commit** `git add crates && git commit -m "feat: prism doctor"`

---

### Task 11: Doctor card in the UI

**Files:**
- Create: `ui/src/analysis/doctor.ts`, `ui/src/analysis/doctor.test.ts`, `ui/src/views/Doctor.tsx`
- Modify: `ui/src/api.ts` (`getDoctor`), `ui/src/api.test.ts`, `ui/src/views/SessionList.tsx:153` (render the card above `SessionTimeline`), `ui/src/theme.css`

**Interfaces:**
- Produces in `api.ts`: types `Diagnosis`, `DoctorFinding`, `NextStep` (mirroring Task 8's camelCase JSON) and `getDoctor = (sessionRef: string) => request<Diagnosis>(\`/api/sessions/${encodeURIComponent(sessionRef)}/doctor\`)`.
- Produces in `doctor.ts` (pure): `verdictTitle(v: Diagnosis['verdict']): string` (`'CPU-bound'`, `'I/O-bound'`, `'Lock-bound'`, `'Scheduler-starved'`, `'Waiting, cause unknown'`, `'Idle'`, `'Inconclusive'`); `confidenceLabel(c, fidelity): string` (`'high confidence'` etc., plus ` (estimated)` when fidelity is `approximate`); `isFiniteShare(n: number): boolean`.
- Produces `Doctor({ sessionId, onOpenProfile })`: loads on mount, shows the verdict title and summary, each finding with its confidence label, each next-step `command` in a `<code>` with a Copy button (clipboard failure shows nothing broken), an "Open view" button when `openProfile` is set, and a muted line when the request fails ("Doctor is not available for this session"). No chart.

- [ ] **Step 1: Write failing tests**: `doctor.test.ts` (`verdictTitle` for all seven values; `confidenceLabel('low','approximate') === 'low confidence (estimated)'`; `isFiniteShare(NaN) === false`) and an `api.test.ts` case that `getDoctor('a b')` requests `/api/sessions/a%20b/doctor`.
- [ ] **Step 2: Run** `cd ui && npx vitest run` — FAIL.
- [ ] **Step 3: Implement** the helpers, `getDoctor`, the component and the mount in `SessionList.tsx`.
- [ ] **Step 4: Run** `cd ui && npx vitest run && npx tsc -b && npx oxlint && npm run build` — PASS. Then `cargo build -p prism-cli` (the UI bundle is embedded) and open the UI on a seeded session to confirm the card renders (note in the commit message if not checked in a browser).
- [ ] **Step 5: Commit** `git add ui crates && git commit -m "feat(ui): Doctor card on sessions"`

---

### Task 12: Known-answer demo and end-to-end test

**Files:**
- Modify: `examples/go-demo/main.go`, `examples/README.md`, `docs/USER_GUIDE.md`, `docs/FOLLOWUPS.md`, `docs/PLAN.md` (roadmap status line for Phase 2)
- Test: `crates/prism-capture/tests/e2e.rs` (add to the existing gated file) or `crates/prism-cli/tests/doctor_e2e.rs`

**Interfaces:**
- Produces: go-demo flag `-mode cpu|lock|disk|mixed` (default `mixed`, today's behaviour). `cpu`: only the request handlers. `lock`: only the 8 inventory workers, holding the lock for 200 µs. `disk`: 4 workers that each loop `os.ReadFile` + `os.WriteFile` of a 1 MiB file in `os.TempDir()` with `fsync`, so most wall time is I/O wait.
- Produces e2e tests (`#[ignore]`, gated on `PRISM_E2E=1` like the existing ones): `doctor_cpu_mode_is_cpu_bound` (verdict `CpuBound`), `doctor_lock_mode_is_lock_bound` (verdict `LockBound`), `doctor_disk_mode_is_io_likely` (verdict `IoBound` with a `low`-confidence finding and a follow-up step, since no system provider exists yet). Each records 10 s with `prism_capture::record` and calls `prism_server::doctor::diagnose_session`.

- [ ] **Step 1: Write the three e2e tests** (they fail or are skipped until the demo has the modes).
- [ ] **Step 2: Implement** `-mode` in the demo.
- [ ] **Step 3: Run** `PRISM_E2E=1 cargo test -p prism-cli --test doctor_e2e -- --include-ignored --nocapture`. If a verdict is wrong on real data, tune the `DoctorConfig` defaults in Task 8's file (and its fixtures) rather than the test, and say so in the commit message.
- [ ] **Step 4: Docs:** add a "Find what is slow with `prism doctor`" section to `USER_GUIDE.md` (the three-step flow: doctor, read verdict, run the printed command; what privilege adds); list the new kinds in the CLI reference; mark Phase 2 M1/M2 done and M3 to M5 pending in `PLAN.md` §7; record anything deferred in `FOLLOWUPS.md`.
- [ ] **Step 5: Verify the whole tree:** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo deny check licenses`, `cd ui && npx vitest run && npm run build`.
- [ ] **Step 6: Commit** `git add -A examples docs crates && git commit -m "feat: known-answer Doctor demo and docs"`

---

## Self-review

- **Spec coverage:** M1 (kinds, file leaf, sample types, timelines, guides, query gate, data dir) = Tasks 1 to 6; seam = Task 7; M2 rules, `--follow-up`, `--check`, JSON, UI card = Tasks 8 to 11; demo apps = Task 12. M3 to M5 are explicitly later plans.
- **Spec items deliberately not in this plan:** the timeline "lane per kind" is satisfied by the existing session timeline and heat map consuming the new kinds' timelines (Task 3); no new lane code is needed. The overhead text is a static string per provider (Task 7).
- **Types:** `NextStep.follow_up`, `Verdict`, `Confidence`, `Diagnosis` are defined in Task 8 and used unchanged in Tasks 9 to 11; `SYSTEM_KINDS` is defined in Task 7 and used in Task 10.
