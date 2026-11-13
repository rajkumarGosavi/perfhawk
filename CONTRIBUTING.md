# Contributing to Prism

Thanks for helping. Bug reports, reproducible profiles, docs fixes and pull requests are all welcome.

## Before you start

- Search [existing issues](https://github.com/rajkumarGosavi/prism/issues) first.
- For a bigger change (a new command, a new language), open an issue to discuss it before writing code.
- Prism is for **on-demand profiling on a developer machine**, not continuous production profiling. Proposals that turn it into the latter are out of scope.
- Be kind: see the [code of conduct](CODE_OF_CONDUCT.md).

## Reporting a bug

Include:

- `prism --version` and your Windows version
- the exact command you ran and the full output
- whether the target is Go or .NET and its version
- if possible, a small profile that reproduces it (strip anything sensitive first)

Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## Building from source

Prerequisites: Rust stable (1.88+), Node 22.12+, Go 1.22+, .NET 8 SDK.

```powershell
git clone https://github.com/rajkumarGosavi/prism
cd prism

# 1. UI (embedded into the binary at build time)
cd ui; npm ci; npm run build; cd ..

# 2. Binary
cargo build --release -p prism-cli          # target\release\prism.exe

# 3. .NET helper (put next to prism.exe, or set PRISM_DOTNET)
dotnet publish helpers/prism-dotnet -c Release -r win-x64 --self-contained `
  -p:PublishSingleFile=true -o target\helper
```

Build the UI first; otherwise the server shows a "UI not built" page.

## Running the tests

```powershell
cargo test --workspace                 # Rust
cd ui; npm test; cd ..                 # UI (vitest)
cd agents/go; go vet ./...; go test ./...; cd ../..   # Go agent
dotnet test helpers/prism-dotnet/tests # .NET helper
```

End-to-end capture tests need Go and .NET and are opt-in:

```powershell
$env:PRISM_E2E = "1"
cargo test -p prism-capture -- --include-ignored --test-threads=1
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
| `crates/prism-model` | The unified profile model |
| `crates/prism-import` | pprof, collapsed, speedscope readers |
| `crates/prism-analysis` | Flame, top, diff, graph, heat map, findings |
| `crates/prism-store` | Local SQLite and blob store |
| `crates/prism-capture` | Recording from live Go and .NET processes |
| `crates/prism-server` | Local HTTP API and embedded UI |
| `crates/prism-cli` | The `prism` command |
| `agents/go` | Go agent library |
| `helpers/prism-dotnet` | .NET recording helper |
| `ui/` | React and TypeScript browser UI |
| `docs/` | User and design documentation |
