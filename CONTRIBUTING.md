# Contributing to Perfhawk

Thanks for helping. Bug reports, reproducible profiles, docs fixes and pull requests are all welcome.

## Before you start

- Search [existing issues](https://github.com/rajkumarGosavi/perfhawk/issues) first.
- For a bigger change (a new command, a new language), open an issue to discuss it before writing code.
- Perfhawk is for **on-demand profiling on a developer machine**, not continuous production profiling. Proposals that turn it into the latter are out of scope.
- Be kind: see the [code of conduct](CODE_OF_CONDUCT.md).

## Reporting a bug

Include:

- `perfhawk --version` and your Windows version
- the exact command you ran and the full output
- whether the target is Go or .NET and its version
- if possible, a small profile that reproduces it (strip anything sensitive first)

Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## Building from source

Prerequisites: Rust stable (1.88+), Node 22.12+, Go 1.22+, .NET 8 SDK.

```powershell
git clone https://github.com/rajkumarGosavi/perfhawk
cd perfhawk

# 1. UI (embedded into the binary at build time)
cd ui; npm ci; npm run build; cd ..

# 2. Binary
cargo build --release -p perfhawk-cli          # target\release\perfhawk.exe

# 3. .NET helper (put next to perfhawk.exe, or set PERFHAWK_DOTNET)
dotnet publish helpers/perfhawk-dotnet -c Release -r win-x64 --self-contained `
  -p:PublishSingleFile=true -o target\helper
```

Build the UI first; otherwise the server shows a "UI not built" page.

## Running the tests

```powershell
cargo test --workspace                 # Rust
cd ui; npm test; cd ..                 # UI (vitest)
cd agents/go; go vet ./...; go test ./...; cd ../..   # Go agent
dotnet test helpers/perfhawk-dotnet/tests # .NET helper
```

End-to-end capture tests need Go and .NET and are opt-in:

```powershell
$env:PERFHAWK_E2E = "1"
cargo test -p perfhawk-capture -- --include-ignored --test-threads=1
```

CI runs all of this on pull requests.

## Pull requests

- Keep each pull request focused; one change per PR.
- Add or update tests with the change. UI logic lives in `ui/src/**` with `*.test.ts` beside it.
- Run `cargo fmt`, `cargo clippy --workspace` and `npm run lint` in `ui/` first.
- Update [CHANGELOG.md](CHANGELOG.md) under **Unreleased**, and the docs if behaviour changed.
- Dependencies must be permissively licensed (`cargo deny check licenses` must pass).
- Write commit messages in the imperative mood, with a type prefix as in the existing history (`feat:`, `fix:`, `docs:`, `test:`).

## Licence

By contributing you agree that your contribution is licensed under the project's [Apache-2.0](LICENSE) licence.

## Project layout

| Path | What lives there |
|---|---|
| `crates/perfhawk-model` | The unified profile model |
| `crates/perfhawk-import` | pprof, collapsed, speedscope readers |
| `crates/perfhawk-analysis` | Flame, top, diff, graph, heat map, findings |
| `crates/perfhawk-store` | Local SQLite and blob store |
| `crates/perfhawk-capture` | Recording from live Go and .NET processes |
| `crates/perfhawk-server` | Local HTTP API and embedded UI |
| `crates/perfhawk-cli` | The `perfhawk` command |
| `agents/go` | Go agent library |
| `helpers/perfhawk-dotnet` | .NET recording helper |
| `ui/` | React and TypeScript browser UI |
| `docs/` | User and design documentation |
