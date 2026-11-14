# UI parity with the CLI: record, targets, diagnose, regression check

Date: 2026-11-14. Status: draft for approval. Scope: `crates/perfhawk-server`, `ui/`.

## Intent and why

Everything in the CLI should be doable from the browser so users do not switch context. Today the UI covers
`import`, `ls`, `show`, `sessions`, `diff`, `rm` and the history/finding views. It does not cover:

| CLI | UI today | Gap |
|---|---|---|
| `record` | none | cannot start a capture |
| `targets` | none | cannot see what can be recorded |
| `doctor [TARGET]` | Doctor card for an existing session only | cannot diagnose a live process |
| `check` | none (diff + Findings only) | no pass/fail against a threshold |

`open`, `ui` are launchers and need no UI equivalent.

## Goals

- From the Home screen: see recordable targets, pick one (or enter a pid / pprof URL), choose duration and kinds, start,
  watch progress, cancel, and land on the finished session.
- "Diagnose" = record, then open the session with the Doctor card shown: the UI form of `doctor TARGET`.
- A regression check panel in the Diff view: threshold %, minimum value, pass/fail list: the UI form of `check`.
- Same error handling and accessibility standards as the rest of the UI (`ErrorNotice`, focus, live announcements).

## Non-goals and anti-goals

- **No launching programs from the browser** (`-- <program>`) and no `--go-test` (decided 2026-11-14: attach only).
  A web page must never be able to make the server run an arbitrary command.
- No remote URL attach from the UI: only loopback hosts. Use the CLI for remote endpoints.
- No new capture capabilities; same providers, same kinds, same limits as the CLI (duration 1s to 10m, default 30s).
- No multi-user or auth system; Perfhawk stays a local single-user tool.
- A UI that can start captures but hides why one failed (for example ETW needs an administrator shell) is a failure.

## Constraints

- Server today depends on model, store, analysis, import only (`crates/perfhawk-server/Cargo.toml`). Recording needs
  `perfhawk-capture` (`session::record(store, registry, req, cancel, progress)`, `CancelToken`, `ProviderRegistry`).
- The existing guard stays: loopback Host and Origin only, `X-Perfhawk-Request: 1` on every state-changing call
  (`api.rs:155`), CSP `script-src 'self'`.
- Captures take up to 10 minutes: the API must be asynchronous (start, poll, cancel); a page reload must not lose the job.
- One capture at a time per server (the CLI does the same); a second start returns a clear error.
- Never elevate privileges. When a kind needs administrator rights, surface the CLI's own message and exact rerun command.

## Approaches considered

1. **Server runs captures in-process (chosen).** `perfhawk-server` depends on `perfhawk-capture`; a small job manager
   runs `session::record` on a blocking thread. Reuses all logic and the same store; progress and cancel map straight onto
   the existing `progress` callback and `CancelToken`. Cost: bigger server crate, platform-specific capture code linked in.
2. Server shells out to `perfhawk record` as a child process. Isolates platform code, but parses output, doubles process
   management, and makes cancel and progress fragile.
3. UI only shows a copyable command (status quo). Zero risk, but it is exactly the context switch the user wants gone.

## Design

### API (all under `/api`)

- `GET /targets` -> `[{ kind: "pid"|"url", label, pid?, url?, runtime, command? }]` using the same discovery as
  `perfhawk targets` (Go agents, .NET processes). Read-only.
- `POST /record` (needs `X-Perfhawk-Request: 1`) body `{ pid?: number, url?: string, kinds?: string[], duration_secs: number,
  name?: string, tags?: string[], timeline?: boolean }`. Exactly one of `pid`, `url`. `url` must be `http(s)` with host
  `127.0.0.1`, `localhost` or `[::1]`. Returns `202 { job_id }`. 409 if a capture is running. 400 for bad input
  (reuse the CLI's duration validation).
- `GET /record/current` -> the running or most recent job: `{ job_id, state: "running"|"done"|"failed"|"cancelled",
  started_ms, duration_ms, progress: string[], session_id?, error?: { code, message } }` (in-memory only).
- `DELETE /record/{job_id}` (header required) -> fires the job's `CancelToken`; the partial session is kept the way the CLI
  keeps it (status `cancelled`/`partial`).
- `GET /capabilities?pid=|url=` -> kinds available for a target and why others are not (from `Provider::capabilities` and
  `system_capabilities`), so the form can disable options with the reason.
- Regression check needs no new endpoint: it runs on the existing `/api/diff` data (see below).

### UI

- **Home: "Record" tab** (next to Sessions and Profiles): targets list with refresh; pid / URL fields; kinds checkboxes
  loaded from capabilities (disabled with the reason as help text); duration (1s to 10m, default 30s); name and tags
  under "More options"; buttons "Start recording" and "Record and diagnose".
- **Progress**: a panel (`role="status"`, polite live region) with elapsed time, the progress lines, and Cancel. Survives
  reload by reading `GET /record/current` on mount. Done -> button and auto-navigation to the session (`?session=<id>`);
  "Record and diagnose" opens it with the Doctor card expanded.
- **Errors** use `ErrorNotice`; capture errors (unsupported kind, missing privilege) keep the CLI's message and any rerun
  command verbatim under Technical details, with a plain hint on top.
- **Welcome and Help** replace the copy-a-command text with the real flow, keeping the CLI command as the secondary
  option ("or from a terminal: `perfhawk record --pid <pid> --open`").
- **Regression check** in Diff view: fields Threshold % and Minimum value (defaults from the CLI's `check` flags), a
  "Run check" button, result "PASS"/"FAIL: N regressions" with the offending functions listed and linked, plus a copyable
  equivalent `perfhawk check ...` command. Logic is the same rule as `perfhawk check`; to avoid two implementations it moves
  into `perfhawk-analysis` and both the CLI and a new `GET /api/check` use it.

## Security notes (what could go wrong)

- CSRF / hostile page: Host and Origin guard plus the custom header on every POST/DELETE (preflight never approved).
- SSRF through `url`: loopback hosts only.
- Resource abuse: one capture at a time, duration capped at 10m, bounded progress buffer.
- No program launch or file path input, so no command injection surface.

## Left to the builder

- Exact JSON field names and error codes (follow existing `ApiError` codes).
- Job manager shape (thread vs `spawn_blocking`), and how `GET /capabilities` caches detection.
- Layout and copy of the Record tab; wording of hints.
- Whether `perfhawk targets` discovery needs refactoring into a shared function the server can call.
- Where the `check` rule lives in `perfhawk-analysis` and how the CLI is adapted without changing its output or exit codes.
