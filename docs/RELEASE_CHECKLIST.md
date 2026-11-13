# v0.1 Release Checklist (Windows first)

**Goal:** a Windows developer goes from "heard of Prism" to "found a hot function in my own Go or .NET app" in **under 10 minutes**, without building anything from source.

**Scope:** Windows 10/11 x64 only. Linux and macOS follow in v0.2 (see §12). Go 1.22+ and .NET 8+ targets. CLI plus local browser UI.

Legend: **P0** = blocks release · **P1** = should ship, can slip a week · **P2** = after launch. Sizes: S ≤ ½ day, M ≈ 1–2 days, L ≈ 3+ days.

---

## 0. Where we are today (from the repo)

| Area | State |
|---|---|
| Features | Import (pprof, collapsed, speedscope, `.nettrace`), `prism record` for Go and .NET, flame/top/sandwich/call graph/heat map/history/diff/findings, `prism check` for CI, sessions |
| Tests | Rust, UI (vitest), Go agent, .NET helper, gated e2e. **Only ever run locally on Windows** |
| Git remote | **None.** Nothing has been pushed; GitHub Actions has never run |
| Docs | `USER_GUIDE.md` (575 lines), `CI.md`, `PLAN.md`, `VISUALIZATION.md`, `examples/README.md`. **No root `README.md`** |
| Install path | Build from source (Rust + Node 22.12+). No binaries, no installer |
| Release hygiene | No CHANGELOG, no third-party notices, no SECURITY/CONTRIBUTING, no version tags |
| Version | `0.1.0` in `Cargo.toml` |

The product works. What is missing is everything between "it works on my machine" and "a stranger can install it".

---

## 1. P0 blockers (cannot ship without these)

| # | Item | Why | Size |
|---|---|---|---|
| 1 | **Push to GitHub and get CI green** on Windows (Linux/macOS red is allowed for v0.1 but fix what is cheap) | CI has never run. Unknown breakage is the biggest risk | M |
| 2 | **Fix the Go agent import path.** `agents/go/go.mod` declares `prism.dev/agent`, a domain/vanity path nobody serves. Go users cannot `go get` it | The Go "record" story needs `import _ "…/agent"` in the target app. Rename to `github.com/<org>/prism/agents/go` (or serve a vanity path), tag `agents/go/v0.1.0` (nested-module tags are required), update docs and examples | M |
| 3 | **Prebuilt `prism.exe` with the UI embedded**, built in CI by a release workflow on a `v*` tag | Users must not need Rust or Node | M |
| 4 | **Bundle the .NET helper.** Ship `prism-dotnet.exe` next to `prism.exe`, self-contained (the +30–60 MB the plan accepted), or make `prism` extract/locate it. Today the helper is a `.dll` located via `PRISM_DOTNET` | `.NET` recording is half of the pitch. Must work out of the zip | M |
| 5 | **Code signing** for `prism.exe` and `prism-dotnet.exe` | Unsigned binaries that attach to other processes will hit SmartScreen and Defender. See §3 | M (+ lead time) |
| 6 | **Product name cleared.** "Prism" collides with Prism.js, Stoplight Prism, Prisma, Microsoft's Prism, and many crates/packages. Check trademark, GitHub org, domain, crates.io, winget id, npm | Renaming after launch is expensive. Decide before any public artifact exists | S |
| 7 | **Root `README.md`** with install, a 5-minute tour and a screenshot/GIF | The README is the product page for an OSS tool | M |
| 8 | **Third-party notices** (Rust crates via `cargo-about`, npm prod deps, .NET helper deps, Go `fgprof`/`google/pprof`) shipped in the zip. The embedded UI bundle currently drops React's MIT notice | License compliance. Already tracked in FOLLOWUPS | M |
| 9 | **Security headers**: `X-Frame-Options: DENY` / `frame-ancestors 'none'` (and Host-header check for DNS rebinding on the server, and on the Go agent) | A local server with profile data is a rebinding target. FOLLOWUPS already lists it | S–M |
| 10 | **Clean-machine test** on a fresh Windows 11 VM/Sandbox: install from the zip, run the tour, record a Go app and a .NET app, uninstall | The only test that proves the UX | M |

---

## 2. Packaging and distribution (Windows)

Ship several channels, easiest first. v0.1 minimum is the first three.

| Channel | Priority | Notes |
|---|---|---|
| **GitHub Release zip** (`prism-0.1.0-windows-x64.zip`) with `SHA256SUMS` | P0 | Contains `prism.exe`, `prism-dotnet.exe`, `LICENSE`, `THIRD_PARTY_NOTICES.txt`, `README.txt`, `examples/` |
| **Install script**: `irm https://…/install.ps1 \| iex` | P0 | Downloads the zip, verifies the checksum, unpacks to `%LOCALAPPDATA%\Programs\prism`, adds that dir to the **user** PATH (no admin), prints next steps. Also offer a documented "inspect first" form |
| **Scoop** (own bucket `scoop bucket add prism …`) | P1 | Cheapest package-manager path; no review queue |
| **winget** manifest PR to `microsoft/winget-pkgs` | P1 | Most discoverable on Windows; review takes days, start early. Needs a signed binary or installer to pass smoothly |
| **MSI or Inno Setup installer** | P1/P2 | Start-menu entry, PATH, clean uninstall in "Apps & features". The zip + script is enough for v0.1 |
| Chocolatey | P2 | |
| `cargo install prism-cli` / crates.io | P2 | Only useful to Rust users; also needs the UI built and embedded |
| Windows ARM64 build | P2 | CI matrix addition once x64 is stable |

Release workflow requirements:

- Triggered by a `v*` tag; builds UI, Rust (release, LTO), publishes the helper, signs, zips, checksums, uploads to the GitHub Release.
- Reproducible-ish: pinned toolchains (`rust-toolchain.toml`, Node 22.12, .NET 8 SDK).
- Release notes generated from `CHANGELOG.md`.
- A smoke-test job that downloads the **published** zip and runs `prism --version`, `prism import` on a bundled sample, and `prism ui` health check.

---

## 3. Windows trust and security

- **Code signing.** Options: Azure Trusted Signing (low cost, no hardware token), SignPath.io (free for OSS), or an OV/EV certificate. Without it, expect "Windows protected your PC" and Defender false positives. Start the identity validation **now**; it has lead time.
- **Defender/SmartScreen:** submit each release to Microsoft for malware analysis proactively; document "Why does Prism need to attach to my process?" in the FAQ.
- **Permissions model, documented:** attaching to a process needs the same user (or elevation for elevated targets). Say so, and give a clear error when it fails.
- **Local-only server:** binds `127.0.0.1`, no telemetry, no network calls by default. State this prominently; it is a selling point.
- **`SECURITY.md`:** how to report vulnerabilities, supported versions, the trust model (local single-user; not safe to expose on a network yet).
- **Supply chain:** `cargo deny` (exists), `npm audit`, `dotnet list package --vulnerable`, `go vulncheck`. Pin GitHub Actions by SHA for the release workflow.
- Decompression limits and upload size (FOLLOWUPS "Hardening") are acceptable under the local trust model; note it in `SECURITY.md`.

---

## 4. Installation and first-run UX

The first five minutes decide adoption.

| # | Item | Pri | Size |
|---|---|---|---|
| 1 | **`prism doctor`**: checks PATH, version, data dir writable, port 7878 free, `prism-dotnet` found, Go/`dotnet` on PATH, diagnostics IPC reachable; each failure prints a fix | P0 | M |
| 2 | **`prism demo`** (or `prism ui --demo`): loads a bundled sample session so the UI shows value with **no Go/.NET installed and no profile on hand** | P0 | M |
| 3 | **First-run empty state in the UI**: when the store is empty, show "Record a process / Import a file / Try the demo" with copy-paste commands, not a blank table | P0 | M |
| 4 | **`prism targets` onboarding**: when nothing is found, explain how to make a Go app visible (add the agent) and that .NET needs no change | P1 | S |
| 5 | **Actionable errors everywhere**: every failure says what happened, why, and the next command to run. Review all `CaptureError` messages (the current helper hint says "build it with `dotnet publish…`", which is wrong for a binary user) | P0 | M |
| 6 | **`prism --version`** shows version, git SHA, build date, helper version | P0 | S |
| 7 | **`prism ui` opens the browser by default** (recent commit made it opt-in via `--open`; reconsider for first-run, or print a clickable URL prominently) | P1 | S |
| 8 | **Shell completions** (`prism completions powershell`), generated by clap | P1 | S |
| 9 | **Data directory:** move default from Roaming `%APPDATA%` to `%LOCALAPPDATA%` (multi-hundred-MB blobs shouldn't roam). Provide a migration or clear note. FOLLOWUPS already lists it | P0 | S |
| 10 | **`prism uninstall`/docs for removal**: remove the binary, PATH entry and (optionally) data dir; state where data lives | P1 | S |
| 11 | **Update story:** `prism --version` plus a documented upgrade per channel. Optional **opt-in** update check; never on by default | P2 | M |
| 12 | **Windows terminal polish:** colour and Unicode in Windows Terminal, legacy conhost fallback, PowerShell glob note surfaced where users hit it | P1 | S |
| 13 | **Port in use:** automatic fallback or a clear message with `--port` hint | P1 | S |

### Dependency story (what the user needs)

| User wants to… | Needs installed |
|---|---|
| Import files, browse UI, diff, `prism check` | Nothing |
| Record **Go** | Go 1.22+ in their project **and** the prism agent added to the app (`import _ "…/agent"`) — or `-- go test` mode / an existing `net/http/pprof` endpoint |
| Record **.NET** | A .NET 8+ app running; the helper is bundled, no SDK required to *record* |
| Run the examples | Go 1.22+ / .NET 8 SDK |

State this table in the README and in `prism doctor` output. The Go agent requirement is the biggest friction point: consider a zero-code path (`prism record --url` against `net/http/pprof`, already supported) and promote it as the "no code change" option.

---

## 5. Documentation

| Doc | Pri | Notes |
|---|---|---|
| **README.md** (root) | P0 | One-sentence pitch, screenshot/GIF, install (3 channels), 5-minute tour, supported languages table, links. Under 300 lines |
| **Getting started / quickstart** | P0 | The first 10 minutes, Windows-flavoured (PowerShell, not bash). Today's USER_GUIDE uses bash blocks and `&` backgrounding that do not work in PowerShell |
| **Install guide** | P0 | Each channel, upgrade, uninstall, offline/zip install, troubleshooting |
| **How-to guides** (task-oriented, each ≤ 1 page) | P0/P1 | "Find why my Go service is slow" · "Find a memory leak" · "Find lock contention" · "Profile a .NET app" · "Profile `go test`" · "Compare before/after a change" · "Fail CI on a perf regression" · "Import a profile from another tool" |
| **CLI reference** | P0 | Generated from clap so it never drifts (`prism help`, plus a `docs/cli.md` build step) |
| **Concepts** | P1 | Kinds, self vs total, fidelity badges, sessions, what a flame graph/heat map shows. Exists in USER_GUIDE §3; split out |
| **Troubleshooting & FAQ** | P0 | "Attach denied", "no targets found", "SmartScreen blocked it", "port in use", "profile looks empty", "numbers differ from dotnet-trace" |
| **Supported matrix** | P0 | OS × language × profile kind, with fidelity (e.g. .NET CPU is approximate, Go wall-clock has no time data) |
| **Known limitations** | P0 | Curate from FOLLOWUPS "Known limits"; honesty builds trust |
| **Privacy statement** | P0 | Local only, no telemetry |
| **CHANGELOG.md** | P0 | Keep-a-Changelog format, `0.1.0` entry |
| **CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md** | P0 | Short. CONTRIBUTING covers build-from-source (currently USER_GUIDE §1) |
| **Issue/PR templates** | P1 | Bug template asks for `prism doctor` output and `prism --version` |
| **Docs site** (mdBook or Docusaurus on GitHub Pages) | P1 | Searchable; the README links in. Markdown-in-repo is fine for v0.1 |
| **In-product help** | P0 | See §6 |
| **Video/GIF walkthroughs** | P1 | A 60–90 s screen recording of record → flame graph → finding |

Doc quality bar: every command in the docs is copy-pasteable on a clean Windows machine; test them in the clean-VM pass (§1 #10).

---

## 6. In-product help

- **CLI:** `prism --help` and every subcommand have examples (`after_help`), not just flag lists. A "Quick start" epilogue on the bare `prism` command. Typo suggestions (clap provides).
- **UI:** a visible **Help** entry (`?`), keyboard shortcut cheat sheet, tooltips on kinds/fidelity badges/self vs total, a short guided tour on first open (dismissible, never repeats), links to the matching how-to for each view. `guide.ts` already holds analysis guidance; surface it.
- **Findings** panel already explains hot spots; make sure each finding links to a doc page.
- **Empty and error states** in every view (no timeline, no samples, profile truncated) say what to do next.
- **Accessibility:** canvas has no keyboard path (the Top table is the fallback). Document it; run the `a11y-audit` pass on the UI before launch (P1).

---

## 7. Product quality

| # | Item | Pri |
|---|---|---|
| 1 | Verify the "**not verified live**" list from FOLLOWUPS on Windows by hand: drag/Shift-drag selection, graph pan/zoom, window-vs-window diff, Export downloads and clipboard, a full .NET capture with timelines | P0 |
| 2 | Run the perf budgets on a real large profile (500 MB pprof import ≤ 20 s; 1 h heat map; 1 M-sample graph) | P1 |
| 3 | **Real-world validation:** profile 5–10 real open-source Go and .NET apps (not the demos). Fix crashes, misleading output | P0 |
| 4 | .NET matrix: .NET 8 and 9, ASP.NET, console, self-contained apps, elevated processes, containers | P1 |
| 5 | Go matrix: Go 1.22/1.23/1.24, Windows `go test` mode (known: kills only the `go` tool, not the test binary) | P1 |
| 6 | Ctrl-C behaviour while recording (known: abandons in-flight Go requests; exits 0) — make the partial-session message clear | P1 |
| 7 | UI component/interaction tests (none exist); add a Playwright smoke test of the shipped binary | P1 |
| 8 | Crash/panic handling: a panic should print a friendly message with a path to a log file and a bug-report link | P1 |
| 9 | Logging: `--verbose` / `PRISM_LOG`, a log file in the data dir | P1 |
| 10 | Store migration test across versions (v0.1 → v0.2 must not lose user data); keep the schema-version gate | P0 |
| 11 | Windows specifics: paths with spaces and non-ASCII usernames, long paths, OneDrive-redirected folders, antivirus locking blobs, `%LOCALAPPDATA%` redirect | P1 |

---

## 8. Telemetry and feedback

Default stance: **no telemetry**. Say it loudly. Learn through other channels:

- GitHub Discussions (Q&A, show-and-tell) and Issues.
- A feedback link in the UI footer and in `prism --help`.
- An optional, anonymous, **opt-in** usage ping is a v0.2 decision, not v0.1.
- Track from public signals: release downloads, GitHub stars/clones, winget/scoop install counts, issue volume, docs traffic.

---

## 9. Legal and project hygiene

- Apache-2.0 `LICENSE` exists; add `NOTICE` and copyright holder in headers where required.
- Decide the **open-core boundary in writing** (what stays Apache-2.0 forever; what the commercial share-server/team tier is) and say it in the README so no one is surprised later. Consider a CLA vs DCO sign-off.
- Trademark/name: see §1 #6. Domain, GitHub org, social handles reserved.
- Privacy policy page (one paragraph) for the website.
- Dependency licence audit (`cargo deny` passes; add npm and NuGet/Go checks).
- Contributor guide mentions how decisions are made and what is out of scope (not a continuous profiler).

---

## 10. Support and community

- GitHub Issues with templates, Discussions enabled, labels (`bug`, `good first issue`, `windows`, `docs`).
- A public **ROADMAP.md** (Linux/macOS v0.2, Phase 2 contention and I/O, Java/Python/Node later; AI deferred).
- Response-time commitment you can keep (e.g., first reply within 48 h for launch month).
- A Discord/Slack is optional; Discussions are enough at launch.
- Pinned "Known issues" discussion; "Roadmap and voting" post.

---

## 11. Release process (repeatable)

1. Branch `release/0.1.0`; freeze features.
2. `CHANGELOG.md` and version bump (`Cargo.toml`, helper csproj, agent tag).
3. Full CI green on Windows (plus Linux/macOS best-effort).
4. Clean-VM install test (§1 #10) from the **release candidate** artifacts (`v0.1.0-rc.1` pre-release).
5. Dogfood for a week on real projects; soak the RC with 3–5 friendly external testers (see LAUNCH.md beta plan).
6. Tag `v0.1.0`; release workflow publishes the zip, checksums, signatures, agent module tag.
7. Publish install script, Scoop manifest, winget PR.
8. Smoke-test the **published** artifacts.
9. Announce (LAUNCH.md).
10. Hotfix path: `v0.1.1` can ship within a day; document the procedure.

---

## 12. After v0.1 (not blocking)

| Release | Content |
|---|---|
| v0.2 | Linux and macOS binaries (CI matrix already builds both; resolve the "not yet run on Unix" items, e.g. `record_ctrl_c_exit_130`, `/proc` hint, per-user discovery dir), Homebrew tap, `.deb`/`.rpm`/AppImage or a tarball + install script |
| v0.2 | Phase 2 contention and I/O, ungated query paths made fast |
| v0.3 | Windows MSI, ARM64, Chocolatey, docs site, opt-in telemetry decision |
| Later | Java/Python/Node, remote/Kubernetes, Tauri desktop shell, share server (commercial), AI "Prism Ask" |

---

## 13. Suggested schedule (about 4–5 weeks of focused work)

| Week | Focus |
|---|---|
| 1 | Push to GitHub, fix CI; name decision; Go agent module path; start code-signing validation; helper bundling; data dir move |
| 2 | Release workflow (zip, checksums, signing); `prism doctor`; `prism demo`; UI empty state; error-message audit; security headers; notices |
| 3 | Docs: README, quickstart, install, how-tos, FAQ, limitations; screenshots/GIF; install script, Scoop manifest |
| 4 | Clean-VM pass; real-world app validation; fix findings; RC1; private beta with testers; winget PR |
| 5 | RC2 → v0.1.0; launch week (see [LAUNCH.md](LAUNCH.md)) |

---

## 14. Go/No-Go gate

Release only when **all** are true:

- [ ] CI green on Windows for the tagged commit
- [ ] Binaries signed; SmartScreen/Defender checked on a clean machine
- [ ] Clean-VM install → demo → record Go → record .NET → uninstall all pass from the published zip
- [ ] README, quickstart, install, troubleshooting, limitations, privacy, CHANGELOG, LICENSE, notices, SECURITY all present
- [ ] `prism doctor` and `prism demo` work
- [ ] Go agent installable via `go get` from its final path
- [ ] No P0 bugs open; known limitations documented
- [ ] Name, domain and org secured
- [ ] Rollback/hotfix procedure written
