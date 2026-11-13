# Phase 2 design: contention and I/O

Status: draft for approval. Date: 2026-11-13. Roadmap entry: [PLAN.md §7](../../PLAN.md).

## 1. Intent

A developer's Go (or .NET) service is slow and they do not know whether the cause is disk, locks or CPU. Today Prism can capture CPU, wall, memory, lock, blocking and thread profiles per runtime, but it cannot tell the user *which one matters*, has no Block I/O data (the `BlockIo` kind is declared in `prism-model` but nothing produces it), and cannot see what happens outside the runtime (the OS scheduler, the disk).

Phase 2 closes that gap. After it, `prism doctor` takes about 60 seconds, says what is slow, and prints the exact next command to run. That scenario is the success test, not any single capture feature.

## 2. Goals

- `prism doctor` produces a verdict with evidence, a ready-to-run follow-up command, and short likely-fix hints. It stops there by default; `--follow-up` runs the follow-up capture.
- Block I/O per stack (time and bytes, with the file as the leaf) for Go and .NET on **Windows (ETW)** and **Linux (eBPF)**. macOS is best-effort.
- Off-CPU time per stack (waiting on I/O, locks, channels, sleep) and scheduler analysis (run-queue delay; container CPU throttling on Linux).
- The existing timeline and heat map show I/O, off-CPU and scheduler data, aligned to the same session window as the language profiles.
- Phase 2 exit criterion (from the roadmap): Block I/O per stack for Go and .NET on Linux and Windows.

## 3. Non-goals

- No new languages (Phase 3). No heap snapshots. No share server or CI gating changes.
- No AI diagnosis. Doctor is a deterministic rule engine. AI stays deferred (see AI.md).
- No whole-system profiling: capture is scoped to one target process (and its threads).
- No Perfetto-style general trace explorer. Only the lanes needed for I/O, off-CPU and scheduler data.

## 4. Anti-goals (a failure even if it technically works)

- Prism silently elevating privileges, or running a privileged capture the user did not request.
- A Doctor verdict stated with more confidence than the data supports (for example claiming "I/O-bound" from wall-vs-CPU alone, without OS data). Every finding carries a confidence and the fidelity of its evidence.
- Doctor leaving the user with a verdict and no idea what to do next.
- OS-level tracing adding noticeable overhead on the target without telling the user the cost first.

## 5. Constraints

- Targets: Go 1.22+, .NET 8+. Linux, macOS, Windows as in Phase 1. Apache-2.0 core, permissive-licensed bundled dependencies only.
- ETW needs administrator; eBPF needs `CAP_BPF`/root and a recent kernel (CO-RE, roughly 5.8+). Prism never self-elevates; it explains and prints the exact elevated command.
- The development machine is Windows. The Linux provider cannot be built or tested here; it needs a Linux CI runner (and so a git remote, which does not exist yet).
- Single static `prism` binary stays the goal; any eBPF programs are embedded in it.

## 6. Approaches considered

- **A. Provider-first, one OS at a time** (ETW, then eBPF, then Doctor). Testable here, but Doctor, the feature that delivers the goal, arrives last.
- **B. Core-first, then providers** — chosen by the user on 2026-11-13. Shared model and views first, then Doctor v0 on data Go and .NET already provide, then ETW, then eBPF. Earliest delivery of the "disk, locks or CPU?" answer; the risky Linux work comes last when the event model is proven.
- **C. Linux-first.** Rejected: biggest and riskiest piece first, and it cannot be developed or tested on the current machine.

Doctor stops at the verdict plus the exact command, and offers `--follow-up` (the user's decision, 2026-11-13).

## 7. Design

### 7.1 Milestones (approach B)

**M1. Shared core.**
- Model: add the kinds `OffCpu` and `SchedDelay` next to the existing `BlockIo` (my call; builder may merge `SchedDelay` into a timeline-only lane if the store cost is high). Values in nanoseconds; Block I/O also records bytes.
- Block I/O stacks end in a synthetic leaf frame such as `[file: /data/x.db]` so flame and top views need no model change (my call).
- Timeline: I/O, off-CPU and scheduler data reuse the existing bucketed timeline and heat map; the UI gets a lane per kind.
- UI views for the new kinds, with fidelity badges and "how this was measured" text. Exercised with fixture profiles, no OS tracing required.
- Housekeeping from FOLLOWUPS that this phase picked up: extend the fast trie path (or gate) filtered flame, `inverted_flame` and `sandwich`; move the Windows data directory to `%LOCALAPPDATA%`.

**M2. Doctor v0.**
- A `SystemProvider` seam (see 7.2) is defined in M1/M2 so M3 and M4 plug in without changing Doctor.
- `prism doctor [target]`: default 60 s, one session, capturing CPU, wall, lock, blocking and threads through the existing language provider, plus Block I/O/off-CPU if a system provider is available and permitted.
- Rule engine in `prism-analysis` (pure function over a session's profiles, unit-testable with fixtures). Each rule yields a Finding: id, severity, confidence, evidence (numbers and top stacks), next command, likely-fix hints.
- Without OS data Doctor can say "off-CPU dominated, likely I/O or waiting" and why it cannot say more, and tells the user how to enable OS tracing.
- Output: terminal report and `--json`. The UI session page shows the verdict card and a button/deep link to the matching view.
- `prism doctor --follow-up` runs the first recommended capture after the verdict. `prism doctor --check` reports which kinds are available and what privilege is missing.

Initial rules (thresholds are builder-tunable, validated on the demo apps):

| Finding | Evidence | Next command |
|---|---|---|
| CPU-bound | CPU samples cover most of wall time; a few stacks dominate | CPU flame / `prism show --view top` |
| Waiting, cause unknown | wall time much greater than CPU time | `prism record --kind block-io,offcpu` (needs elevation) |
| I/O-bound | off-CPU time dominated by Block I/O stacks | Block I/O view, by file |
| Lock contention | lock/mutex profile has large wait relative to wall | Lock view |
| Scheduler starved | run-queue delay high, or cgroup throttling | Scheduler view |
| GC pressure | GC share of time (Go/.NET counters) | Memory view |

**M3. ETW provider (Windows).**
- Kernel session for the target process: file and disk I/O events, context switches (off-CPU), ready-thread events (scheduler delay), with stacks.
- Stack symbolization is a first-class part of this milestone: ETW gives addresses. Go via the binary's symbol data (pclntab/DWARF), .NET via ETW rundown/method-load events, native frames via PDB. A shared `symbolize` component is introduced here and reused on Linux.
- Verified on this machine. Real-ETW tests need admin and are opt-in; a fake system provider (like `fake-dotnet-helper`) covers the plumbing in normal CI.
- Doctor's I/O verdict becomes firm on Windows.

**M4. eBPF provider (Linux).**
- Same event contract as M3. Block I/O (`biolatency`/`fileslower`-style at the VFS/block layer), off-CPU (`sched_switch`), run-queue latency, cgroup `cpu.stat` throttling.
- Requires a Linux CI runner with privileges, so a remote and a working Linux CI job are a prerequisite (also a release blocker).
- Go: frame-pointer stacks resolved with ELF symbols. .NET: perf map enabled via the diagnostics IPC, resolved through the same symbolizer.
- Implementation mechanism (embedded Aya program versus calling out to bpftrace/BCC) is the builder's call; constraint: no extra install steps for the user if avoidable, and a clear permission/kernel-version error otherwise.

**M5. macOS best-effort (optional).** `fs_usage`/dtrace wrappers where SIP allows; marked `approximate`. Cut first if the schedule slips.

### 7.2 System provider seam

`prism-capture` gets a second provider kind beside the language `Provider`: a `SystemProvider` that attaches to a pid (obtained from the language provider's `RuntimeInfo`) and captures `BlockIo`/`OffCpu`/`SchedDelay` for the same window. The session already stores several profiles for one window; the system provider just adds more artifacts. Capability reporting says which kinds are available and the missing privilege, so Doctor and `prism record` can explain instead of failing.

### 7.3 Permissions and overhead

- `--check` and capability notes state the missing privilege and print the exact command to rerun with it (administrator shell on Windows, `sudo` or capabilities on Linux).
- Before an OS-level capture starts, Prism prints an estimated overhead and the duration cap.
- Without privilege, Doctor still runs (language-provider data only) and says what it could not see.

## 8. Testing

- Fixture-driven unit tests for rules and views; a `fake` system provider for end-to-end tests in CI.
- Real ETW tests on Windows, opt-in (admin). Real eBPF tests on a privileged Linux CI job.
- A demo app with a deliberate disk-bound path and a lock-bound path (Go first, then .NET) so Doctor's verdicts are checked against known answers.
- Perf gate: new queries covered by the existing perf gate; Doctor's analysis runs well within one second on a typical 60 s session.

## 9. Left to the builder

Rule thresholds; exact kind names and store schema migration; ETW and eBPF event selection details; eBPF implementation mechanism; symbolizer internals; UI layout of the verdict card and new lanes; the overhead estimation method.

## 10. Calls made after the builder check (mine; please check)

- **Existing model:** `ProfileKind::BlockIo` exists today as a name only. Block I/O is one profile per capture: time (ns) as the value, bytes as a second value, file as the synthetic leaf frame. The builder confirms against the store's actual sample layout and owns the migration.
- **Alignment:** the system provider attaches to the pid the language provider reports and records wall-clock UTC timestamps (ETW QPC and eBPF monotonic times are converted at capture start). Process restart or exit ends the capture with `TargetExited`. Linux containers: the provider resolves the host pid from the target's pid namespace; if it cannot, it reports a clear error instead of capturing the wrong process.
- **Doctor maths:** all shares use **thread-seconds** over the window (sum across threads), excluding threads that are parked in the runtime's own idle pools (Go idle Ps, .NET idle thread-pool workers, GC threads reported separately). "CPU-bound" means CPU thread-seconds are a large share of *busy* (non-idle) thread-seconds. Doctor assumes the target is under representative load; if the target is mostly idle it says "target looked idle, rerun under load" instead of a verdict. Doctor does not generate load.
- **Symbolization bar for M3/M4:** required: Go frames from unstripped binaries, and .NET JIT-compiled methods (via rundown or perf map). Allowed to fall back: ReadyToRun/native frames as `module+offset`, with PDB/DWARF names when available. Out of scope: stripped Go binaries (reported as such). New permissively licensed Rust crates are fine (for example `ferrisetw`, `object`, `gimli`); a Windows-only Rust provider is expected, not a .NET helper.
- **Linux CI gate:** creating the git remote and a privileged Linux runner (GitHub-hosted runners may not allow eBPF; a self-hosted runner or VM may be needed) is the user's action. M4 is planned as a gated milestone: it may be written against the shared contract but is reported "unverified" until it passes on real Linux.
- **`--follow-up`:** if the follow-up needs privilege Prism does not have, it refuses and prints the exact elevated command; it never elevates. The follow-up capture creates a new session tagged as a child of the Doctor session, default 30 s.
- **Overhead estimate:** a static per-provider range plus the duration cap, shown before the capture. No calibration in Phase 2.
- **Windows data directory:** new homes use `%LOCALAPPDATA%\prism`; an existing `%APPDATA%\prism` keeps being used (no copy), unless the new directory exists too. Changed during planning from "copy on first write".

## 11. Risks

- **Symbolization** of Go and .NET frames from OS-level stacks is the largest technical risk; a stack of raw addresses would fail the exit criterion.
- **Linux is untested until CI exists.** Mitigation: the shared core and event contract are proven on Windows first; get the remote and Linux CI in place before M4.
- **Doctor over-claiming.** Mitigation: confidence and evidence fidelity on every finding; thresholds validated on demo apps.
- **Privilege friction** discourages use. Mitigation: Doctor works unprivileged and says exactly what elevation would add.
