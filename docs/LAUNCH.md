# Launching Prism v0.1

> Working name: **Prism**. The name is not yet cleared (see [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) §1 #6). Replace it everywhere below if it changes. Anything in `[brackets]` is a placeholder to fill in before publishing.

This is the launch plan and the copy bank. Pair it with the engineering checklist in [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md).

---

## 1. The product in one page

**Prism is a local-first profiling workbench for Go and .NET.**
Attach to a running process (or import a profile), and get flame graphs, a call graph, a time heat map, before/after diffs and plain-language findings in your browser. Nothing leaves your machine.

| | |
|---|---|
| **Tagline** | Find out why your code is slow. In minutes, on your laptop. |
| **Alt taglines** | One profiler for Go and .NET. · Record, see, fix. · Profiling without the setup tax. |
| **For** | Developers who need to find a performance problem *now*, on a dev machine or local container |
| **Not for** | Continuous production profiling (use Pyroscope, Parca or Grafana Cloud Profiles) |
| **Platform at launch** | Windows 10/11 x64. Linux and macOS next |
| **Price / licence** | Free, open source (Apache-2.0). Team features may be commercial later |

### The problem

Profiling is a tool tax. Each language has its own profiler, its own file format and its own viewer. You collect a `.pb.gz` here and a `.nettrace` there, then convert, then squint at a flame graph with no context. Most developers never do it, and guess instead.

### What Prism does differently

1. **One workflow for several languages.** Go and .NET record into the same model and open in the same UI. Imports from pprof, collapsed stacks, speedscope and `.nettrace` too.
2. **Zero-config recording.** `prism record --pid <pid>` for .NET. For Go, add one import line, or point it at an existing `net/http/pprof` endpoint.
3. **Answers, not just pictures.** Flame graph, Top, sandwich, call graph and a heat map over time, plus a Findings panel that says where to look and why.
4. **Compare anything.** Diff two profiles or two time windows. Run `prism check` in CI to fail a build on a performance regression.
5. **Private by design.** Local store, server bound to `127.0.0.1`, no telemetry, no account.
6. **Honest numbers.** Every profile carries a fidelity badge (native, approximate, derived) so you know how far to trust it.

### Proof points to collect before launch (don't launch without 2–3)

- [ ] A real open-source Go or .NET project where Prism found a real hotspot, with before/after numbers: `[project, fix, % improvement]`
- [ ] Time from install to first flame graph on a clean Windows machine: `[N minutes]`
- [ ] A 60–90 s screen recording of record → flame graph → finding → diff
- [ ] 3–5 beta-tester quotes (with permission)

---

## 2. Audience and positioning

| Segment | Pain | Message |
|---|---|---|
| **.NET developers on Windows** (primary at launch) | PerfView is powerful but dense; `dotnet-trace` output needs another tool; Visual Studio profiler is heavyweight and VS-only | "A friendly, modern profiler that attaches to your running .NET app with one command" |
| **Go developers on Windows** | pprof's web UI needs Graphviz and manual steps; heap, mutex and block profiles are separate files | "All Go profile kinds in one place, no Graphviz, no juggling files" |
| **Teams shipping both** | Two toolchains, two habits | "One tool, one workflow" |
| **CI-minded engineers** | Perf regressions slip in unnoticed | "`prism check` fails the PR when a function got slower" |

**Competitive frame (be fair; people check):**

| Tool | Where it wins | Where Prism fits |
|---|---|---|
| `go tool pprof` | Ubiquitous, scriptable | Prism adds a UI that needs no Graphviz, sessions, diffs, heat map |
| PerfView, VS Profiler, dotTrace | Deep .NET detail | Prism is lighter, cross-language, browser-based, free |
| Speedscope, FlameGraph | Great viewers | Prism records too, and adds findings, diffs, history |
| Pyroscope, Parca, Grafana Profiles | Continuous production profiling | Different job; Prism is on-demand on a dev machine |
| JetBrains dotTrace/Rider | Polished IDE integration | Paid, IDE-bound; Prism is standalone and open source |

Rule: never claim "faster than" or "more accurate than" without a benchmark we can publish.

---

## 3. Launch assets

| Asset | Owner | Status |
|---|---|---|
| Name, domain, GitHub org, handles | | [ ] |
| Logo and favicon (`profiler-icon.svg` exists; finalise wordmark) | | [ ] |
| Root README with screenshot and GIF | | [ ] |
| Landing page (copy in §5) | | [ ] |
| 60–90 s demo video; 3 short GIFs (flame graph, heat map, diff) | | [ ] |
| 5 UI screenshots, light and dark | | [ ] |
| "Why I built this" blog post (copy skeleton in §6) | | [ ] |
| How-to articles ×3 (see §8) | | [ ] |
| Release notes for v0.1.0 | | [ ] |
| Social preview image (1280×640) for the GitHub repo | | [ ] |
| Press/Show HN text (§6) | | [ ] |

---

## 4. Timeline

### T-4 to T-2 weeks: build in private
- Finish P0 items in the release checklist.
- Start code-signing validation; reserve name, domain, handles.
- Collect proof points (§1).

### T-2 to T-1 weeks: private beta
- Invite **5–10 testers** (Windows .NET and Go developers you trust). Give them the RC zip.
- Ask them to do one task: "profile something you work on and tell me where it got stuck." Watch one or two over a screen share without helping; note every stumble.
- Fix the top 5 stumbles. Collect quotes.

### T-1 week: ready
- Release candidate frozen. Clean-VM install passes. Go/No-Go gate green.
- Landing page and docs live but unlinked. Videos recorded.
- Draft all posts; schedule them. Prepare an FAQ of the 15 questions you expect.

### Launch day (Tuesday–Thursday, morning US Pacific is best for HN/Reddit; pick a day you can stay online for 8 hours)
1. Tag `v0.1.0`; verify published artifacts by installing them.
2. Make the repo public (if not already); pin the announcement discussion.
3. Post **Show HN** (§6). Stay in the thread; answer every comment within the hour.
4. Post to Reddit (r/golang, r/dotnet, r/csharp, r/programming) following each sub's self-promotion rules; one at a time, spaced out, tailored text.
5. Post on X/Bluesky/LinkedIn/Mastodon with the GIF.
6. Email the beta testers and ask for a star/share only if they genuinely liked it.
7. Submit to: **Go Weekly**, **.NET Weekly**, **Awesome Go**, **Awesome .NET**, **Console.dev**, **Hacker Newsletter** pickup, **Product Hunt** (optional; developer tools do modestly there).
8. Winget/Scoop already live so "install" works.

### Launch week
- Daily: triage issues within 24 h, ship `v0.1.1` quick fixes, post a short "what we fixed" note.
- Publish one how-to article per day or two (§8).
- Thank people publicly; turn good feedback into issues.

### Day 30 review
- Look at the metrics (§9). Decide v0.2 priority (Linux/macOS vs Phase 2 contention/I/O) from actual demand.

---

## 5. Landing page copy

**Hero**

> # Find out why your code is slow.
> Prism is a local-first profiler for Go and .NET. Attach to a running process, see a flame graph in seconds, and get a plain-language list of what to fix. Free and open source. Nothing leaves your machine.
>
> **[Download for Windows]**  ·  [View on GitHub]  ·  [Watch the 90-second demo]

**Install strip**

```powershell
irm https://[domain]/install.ps1 | iex
prism demo
```
```powershell
scoop bucket add prism https://github.com/[org]/scoop-prism; scoop install prism
winget install [Org].Prism
```

**Three-step section**

1. **Record.** `prism record --pid 1234` attaches to a live .NET or Go process. No restart for .NET. One import line for Go.
2. **See.** Flame graph, Top, call graph and a heat map of when the time went. Switch between CPU, wall-clock, memory, locks and threads.
3. **Fix and compare.** Findings point at the hot path. Run again, diff the two, and prove it got faster. Put `prism check` in CI to keep it that way.

**Feature grid**

- *One tool, many languages*: Go and .NET today; imports from pprof, collapsed stacks, speedscope, `.nettrace`.
- *Findings*: plain-language explanations of where time and memory go.
- *Time heat map*: see bursts and stalls, then select a window and zoom in.
- *Diff*: compare two profiles or two time windows, with colours for better and worse.
- *CI regression check*: `prism check` exits non-zero when a function slows down.
- *Private*: runs on `127.0.0.1`, stores data locally, no telemetry, no account.
- *Honest*: every profile shows how trustworthy its numbers are.

**Comparison note** (small print, factual): "Prism is for on-demand profiling on a developer machine. For always-on production profiling, see Pyroscope, Parca or Grafana Cloud Profiles."

**FAQ**

- *Does it send my data anywhere?* No. The server listens on `127.0.0.1` only and Prism has no telemetry.
- *What do I need installed?* Nothing to browse and import. To record Go, a Go 1.22+ project (one import line or an existing `net/http/pprof` endpoint). To record .NET, a running .NET 8+ app.
- *Why does Windows warn me?* [Signed? Say so. If not: explain SmartScreen and how to verify the SHA-256.]
- *Linux and macOS?* Coming next. [Link to roadmap.]
- *Is it free?* Yes, Apache-2.0. We may offer paid team features later; the local tool stays free.
- *Is this a continuous profiler?* No. It is for on-demand investigation.

---

## 6. Announcement copy

### Show HN

**Title options** (pick one; no hype words):
- `Show HN: Prism – a local profiler for Go and .NET with flame graphs and diffs`
- `Show HN: Prism – record a running Go/.NET process and see why it's slow`

**Body**

> Hi HN. I built Prism because profiling Go and .NET meant juggling different tools and file formats, and most of the time I just guessed instead.
>
> Prism attaches to a running process (or imports pprof, collapsed stacks, speedscope, .nettrace) and opens a browser UI: flame graph, top functions, call graph, a heat map over time, before/after diffs, and a findings panel that points at likely hotspots. `prism check` returns a non-zero exit code when a function regresses, for CI.
>
> It's local-first: the server binds to 127.0.0.1, data stays on your machine, no telemetry, no account. It's written in Rust (core, CLI, embedded UI server) with a small Go agent and a .NET helper for capture. Apache-2.0.
>
> Today it's Windows only (Linux/macOS are next). It is for on-demand investigation, not continuous production profiling, so it's complementary to Pyroscope/Parca.
>
> Known limits: [list 3 honestly: e.g. .NET CPU numbers are approximate; Go wall-clock has no time axis; no Linux/macOS yet].
>
> Try it without any setup: `prism demo`. Repo: [link]. I'd love to hear where the install or the first run tripped you up.

Tips: reply to every comment; thank critics; never argue; fix a reported bug within the thread and say so.

### Reddit (r/golang, r/dotnet)

Tailor each. Lead with the problem in that ecosystem, not the product.

> **r/dotnet:** "I made a free profiler that attaches to a running .NET app with one command: flame graph, heat map, diffs. Windows first. Looking for feedback from people who've used PerfView or dotTrace."
>
> **r/golang:** "A pprof UI that needs no Graphviz and shows CPU, heap, mutex, block and goroutine profiles together, with diffs and a CI regression check. Windows first; feedback wanted on the agent API."

### X / Bluesky / LinkedIn

> I've open-sourced Prism: a local-first profiler for Go and .NET.
> Attach to a process → flame graph in seconds → findings → diff after your fix.
> Free, private, Windows today; Linux/macOS next.
> [GIF] [link]

### "Why I built this" blog skeleton

1. The moment that started it (a real slow-code story: `[your story]`).
2. Why existing tools were a tax (name them kindly).
3. What I wanted: one workflow, local, honest numbers.
4. A walk-through on a real project with before/after: `[numbers]`.
5. How it works in two paragraphs (the unified model; fidelity badges).
6. What it isn't (not continuous profiling) and what's next (Linux/macOS, contention and I/O).
7. How to try it and how to help.

### v0.1.0 release notes template

```markdown
# Prism v0.1.0 (Windows)

First public release.

## Highlights
- Record live Go and .NET processes, or import pprof / collapsed / speedscope / .nettrace
- Flame graph, Top, sandwich, call graph, heat map, history, findings
- Diff profiles or time windows; `prism check` for CI
- Local-only: binds to 127.0.0.1, no telemetry

## Install
- winget: `winget install [Org].Prism`
- Scoop: `scoop bucket add prism … ; scoop install prism`
- PowerShell: `irm https://[domain]/install.ps1 | iex`
- Or download the zip below and verify the SHA-256

## Try it
`prism demo`

## Known limitations
- Windows x64 only (Linux and macOS next)
- [.NET CPU values are approximate; Go wall-clock has no time axis]

## Thanks
[beta testers]
```

---

## 7. Messaging guardrails

- **Say:** local-first, private, on-demand, one workflow, honest fidelity, free and open source.
- **Don't say:** "replaces" PerfView/pprof, "production-ready continuous profiling", "AI-powered" (deferred), "enterprise-grade".
- **Be upfront about:** Windows-only at launch, v0.1, approximate .NET CPU numbers, Go agent import requirement.
- **Open core:** say clearly what's free forever and what may become paid, in the README.
- **Respond to criticism with data and fixes**, not defence.

---

## 8. Content plan (first 6 weeks)

| Week | Piece | Channel |
|---|---|---|
| Launch | Show HN, Reddit, social, release notes | HN, Reddit, X |
| 1 | "Find a hot loop in a Go service in 5 minutes" | Blog, r/golang |
| 1–2 | "Profile a running .NET app without restarting it" | Blog, r/dotnet |
| 2 | "Fail your CI when a function gets slower (`prism check`)" | Blog, dev.to |
| 3 | "Heat maps: seeing *when* your app is slow" | Blog, short video |
| 4 | "How Prism normalises Go and .NET profiles" (technical deep dive) | Blog, HN |
| 5 | Case study from a beta tester | Blog |
| 6 | Linux/macOS preview and roadmap post | Blog, community |

Also: record a screencast for each how-to, pin a "Getting started" video on the README, answer related Stack Overflow questions with helpful, non-spammy links.

---

## 9. Metrics (what "good" looks like at day 30)

No telemetry, so use public signals. Set your own targets; these are starting points.

| Signal | Source | Day-30 target |
|---|---|---|
| Installs (release downloads + winget + scoop) | GitHub API, package managers | [500] |
| GitHub stars | GitHub | [500] |
| Time-to-first-flame-graph from beta sessions | Observed | ≤ 10 min |
| Issues opened / closed | GitHub | median first response ≤ 24 h |
| Install failures reported | Issues | < 10 % of installs |
| Docs visits → install click | Web analytics (privacy-friendly, e.g. Plausible) | [20 %] |
| Contributors (non-author) | GitHub | [5] |
| "Would recommend" from beta users | Survey (1 question) | ≥ 8/10 |

**Decision rule:** if install friction (not features) dominates feedback, spend v0.1.x on the installer and `prism doctor` before adding anything new.

---

## 10. Risks and responses

| Risk | Response |
|---|---|
| SmartScreen/Defender blocks the exe | Signed binaries, checksum docs, pre-submit to Microsoft, FAQ entry, winget/Scoop channels |
| "Windows only?" backlash | Say it upfront; publish the Linux/macOS roadmap with a rough date; accept feature votes |
| Go agent import feels invasive | Lead with `--url` (existing `net/http/pprof`) and `go test` modes; keep the agent tiny and documented |
| Name conflict | Resolve before launch (checklist §1 #6) |
| Launch-day install bug | Clean-VM test, RC phase, same-day `v0.1.1` path ready |
| Low attention | Lean on proof points (real project, real numbers), not feature lists; target the ecosystems' newsletters |
| Support overload | Templates, FAQ, `prism doctor` output in every bug report, label `good first issue` |
| Comparison to Pyroscope/Parca | Clear "on-demand vs continuous" framing in the first screen of the README |

---

## 11. Launch-day checklist

- [ ] v0.1.0 tagged; zip, checksums and signatures published
- [ ] Installed the **published** artifacts on a clean Windows machine
- [ ] winget/Scoop/install script work
- [ ] `prism demo` works offline
- [ ] README, docs, landing page live; links checked
- [ ] Demo video and GIFs embedded
- [ ] Discussions and issue templates enabled
- [ ] Show HN, Reddit, social drafts ready
- [ ] FAQ drafted for the likely questions (privacy, licence, why not X, Linux/macOS, signing)
- [ ] You are free for the next 8 hours
- [ ] Hotfix procedure rehearsed
