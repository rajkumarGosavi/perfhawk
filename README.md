# Prism

**A local-first profiling workbench for Go and .NET.**
Attach to a running process (or import a profile file) and explore flame graphs, a call graph, a time heat map, before/after diffs and plain-language findings in your browser. Nothing leaves your machine.

> **Status: v0.1, Windows first.** Linux and macOS are next. See [limitations](docs/LIMITATIONS.md) and the [roadmap](docs/PLAN.md#7-roadmap).

<!-- TODO before launch: add a screenshot or GIF of record -> flame graph -> finding. -->

## Why Prism

- **One workflow for several languages.** Go and .NET record into the same model and open in the same UI. You can also import pprof, collapsed stacks, speedscope and `.nettrace` files.
- **Zero-config recording.** `prism record --pid <pid>` for .NET. For Go, add one import line, or point Prism at an existing `net/http/pprof` endpoint.
- **Answers, not just pictures.** Flame graph, Top, sandwich, call graph and a heat map over time, plus a Findings panel that says where to look.
- **Compare anything.** Diff two profiles or two time windows. `prism check` exits non-zero when a function got slower, for CI.
- **Private by design.** Local store, server bound to `127.0.0.1`, no telemetry, no account.
- **Honest numbers.** Every profile carries a fidelity badge (native, approximate, derived) so you know how far to trust it.

Prism is for **on-demand investigation on a developer machine**. It is not a continuous production profiler; for that, see Pyroscope, Parca or Grafana Cloud Profiles.

## Install (Windows 10/11, x64)

PowerShell, no administrator rights needed:

```powershell
irm https://github.com/rajkumarGosavi/prism/releases/latest/download/install.ps1 | iex
```

The script downloads the release zip, verifies its SHA-256 checksum, unpacks it to `%LOCALAPPDATA%\Programs\prism` and adds that folder to your user `PATH`. Open a new terminal, then:

```powershell
prism --version
```

Prefer to do it by hand? Download `prism-<version>-windows-x64.zip` and `SHA256SUMS` from the [latest release](https://github.com/rajkumarGosavi/prism/releases/latest), check the hash, and unzip anywhere. Keep `prism-dotnet.exe` next to `prism.exe`. Full details, upgrade and uninstall: [docs/INSTALL.md](docs/INSTALL.md).

## Try it in five minutes

The shortest path uses the Go demo (needs Go 1.22+). Full walkthrough: [docs/QUICKSTART.md](docs/QUICKSTART.md).

```powershell
git clone https://github.com/rajkumarGosavi/prism
cd prism\examples\go-demo
go run . -out profiles -duration 10s     # writes six profiles and prints an import command
prism import (Get-ChildItem profiles\*.pb.gz)
prism show cpu --top 10
prism ui                                  # open the printed http://127.0.0.1:7878/ address
```

Record a live process instead:

```powershell
prism targets                             # what can Prism see?
prism record --pid 1234 --duration 15s    # .NET 8+: no code change needed
prism ui
```

## What you need installed

| You want to... | Needs |
|---|---|
| Import files, browse the UI, diff, run `prism check` | Nothing else |
| Record **.NET** | A running .NET 8+ app. The recording helper is bundled; no SDK needed |
| Record **Go** | Go 1.22+ and **one** of: the Prism agent in your app (`import _ "github.com/rajkumarGosavi/prism/agents/go"`), a `net/http/pprof` endpoint (`prism record --url ...`), or `prism record --go-test <pkg>` |
| Run the examples | Go 1.22+ and/or the .NET 8 SDK |

## Documentation

| | |
|---|---|
| [Quickstart](docs/QUICKSTART.md) | The first ten minutes on Windows |
| [Install, upgrade, uninstall](docs/INSTALL.md) | All the ways to install, and how to remove it |
| [User guide](docs/USER_GUIDE.md) | Concepts, every command, every view, recipes |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | Windows warnings, "no targets found", attach errors |
| [Known limitations](docs/LIMITATIONS.md) | What v0.1 does not do yet |
| [Regression checks in CI](docs/CI.md) | `prism check` and a GitHub Actions example |
| [Visualisations](docs/VISUALIZATION.md) | What each view shows and how to read it |
| [Roadmap and design](docs/PLAN.md) | Where Prism is going |
| [Changelog](CHANGELOG.md) | What changed in each release |

## Privacy and security

Prism runs entirely on your machine. The UI server listens on `127.0.0.1` only, rejects other host names (DNS-rebinding protection), sends browser hardening headers, and has no telemetry. Profiles are stored in a local folder you can delete at any time. To report a vulnerability see [SECURITY.md](SECURITY.md).

## Contributing

Bug reports, ideas and pull requests are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md). Please follow the [code of conduct](CODE_OF_CONDUCT.md).

## Building from source

You need Rust (stable), Node 22.12+, and for recording tests Go 1.22+ and the .NET 8 SDK. See [CONTRIBUTING.md](CONTRIBUTING.md#building-from-source).

## Licence

Apache-2.0. See [LICENSE](LICENSE) and, in release packages, `THIRD_PARTY_NOTICES.txt`.
