# UI/CLI Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Do `record`, `targets`, diagnose and `check` from the browser UI, with the CLI behaviour unchanged.

**Architecture:** `perfhawk-server` links `perfhawk-capture` and runs `perfhawk_capture::record` on a blocking thread inside a one-slot job manager (start, poll, cancel). Target discovery and the `check` rule move into shared library code so CLI and server call one implementation. The UI gets a Record tab on Home and a check panel in the Diff view.

**Tech Stack:** Rust (axum 0.8, tokio, serde), React + TypeScript (vite, vitest, oxlint).

**Spec:** `docs/superpowers/specs/2026-11-14-ui-cli-parity-design.md`

## Global Constraints

- Attach only: no `-- <program>`, no `--go-test`, no file-path input from the browser.
- URL targets: `http`/`https` with host `127.0.0.1`, `localhost` or `[::1]` only.
- Duration 1s to 10m, default 30s (reuse `perfhawk_capture::duration` constants and range).
- One capture at a time per server; a second start returns 409.
- Every POST/DELETE needs `X-Perfhawk-Request: 1` (`require_custom_header`, `crates/perfhawk-server/src/api.rs:157`). Loopback Host/Origin guard and CSP `script-src 'self'` stay unchanged.
- Never elevate privileges. Privilege errors show the CLI message and exact rerun command.
- CLI output text and exit codes (`check`: 0/1/2; `record`) must not change.
- Errors use `ApiError` codes in the existing `{"error":{"code","message"}}` shape; UI shows them with `ErrorNotice`.
- Accessibility: progress is a `role="status"` polite live region; all controls keyboard reachable; `npm test` axe checks pass.

## Review Focus

- Second `POST /api/record` while one runs -> 409, first job unaffected.
- Browser reload mid-capture -> `GET /api/record/current` still returns the running job and the UI re-attaches.
- `url` with host `evil.com`, `http://127.0.0.1.evil.com`, `file://`, userinfo (`http://localhost@evil.com`) -> 400.
- Both `pid` and `url`, or neither -> 400; `duration_secs` 0 or 601 -> 400.
- Cancel after the job already finished -> no crash, state stays `done`.
- Target exits mid-capture -> job `done` with partial session, not `failed`.
- `check` where baseline has zero samples -> 400 with the CLI's message, not 500.
- ETW kind without admin -> `needs_privilege` error carrying the rerun command.

---

### Task 1: Shared `check` rule in perfhawk-analysis

**Files:**
- Create: `crates/perfhawk-analysis/src/check.rs`
- Modify: `crates/perfhawk-analysis/src/lib.rs` (add `mod check; pub use check::*;`), `crates/perfhawk-cli/src/cmd/check.rs`
- Test: `crates/perfhawk-analysis/src/check.rs` (`#[cfg(test)]`; move the three `growth_pct` tests here)

**Interfaces:**
- Produces:
  - `pub struct CheckInput<'a> { pub base: &'a Profile, pub head: &'a Profile, pub base_name: &'a str, pub head_name: &'a str, pub base_unit_hint: Option<&'a str>, pub sample_type: Option<&'a str>, pub threshold: f64, pub min_value: f64 }`
  - `pub struct Regression { pub name: String, pub base_self: i64, pub head_self: i64, pub growth_pct: Option<f64>, pub delta_points: f64 }` (serde `camelCase`, `Serialize`)
  - `pub struct CheckReport { pub ok: bool, pub sample_type_name: String, pub base_total: i64, pub head_total: i64, pub regressions: Vec<Regression> }`
  - `pub fn run_check(i: &CheckInput) -> Result<CheckReport, String>` — error strings identical to today's (`sample type '{n}' is not present in both profiles`, `the baseline '{}' has no samples to compare against`, `the profile '{}' has no samples`).
  - `pub fn growth_pct(base_self: i64, head_self: i64, base_dur: Option<u64>, head_dur: Option<u64>) -> Option<f64>`

- [ ] **Step 1: Write failing tests** in `check.rs`: the three existing `growth_pct` tests plus `run_check_flags_new_function` (head has function absent from base holding >= `min_value` points -> `ok == false`, `growth_pct == None`) and `run_check_empty_baseline_errors` (error text equals `the baseline 'b' has no samples to compare against`).
- [ ] **Step 2: Run** `cargo test -p perfhawk-analysis check` — Expected: FAIL (unresolved module).
- [ ] **Step 3: Implement** `run_check` and `growth_pct` by moving the body of `run_inner` (`cmd/check.rs:59-138`) and `growth_pct` (`:177-195`) verbatim; no printing, no store access.
- [ ] **Step 4: Adapt CLI** `cmd/check.rs::run_inner` to resolve/load via `Store`, call `run_check`, keep its own printing and `Report` JSON shape (`sample_type {name, unit}` still built from `brec`). Delete the moved code and tests from the CLI file.
- [ ] **Step 5: Run** `cargo test -p perfhawk-analysis -p perfhawk-cli` — Expected: PASS, including any existing CLI check tests untouched.
- [ ] **Step 6: Commit** `git commit -am "refactor(analysis): move check rule into perfhawk-analysis"`

### Task 2: `GET /api/check`

**Files:**
- Modify: `crates/perfhawk-server/src/api.rs` (route + handler, near `diff_h` at `:644`)
- Test: `crates/perfhawk-server/tests/api_check.rs` (create; copy the `start`/`get` helpers and a two-profile fixture from `tests/api_doctor.rs`)

**Interfaces:**
- Consumes: Task 1 `run_check`, `CheckInput`.
- Produces: `GET /api/check?head=REF&base=REF&st=NAME&threshold=10&min_value=1.0` -> `{ ok, sampleType: {name, unit}, baseTotal, headTotal, regressions: [{name, baseSelf, headSelf, growthPct, deltaPoints}] }`. Defaults `threshold=10`, `min_value=1.0`. Negative or non-finite numbers -> 400 `bad_request`. Data errors from `run_check` -> 400 `bad_request` with the message. Unknown ref -> 404 via `StoreError`.

- [ ] **Step 1: Write failing tests**: `check_reports_regression` (head has a function 3x base -> `ok:false`, regressions[0].name matches), `check_ok_when_identical` (`ok:true`, empty list), `check_rejects_negative_threshold` (400), `check_empty_baseline_is_400_with_cli_message`, `check_unknown_ref_is_404`.
- [ ] **Step 2: Run** `cargo test -p perfhawk-server --test api_check` — Expected: FAIL (404 no endpoint).
- [ ] **Step 3: Implement** `check_h(State, Q<CheckQuery>)` using `record_and_profile` (`api.rs:636`) for both refs inside `spawn_blocking`; unit comes from the head `ProfileRecord.sample_types[idx].unit`.
- [ ] **Step 4: Run** the same command — Expected: PASS.
- [ ] **Step 5: Commit** `feat(server): GET /api/check`

### Task 3: Shared target discovery

**Files:**
- Create: `crates/perfhawk-capture/src/targets.rs`
- Modify: `crates/perfhawk-capture/src/lib.rs` (`pub mod targets; pub use targets::*;`), `crates/perfhawk-cli/src/cmd/targets.rs`
- Test: `crates/perfhawk-capture/src/targets.rs` (`#[cfg(test)]` for the pure mapping)

**Interfaces:**
- Produces:
  - `pub struct TargetRow { pub runtime: &'static str, pub pid: u32, pub version: Option<String>, pub agent: bool, pub command: Option<String> }` (`Serialize`, `Clone`)
  - `pub fn list_targets() -> (Vec<TargetRow>, Option<String>)` — rows plus the .NET warning text (the CLI prints `warning: {e}; showing Go targets only`).
  - `pub fn rows_from(go: Vec<go::discovery::Discovery>, dotnet: Vec<RuntimeInfo>) -> Vec<TargetRow>` (pure, for the test).

- [ ] **Step 1: Write failing test** `rows_from_maps_go_agent_and_dotnet_process` (Go row `agent:true, command:None`; .NET row `agent:false`; .NET entry with `pid: None` dropped).
- [ ] **Step 2: Run** `cargo test -p perfhawk-capture targets` — Expected: FAIL.
- [ ] **Step 3: Implement** by moving the mapping from `cmd/targets.rs:13-34`.
- [ ] **Step 4: Adapt CLI** `targets::run` to call `list_targets` and keep its table/JSON output byte-identical.
- [ ] **Step 5: Run** `cargo test -p perfhawk-capture -p perfhawk-cli` — Expected: PASS.
- [ ] **Step 6: Commit** `refactor(capture): shared target discovery`

### Task 4: Server wiring, `GET /api/targets`, `GET /api/capabilities`

**Files:**
- Modify: `crates/perfhawk-server/Cargo.toml` (add `perfhawk-capture = { path = "../perfhawk-capture" }`), `crates/perfhawk-server/src/lib.rs` (`AppState.registry: Arc<ProviderRegistry>`; `pub fn router_with_registry(store, registry)` for tests; `router`/`serve` use `ProviderRegistry::with_defaults()`), `api.rs` (routes)
- Create: `crates/perfhawk-server/src/capture_api.rs` (new handlers for Tasks 4-5, keeps `api.rs` from growing)
- Test: `crates/perfhawk-server/tests/api_targets.rs` (create)

**Interfaces:**
- Consumes: Task 3 `list_targets`.
- Produces:
  - `GET /api/targets` -> `{ targets: [{ kind: "pid", label, pid, url: null, runtime, command }], warning: string|null }`. Go rows use `kind:"pid"` with `pid` set (the Go agent URL is not exposed; attach is by pid).
  - `GET /api/capabilities?pid=N` or `?url=U` (same one-of and URL rules as record) -> `{ runtime, kinds: [{ kind, available, fidelity, note }] }` using `registry.detect(&target)` then `provider.capabilities(&rt)` merged with `registry.system_capabilities(&rt)`. Detect failure -> 400 code `not_detected` with the `CaptureError` text.
  - `pub(crate) fn parse_target(pid: Option<u32>, url: Option<&str>) -> Result<Target, ApiError>` with the loopback rule.
  - `ApiError::conflict(msg)` (409 `capture_running`), `ApiError::bad_code(code, msg)`.

- [ ] **Step 1: Write failing tests** with a fake provider (`impl Provider` returning one available `Cpu` and one unavailable `OffCpu` with note `"needs an elevated shell"`) injected via `router_with_registry`: `capabilities_lists_available_and_unavailable_with_note`; `capabilities_requires_exactly_one_target` (neither/both -> 400); `parse_target_rejects_non_loopback` as a unit test over `["http://evil.com","http://127.0.0.1.evil.com","file:///x","http://localhost@evil.com/","ftp://localhost"]` (all Err) and `["http://127.0.0.1:6060","https://localhost/x","http://[::1]:1"]` (all Ok); `targets_endpoint_returns_shape` (200, has `targets` array).
- [ ] **Step 2: Run** `cargo test -p perfhawk-server --test api_targets` and `--lib` — Expected: FAIL.
- [ ] **Step 3: Implement** `parse_target` by parsing with `url::Url` (add `url = "2"` dep) and checking scheme and `host_str()` exactly, rejecting userinfo. Handlers run discovery in `spawn_blocking`.
- [ ] **Step 4: Run** same commands plus `cargo test -p perfhawk-server` — Expected: PASS (existing tests unaffected).
- [ ] **Step 5: Commit** `feat(server): targets and capabilities endpoints`

### Task 5: Record job manager and record API

**Files:**
- Create: `crates/perfhawk-server/src/jobs.rs` (job manager); extend `capture_api.rs`
- Modify: `lib.rs` (`AppState.jobs: JobManager`), `api.rs` routes
- Test: `crates/perfhawk-server/tests/api_record.rs` (create; fake provider that sleeps on `cancel.wait_timeout(duration)` and writes one pprof artifact from `perfhawk-import` fixtures, plus a variant that returns `CaptureError::Helper("... elevated shell ...")`)

**Interfaces:**
- Consumes: Task 4 `parse_target`, `ApiError::conflict`, `AppState.registry`.
- Produces:
  - `JobManager::start(&self, work: impl FnOnce(&CancelToken, &(dyn Fn(&str)+Sync)) -> Result<SessionRecord, CaptureError> + Send + 'static) -> Result<JobSnapshot, JobBusy>`; `snapshot() -> Option<JobSnapshot>`; `cancel(job_id) -> bool`.
  - `JobSnapshot { job_id: String, state: "running"|"done"|"failed"|"cancelled", started_ms: i64, duration_ms: i64, progress: Vec<String>, session_id: Option<String>, error: Option<{code, message}> }` (serde snake_case as the spec). Progress buffer bounded to 200 lines (drop oldest).
  - `POST /api/record` body `{ pid?, url?, kinds?: string[], duration_secs: u64, name?, tags?: string[], timeline?: bool }` -> `202 {job_id}`. `kinds` parsed with `serde_json::from_value::<ProfileKind>` (unknown -> 400 listing valid kinds, same text as CLI `parse_kind`, `main.rs:236`). Duration outside 1..=600 -> 400.
  - `GET /api/record/current` -> snapshot or `204`.
  - `DELETE /api/record/{job_id}` -> `204`; unknown id -> 404 `not_found`; finished job -> `204` no-op.
  - Failure mapping: `CaptureError::Unsupported` -> code `unsupported`; message containing `elevated shell` -> code `needs_privilege` and message gains `\nrun: perfhawk record <--pid N|--url U> --kinds <list> --duration <N>s` built from the request (same shape as `elevated_command`, `cmd/record.rs:129`); other -> `capture_failed`. Session `Cancelled` -> state `cancelled` with `session_id`; `Partial`/`Complete` -> `done`.

- [ ] **Step 1: Write failing tests**: `record_runs_and_lands_a_session` (POST 202, poll `current` until `done`, `session_id` resolves via `/api/sessions/{id}`); `second_start_is_409`; `cancel_keeps_partial_and_state_cancelled`; `cancel_after_done_is_noop`; `missing_header_is_403`; `both_pid_and_url_is_400`; `duration_bounds` (0, 601 -> 400; 1 and 600 accepted); `unknown_kind_is_400`; `privilege_error_carries_rerun_command` (error.code `needs_privilege`, message contains `perfhawk record --pid`); `progress_is_bounded` (jobs.rs unit test, 500 lines -> 200 kept, newest last); `current_is_204_before_any_job`.
- [ ] **Step 2: Run** `cargo test -p perfhawk-server --test api_record` — Expected: FAIL.
- [ ] **Step 3: Implement** `JobManager` with `Mutex<Option<Job>>`; `start` spawns `std::thread` (not tokio pool) and holds the slot until the thread stores the final snapshot. The record closure calls `perfhawk_capture::record(&store, &registry, &req, cancel, progress)`.
- [ ] **Step 4: Run** `cargo test -p perfhawk-server` — Expected: PASS. Also run `cargo clippy -p perfhawk-server --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `feat(server): async record API with cancel`

### Task 6: UI API client, Record tab and progress

**Files:**
- Modify: `ui/src/api.ts` (types `Target`, `Capability`, `RecordJob`, `RecordRequest`; fns `getTargets`, `getCapabilities`, `startRecord`, `getRecordCurrent`, `cancelRecord`; POST/DELETE send `X-Perfhawk-Request: 1`), `ui/src/views/Home.tsx` (third tab `record`), `ui/src/url.ts` only if the tab needs a URL key (it does not: reuse existing `?session=`)
- Create: `ui/src/views/RecordTab.tsx` (targets, form, capabilities), `ui/src/views/RecordProgress.tsx` (live region, elapsed, lines, Cancel), `ui/src/views/recordLogic.ts` (pure: `validateRecordForm`, `buildCliCommand`)
- Test: `ui/src/views/recordLogic.test.ts`, `ui/src/api.test.ts` (extend), `ui/src/a11y/` axe test for `RecordTab` following the existing pattern there

**Interfaces:**
- Consumes: Task 4/5 endpoints.
- Produces: `validateRecordForm({pid, url, durationSecs}) -> string | null` (one target required, pid positive int, URL loopback http(s), duration 1..600); `buildCliCommand(form) -> string` (e.g. `perfhawk record --pid 123 --duration 30s --open`); `<RecordTab onOpenSession={(shortId, diagnose: boolean) => void} />`.

- [ ] **Step 1: Write failing tests**: `validateRecordForm` cases mirroring Review Focus (both/neither target, `http://evil.com`, duration 0/601, valid pid, valid `http://127.0.0.1:6060`); `buildCliCommand` for pid and url; `api.test.ts`: `startRecord` sends method POST and the `X-Perfhawk-Request` header, `cancelRecord` sends DELETE with the header; axe test: tab renders with no violations, progress panel has `role="status"`.
- [ ] **Step 2: Run** `npm --prefix ui test -- recordLogic api a11y` — Expected: FAIL.
- [ ] **Step 3: Implement** the client functions and `recordLogic.ts` first, then the components. Form: targets list with Refresh, pid/URL inputs, kinds checkboxes from capabilities (disabled with note as help text, `aria-describedby`), duration default 30, name/tags under a `<details>` "More options", buttons "Start recording" and "Record and diagnose". On mount call `getRecordCurrent`; if running, show progress. Poll every 1s while running. On `done` call `onOpenSession`. Errors via `ErrorNotice`; `needs_privilege` shows the message with the rerun command as copyable text under Technical details.
- [ ] **Step 4: Wire** `Home.tsx`: `Tab = 'sessions' | 'profiles' | 'record'`; button "Record" beside the others; `onOpenSession` sets `?session=<id>` and, when `diagnose`, passes a flag so `SessionList` expands the session with Doctor visible (SessionList already renders `<Doctor>` when expanded, `SessionList.tsx:169`).
- [ ] **Step 5: Run** `npm --prefix ui test` and `npm --prefix ui run lint` and `npm --prefix ui run build` — Expected: all pass.
- [ ] **Step 6: Commit** `feat(ui): record tab with progress and cancel`

### Task 7: Regression check panel in Diff view

**Files:**
- Modify: `ui/src/api.ts` (`CheckReport` type, `getCheck`), `ui/src/views/DiffView.tsx` (render panel below the toolbar when `base` is set)
- Create: `ui/src/views/CheckPanel.tsx`, `ui/src/views/checkLogic.ts` (`checkCommand(head, base, threshold, minValue) -> string`)
- Test: `ui/src/views/checkLogic.test.ts`, axe test for `CheckPanel`

**Interfaces:**
- Consumes: Task 2 `GET /api/check`.
- Produces: `<CheckPanel head={string} base={string} sampleType?={string} onOpenFunction?={(name:string)=>void} />`. Defaults Threshold 10, Minimum value 1.0.

- [ ] **Step 1: Write failing tests**: `checkCommand('h','b',10,1)` equals `perfhawk check h --baseline b --threshold 10 --min-value 1`; axe clean; panel shows `PASS` for `ok:true` and `FAIL: 2 regressions` (singular `FAIL: 1 regression`) with names listed.
- [ ] **Step 2: Run** `npm --prefix ui test -- check` — Expected: FAIL.
- [ ] **Step 3: Implement** panel: two number inputs (min 0), "Run check" button, result in a `role="status"` region, copyable command. API errors via `ErrorNotice`.
- [ ] **Step 4: Run** `npm --prefix ui test`, lint, build — Expected: pass.
- [ ] **Step 5: Commit** `feat(ui): regression check panel in diff view`

### Task 8: Copy, docs and end-to-end check

**Files:**
- Modify: `ui/src/views/Welcome.tsx`, `ui/src/views/HelpPanel.tsx` (replace copy-a-command text with the Record flow; keep `perfhawk record --pid <pid> --open` as secondary), `CHANGELOG.md`, `README.md` (UI section), `docs/` UI manual a11y checklist if it lists Home controls
- Test: existing Welcome/Help tests adjusted to new copy

- [ ] **Step 1: Update** tests that assert old copy, run `npm --prefix ui test` — Expected: FAIL on old strings.
- [ ] **Step 2: Update** Welcome/Help copy; keep wording plain, no new jargon.
- [ ] **Step 3: Run** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm --prefix ui test`, `npm --prefix ui run lint`, `npm --prefix ui run build` — Expected: all pass.
- [ ] **Step 4: Manual verify** (PowerShell): start a Go or .NET sample from `examples/`, run `cargo run -p perfhawk-cli -- ui`, record 5s from the Record tab, cancel one run, run Diagnose, run a check between two sessions; confirm ETW kind without admin shows the rerun command. Record what was not verified.
- [ ] **Step 5: Commit** `docs: UI record, diagnose and check`

---

## Self-review notes

- Spec coverage: targets (T3, T4), record/progress/cancel/reload (T5, T6), capabilities (T4, T6), diagnose (T6 `diagnose` flag), check (T1, T2, T7), copy (T8), security rules (T4, T5 tests), error handling (T5 mapping, T6).
- Task 6 depends on `SessionList` accepting a "show Doctor" flag; implementer must read `SessionList.tsx` before editing.
- `GET /api/targets` returns Go agents by pid only; the spec's `kind:"url"` rows are not produced (Go agent URL is loopback and attach-by-pid works). Flag if URL rows are wanted.
