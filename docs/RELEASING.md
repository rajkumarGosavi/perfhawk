# Releasing Perfhawk

For maintainers. The engineering checklist is in [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md); the launch plan is in [LAUNCH.md](LAUNCH.md).

## The flow (3 steps)

1. **Prep PR, merged to `main`:** set the same `version` in `Cargo.toml` `[workspace.package]` and `<Version>` in `helpers/perfhawk-dotnet/PerfhawkDotnet.csproj`; in `CHANGELOG.md` change `## [x.y.z] - unreleased` to `## [x.y.z] - YYYY-MM-DD`. CI green.
2. **Run the release script** from a clean, up-to-date `main`:
   ```powershell
   ./tools/release/release.ps1 -Version 0.1.0 -DryRun   # checks only
   ./tools/release/release.ps1 -Version 0.1.0           # tags + pushes
   ```
   It verifies branch, clean tree, versions, changelog and that the tag is unused, then pushes `v0.1.0` (and `agents/go/v0.1.0` for stable releases).
3. **Wait for the Release workflow.** It builds, packages, smoke-tests and **publishes** the release automatically. Nothing to click.

Then verify on a clean Windows machine: `irm https://github.com/rajkumarGosavi/perfhawk/releases/latest/download/install.ps1 | iex`, follow the [Quickstart](QUICKSTART.md), uninstall. Announce per [LAUNCH.md](LAUNCH.md).

## Rules

- **Stable** `v0.1.0` becomes "latest" (what `install.ps1` and the `latest/download` URL resolve to).
- **Pre-release** `v0.1.0-rc.1` is published but never "latest". Test it with the tag URL: `irm https://github.com/rajkumarGosavi/perfhawk/releases/download/v0.1.0-rc.1/install.ps1 | iex`.
- **Never create releases by hand** in the GitHub UI: a manual release on an existing tag has no assets and breaks `latest`.
- **Never move or delete a pushed tag** (Go's module proxy caches forever). If something is wrong, ship the next patch version.
- If the workflow fails before publishing, fix on `main`, bump the version (or delete the tag only if nothing was published), and re-run the script.

## What a release produces

| Asset | What |
|---|---|
| `perfhawk-<version>-windows-x64.zip` | `perfhawk.exe` (UI embedded), `perfhawk-dotnet.exe` (self-contained helper), `LICENSE`, `THIRD_PARTY_NOTICES.txt`, `CHANGELOG.md`, `README.txt`, `examples\` |
| `SHA256SUMS` | SHA-256 of the zip |
| `install.ps1` | The installer, from `scripts/install.ps1` |

Plus the Go agent module tag `agents/go/v<version>` (nested module, prefixed with its directory). Verify from an empty module:
```powershell
mkdir $env:TEMP\gotest; cd $env:TEMP\gotest
go mod init example.com/t
go get github.com/rajkumarGosavi/perfhawk/agents/go@v0.1.0
```

## Hotfix

Branch from the tag, fix, bump the patch version, merge to `main`, run the script with the new version.

## Code signing

Unsigned binaries trigger SmartScreen and antivirus warnings. Sign **`perfhawk.exe` and `perfhawk-dotnet.exe` before they are zipped**, i.e. in the Release workflow between "Third-party notices" and "Package" (there is a comment marking the spot).

Options, cheapest first:

- **SignPath Foundation**: free for open-source projects, applies via their website, integrates with GitHub Actions.
- **Azure Trusted Signing**: low monthly cost, no hardware token. Needs an Azure subscription and identity validation (can take days).
- **OV/EV certificate** from a CA: EV gives immediate SmartScreen reputation but needs a hardware token or cloud HSM.

Start identity validation early; it has lead time. After signing, add a verification step: `Get-AuthenticodeSignature .\perfhawk.exe` must be `Valid`.
Submit each release to Microsoft Security Intelligence (<https://www.microsoft.com/wdsi/filesubmission>) as a proactive false-positive check.

## Local dry run

From a Windows PowerShell prompt at the repo root (needs Rust, Node, .NET 8 SDK, cargo-about):

```powershell
cd ui; npm ci; npm run build; cd ..
cargo build --release --locked -p perfhawk-cli
dotnet publish helpers/perfhawk-dotnet -c Release -r win-x64 --self-contained `
  -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true `
  -p:EnableCompressionInSingleFile=true -o target/helper
./tools/release/build-notices.ps1
./tools/release/package.ps1 -Version dev-local
./tools/release/smoke-test.ps1 -Zip dist/perfhawk-dev-local-windows-x64.zip
```

## Generated files

`THIRD_PARTY_NOTICES.txt`, `dist/` and `target/` are generated and git-ignored.
